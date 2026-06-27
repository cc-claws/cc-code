mod file_reader;
mod parser;

pub use file_reader::FileContent;

use std::path::PathBuf;

use async_trait::async_trait;
use peri_agent::{
    agent::state::State,
    error::AgentResult,
    messages::{BaseMessage, ContentBlock},
    middleware::r#trait::Middleware,
};

use crate::tool_search::core_tools::TOOL_READ;

/// AtMentionMiddleware — 解析用户消息中的 @path 提及，注入 Read 工具调用结果
///
/// 在 `before_agent` 时从最后一条 Human 消息中提取 @ 提及，
/// 读取对应文件内容，以 Ai[ToolUse{Read}] → Tool[ToolResult] 消息序列追加到 state。
///
/// 消息结构（与 SkillPreloadMiddleware 一致）：
/// ```text
/// [Human "用户消息（含 @path）"]
/// [Ai]    [ToolUse{Read, call_{hex}}, ...]
/// [Tool]  ToolResult{call_{hex}, file_content}
/// ...
/// ```
pub struct AtMentionMiddleware {
    cwd: PathBuf,
}

impl AtMentionMiddleware {
    pub fn new(cwd: PathBuf) -> Self {
        Self { cwd }
    }
}

#[async_trait]
impl<S: State> Middleware<S> for AtMentionMiddleware {
    fn name(&self) -> &str {
        "AtMentionMiddleware"
    }

    async fn before_agent(&self, state: &mut S) -> AgentResult<()> {
        // 取最后一条 Human 消息
        let last_human = state
            .messages()
            .iter()
            .rev()
            .find(|m| matches!(m, BaseMessage::Human { .. }));

        let text = match last_human {
            Some(msg) => msg.content(),
            None => return Ok(()),
        };

        let mentions = parser::extract_at_mentions(&text);
        if mentions.is_empty() {
            return Ok(());
        }

        // 在 blocking 线程中读取文件
        let cwd = self.cwd.clone();
        let file_contents: Vec<(parser::AtMention, Option<FileContent>)> =
            tokio::task::spawn_blocking(move || {
                mentions
                    .into_iter()
                    .map(|m| {
                        let content =
                            file_reader::read_file_content(&cwd, &m.path, m.line_start, m.line_end);
                        (m, content)
                    })
                    .collect::<Vec<_>>()
            })
            .await
            .map_err(|e| peri_agent::error::AgentError::MiddlewareError {
                middleware: "AtMentionMiddleware".to_string(),
                reason: format!("spawn_blocking 失败: {e}"),
            })?;

        // 过滤掉读取失败的
        let valid: Vec<_> = file_contents
            .into_iter()
            .filter_map(|(m, c)| c.map(|c| (m, c)))
            .collect();

        if valid.is_empty() {
            return Ok(());
        }

        // 按文件/目录分组：文件走 Read ToolUse（语义"读取文件内容"），
        // 目录改为 System 消息注入——Read 工具的语义是"读文件"，目录列表
        // 用 ToolUse 包装会让 LLM 误以为是文件内容（issue #28）。
        let (files, dirs): (Vec<_>, Vec<_>) = valid
            .into_iter()
            .partition(|(_, fc)| !fc.is_dir);

        if files.is_empty() && dirs.is_empty() {
            return Ok(());
        }

        // 文件 @mention：Ai[ToolUse{Read}] + Tool[ToolResult]
        if !files.is_empty() {
            // 生成 call_id
            let call_ids: Vec<String> = (0..files.len())
                .map(|_| format!("call_{}", uuid::Uuid::new_v4().simple()))
                .collect();

            // 构造 ToolUse blocks
            let tool_use_blocks: Vec<ContentBlock> = files
                .iter()
                .zip(call_ids.iter())
                .map(|((mention, _), id)| {
                    let mut input = serde_json::json!({
                        "file_path": mention.path,
                    });
                    if let Some(offset) = mention.line_start {
                        input["offset"] = serde_json::json!(offset);
                    }
                    ContentBlock::tool_use(id.clone(), TOOL_READ, input)
                })
                .collect();

            // 追加 Ai 消息
            state.add_message(BaseMessage::ai_from_blocks(tool_use_blocks));

            // 追加 ToolResult 消息
            for (id, (_mention, fc)) in call_ids.iter().zip(files.iter()) {
                let prefix = match (fc.line_start, fc.line_end) {
                    (Some(s), Some(e)) => format!("→ {} (L{s}-L{e})", fc.path),
                    (Some(s), None) => format!("→ {} (L{s})", fc.path),
                    _ => format!("→ {}", fc.path),
                };
                let content = format!("{prefix}\n{}", fc.content);
                state.add_message(BaseMessage::tool_result(id.clone(), content));
            }
        }

        // 目录 @mention：System 消息注入，明确标注是目录列表
        if !dirs.is_empty() {
            let mut buf = String::from("## @mentioned directories\n\n");
            for (mention, fc) in &dirs {
                buf.push_str(&format!(
                    "### {path}\n\n```\n{content}\n```\n\n",
                    path = mention.path,
                    content = fc.content
                ));
            }
            buf.push_str(
                "以上为 `@` 提及的目录列表，由 AtMentionMiddleware 在用户提交时展开。\
                 如需进一步查看子文件，请用 Read/Glob 工具。",
            );
            state.add_message(BaseMessage::system(buf));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use peri_agent::agent::state::AgentState;
    use std::fs;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_no_mentions_no_injection() {
        // 无 @ 提及时不注入任何消息
        let dir = tempdir().unwrap();
        let mw = AtMentionMiddleware::new(dir.path().to_path_buf());
        let mut state = AgentState::default();
        state.cwd = dir.path().to_string_lossy().to_string();
        state.add_message(BaseMessage::human("你好世界"));

        let before_len = state.messages().len();
        mw.before_agent(&mut state).await.unwrap();
        // 没有注入，消息数不变
        assert_eq!(state.messages().len(), before_len);
    }

    #[tokio::test]
    async fn test_mention_injects_read_tool() {
        // @test.rs 注入 Ai[ToolUse] + Tool[ToolResult] 共 2 条消息
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("test.rs"), "fn main() {}\n").unwrap();
        let mw = AtMentionMiddleware::new(dir.path().to_path_buf());
        let mut state = AgentState::default();
        state.cwd = dir.path().to_string_lossy().to_string();
        state.add_message(BaseMessage::human("看看 @test.rs"));

        mw.before_agent(&mut state).await.unwrap();

        // 1 Human + 1 Ai + 1 Tool = 3
        assert_eq!(state.messages().len(), 3);

        // 第二条是 Ai，包含 ToolUse
        let ai_msg = &state.messages()[1];
        assert!(matches!(ai_msg, BaseMessage::Ai { .. }));
        assert!(ai_msg.has_tool_calls());

        // 第三条是 Tool 结果
        let tool_msg = &state.messages()[2];
        assert!(matches!(tool_msg, BaseMessage::Tool { .. }));
        let tool_content = tool_msg.content();
        assert!(tool_content.starts_with("→ test.rs"));
        assert!(tool_content.contains("fn main() {}"));
    }

    #[tokio::test]
    async fn test_mention_directory_injects_system_message() {
        // @some_dir 注入为 System 消息，不再走 Read ToolUse（issue #28）
        let dir = tempdir().unwrap();
        fs::create_dir(dir.path().join("some_dir")).unwrap();
        fs::write(dir.path().join("some_dir").join("a.txt"), "aaa").unwrap();
        fs::write(dir.path().join("some_dir").join("b.txt"), "bbb").unwrap();
        let mw = AtMentionMiddleware::new(dir.path().to_path_buf());
        let mut state = AgentState::default();
        state.cwd = dir.path().to_string_lossy().to_string();
        state.add_message(BaseMessage::human("看看 @some_dir"));

        mw.before_agent(&mut state).await.unwrap();

        // 1 Human + 1 System = 2
        assert_eq!(state.messages().len(), 2, "目录应注入 System 而非 ToolUse");

        let sys_msg = &state.messages()[1];
        assert!(matches!(sys_msg, BaseMessage::System { .. }));
        let content = sys_msg.content();
        assert!(
            content.contains("mentioned directories"),
            "应明确标注是目录列表: {content}"
        );
        // 不应伪装成 ToolResult
        assert!(!content.starts_with("→ "), "目录不应走 ToolResult 前缀");
    }

    #[tokio::test]
    async fn test_mention_mixed_files_and_dirs() {
        // 混合场景：1 个文件 + 1 个目录
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("file.rs"), "fn main() {}\n").unwrap();
        fs::create_dir(dir.path().join("subdir")).unwrap();
        fs::write(dir.path().join("subdir").join("inside.txt"), "x").unwrap();
        let mw = AtMentionMiddleware::new(dir.path().to_path_buf());
        let mut state = AgentState::default();
        state.cwd = dir.path().to_string_lossy().to_string();
        state.add_message(BaseMessage::human("看 @file.rs 和 @subdir"));

        mw.before_agent(&mut state).await.unwrap();

        // 1 Human + 1 Ai(ToolUse file) + 1 ToolResult(file) + 1 System(dir) = 4
        assert_eq!(state.messages().len(), 4, "混合场景应有 4 条消息");

        // 验证：[1]=Ai(ToolUse), [2]=Tool(file content), [3]=System(dir)
        assert!(matches!(state.messages()[1], BaseMessage::Ai { .. }));
        assert!(matches!(state.messages()[2], BaseMessage::Tool { .. }));
        assert!(matches!(state.messages()[3], BaseMessage::System { .. }));
        let sys_content = state.messages()[3].content();
        assert!(
            sys_content.contains("inside.txt"),
            "System 消息应含目录列表: {sys_content}"
        );
    }
}
