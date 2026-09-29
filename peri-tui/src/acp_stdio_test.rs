//! `acp_stdio` 辅助函数单测 —— prompt 内容转换与历史回放映射。

use super::*;

/// 取出 peri 文本块内容，非文本块直接 panic。
fn peri_block_text(block: &PeriContentBlock) -> String {
    match block {
        PeriContentBlock::Text { text } => text.to_string(),
        other => panic!("应为文本块，实际 {other:?}"),
    }
}

/// 取出 ACP 文本块内容，非文本块直接 panic。
fn acp_chunk_text(update: &SessionUpdate) -> String {
    let chunk = match update {
        SessionUpdate::UserMessageChunk(chunk) | SessionUpdate::AgentMessageChunk(chunk) => chunk,
        other => panic!("应为消息块，实际 {other:?}"),
    };
    match &chunk.content {
        AcpBlock::Text(t) => t.text.clone(),
        other => panic!("应为文本块，实际 {other:?}"),
    }
}

// ── prompt_content_from_acp ─────────────────────────────────────────────────

#[test]
fn test_prompt_content_from_acp_preserves_text_and_image() {
    let blocks = vec![
        AcpBlock::Text(TextContent::new("看这张图")),
        AcpBlock::Image(agent_client_protocol::schema::ImageContent::new(
            "aGk=",
            "image/png",
        )),
    ];

    let content = prompt_content_from_acp(&blocks).unwrap();

    let parsed = content.content_blocks();
    assert_eq!(parsed.len(), 2);
    assert_eq!(peri_block_text(&parsed[0]), "看这张图");
    assert!(matches!(parsed[1], PeriContentBlock::Image { .. }));
}

#[test]
fn test_prompt_content_from_acp_keeps_resource_link_as_text() {
    let link =
        agent_client_protocol::schema::ResourceLink::new("main.rs", "file:///proj/src/main.rs");

    let content = prompt_content_from_acp(&[AcpBlock::ResourceLink(link)]).unwrap();

    let parsed = content.content_blocks();
    assert_eq!(parsed.len(), 1, "resource link 不应被丢弃");
    let text = peri_block_text(&parsed[0]);
    assert!(text.contains("main.rs"), "资源名应保留：{text}");
    assert!(
        text.contains("file:///proj/src/main.rs"),
        "URI 应保留：{text}"
    );
}

#[test]
fn test_prompt_content_from_acp_keeps_embedded_text_resource() {
    use agent_client_protocol::schema::{EmbeddedResource, EmbeddedResourceResource};
    let res = EmbeddedResource::new(EmbeddedResourceResource::TextResourceContents(
        agent_client_protocol::schema::TextResourceContents::new("fn main() {}", "file:///a.rs"),
    ));

    let content = prompt_content_from_acp(&[AcpBlock::Resource(res)]).unwrap();

    let parsed = content.content_blocks();
    assert_eq!(parsed.len(), 1);
    let text = peri_block_text(&parsed[0]);
    assert!(text.contains("fn main() {}"), "内嵌文本应保留：{text}");
}

#[test]
fn test_prompt_content_from_acp_rejects_audio_block() {
    let blocks = vec![AcpBlock::Audio(
        agent_client_protocol::schema::AudioContent::new("YXVkaW8=", "audio/wav"),
    )];

    let err = prompt_content_from_acp(&blocks).unwrap_err();

    assert!(err.contains("audio"), "错误信息应说明 audio 不支持：{err}");
}

#[test]
fn test_prompt_content_from_acp_returns_empty_text_for_empty_prompt() {
    let content = prompt_content_from_acp(&[]).unwrap();

    assert_eq!(content.text_content(), "");
}

// ── history_message_updates ─────────────────────────────────────────────────

#[test]
fn test_history_message_updates_replays_human_text_as_user_chunk() {
    let updates = history_message_updates(&BaseMessage::human("你好"));

    assert_eq!(updates.len(), 1);
    assert!(matches!(updates[0], SessionUpdate::UserMessageChunk(_)));
    assert_eq!(acp_chunk_text(&updates[0]), "你好");
}

#[test]
fn test_history_message_updates_replays_ai_reasoning_then_text() {
    let msg = BaseMessage::ai_from_blocks(vec![
        PeriContentBlock::Reasoning {
            text: "先想一想".to_string(),
            signature: None,
            duration_ms: None,
        },
        PeriContentBlock::Text {
            text: "答案是 42".into(),
        },
    ]);

    let updates = history_message_updates(&msg);

    assert_eq!(updates.len(), 2);
    assert!(matches!(updates[0], SessionUpdate::AgentThoughtChunk(_)));
    assert!(matches!(updates[1], SessionUpdate::AgentMessageChunk(_)));
    assert_eq!(acp_chunk_text(&updates[1]), "答案是 42");
}

#[test]
fn test_history_message_updates_dedupes_tool_use_against_tool_calls_field() {
    // ai_from_blocks 会把 ToolUse block 同步提取到 tool_calls 字段
    let msg = BaseMessage::ai_from_blocks(vec![
        PeriContentBlock::Text {
            text: "调用工具".into(),
        },
        PeriContentBlock::ToolUse {
            id: "call_1".to_string(),
            name: "Read".to_string(),
            input: serde_json::json!({ "file_path": "a.rs" }),
        },
    ]);
    assert_eq!(msg.tool_calls().len(), 1, "前置条件：tool_calls 已同步提取");

    let updates = history_message_updates(&msg);

    let tool_calls = updates
        .iter()
        .filter(|u| matches!(u, SessionUpdate::ToolCall(_)))
        .count();
    assert_eq!(tool_calls, 1, "同一次工具调用不应回放两次");
}

#[test]
fn test_history_message_updates_maps_tool_error_to_failed_update() {
    let updates = history_message_updates(&BaseMessage::tool_error("call_1", "boom"));

    assert_eq!(updates.len(), 1);
    match &updates[0] {
        SessionUpdate::ToolCallUpdate(update) => {
            assert_eq!(update.tool_call_id.to_string(), "call_1");
            assert_eq!(update.fields.status, Some(ToolCallStatus::Failed));
            assert_eq!(
                update.fields.raw_output,
                Some(serde_json::Value::String("boom".to_string()))
            );
        }
        other => panic!("应为 ToolCallUpdate，实际 {other:?}"),
    }
}

#[test]
fn test_history_message_updates_skips_system_message() {
    let updates = history_message_updates(&BaseMessage::system("内部提示词"));

    assert!(updates.is_empty(), "System 消息不应回放");
}
