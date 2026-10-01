//! 真实会话端到端测试（PRD §2.8 多轮合并）
//!
//! 夹具 `fixtures/session_01a0ebee.json` 为**真实会话**（逐字导出自
//! `~/.cc-code/threads/threads.db` 的 thread `01a0ebee-6d24-7422-9040-4f564d4e00dc`）。
//! 该会话含用户实测发现的「多轮思考未合并」场景（9 条连续 Thought）。

use super::*;
use crate::ui::message_render::render_view_model;
use crate::ui::message_view::ContentBlockView;
use cc_agent::messages::BaseMessage;

/// 真实会话：52 条消息（含 reasoning / tool / text 混合）
fn load_real_session() -> Vec<BaseMessage> {
    let raw = include_str!("fixtures/session_01a0ebee.json");
    serde_json::from_str(raw).expect("真实 session 必须能反序列化")
}

/// 统计 VM 中所有 `Thought for` 标题（即含 Reasoning 的 block）
fn thought_titles(vms: &[MessageViewModel]) -> Vec<(Option<u64>, Option<String>)> {
    vms.iter()
        .flat_map(|vm| {
            if let MessageViewModel::AssistantBubble { blocks, .. } = vm {
                blocks
                    .iter()
                    .filter_map(|b| {
                        if let ContentBlockView::Reasoning {
                            duration_ms,
                            action_summary,
                            ..
                        } = b
                        {
                            Some((*duration_ms, action_summary.clone()))
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>()
            } else {
                Vec::new()
            }
        })
        .collect()
}

#[test]
fn test_real_session_full_pipeline_runs() {
    // Arrange
    let msgs = load_real_session();

    // Act
    let vms = MessagePipeline::messages_to_view_models(&msgs, "/tmp/work");

    // Assert：产出非空且可渲染
    assert!(!vms.is_empty(), "真实会话应产出 VM");
    for vm in &vms {
        let _ = render_view_model(vm, None, 100, false, 0);
        let _ = render_view_model(vm, None, 100, true, 0);
    }
}

#[test]
fn test_real_session_multi_round_thinking_merged() {
    // Arrange：真实会话（旧数据无 duration_ms，故合并主要靠计数聚合）
    let msgs = load_real_session();

    // Act
    let vms = MessagePipeline::messages_to_view_models(&msgs, "/tmp/work");
    let titles = thought_titles(&vms);

    // Assert 1：标题数远少于原始 reasoning 总数（说明发生了合并）
    let raw_reasoning_count = msgs
        .iter()
        .filter_map(|m| {
            if let BaseMessage::Ai { content, .. } = m {
                Some(
                    content
                        .content_blocks()
                        .iter()
                        .filter(|b| {
                            matches!(b, cc_agent::messages::ContentBlock::Reasoning { .. })
                        })
                        .count(),
                )
            } else {
                None
            }
        })
        .sum::<usize>();
    assert!(raw_reasoning_count > 0, "真实会话应含 reasoning");
    assert!(
        titles.len() < raw_reasoning_count,
        "标题数应少于原始 reasoning 数（{}/{}），说明发生合并",
        titles.len(),
        raw_reasoning_count
    );

    // Assert 2：至少有一条标题带 action_summary（只读工具计数被聚合）
    assert!(
        titles.iter().any(|(_, a)| a.is_some()),
        "应至少有一条标题带只读工具计数，实际: {titles:?}"
    );
}

#[test]
fn test_real_session_legacy_data_degrades_to_chars() {
    // Arrange：真实会话为旧数据（reasoning 无 duration_ms）
    let msgs = load_real_session();

    // Act
    let vms = MessagePipeline::messages_to_view_models(&msgs, "/tmp/work");
    let titles = thought_titles(&vms);

    // Assert：所有 duration_ms 均为 None（旧数据降级）
    assert!(
        titles.iter().all(|(d, _)| d.is_none()),
        "旧数据的所有 duration_ms 应为 None，实际: {titles:?}"
    );
}
