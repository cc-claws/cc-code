use base64::{engine::general_purpose::STANDARD, Engine};
use peri_agent::prelude::*;
use peri_middlewares::{
    middleware::FilesystemMiddleware,
    tools::{ArcToolWrapper, BoxToolWrapper, ReadFileTool},
};
use std::sync::Arc;

// 1×1 PNG，验证包装和执行链路保留完整图片字节。
const PNG_BASE64: &str =
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+aD1sAAAAASUVORK5CYII=";

fn make_shared_read(cwd: &str) -> ArcToolWrapper {
    ArcToolWrapper(Arc::new(BoxToolWrapper(Box::new(ReadFileTool::new(cwd)))))
}

async fn assert_read_image_through_agent(shared: bool) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("图片.png"),
        STANDARD.decode(PNG_BASE64).unwrap(),
    )
    .unwrap();
    let cwd = dir.path().to_str().unwrap();
    let llm =
        MockLLM::tool_then_answer("Read", serde_json::json!({"file_path": "图片.png"}), "done");
    let mut agent = ReActAgent::new(llm);
    if shared {
        // 对齐子 Agent：父工具 Box -> Arc，再由子 Agent 包回 Box 注册。
        agent = agent.register_tool(Box::new(make_shared_read(cwd)));
    } else {
        // 对齐主 Agent：collect_tools -> box_to_arc -> tool_dispatch。
        agent = agent.add_middleware(Box::new(FilesystemMiddleware::new()));
    }
    let mut state = AgentState::new(cwd);
    let output = agent
        .execute(AgentInput::text("读取图片"), &mut state, None)
        .await
        .expect("读取图片的 Agent 执行应成功");
    assert_eq!(output.tool_calls.len(), 1, "应执行一次 Read");
    let result = &output.tool_calls[0].1;
    assert!(!result.is_error, "读取图片不应报错");
    assert!(
        result.output.contains("image/png"),
        "展示摘要应包含图片类型"
    );
    assert!(
        !result.output.contains("BINARY FILE DETECTED"),
        "图片不应降级为二进制提示"
    );
    let expected =
        MessageContent::blocks(vec![ContentBlock::image_base64("image/png", PNG_BASE64)]);
    assert_eq!(
        result.content.as_ref(),
        Some(&expected),
        "工具结果应保留图片数据"
    );
    let tool_messages: Vec<_> = state
        .messages()
        .iter()
        .filter_map(|message| match message {
            BaseMessage::Tool {
                content, is_error, ..
            } => Some((content, is_error)),
            _ => None,
        })
        .collect();
    assert_eq!(tool_messages.len(), 1, "state 应写入一次工具结果");
    assert_eq!(
        tool_messages[0].0, &expected,
        "后续模型上下文应保留完整图片"
    );
    assert!(!tool_messages[0].1, "state 中图片结果不应标记为错误");
}

#[tokio::test]
async fn test_read_image_through_middleware_wrapper() {
    assert_read_image_through_agent(false).await;
}

#[tokio::test]
async fn test_read_image_through_subagent_wrappers() {
    assert_read_image_through_agent(true).await;
}

#[tokio::test]
async fn test_read_subagent_wrappers_preserve_text() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("hello.txt"), "文本内容").unwrap();
    let tool = make_shared_read(dir.path().to_str().unwrap());
    let input = serde_json::json!({"file_path": "hello.txt"});
    let legacy = tool.invoke(input.clone()).await.expect("原文本调用应成功");
    let result = tool.invoke_content(input).await.expect("结构化调用应成功");
    assert!(result.output.contains("文本内容"), "文本读取内容应保持不变");
    assert_eq!(result.output, legacy, "文本结果应与原 invoke 一致");
    assert!(result.content.is_none(), "文本结果无需结构化内容");
}

#[tokio::test]
async fn test_read_subagent_wrappers_preserve_error() {
    let dir = tempfile::tempdir().unwrap();
    let tool = make_shared_read(dir.path().to_str().unwrap());
    let result = tool.invoke_content(serde_json::json!({})).await;
    assert!(result.is_err(), "缺少必填参数必须透传为错误");
    assert!(
        result
            .expect_err("应返回参数错误")
            .to_string()
            .contains("file_path"),
        "应保留原始错误信息"
    );
}
