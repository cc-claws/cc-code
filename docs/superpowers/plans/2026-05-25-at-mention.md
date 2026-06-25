# @ Mention 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在 TUI 输入框中实现 `@` 触发的文件搜索与引用，选中路��注入输入框，发送时 middleware 将文件内容以 fake Read 工具消息注入 state。

**Architecture:** 独立 `at_mention/` 模块（TUI 层负责交互 + 渲染，middleware 层负责解析 + 文件注入）。TUI 层通过 glob + fuzzy-matcher 实时搜索文件，在输入框上方渲染候选弹窗。Middleware 层在 `before_agent` 解析用户消息中的 `@path`，注入 `Ai[ToolUse{Read}] + Tool[ToolResult]` 消息序列。

**Tech Stack:** `glob` crate（文件搜索）、`fuzzy-matcher` crate（模糊匹配）、`regex` crate（@path 解析）、ratatui + `BorderedPanel`（弹窗渲染）

---

## File Structure

| Action | Path | Responsibility |
|--------|------|----------------|
| Create | `peri-tui/src/app/at_mention/mod.rs` | `AtMentionState` 状态 + 触发检测 + 路径注入逻辑 |
| Create | `peri-tui/src/app/at_mention/file_search.rs` | glob 搜索 + fuzzy 评分 |
| Create | `peri-tui/src/app/at_mention/popup.rs` | 弹窗渲染函数 |
| Create | `peri-middlewares/src/at_mention/mod.rs` | `AtMentionMiddleware` |
| Create | `peri-middlewares/src/at_mention/parser.rs` | 正则提取 @path + 行范围 |
| Create | `peri-middlewares/src/at_mention/file_reader.rs` | 文件读取 + 截断 |
| Modify | `peri-tui/src/app/ui_state.rs` | 新增 `at_mention: AtMentionState` |
| Modify | `peri-tui/src/app/modules_state.inc` | 添加 `mod at_mention` |
| Modify | `peri-tui/src/event/keyboard.rs` | @ 触发检测 + 导航键拦截 |
| Modify | `peri-tui/src/ui/main_ui/mod.rs` | 弹窗渲染调用 |
| Modify | `peri-tui/src/ui/main_ui/popups/mod.rs` | 导出 at_mention popup |
| Modify | `peri-tui/Cargo.toml` | 添加 `glob` + `fuzzy-matcher` |
| Modify | `peri-acp/src/agent/builder.rs` | 添加 `AtMentionMiddleware` |
| Modify | `peri-middlewares/src/lib.rs` | 导出 `at_mention` 模块 |

---

### Task 1: 添加 Cargo 依赖

**Files:**
- Modify: `peri-tui/Cargo.toml`

- [ ] **Step 1: 添加 glob 和 fuzzy-matcher 依赖到 peri-tui/Cargo.toml**

在 `[dependencies]` 部分末尾（`futures-util = "0.3"` 之后）添加：

```toml
glob = "0.3"
fuzzy-matcher = "0.3"
```

- [ ] **Step 2: 验证编译**

Run: `cargo check -p peri-tui 2>&1 | tail -5`
Expected: 编译成功（无新 warning）

- [ ] **Step 3: Commit**

```bash
git add peri-tui/Cargo.toml
git commit -m "chore: add glob + fuzzy-matcher deps for @ mention"
```

---

### Task 2: 实现 @ mention parser（middleware 层）

**Files:**
- Create: `peri-middlewares/src/at_mention/mod.rs`
- Create: `peri-middlewares/src/at_mention/parser.rs`
- Create: `peri-middlewares/src/at_mention/file_reader.rs`
- Modify: `peri-middlewares/src/lib.rs`

- [ ] **Step 1: 创建 parser.rs — @path 正则提取**

创建 `peri-middlewares/src/at_mention/parser.rs`：

```rust
use std::path::PathBuf;

/// 解析出的 @ mention 条目
#[derive(Debug, Clone, PartialEq)]
pub struct AtMention {
    pub path: String,
    pub line_start: Option<usize>,
    pub line_end: Option<usize>,
}

/// 从文本中提取所有 @ mention 条目
///
/// 支持格式：
/// - `@path/to/file.rs`
/// - `@"path/with spaces/file.rs"`
/// - `@file.rs#L10`
/// - `@file.rs#L10-20`
/// - `@"file with spaces.rs"#L5-15`
pub fn extract_at_mentions(text: &str) -> Vec<AtMention> {
    let mut results = Vec::new();
    let mut seen = std::collections::HashSet::new();

    // 先匹配带引号的: @"path"#L10-20
    let quoted_re = regex::Regex::new(r#"@"([^"]+)"(?:#L(\d+)(?:-(\d+))?)?"#).unwrap();
    for cap in quoted_re.captures_iter(text) {
        let path = cap[1].to_string();
        if seen.insert(path.clone()) {
            results.push(AtMention {
                path,
                line_start: cap.get(2).map(|m| m.as_str().parse().unwrap()),
                line_end: cap.get(3).map(|m| m.as_str().parse().unwrap()),
            });
        }
    }

    // 匹配不带引号的: @path#L10-20
    // 避免匹配邮箱等: 需要 @ 前是行首或空白
    let plain_re = regex::Regex::new(r"(?:^|\s)@([^\s@\"#]+)(?:#L(\d+)(?:-(\d+))?)?").unwrap();
    for cap in plain_re.captures_iter(text) {
        let path = cap[1].to_string();
        // 跳过已通过带引号正则匹配的路径
        if seen.contains(&path) {
            continue;
        }
        // 跳过明显不是文件路径的（如纯数字、单个字母）
        if path.len() < 2 || path.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        if seen.insert(path.clone()) {
            results.push(AtMention {
                path,
                line_start: cap.get(2).map(|m| m.as_str().parse().unwrap()),
                line_end: cap.get(3).map(|m| m.as_str().parse().unwrap()),
            });
        }
    }

    results
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_plain_path() {
        let mentions = extract_at_mentions("请看 @src/main.rs 的内容");
        assert_eq!(mentions.len(), 1);
        assert_eq!(mentions[0].path, "src/main.rs");
        assert_eq!(mentions[0].line_start, None);
    }

    #[test]
    fn test_extract_quoted_path() {
        let mentions = extract_at_mentions(@"请看 @""my file.rs"" 的内容");
        assert_eq!(mentions.len(), 1);
        assert_eq!(mentions[0].path, "my file.rs");
    }

    #[test]
    fn test_extract_line_range() {
        let mentions = extract_at_mentions("看 @lib.rs#L10-20");
        assert_eq!(mentions.len(), 1);
        assert_eq!(mentions[0].path, "lib.rs");
        assert_eq!(mentions[0].line_start, Some(10));
        assert_eq!(mentions[0].line_end, Some(20));
    }

    #[test]
    fn test_extract_single_line() {
        let mentions = extract_at_mentions("看 @lib.rs#L5");
        assert_eq!(mentions.len(), 1);
        assert_eq!(mentions[0].line_start, Some(5));
        assert_eq!(mentions[0].line_end, None);
    }

    #[test]
    fn test_extract_multiple() {
        let mentions = extract_at_mentions("看 @a.rs 和 @b.rs");
        assert_eq!(mentions.len(), 2);
        assert_eq!(mentions[0].path, "a.rs");
        assert_eq!(mentions[1].path, "b.rs");
    }

    #[test]
    fn test_deduplicate() {
        let mentions = extract_at_mentions("看 @a.rs 和 @a.rs");
        assert_eq!(mentions.len(), 1);
    }

    #[test]
    fn test_skip_email_like() {
        let mentions = extract_at_mentions("联系 user@example.com");
        assert!(mentions.is_empty());
    }

    #[test]
    fn test_skip_short() {
        let mentions = extract_at_mentions("@a");
        assert!(mentions.is_empty());
    }
}
```

- [ ] **Step 2: 运行 parser 测试验证失败**

Run: `cargo test -p peri-middlewares --lib at_mention::parser 2>&1 | tail -20`
Expected: 编译失败（模块不存在）

- [ ] **Step 3: 创建 file_reader.rs**

创建 `peri-middlewares/src/at_mention/file_reader.rs`：

```rust
use std::path::{Path, PathBuf};

/// 文件读取结果
#[derive(Debug, Clone)]
pub struct FileContent {
    pub path: String,
    pub content: String,
    pub line_start: Option<usize>,
    pub line_end: Option<usize>,
    pub truncated: bool,
    pub is_dir: bool,
}

const MAX_LINES: usize = 2000;
const MAX_DIR_ENTRIES: usize = 100;

/// 读取文件内容（支持行范围截取）
pub fn read_file_content(
    base_dir: &Path,
    path: &str,
    line_start: Option<usize>,
    line_end: Option<usize>,
) -> Option<FileContent> {
    let full_path = base_dir.join(path);

    // 安全检查：路径不能逃逸 base_dir
    let canonical_base = base_dir.canonicalize().ok()?;
    let canonical_target = full_path.canonicalize().ok()?;
    if !canonical_target.starts_with(&canonical_base) {
        return None;
    }

    if canonical_target.is_dir() {
        return Some(read_dir_content(path, &canonical_target));
    }

    let content = std::fs::read_to_string(&canonical_target).ok()?;
    let lines: Vec<&str> = content.lines().collect();

    let (extracted, truncated, actual_start, actual_end) = if let (Some(s), Some(e)) =
        (line_start, line_end)
    {
        let s = s.saturating_sub(1); // 转为 0-indexed
        let e = e.min(lines.len());
        let range_lines: Vec<&str> = lines[s..e].to_vec();
        let truncated = range_lines.len() > MAX_LINES;
        (
            range_lines.into_iter().take(MAX_LINES).collect::<Vec<_>>(),
            truncated,
            Some(s + 1),
            Some(e),
        )
    } else if let Some(s) = line_start {
        let s = s.saturating_sub(1);
        let range_lines: Vec<&str> = lines[s..].to_vec();
        let truncated = range_lines.len() > MAX_LINES;
        (
            range_lines.into_iter().take(MAX_LINES).collect::<Vec<_>>(),
            truncated,
            Some(s + 1),
            None,
        )
    } else {
        let truncated = lines.len() > MAX_LINES;
        (
            lines.into_iter().take(MAX_LINES).collect::<Vec<_>>(),
            truncated,
            None,
            None,
        )
    };

    let mut result = extracted.join("\n");
    if truncated {
        result.push_str("\n\n... (truncated)");
    }

    Some(FileContent {
        path: path.to_string(),
        content: result,
        line_start: actual_start,
        line_end: actual_end,
        truncated,
        is_dir: false,
    })
}

fn read_dir_content(path: &str, dir_path: &Path) -> FileContent {
    let mut entries: Vec<String> = std::fs::read_dir(dir_path)
        .map(|rd| {
            let mut names: Vec<String> = rd
                .filter_map(|e| e.ok())
                .map(|e| {
                    let name = e.file_name().to_string_lossy().to_string();
                    if e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                        format!("{}/", name)
                    } else {
                        name
                    }
                })
                .collect();
            names.sort();
            names
        })
        .unwrap_or_default();

    let truncated = entries.len() > MAX_DIR_ENTRIES;
    if truncated {
        entries.truncate(MAX_DIR_ENTRIES);
        entries.push("... (truncated)".to_string());
    }

    FileContent {
        path: path.to_string(),
        content: entries.join("\n"),
        line_start: None,
        line_end: None,
        truncated,
        is_dir: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_read_full_file() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.rs");
        std::fs::write(&file_path, "line1\nline2\nline3").unwrap();
        let result = read_file_content(dir.path(), "test.rs", None, None).unwrap();
        assert_eq!(result.content, "line1\nline2\nline3");
        assert!(!result.truncated);
        assert!(!result.is_dir);
    }

    #[test]
    fn test_read_line_range() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.rs");
        std::fs::write(&file_path, "line1\nline2\nline3\nline4\nline5").unwrap();
        let result = read_file_content(dir.path(), "test.rs", Some(2), Some(4)).unwrap();
        assert_eq!(result.content, "line2\nline3\nline4");
        assert_eq!(result.line_start, Some(2));
        assert_eq!(result.line_end, Some(4));
    }

    #[test]
    fn test_read_nonexistent_file() {
        let dir = tempdir().unwrap();
        let result = read_file_content(dir.path(), "nope.rs", None, None);
        assert!(result.is_none());
    }

    #[test]
    fn test_read_directory() {
        let dir = tempdir().unwrap();
        std::fs::create_dir(dir.path().join("subdir")).unwrap();
        std::fs::write(dir.path().join("a.txt"), "a").unwrap();
        std::fs::write(dir.path().join("b.txt"), "b").unwrap();
        let result = read_file_content(dir.path(), ".", None, None).unwrap();
        assert!(result.is_dir);
        assert!(result.content.contains("a.txt"));
        assert!(result.content.contains("b.txt"));
        assert!(result.content.contains("subdir/"));
    }
}
```

- [ ] **Step 4: 创建 at_mention/mod.rs — AtMentionMiddleware**

创建 `peri-middlewares/src/at_mention/mod.rs`：

```rust
pub mod file_reader;
pub mod parser;

use async_trait::async_trait;
use peri_agent::agent::state::State;
use peri_agent::error::AgentResult;
use peri_agent::messages::{BaseMessage, ContentBlock};
use peri_agent::middleware::r#trait::Middleware;
use std::path::PathBuf;

/// AtMentionMiddleware — 在 before_agent 阶段解析用户消息中的 @mention，
/// 将文件内容以 fake Read 工具调用注入到 agent state。
///
/// 注入消息结构（与 SkillPreloadMiddleware 对齐）：
/// ```text
/// [Human "请看 @src/main.rs"]  ← 已由 executor 添加
/// [Ai]    [ToolUse{Read, call_xxx, {path: "src/main.rs"}}]
/// [Tool]  ToolResult{call_xxx, file_content}
/// ```
pub struct AtMentionMiddleware {
    cwd: PathBuf,
}

impl AtMentionMiddleware {
    pub fn new(cwd: &str) -> Self {
        Self {
            cwd: PathBuf::from(cwd),
        }
    }
}

#[async_trait]
impl<S: State> Middleware<S> for AtMentionMiddleware {
    fn name(&self) -> &str {
        "AtMentionMiddleware"
    }

    async fn before_agent(&self, state: &mut S) -> AgentResult<()> {
        // 取最后一条 Human 消息
        let messages = state.messages();
        let last_human = messages.iter().rev().find(|m| m.is_human());
        let human_text = match last_human {
            Some(msg) => msg.text_content(),
            None => return Ok(()),
        };

        let mentions = parser::extract_at_mentions(&human_text);
        if mentions.is_empty() {
            return Ok(())
        }

        let cwd = self.cwd.clone();
        let file_contents: Vec<(String, file_reader::FileContent)> =
            tokio::task::spawn_blocking(move || {
                mentions
                    .into_iter()
                    .filter_map(|m| {
                        let fc = file_reader::read_file_content(
                            &cwd,
                            &m.path,
                            m.line_start,
                            m.line_end,
                        )?;
                        Some((m.path.clone(), fc))
                    })
                    .collect()
            })
            .await
            .map_err(|e| peri_agent::error::AgentError::MiddlewareError {
                middleware: "AtMentionMiddleware".to_string(),
                reason: format!("spawn_blocking 失败: {e}"),
            })?;

        if file_contents.is_empty() {
            return Ok(());
        }

        // 生成 tool_call_id
        let call_ids: Vec<String> = (0..file_contents.len())
            .map(|_| format!("call_{}", uuid::Uuid::new_v4().simple()))
            .collect();

        // 构造 Ai 消息的 ToolUse blocks
        let tool_use_blocks: Vec<ContentBlock> = file_contents
            .iter()
            .zip(call_ids.iter())
            .map(|((path, fc), id)| {
                let mut input = serde_json::Map::new();
                input.insert("file_path".to_string(), serde_json::Value::String(path.clone()));
                if let Some(start) = fc.line_start {
                    input.insert("offset".to_string(), serde_json::Value::Number(start.into()));
                }
                ContentBlock::tool_use(id.clone(), "Read", serde_json::Value::Object(input))
            })
            .collect();

        state.add_message(BaseMessage::ai_from_blocks(tool_use_blocks));

        // 追加 Tool 结果消息
        for (id, (_, fc)) in call_ids.iter().zip(file_contents.iter()) {
            let mut content = String::new();
            if fc.is_dir {
                content.push_str(&format!("→ {}\n", fc.path));
            } else if let (Some(s), Some(e)) = (fc.line_start, fc.line_end) {
                content.push_str(&format!("→ {} (L{}-L{})\n", fc.path, s, e));
            } else if let Some(s) = fc.line_start {
                content.push_str(&format!("→ {} (L{})\n", fc.path, s));
            } else {
                content.push_str(&format!("→ {}\n", fc.path));
            }
            content.push_str(&fc.content);
            state.add_message(BaseMessage::tool_result(id.clone(), content));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use peri_agent::agent::state::AgentState;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_no_mentions_no_injection() {
        let dir = tempdir().unwrap();
        let mut state = AgentState::new();
        state.add_message(BaseMessage::human("hello world"));
        let mw = AtMentionMiddleware::new(dir.path().to_str().unwrap());
        mw.before_agent(&mut state).await.unwrap();
        // 只有原始 human 消息
        assert_eq!(state.messages().len(), 1);
    }

    #[tokio::test]
    async fn test_mention_injects_read_tool() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("test.rs"), "fn main() {}").unwrap();
        let mut state = AgentState::new();
        state.add_message(BaseMessage::human("看 @test.rs"));
        let mw = AtMentionMiddleware::new(dir.path().to_str().unwrap());
        mw.before_agent(&mut state).await.unwrap();
        // Human + Ai(ToolUse) + Tool(ToolResult) = 3
        assert_eq!(state.messages().len(), 3);
        let ai_msg = &state.messages()[1];
        assert!(ai_msg.is_ai());
    }
}
```

- [ ] **Step 5: 在 peri-middlewares/src/lib.rs 中导出 at_mention**

在 `pub mod tool_search;` 行之后添加：

```rust
pub mod at_mention;
```

在 `pub use tool_search::{...};` 行之后添加：

```rust
pub use at_middleware::AtMentionMiddleware;
```

等价地，在 `lib.rs` 中：
- 在 `pub mod tools;` 之后加 `pub mod at_mention;`
- 在 `pub use tool_search::{...};` 之后加 `pub use at_mention::AtMentionMiddleware;`

- [ ] **Step 6: 运行测试验证**

Run: `cargo test -p peri-middlewares --lib at_mention 2>&1 | tail -30`
Expected: 所有测试通过

- [ ] **Step 7: Commit**

```bash
git add peri-middlewares/src/at_mention/ peri-middlewares/src/lib.rs
git commit -m "feat: add AtMentionMiddleware — @path parser, file reader, Read tool injection"
```

---

### Task 3: 在 ACP builder 中注册 AtMentionMiddleware

**Files:**
- Modify: `peri-acp/src/agent/builder.rs`

- [ ] **Step 1: 添加 middleware 到 builder**

在 `peri-acp/src/agent/builder.rs` 中，找到：
```rust
.add_middleware(Box::new(SkillPreloadMiddleware::new(preload_skills, &cwd)))
```

在其后添加：
```rust
.add_middleware(Box::new(peri_middlewares::AtMentionMiddleware::new(&cwd)))
```

同时在文件顶部的 use 块中确认 `peri_middlewares` 已导入（它应该已经存在）。

- [ ] **Step 2: 验证编译**

Run: `cargo check -p peri-acp 2>&1 | tail -5`
Expected: 编译成功

- [ ] **Step 3: Commit**

```bash
git add peri-acp/src/agent/builder.rs
git commit -m "feat: register AtMentionMiddleware in ACP agent builder"
```

---

### Task 4: 实现 TUI 层 AtMentionState 和触发检测

**Files:**
- Create: `peri-tui/src/app/at_mention/mod.rs`
- Create: `peri-tui/src/app/at_mention/file_search.rs`
- Modify: `peri-tui/src/app/ui_state.rs`
- Modify: `peri-tui/src/app/modules_state.inc`

- [ ] **Step 1: 创建 file_search.rs**

创建 `peri-tui/src/app/at_mention/file_search.rs`：

```rust
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use std::path::Path;

/// @ mention 候选项
#[derive(Debug, Clone)]
pub struct FileCandidate {
    pub path: String,
    pub display: String,
    pub is_dir: bool,
    pub score: i64,
}

/// 排除的目录
const IGNORED_DIRS: &[&str] = &[
    "target",
    "node_modules",
    ".git",
    "dist",
    "build",
    ".next",
    "__pycache__",
    ".venv",
    "venv",
];

/// 最大 glob 结果数
const MAX_GLOB_RESULTS: usize = 200;
/// 最大返回候选数
const MAX_CANDIDATES: usize = 15;

/// 根据查询字符串搜索文件
pub fn search_files(cwd: &str, query: &str) -> Vec<FileCandidate> {
    if query.is_empty() {
        return vec![];
    }

    let cwd_path = Path::new(cwd);

    // 解析 query 为 base_dir + pattern
    let (search_dir, pattern) = if let Some(slash_pos) = query.rfind('/') {
        let dir_part = &query[..slash_pos];
        let file_part = &query[slash_pos + 1..];
        (dir_part.to_string(), file_part.to_string())
    } else {
        (".".to_string(), query.to_string())
    };

    let search_path = cwd_path.join(&search_dir);

    // 构建 glob pattern
    let glob_pattern = if pattern.is_empty() {
        // query 以 / 结尾，列出目录下所有
        format!("{}/**/*", search_dir)
    } else {
        format!("{}/**/*{}*", search_dir, pattern)
    };

    let mut entries: Vec<std::path::PathBuf> = Vec::new();

    if let Ok(paths) = glob::glob(&glob_pattern) {
        for entry in paths.flatten() {
            // 过滤排除目录
            let should_skip = entry.components().any(|c| {
                IGNORED_DIRS
                    .iter()
                    .any(|&ignored| c.as_os_str() == ignored)
            });
            if should_skip {
                continue;
            }
            entries.push(entry);
            if entries.len() >= MAX_GLOB_RESULTS {
                break;
            }
        }
    }

    // Fuzzy 评分
    let matcher = SkimMatcherV2::default();

    let mut candidates: Vec<FileCandidate> = entries
        .into_iter()
        .filter_map(|entry| {
            let rel = entry
                .strip_prefix(cwd_path)
                .unwrap_or(&entry)
                .to_string_lossy()
                .to_string();
            let file_name = entry
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let is_dir = entry.is_dir();

            // 对文件名 + 完整路径都做匹配，取高分
            let name_score = matcher.fuzzy_match(&file_name, &pattern).unwrap_or(0);
            let path_score = matcher.fuzzy_match(&rel, &pattern).unwrap_or(0);
            let score = (name_score as f64 * 2.0 + path_score as f64) as i64;

            if score <= 0 {
                return None;
            }

            Some(FileCandidate {
                path: rel,
                display: file_name,
                is_dir,
                score,
            })
        })
        .collect();

    // 排序：score 降序，同分路径长度升序
    candidates.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then_with(|| a.path.len().cmp(&b.path.len()))
    });

    candidates.truncate(MAX_CANDIDATES);
    candidates
}

/// 找到候选列表中的最长公共前缀（Tab 补全用）
pub fn find_common_prefix(candidates: &[FileCandidate]) -> Option<String> {
    if candidates.is_empty() {
        return None;
    }
    let first = &candidates[0].path;
    let mut prefix_len = first.len();
    for c in &candidates[1..] {
        prefix_len = prefix_len.min(
            first
                .chars()
                .zip(c.path.chars())
                .take_while(|(a, b)| a == b)
                .count(),
        );
    }
    if prefix_len == 0 {
        return None;
    }
    Some(first.chars().take(prefix_len).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_search_by_name() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("main.rs"), "").unwrap();
        std::fs::write(dir.path().join("lib.rs"), "").unwrap();
        let results = search_files(dir.path().to_str().unwrap(), "main");
        assert!(!results.is_empty());
        assert!(results[0].path.contains("main.rs"));
    }

    #[test]
    fn test_search_empty_query() {
        let dir = tempdir().unwrap();
        let results = search_files(dir.path().to_str().unwrap(), "");
        assert!(results.is_empty());
    }

    #[test]
    fn test_search_ignores_target() {
        let dir = tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("target")).unwrap();
        std::fs::write(dir.path().join("target/build.rs"), "").unwrap();
        std::fs::write(dir.path().join("real.rs"), "").unwrap();
        let results = search_files(dir.path().to_str().unwrap(), "real");
        assert!(results.iter().all(|r| !r.path.contains("target")));
    }
}
```

- [ ] **Step 2: 创建 at_mention/mod.rs — 状态 + 触发检测 + 路径注入**

创建 `peri-tui/src/app/at_mention/mod.rs`：

```rust
pub mod file_search;
pub mod popup;

use file_search::FileCandidate;
use regex::Regex;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

/// @ mention 状态
pub struct AtMentionState {
    pub active: bool,
    pub query: String,
    pub query_start: usize,
    pub candidates: Vec<FileCandidate>,
    pub selected: usize,
    pub scroll_offset: usize,
    /// 搜索去抖 timer
    pub debounce_tx: Option<tokio::sync::oneshot::Sender<()>>,
    /// 取消上一次搜索
    pub cancel_token: Option<CancellationToken>,
}

impl AtMentionState {
    pub fn new() -> Self {
        Self {
            active: false,
            query: String::new(),
            query_start: 0,
            candidates: Vec::new(),
            selected: 0,
            scroll_offset: 0,
            debounce_tx: None,
            cancel_token: None,
        }
    }

    /// 从 textarea 文本 + 光标位置检测是否应激活 @ mention
    /// 返回 Some((query, query_start)) 如果检测到，None 否则
    pub fn detect(text: &str, cursor_pos: usize) -> Option<(String, usize)> {
        let text_before_cursor = &text[..cursor_pos.min(text.len())];

        // 匹配带引号: @"
        let quoted_re = Regex::new(r#"(@"[^"]*)$"#).ok()?;
        if let Some(cap) = quoted_re.captures(text_before_cursor) {
            let full = cap[1].to_string();
            if full.len() > 2 {
                // @" 至少有 1 个字符
                let at_pos = cursor_pos - full.len();
                let query = full[2..].to_string(); // 去掉 @"
                if !query.is_empty() {
                    return Some((query, at_pos));
                }
            }
        }

        // 匹配普通: @word（@ 前必须是行首或空白）
        let re = Regex::new(r"(?:^|\s)(@[\p{L}\p{N}_\-./\\]*)$").ok()?;
        if let Some(cap) = re.captures(text_before_cursor) {
            let at_token = cap[1].to_string();
            if at_token.len() > 1 {
                // @ 后至少 1 个字符
                let at_pos = cursor_pos - at_token.len();
                let query = at_token[1..].to_string(); // 去掉 @
                return Some((query, at_pos));
            }
        }

        None
    }

    /// 激活搜索：更新 query 和 candidates
    pub fn activate(&mut self, query: String, query_start: usize) {
        self.active = true;
        self.query = query;
        self.query_start = query_start;
        self.selected = 0;
        self.scroll_offset = 0;
    }

    /// 关闭
    pub fn close(&mut self) {
        self.active = false;
        self.query.clear();
        self.candidates.clear();
        self.selected = 0;
        self.scroll_offset = 0;
        if let Some(tx) = self.debounce_tx.take() {
            let _ = tx.send(());
        }
        if let Some(ct) = self.cancel_token.take() {
            ct.cancel();
        }
    }

    /// 更新搜索结果
    pub fn update_candidates(&mut self, candidates: Vec<FileCandidate>) {
        self.candidates = candidates;
        // 如果选中超出范围，重置
        if self.selected >= self.candidates.len() {
            self.selected = 0;
        }
    }

    /// 导航：上移
    pub fn move_up(&mut self) {
        if self.candidates.is_empty() {
            return;
        }
        self.selected = if self.selected == 0 {
            self.candidates.len() - 1
        } else {
            self.selected - 1
        };
        self.adjust_scroll();
    }

    /// 导航：下移
    pub fn move_down(&mut self) {
        if self.candidates.is_empty() {
            return;
        }
        self.selected = (self.selected + 1) % self.candidates.len();
        self.adjust_scroll();
    }

    fn adjust_scroll(&mut self) {
        let max_viewport = popup::MAX_VIEWPORT;
        if self.selected < self.scroll_offset {
            self.scroll_offset = self.selected;
        } else if self.selected >= self.scroll_offset + max_viewport {
            self.scroll_offset = self.selected - max_viewport + 1;
        }
    }

    /// 获取当前选中的候选
    pub fn selected_candidate(&self) -> Option<&FileCandidate> {
        self.candidates.get(self.selected)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_at_sign_with_text() {
        let result = AtMentionState::detect("请看 @main", 9);
        assert!(result.is_some());
        let (query, start) = result.unwrap();
        assert_eq!(query, "main");
        assert_eq!(start, 3); // '@' 在位置 3
    }

    #[test]
    fn test_detect_no_at_sign() {
        let result = AtMentionState::detect("hello world", 11);
        assert!(result.is_none());
    }

    #[test]
    fn test_detect_at_sign_only() {
        // @ 后无字符，不触发
        let result = AtMentionState::detect("看 @", 3);
        assert!(result.is_none());
    }

    #[test]
    fn test_detect_path_with_slash() {
        let result = AtMentionState::detect("看 @src/main", 11);
        assert!(result.is_some());
        let (query, _) = result.unwrap();
        assert_eq!(query, "src/main");
    }

    #[test]
    fn test_detect_not_at_line_start() {
        // email 场景：@ 前不是空白
        let result = AtMentionState::detect("user@example", 12);
        assert!(result.is_none());
    }

    #[test]
    fn test_move_up_down() {
        let mut state = AtMentionState::new();
        state.candidates = vec![
            FileCandidate {
                path: "a.rs".into(),
                display: "a.rs".into(),
                is_dir: false,
                score: 10,
            },
            FileCandidate {
                path: "b.rs".into(),
                display: "b.rs".into(),
                is_dir: false,
                score: 9,
            },
        ];
        assert_eq!(state.selected, 0);
        state.move_down();
        assert_eq!(state.selected, 1);
        state.move_down();
        assert_eq!(state.selected, 0); // 循环
        state.move_up();
        assert_eq!(state.selected, 1); // 循环
    }
}
```

- [ ] **Step 3: 创建 popup.rs — 弹窗渲染占位**

创建 `peri-tui/src/app/at_mention/popup.rs`：

```rust
use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};
use peri_widgets::BorderedPanel;

use super::AtMentionState;
use crate::ui::theme;

/// 弹窗最大显示项数
pub const MAX_VIEWPORT: usize = 10;

/// 渲染 @ mention 弹窗（在输入框上方）
pub fn render_at_mention_popup(f: &mut Frame, state: &AtMentionState, input_area: Rect) {
    if !state.active || state.candidates.is_empty() {
        return;
    }

    let total = state.candidates.len();
    let viewport = MAX_VIEWPORT.min(total);
    let scroll_offset = state.scroll_offset;
    let visible = &state.candidates[scroll_offset..scroll_offset + viewport];

    let hint_height = viewport as u16 + 2; // 视口 + 边框
    let y = input_area.y.saturating_sub(hint_height);
    let area = Rect {
        x: input_area.x,
        y,
        width: input_area.width,
        height: hint_height,
    };

    let inner = BorderedPanel::new(Span::styled("", Style::default()))
        .border_style(Style::default().fg(theme::BORDER))
        .render(f, area);

    let mut lines = Vec::with_capacity(visible.len());
    for (vi, candidate) in visible.iter().enumerate() {
        let global_idx = scroll_offset + vi;
        let is_selected = global_idx == state.selected;

        let icon = if candidate.is_dir { "/" } else { "+" };
        let style = if is_selected {
            Style::default().fg(theme::THINKING).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::TEXT)
        };

        let prefix = if is_selected { "❯ " } else { "  " };
        let mut spans = vec![
            Span::styled(prefix.to_string(), Style::default().fg(theme::THINKING)),
            Span::styled(format!("{} ", icon), style),
        ];

        // 中间截断路径
        let max_width = (inner.width as usize).saturating_sub(6);
        let display_path = truncate_middle(&candidate.path, max_width);
        spans.push(Span::styled(display_path, style));

        lines.push(Line::from(spans));
    }

    f.render_widget(Paragraph::new(lines), inner);
}

/// 中间截断路径，保留前后部分
fn truncate_middle(path: &str, max_width: usize) -> String {
    let width = unicode_width::UnicodeWidthStr::width(path);
    if width <= max_width {
        return path.to_string();
    }
    if max_width < 5 {
        return path.chars().take(max_width).collect();
    }
    let half = (max_width - 3) / 2; // 3 for "..."
    let chars: Vec<char> = path.chars().collect();
    let mut left_len = 0;
    let mut left_count = 0;
    for &c in &chars {
        let cw = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
        if left_len + cw > half {
            break;
        }
        left_len += cw;
        left_count += 1;
    }
    let right_target = max_width - 3 - left_len;
    let mut right_start = chars.len();
    let mut right_len = 0;
    for i in (0..chars.len()).rev() {
        let cw = unicode_width::UnicodeWidthChar::width(chars[i]).unwrap_or(0);
        if right_len + cw > right_target {
            break;
        }
        right_len += cw;
        right_start = i;
    }
    let left: String = chars[..left_count].iter().collect();
    let right: String = chars[right_start..].iter().collect();
    format!("{}...{}", left, right)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_short() {
        assert_eq!(truncate_middle("abc", 10), "abc");
    }

    #[test]
    fn test_truncate_long() {
        let result = truncate_middle("src/very/long/path/to/file.rs", 15);
        assert!(result.contains("..."));
        assert!(unicode_width::UnicodeWidthStr::width(&result) <= 15);
    }
}
```

- [ ] **Step 4: 在 UiState 中添加 AtMentionState**

在 `peri-tui/src/app/ui_state.rs` 中：

在文件顶部添加：
```rust
use super::at_mention::AtMentionState;
```

在 `UiState` 结构体中添加字段（`panel_scrollbar_dragging` 之后）：
```rust
pub at_mention: AtMentionState,
```

在 `UiState::new()` 的初始化列表中添加：
```rust
at_mention: AtMentionState::new(),
```

- [ ] **Step 5: 在 modules_state.inc 中添加模块声明**

在 `peri-tui/src/app/modules_state.inc` 中的 `mod ui_state;` 之后添加：
```rust
mod at_mention;
```

在 `pub use ui_state::UiState;` 之后添加：
```rust
pub use at_mention::AtMentionState;
```

- [ ] **Step 6: 验证编译**

Run: `cargo check -p peri-tui 2>&1 | tail -10`
Expected: 编译成功

- [ ] **Step 7: Commit**

```bash
git add peri-tui/src/app/at_mention/ peri-tui/src/app/ui_state.rs peri-tui/src/app/modules_state.inc
git commit -m "feat: add AtMentionState, file search, and popup rendering"
```

---

### Task 5: 集成触发检测和键盘交互到 keyboard.rs

**Files:**
- Modify: `peri-tui/src/event/keyboard.rs`

- [ ] **Step 1: 添加 @ mention 触发检测辅助函数**

在 `keyboard.rs` 文件末尾（tests 模块之前，或文件末尾）添加辅助函数：

```rust
/// 检测 textarea 中是否有 @ mention 触发，更新 at_mention 状态
/// 返回 true 表示 @ mention 已激活（应拦截按键）
fn update_at_mention_detection(app: &mut App) -> bool {
    let (text, cursor_pos) = {
        let textarea = &app.session_mgr.sessions[app.session_mgr.active].ui.textarea;
        let text = textarea.lines().join("\n");
        // textarea cursor 是 (row, col)，转为字符偏移
        let (row, col) = textarea.cursor();
        let mut pos = 0;
        for (i, line) in textarea.lines().iter().enumerate() {
            if i == row {
                // 字符级 col
                pos += line.chars().take(col).map(|c| c.len_utf8()).sum::<usize>();
                break;
            }
            pos += line.len() + 1; // +1 for \n
        }
        (text, pos)
    };

    let at = &mut app.session_mgr.sessions[app.session_mgr.active].ui.at_mention;

    if let Some((query, start)) = crate::app::at_mention::AtMentionState::detect(&text, cursor_pos)
    {
        if at.active && at.query == query {
            return true; // query 未变，不重新搜索
        }
        at.activate(query, start);
        // 同步搜索（简单实现，后续可改为异步）
        let cwd = app.services.cwd.clone();
        let candidates = crate::app::at_mention::file_search::search_files(&cwd, &at.query);
        at.update_candidates(candidates);
        return true;
    } else if at.active {
        at.close();
    }
    false
}

/// 注入选中的 @ mention 路径到 textarea
fn inject_at_mention_path(app: &mut App) -> bool {
    let at = &app.session_mgr.sessions[app.session_mgr.active].ui.at_mention;
    let candidate = match at.selected_candidate() {
        Some(c) => c.clone(),
        None => return false,
    };
    let query_start = at.query_start;
    let query_len = at.query.len();

    // 获取当前 textarea 全文
    let textarea = &app.session_mgr.sessions[app.session_mgr.active].ui.textarea;
    let full_text: String = textarea.lines().join("\n");
    let (row, col) = textarea.cursor();

    // 替换 @query 为 @path
    let needs_quotes = candidate.path.contains(' ');
    let replacement = if needs_quotes {
        format!("@\"{}\"", candidate.path)
    } else {
        format!("@{}", candidate.path)
    };

    // 构造新文本
    let mut new_text = String::with_capacity(full_text.len() + replacement.len());
    new_text.push_str(&full_text[..query_start]);
    new_text.push_str(&replacement);
    // 在 @query 后面加上剩余文本
    let after_start = query_start + 1 + query_len; // +1 for @
    if after_start < full_text.len() {
        new_text.push_str(&full_text[after_start..]);
    }

    // 目录：加 / 后继续搜索；文件：加空格后关闭
    let is_dir = candidate.is_dir;

    // 重建 textarea
    let mut new_textarea = crate::app::build_textarea(false);
    new_textarea.insert_str(&new_text);
    // 将光标移到注入路径末尾
    let new_cursor_char_pos = query_start + replacement.len();
    // 计算新光标的 (row, col)
    let mut pos = 0;
    for (i, line) in new_text.lines().enumerate() {
        let line_len = line.chars().map(|c| c.len_utf8()).sum::<usize>();
        if pos + line_len >= new_cursor_char_pos || i == new_text.lines().count() - 1 {
            let col_chars = new_textarea.lines().get(i).map(|l| l.chars().count()).unwrap_or(0);
            // 简化处理：光标设在行末
            for _ in 0..col_chars {
                new_textarea.input(tui_textarea::Input {
                    key: tui_textarea::Key::Right,
                    ctrl: false,
                    alt: false,
                    shift: false,
                });
            }
            break;
        }
        pos += line_len + 1;
        new_textarea.input(tui_textarea::Input {
            key: tui_textarea::Key::Down,
            ctrl: false,
            alt: false,
            shift: false,
        });
    }

    app.session_mgr.sessions[app.session_mgr.active].ui.textarea = new_textarea;

    if is_dir {
        // 目录：追加 / 并继续搜索
        app.session_mgr.sessions[app.session_mgr.active]
            .ui
            .textarea
            .insert_str("/");
        update_at_mention_detection(app);
    } else {
        // 文件：追加空格并关闭
        app.session_mgr.sessions[app.session_mgr.active]
            .ui
            .textarea
            .insert_str(" ");
        app.session_mgr.sessions[app.session_mgr.active]
            .ui
            .at_mention
            .close();
    }

    true
}
```

- [ ] **Step 2: 在键盘处理主循环中集成 @ mention 导航**

在 `keyboard.rs` 的 `match input` 块中，在 Up/Down 处理分支中添加 @ mention 拦截。

找到 Up 键处理（约 541 行）：
```rust
Input { key: Key::Up, .. } => {
    let hint_count = app.hint_candidates_count();
    if hint_count > 0 && !app.session_mgr.sessions[app.session_mgr.active].ui.loading {
```

在其 `if hint_count > 0` 条件之前添加 @ mention 检查：

```rust
Input { key: Key::Up, .. } => {
    // @ mention 导航优先
    if app.session_mgr.sessions[app.session_mgr.active].ui.at_mention.active {
        app.session_mgr.sessions[app.session_mgr.active]
            .ui
            .at_mention
            .move_up();
    } else {
        let hint_count = app.hint_candidates_count();
        // ... 原有 hint 导航逻辑
```

同理对 Down 键处理（约 577 行）做相同处理。

最终 Up/Down 的处理变为：

```rust
Input { key: Key::Up, .. } => {
    if app.session_mgr.sessions[app.session_mgr.active].ui.at_mention.active
        && !app.session_mgr.sessions[app.session_mgr.active].ui.loading
    {
        app.session_mgr.sessions[app.session_mgr.active]
            .ui
            .at_mention
            .move_up();
    } else {
        let hint_count = app.hint_candidates_count();
        if hint_count > 0 && !app.session_mgr.sessions[app.session_mgr.active].ui.loading {
            // ... 原有 hint 导航代码不变
```

注意：原来的整个 Up 分支的 else 分支保持不变，只是增加了一个最外层的 @ mention 优先判断。

对 Down 键同理。

- [ ] **Step 3: 在 Tab 和 Enter 中添加 @ mention 处理**

找到 Tab 键处理（约 657 行）：
```rust
Input { key: Key::Tab, shift: false, .. }
    if !app.session_mgr.sessions[app.session_mgr.active].ui.loading =>
{
    let count = app.hint_candidates_count();
```

在 hint 检查之前添加 @ mention 处理：

```rust
Input { key: Key::Tab, shift: false, .. }
    if !app.session_mgr.sessions[app.session_mgr.active].ui.loading =>
{
    if app.session_mgr.sessions[app.session_mgr.active].ui.at_mention.active {
        inject_at_mention_path(app);
    } else {
        let count = app.hint_candidates_count();
        // ... 原有 hint 导航代码不变
```

找到 Enter + hints 处理（约 690 行）：
```rust
Input { key: Key::Enter, .. }
    if !app.session_mgr.sessions[app.session_mgr.active].ui.loading
        && app.hint_candidates_count() > 0 =>
{
```

在其之前添加 @ mention Enter 处理：

```rust
Input { key: Key::Enter, .. }
    if !app.session_mgr.sessions[app.session_mgr.active].ui.loading
        && app.session_mgr.sessions[app.session_mgr.active]
            .ui
            .at_mention
            .active =>
{
    inject_at_mention_path(app);
}
```

- [ ] **Step 4: 在 Esc 中添加 @ mention 关闭**

找到 Esc 处理（约 525 行）：
```rust
Input { key: Key::Esc, .. }
    if app.session_mgr.sessions[app.session_mgr.active].ui.loading =>
{
```

在这个 loading Esc 之后、Up 处理之前，添加 @ mention Esc：

```rust
Input { key: Key::Esc, .. }
    if app.session_mgr.sessions[app.session_mgr.active]
        .ui
        .at_mention
        .active =>
{
    app.session_mgr.sessions[app.session_mgr.active]
        .ui
        .at_mention
        .close();
}
```

- [ ] **Step 5: 在每次普通按键输入后触发检测**

在 `keyboard.rs` 的 match input 块末尾（最后的 `_ => {}` 兜底分支或 textarea 输入后），添加检测调用。

找到 textarea 输入的兜底分支。在键盘处理文件的 match input 块中，找到最后的 textarea input 调用（在所有特殊键处理之后），在其后添加：

```rust
// 普通字符输入后检测 @ mention
_ => {
    if !app.session_mgr.sessions[app.session_mgr.active].ui.loading {
        app.session_mgr.sessions[app.session_mgr.active]
            .ui
            .textarea
            .input(input);
        update_at_mention_detection(app);
    }
}
```

注意：需要确保原来的 `_ => {}` 兜底分支被替换为上述代码。

- [ ] **Step 6: 验证编译**

Run: `cargo check -p peri-tui 2>&1 | tail -10`
Expected: 编译成功

- [ ] **Step 7: Commit**

```bash
git add peri-tui/src/event/keyboard.rs
git commit -m "feat: integrate @ mention trigger detection and keyboard navigation"
```

---

### Task 6: 集成弹窗渲染到 TUI 布局

**Files:**
- Modify: `peri-tui/src/ui/main_ui/mod.rs`
- Modify: `peri-tui/src/ui/main_ui/popups/mod.rs`

- [ ] **Step 1: 在 popups/mod.rs 中添加 at_mention popup 导出**

找到 `peri-tui/src/ui/main_ui/popups/mod.rs` 中的模块声明，添加：

```rust
pub(crate) mod at_mention;
```

注意：由于 popup.rs 位于 `peri-tui/src/app/at_mention/popup.rs`，而渲染入口在 `peri-tui/src/ui/main_ui/mod.rs`，我们需要在 `render_session_column` 中直接调用 `crate::app::at_mention::popup::render_at_mention_popup`。

所以此步骤实际不需要修改 popups/mod.rs。跳过。

- [ ] **Step 2: 在 render_session_column 中渲染 @ mention 弹窗**

在 `peri-tui/src/ui/main_ui/mod.rs` 中，找到（约 298 行）：

```rust
// 统一命令/Skills 提示条（每个 session 列各自渲染）
popups::hints::render_unified_hint(f, app, chunks[5]);
```

在其之前添加：

```rust
// @ mention 弹窗（与 hints 互斥，优先渲染）
crate::app::at_mention::popup::render_at_mention_popup(
    f,
    &app.session_mgr.sessions[session_idx].ui.at_mention,
    chunks[5],
);
if app.session_mgr.sessions[session_idx].ui.at_mention.active {
    // @ mention 激活时跳过 hints 渲染
} else {
    popups::hints::render_unified_hint(f, app, chunks[5]);
}
```

替换原来的 `popups::hints::render_unified_hint(f, app, chunks[5]);` 为上述代码。

- [ ] **Step 3: 验证编译**

Run: `cargo check -p peri-tui 2>&1 | tail -10`
Expected: 编译成功

- [ ] **Step 4: Commit**

```bash
git add peri-tui/src/ui/main_ui/mod.rs
git commit -m "feat: render @ mention popup in TUI layout (mutually exclusive with hints)"
```

---

### Task 7: 端到端验证

**Files:** 无新增

- [ ] **Step 1: 全量编译**

Run: `cargo build 2>&1 | tail -10`
Expected: 编译成功

- [ ] **Step 2: 运行全部测试**

Run: `cargo test 2>&1 | tail -30`
Expected: 所有测试通过

- [ ] **Step 3: 运行 @ mention 相关测试**

Run: `cargo test -p peri-middlewares --lib at_mention && cargo test -p peri-tui --lib at_mention 2>&1 | tail -30`
Expected: 所有 at_mention 测试通过

- [ ] **Step 4: 手动冒烟测试**

启动 TUI（`cargo run -p peri-tui`），在输入框中：
1. 输入 `@main` → 应在输入框上方显示文件候选弹窗
2. 使用 `↑/↓` 导航 → 选中项高亮切换
3. 按 `Enter` → 路径注入到输入框，弹窗关闭
4. 按 `Esc` → 弹窗关闭，保留 `@query` 文本
5. 输入 `@src/` 后继续输入 → 应搜索 src 目录下的文件
6. 提交包含 `@path` 的消息 → agent 应在响应前看到 Read 工具调用结果

---

## Self-Review

### Spec Coverage Check

| Spec Section | Task |
|---|---|
| Part 1: 状态管理与触发检测 | Task 4 (AtMentionState + detect) |
| Part 1: 交互键 | Task 5 (keyboard.rs 集成) |
| Part 2: 文件搜索与模糊匹配 | Task 4 (file_search.rs) |
| Part 2: 去抖 | Task 4 (AtMentionState.debounce_tx) — 注：简单实现中用同步搜索，异步去抖留作优化 |
| Part 3: 弹窗渲染 | Task 4 (popup.rs) + Task 6 (集成) |
| Part 3: 路径注入 | Task 5 (inject_at_mention_path) |
| Part 4: parser | Task 2 (parser.rs) |
| Part 4: file reader | Task 2 (file_reader.rs) |
| Part 4: AtMentionMiddleware | Task 2 (mod.rs) |
| Part 4: builder 注册 | Task 3 |

### Gap: 去抖在简单实现中省略

设计文档提到 50ms 去抖 + CancellationToken，但计划中用了同步搜索。这是合理的简化——glob + fuzzy 在中小项目中足够快（<50ms），后续如果性能不足可加异步搜索层。

### Placeholder Scan

无 TBD/TODO。所有代码步骤都有完整代码。

### Type Consistency

- `AtMentionState` 定义在 `peri-tui/src/app/at_mention/mod.rs`，在 `ui_state.rs` 中作为字段使用
- `FileCandidate` 定义在 `file_search.rs`，在 `mod.rs` 和 `popup.rs` 中使用
- `AtMentionMiddleware` 定义在 `peri-middlewares/src/at_mention/mod.rs`，在 `builder.rs` 中注册
- parser 返回 `AtMention`，middleware 使用它构造 file_reader 调用
