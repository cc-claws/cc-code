use super::*;
use cc_agent::agent::state::AgentState;
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
