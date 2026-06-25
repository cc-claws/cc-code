use super::*;
use serde_json::json;

fn test_config() -> CompactConfig {
    CompactConfig::default()
}

fn ai_with_tool(id: &str, name: &str) -> BaseMessage {
    BaseMessage::ai_with_tool_calls(
        MessageContent::text("using tool"),
        vec![ToolCallRequest::new(id, name, json!({}))],
    )
}

fn tool_result(tc_id: &str, text: &str) -> BaseMessage {
    BaseMessage::tool_result(tc_id, text)
}

fn tool_result_with_image(tc_id: &str, text: &str) -> BaseMessage {
    BaseMessage::tool_result(
        tc_id,
        MessageContent::blocks(vec![
            ContentBlock::text(text),
            ContentBlock::image_base64("image/png", "iVBOR...base64data"),
        ]),
    )
}

fn tool_result_with_large_image(tc_id: &str) -> BaseMessage {
    let large_b64 = "A".repeat(100_000);
    BaseMessage::tool_result(
        tc_id,
        MessageContent::blocks(vec![
            ContentBlock::text("output"),
            ContentBlock::image_base64("image/png", &large_b64),
        ]),
    )
}

use crate::messages::{DocumentSource, ToolCallRequest};

// Whitelist tests

#[test]
fn test_whitelist_only_compactable_tools() {
    let long_text = "x".repeat(600);
    let mut msgs = vec![
        ai_with_tool("tc1", "Bash"),
        tool_result("tc1", &long_text),
        BaseMessage::human("q"),
        ai_with_tool("tc2", "AskUserQuestion"),
        tool_result("tc2", &long_text),
    ];
    let mut config = test_config();
    config.micro_compact_stale_steps = 1;
    let cleared = micro_compact_enhanced(&config, &mut msgs, 0);
    assert_eq!(cleared, 1);
    assert_eq!(msgs[1].content(), "[compacted: 600 chars]");
    assert_ne!(msgs[4].content(), "[compacted: 600 chars]");
}

#[test]
fn test_whitelist_custom_list() {
    let long_text = "x".repeat(600);
    let mut msgs = vec![
        ai_with_tool("tc1", "Bash"),
        tool_result("tc1", &long_text),
        ai_with_tool("tc2", "Read"),
        tool_result("tc2", &long_text),
    ];
    let config = CompactConfig {
        micro_compactable_tools: vec!["Read".to_string()],
        micro_compact_stale_steps: 0,
        ..Default::default()
    };
    let cleared = micro_compact_enhanced(&config, &mut msgs, 0);
    assert_eq!(cleared, 1);
    assert_ne!(msgs[1].content(), "[compacted: 600 chars]");
    assert_eq!(msgs[3].content(), "[compacted: 600 chars]");
}

#[test]
fn test_whitelist_unknown_tool_preserved() {
    let long_text = "x".repeat(600);
    let mut msgs = vec![
        ai_with_tool("tc1", "custom_tool"),
        tool_result("tc1", &long_text),
    ];
    let mut config = test_config();
    config.micro_compact_stale_steps = 0;
    let cleared = micro_compact_enhanced(&config, &mut msgs, 0);
    assert_eq!(cleared, 0);
}

// Stale steps tests

#[test]
fn test_stale_steps_keep_recent() {
    let long_text = "x".repeat(600);
    let mut msgs: Vec<BaseMessage> = Vec::new();
    for i in 0..7 {
        let tc_id = format!("tc{}", i);
        msgs.push(ai_with_tool(&tc_id, "Bash"));
        msgs.push(tool_result(&tc_id, &long_text));
    }
    let mut config = test_config();
    config.micro_compact_stale_steps = 5;
    let cleared = micro_compact_enhanced(&config, &mut msgs, 0);
    assert_eq!(cleared, 2);
}

#[test]
fn test_stale_steps_zero_compact_all() {
    let long_text = "x".repeat(600);
    let mut msgs = vec![
        ai_with_tool("tc1", "Bash"),
        tool_result("tc1", &long_text),
        ai_with_tool("tc2", "Bash"),
        tool_result("tc2", &long_text),
        ai_with_tool("tc3", "Bash"),
        tool_result("tc3", &long_text),
    ];
    let mut config = test_config();
    config.micro_compact_stale_steps = 0;
    let cleared = micro_compact_enhanced(&config, &mut msgs, 0);
    assert_eq!(cleared, 3);
}

#[test]
fn test_stale_steps_large_keep_all() {
    let long_text = "x".repeat(600);
    let mut msgs = vec![
        ai_with_tool("tc1", "Bash"),
        tool_result("tc1", &long_text),
        ai_with_tool("tc2", "Bash"),
        tool_result("tc2", &long_text),
        ai_with_tool("tc3", "Bash"),
        tool_result("tc3", &long_text),
    ];
    let mut config = test_config();
    config.micro_compact_stale_steps = 100;
    let cleared = micro_compact_enhanced(&config, &mut msgs, 0);
    assert_eq!(cleared, 0);
}

// Image/document tests

#[test]
fn test_image_replaced_with_placeholder() {
    let mut msgs = vec![
        ai_with_tool("tc1", "Bash"),
        tool_result_with_image("tc1", "text"),
    ];
    let mut config = test_config();
    config.micro_compact_stale_steps = 0;
    let cleared = micro_compact_enhanced(&config, &mut msgs, 0);
    assert_eq!(cleared, 1);
    let content = msgs[1].content();
    assert!(content.contains("[image]"), "got: {}", content);
}

#[test]
fn test_large_image_compacted_with_token_info() {
    let mut msgs = vec![
        ai_with_tool("tc1", "Bash"),
        tool_result_with_large_image("tc1"),
    ];
    let mut config = test_config();
    config.micro_compact_stale_steps = 0;
    let cleared = micro_compact_enhanced(&config, &mut msgs, 0);
    assert_eq!(cleared, 1);
    let content = msgs[1].content();
    assert!(content.contains("compacted: image"), "got: {}", content);
    // 100_000 base64 chars * 3/4 (decode) / 4 (token est) = 18750 tokens
    assert!(content.contains("18750 tokens"), "got: {}", content);
}

#[test]
fn test_image_in_recent_step_preserved() {
    let mut msgs = vec![
        ai_with_tool("tc1", "Bash"),
        tool_result_with_image("tc1", "text"),
    ];
    let mut config = test_config();
    config.micro_compact_stale_steps = 5;
    let cleared = micro_compact_enhanced(&config, &mut msgs, 0);
    assert_eq!(cleared, 0);
}

// Invariant preservation tests

#[test]
fn test_invariant_preserves_tool_pair() {
    let long_text = "x".repeat(600);
    let mut msgs = vec![
        BaseMessage::human("q"),
        BaseMessage::ai_with_tool_calls(
            MessageContent::text("using tools"),
            vec![
                ToolCallRequest::new("tc1", "Bash", json!({})),
                ToolCallRequest::new("tc2", "Bash", json!({})),
            ],
        ),
        tool_result("tc1", &long_text),
        tool_result("tc2", &long_text),
        ai_plain("done"),
    ];
    let mut config = test_config();
    config.micro_compact_stale_steps = 1;
    let cleared = micro_compact_enhanced(&config, &mut msgs, 0);
    assert_eq!(cleared, 2);
}

#[test]
fn test_invariant_preserves_ai_parent() {
    let long_text = "x".repeat(600);
    let mut msgs = vec![
        ai_with_tool("tc1", "Bash"),
        tool_result("tc1", &long_text),
        BaseMessage::human("q"),
        ai_plain("done"),
    ];
    let mut config = test_config();
    config.micro_compact_stale_steps = 1;
    let cleared = micro_compact_enhanced(&config, &mut msgs, 0);
    assert_eq!(cleared, 1);
    assert!(msgs[0].has_tool_calls());
}

// Edge cases

#[test]
fn test_empty_messages() {
    let mut msgs: Vec<BaseMessage> = vec![];
    let cleared = micro_compact_enhanced(&test_config(), &mut msgs, 0);
    assert_eq!(cleared, 0);
}

#[test]
fn test_no_tool_messages() {
    let mut msgs = vec![
        BaseMessage::human("q"),
        ai_plain("a"),
        BaseMessage::human("q2"),
        ai_plain("a2"),
    ];
    let cleared = micro_compact_enhanced(&test_config(), &mut msgs, 0);
    assert_eq!(cleared, 0);
}

#[test]
fn test_error_tool_result_preserved() {
    let mut msgs = vec![
        ai_with_tool("tc1", "Bash"),
        BaseMessage::tool_error("tc1", "error message"),
    ];
    let mut config = test_config();
    config.micro_compact_stale_steps = 0;
    let cleared = micro_compact_enhanced(&config, &mut msgs, 0);
    assert_eq!(cleared, 0);
}

#[test]
fn test_already_compacted_skipped() {
    let mut msgs = vec![
        ai_with_tool("tc1", "Bash"),
        tool_result("tc1", "[compacted: 600 chars]"),
    ];
    let mut config = test_config();
    config.micro_compact_stale_steps = 0;
    let cleared = micro_compact_enhanced(&config, &mut msgs, 0);
    assert_eq!(cleared, 0, "已压缩的消息应被跳过");
    assert_eq!(
        msgs[1].content(),
        "[compacted: 600 chars]",
        "已压缩消息内容不变"
    );
}

#[test]
fn test_orphan_tool_result_preserved() {
    let mut msgs = vec![tool_result("orphan_id", &"x".repeat(600))];
    let mut config = test_config();
    config.micro_compact_stale_steps = 0;
    let cleared = micro_compact_enhanced(&config, &mut msgs, 0);
    assert_eq!(cleared, 0);
}

#[test]
fn test_mixed_compactable_and_protected() {
    let long_text = "x".repeat(600);
    let mut msgs = vec![
        ai_with_tool("tc1", "Bash"),
        tool_result("tc1", &long_text),
        ai_with_tool("tc2", "AskUserQuestion"),
        tool_result("tc2", &long_text),
        ai_with_tool("tc3", "Bash"),
        tool_result("tc3", &long_text),
    ];
    let mut config = test_config();
    config.micro_compact_stale_steps = 0;
    let cleared = micro_compact_enhanced(&config, &mut msgs, 0);
    assert_eq!(cleared, 2);
    assert_eq!(msgs[1].content(), "[compacted: 600 chars]");
    assert_ne!(msgs[3].content(), "[compacted: 600 chars]");
    assert_eq!(msgs[5].content(), "[compacted: 600 chars]");
}

fn ai_plain(text: &str) -> BaseMessage {
    BaseMessage::ai(text)
}

// Document compaction tests

fn tool_result_with_document(tc_id: &str, source: DocumentSource) -> BaseMessage {
    BaseMessage::tool_result(
        tc_id,
        MessageContent::blocks(vec![
            ContentBlock::text("text content"),
            ContentBlock::Document {
                source,
                title: Some("doc.pdf".into()),
            },
        ]),
    )
}

#[test]
fn test_document_replaced_with_placeholder() {
    let mut msgs = vec![
        ai_with_tool("tc1", "Read"),
        tool_result_with_document(
            "tc1",
            DocumentSource::Text {
                text: "short doc".into(),
            },
        ),
    ];
    let mut config = test_config();
    config.micro_compact_stale_steps = 0;
    let cleared = micro_compact_enhanced(&config, &mut msgs, 0);
    assert_eq!(cleared, 1);
    let content = msgs[1].content();
    assert!(
        content.contains("[document]"),
        "Document 应被替换为占位符，实际: {}",
        content
    );
}

#[test]
fn test_large_document_compacted_with_token_info() {
    let large_b64 = "A".repeat(100_000);
    let mut msgs = vec![
        ai_with_tool("tc1", "Read"),
        tool_result_with_document(
            "tc1",
            DocumentSource::Base64 {
                media_type: "application/pdf".into(),
                data: large_b64,
            },
        ),
    ];
    let mut config = test_config();
    config.micro_compact_stale_steps = 0;
    let cleared = micro_compact_enhanced(&config, &mut msgs, 0);
    assert_eq!(cleared, 1);
    let content = msgs[1].content();
    assert!(
        content.contains("compacted: document"),
        "大 Document 应带 token 信息压缩，实际: {}",
        content
    );
    // 100_000 base64 chars * 3/4 (decode) / 4 (token est) = 18750 tokens
    assert!(
        content.contains("18750 tokens"),
        "应包含估算 token 数，实际: {}",
        content
    );
}

#[test]
fn test_document_compaction_preserves_other_content() {
    let mut msgs = vec![
        ai_with_tool("tc1", "Read"),
        BaseMessage::tool_result(
            "tc1",
            MessageContent::blocks(vec![
                ContentBlock::text("read output"),
                ContentBlock::Document {
                    source: DocumentSource::Url {
                        url: "https://example.com/doc.pdf".into(),
                    },
                    title: Some("remote.pdf".into()),
                },
            ]),
        ),
    ];
    let mut config = test_config();
    config.micro_compact_stale_steps = 0;
    let cleared = micro_compact_enhanced(&config, &mut msgs, 0);
    assert_eq!(cleared, 1);
    let content = msgs[1].content();
    // Document 被替换为占位符，Text 内容保留
    assert!(
        content.contains("[document]"),
        "Document 应被替换为占位符，实际: {}",
        content
    );
    assert!(
        content.contains("read output"),
        "Text 内容应被保留，实际: {}",
        content
    );
}

// tool_call_id preservation test

#[test]
fn test_compact_preserves_tool_call_ids_on_ai_message() {
    let tc1_id = "tc_abc_001";
    let tc1_name = "Bash";
    let tc2_id = "tc_xyz_002";
    let tc2_name = "Read";
    let long_text = "x".repeat(600);
    let mut msgs = vec![
        BaseMessage::ai_with_tool_calls(
            MessageContent::text("using tools"),
            vec![
                ToolCallRequest::new(tc1_id, tc1_name, json!({"cmd": "ls"})),
                ToolCallRequest::new(tc2_id, tc2_name, json!({"path": "a.rs"})),
            ],
        ),
        tool_result(tc1_id, &long_text),
        tool_result(tc2_id, &long_text),
    ];
    let mut config = test_config();
    config.micro_compact_stale_steps = 0;
    let cleared = micro_compact_enhanced(&config, &mut msgs, 0);
    assert_eq!(cleared, 2, "两个工具结果都应被压缩");
    let ai_msg = &msgs[0];
    let tool_calls = ai_msg.tool_calls();
    assert_eq!(tool_calls.len(), 2, "AI 消息应保留 2 个 tool_calls");
    assert_eq!(tool_calls[0].id, tc1_id, "tool_calls[0].id 应保持不变");
    assert_eq!(
        tool_calls[0].name, tc1_name,
        "tool_calls[0].name 应保持不变"
    );
    assert_eq!(tool_calls[1].id, tc2_id, "tool_calls[1].id 应保持不变");
    assert_eq!(
        tool_calls[1].name, tc2_name,
        "tool_calls[1].name 应保持不变"
    );
}
