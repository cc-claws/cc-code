//! 真实会话回放回归测试（PRD §2.3 / §2.8）
//!
//! 夹具取自真实 `~/.cc-code/threads/threads.db`（thread `019f1782-…`），
//! 保留原始 JSON 形态（**reasoning block 无 `duration_ms`**，正是旧数据），
//! 用于验证：
//! 1. 旧数据不 panic、优雅降级（`Thought for N chars`）
//! 2. 只读工具（Grep/Glob）注入 `action_summary`，Bash 不注入
//! 3. 完整渲染管线可跑通

use super::*;
use crate::ui::message_render::render_view_model;
use crate::ui::message_view::ContentBlockView;
use cc_agent::messages::{BaseMessage, ContentBlock, MessageContent, ToolCallRequest};

/// 真实 assistant 消息（含 reasoning + Glob，**逐字取自 threads.db**，含顶层 `tool_calls`）。
/// 注意：reasoning block **无 `duration_ms`**（旧数据）。
fn real_reasoning_msg_glob() -> &'static str {
    r#"{"role":"assistant","id":"019f1783-7b2f-7143-9614-63e803246a9f","content":[{"type":"reasoning","text":"我看到用户想修复三个订单的item数量。他们提供了一个SQL脚本，但需要知道如何连接到数据库。让我先查看项目中是否有数据库配置文件或连接信息。\n\n我需要搜索数据库连接配置，看看项目使用什么数据库（MySQL、PostgreSQL、SQLite等）。让我搜索配置文件。","signature":""},{"type":"tool_use","id":"call_c6072c4896574474b7ed4189","name":"Glob","input":{"pattern":"**/*.{env,config,json,yaml,yml,toml}"}}],"tool_calls":[{"id":"call_c6072c4896574474b7ed4189","name":"Glob","arguments":{"pattern":"**/*.{env,config,json,yaml,yml,toml}"}}]}"#
}

/// 真实 assistant 消息（含 reasoning + Bash；逐字取自 threads.db 的等价结构）。
fn real_reasoning_msg_bash() -> &'static str {
    r#"{"role":"assistant","id":"019f1783-9999-7143-9614-63e803240000","content":[{"type":"reasoning","text":"让我用 Bash 查找 SQL 文件。","signature":""},{"type":"tool_use","id":"call_bash_1","name":"Bash","input":{"command":"find . -type f -name \"*.sql\" 2>/dev/null | head"}}],"tool_calls":[{"id":"call_bash_1","name":"Bash","arguments":{"command":"find . -type f -name \"*.sql\" 2>/dev/null | head"}}]}"#
}

#[test]
fn test_real_legacy_data_deserializes_and_degrades() {
    let raw = real_reasoning_msg_glob();
    let msg: BaseMessage = serde_json::from_str(raw).expect("真实旧数据必须能反序列化");
    if let BaseMessage::Ai { content, .. } = &msg {
        if let MessageContent::Blocks(blocks) = content {
            let reasoning = blocks
                .iter()
                .find_map(|b| b.as_reasoning())
                .expect("应有 reasoning");
            assert!(!reasoning.is_empty(), "reasoning 文本不应丢失");
        } else {
            panic!("应为 Blocks");
        }
    } else {
        panic!("应为 Ai 消息");
    }
}

#[test]
fn test_real_session_readonly_counted_bash_not() {
    // Glob（只读）→ 应注入
    let glob_msg: BaseMessage = serde_json::from_str(real_reasoning_msg_glob()).unwrap();
    let glob_tool = BaseMessage::Tool {
        id: cc_agent::messages::MessageId::new(),
        tool_call_id: "call_c6072c4896574474b7ed4189".to_string(),
        content: MessageContent::text("Found 89 files"),
        is_error: false,
    };
    let vms = MessagePipeline::messages_to_view_models(&[glob_msg, glob_tool], "/p");
    let summary = first_action_summary(&vms);
    assert_eq!(
        summary.as_deref(),
        Some("listed 1 directory"),
        "真实 Glob 会话应注入目录计数"
    );

    // Bash（非只读）→ 不注入
    let bash_msg: BaseMessage = serde_json::from_str(real_reasoning_msg_bash()).unwrap();
    let bash_tool = BaseMessage::Tool {
        id: cc_agent::messages::MessageId::new(),
        tool_call_id: "call_bash_1".to_string(),
        content: MessageContent::text("ok"),
        is_error: false,
    };
    let vms2 = MessagePipeline::messages_to_view_models(&[bash_msg, bash_tool], "/p");
    assert_eq!(first_action_summary(&vms2), None, "Bash 不应产生只读计数");
}

#[test]
fn test_real_legacy_data_render_pipeline_no_panic() {
    let msg: BaseMessage = serde_json::from_str(real_reasoning_msg_glob()).unwrap();
    let vms = MessagePipeline::messages_to_view_models(&[msg], "/p");
    for vm in &vms {
        let normal = render_view_model(vm, Some(0), 80, false, 0);
        let detail = render_view_model(vm, Some(0), 80, true, 0);
        let text: String = normal
            .iter()
            .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref()))
            .collect();
        if text.contains("Thought for") {
            assert!(
                text.contains("chars"),
                "旧数据应降级为 chars 摘要，实际: {text}"
            );
        }
        assert!(!detail.is_empty(), "详细模式应有输出");
    }
}

// ── PRD §2.8：多轮「思考 + 只读工具」合并 ──────────────────────────

/// 提取所有 Reasoning block 的 (duration_ms, action_summary)
fn all_reasoning_meta(vms: &[MessageViewModel]) -> Vec<(Option<u64>, Option<String>)> {
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

fn first_action_summary(vms: &[MessageViewModel]) -> Option<String> {
    all_reasoning_meta(vms).into_iter().find_map(|(_, a)| a)
}

/// 构造：3 轮 reason+只读工具，末轮带可见文本
fn make_merge_sequence() -> Vec<BaseMessage> {
    let mk_ai = |rid: &str, tid: &str, tool: &str, arg: &str| BaseMessage::Ai {
        id: cc_agent::messages::MessageId::new(),
        content: MessageContent::blocks(vec![ContentBlock::reasoning_with_duration(
            format!("reason-{rid}"),
            1000,
        )]),
        tool_calls: vec![ToolCallRequest {
            id: tid.to_string(),
            name: tool.to_string(),
            arguments: if tool == "Bash" {
                serde_json::json!({ "command": arg })
            } else {
                serde_json::json!({ "file_path": arg })
            },
        }],
    };
    let mk_tool = |tid: &str| BaseMessage::Tool {
        id: cc_agent::messages::MessageId::new(),
        tool_call_id: tid.to_string(),
        content: MessageContent::text("结果"),
        is_error: false,
    };
    vec![
        mk_ai("a", "t1", "Read", "/a.rs"),
        mk_tool("t1"),
        mk_ai("b", "t2", "Read", "/b.rs"),
        mk_tool("t2"),
        mk_ai("c", "t3", "Grep", "todo"),
        mk_tool("t3"),
        BaseMessage::ai(MessageContent::blocks(vec![
            ContentBlock::Reasoning {
                text: "总结".to_string(),
                signature: None,
                duration_ms: Some(500),
            },
            ContentBlock::text("已检查完毕"),
        ])),
    ]
}

#[test]
fn test_multi_round_thinking_merged_with_accumulated_stats() {
    let msgs = make_merge_sequence();
    let vms = MessagePipeline::messages_to_view_models(&msgs, "/p");
    let metas = all_reasoning_meta(&vms);

    // 段首一条：累加 3 轮（3000ms）+ read 2 files, searched for 1 pattern
    assert!(
        metas.iter().any(|(d, a)| *d == Some(3000)
            && a.as_deref() == Some("read 2 files, searched for 1 pattern")),
        "段首应累加秒数与计数，实际: {metas:?}"
    );
    // 含文本轮独立（500ms）
    assert!(
        metas.iter().any(|(d, _)| *d == Some(500)),
        "含文本轮应独立，实际: {metas:?}"
    );
    // 被合并的中间轮不应残留（无 Some(1000)）
    assert!(
        !metas.iter().any(|(d, _)| *d == Some(1000)),
        "被合并的中间轮不应残留，实际: {metas:?}"
    );
}

// ── 审计修复回归：R2（多 Reasoning 重复）/ R3（并行计数遗漏）/ R6（时长部分缺失）──

/// 提取某 bubble 中所有 Reasoning block 的 (duration_ms, action_summary)
fn reasoning_blocks_of(vm: &MessageViewModel) -> Vec<(Option<u64>, Option<String>)> {
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
            .collect()
    } else {
        Vec::new()
    }
}

/// R2：同一条 AI 消息含 ≥2 个 Reasoning block 时，合并后**不得**残留多个
/// （否则渲染重复文本）。
#[test]
fn test_merge_removes_extra_reasoning_blocks_in_same_bubble() {
    // Arrange：单条 AI 消息含 2 个 Reasoning + 1 个 Read 工具
    let msg = BaseMessage::Ai {
        id: cc_agent::messages::MessageId::new(),
        content: MessageContent::blocks(vec![
            ContentBlock::reasoning_with_duration("第一段思考", 1000),
            ContentBlock::reasoning_with_duration("第二段思考", 2000),
        ]),
        tool_calls: vec![ToolCallRequest {
            id: "t1".to_string(),
            name: "Read".to_string(),
            arguments: serde_json::json!({ "file_path": "/a.rs" }),
        }],
    };
    let tool = BaseMessage::Tool {
        id: cc_agent::messages::MessageId::new(),
        tool_call_id: "t1".to_string(),
        content: MessageContent::text("内容"),
        is_error: false,
    };

    // Act
    let vms = MessagePipeline::messages_to_view_models(&[msg, tool], "/p");

    // Assert：含 Reasoning 的 bubble 中，Reasoning block 数应为 1（其余已并入/移除）
    let bubble = vms
        .iter()
        .find(|vm| !reasoning_blocks_of(vm).is_empty())
        .expect("应有含 Reasoning 的 bubble");
    let rs = reasoning_blocks_of(bubble);
    assert_eq!(
        rs.len(),
        1,
        "同 bubble 只应保留 1 个 Reasoning，实际: {rs:?}"
    );
    // 时长应为两段之和
    assert_eq!(rs[0].0, Some(3000), "耗时应累加为 3000ms");
    assert_eq!(
        rs[0].1.as_deref(),
        Some("read 1 file"),
        "计数应注入，实际: {rs:?}"
    );
}

/// R3：并行工具（Bash + Read 同一条 AI 消息）时，Read 必须被计入。
#[test]
fn test_parallel_tools_read_counted_alongside_bash() {
    // Arrange：一条 AI 消息同时含 Bash + Read 两个 tool_calls
    let msg = BaseMessage::Ai {
        id: cc_agent::messages::MessageId::new(),
        content: MessageContent::blocks(vec![ContentBlock::reasoning_with_duration("思考", 500)]),
        tool_calls: vec![
            ToolCallRequest {
                id: "b1".to_string(),
                name: "Bash".to_string(),
                arguments: serde_json::json!({ "command": "ls" }),
            },
            ToolCallRequest {
                id: "r1".to_string(),
                name: "Read".to_string(),
                arguments: serde_json::json!({ "file_path": "/a.rs" }),
            },
        ],
    };
    let tools = vec![
        BaseMessage::Tool {
            id: cc_agent::messages::MessageId::new(),
            tool_call_id: "b1".to_string(),
            content: MessageContent::text("ok"),
            is_error: false,
        },
        BaseMessage::Tool {
            id: cc_agent::messages::MessageId::new(),
            tool_call_id: "r1".to_string(),
            content: MessageContent::text("内容"),
            is_error: false,
        },
    ];

    // Act
    let mut all = vec![msg];
    all.extend(tools);
    let vms = MessagePipeline::messages_to_view_models(&all, "/p");

    // Assert：Read 应被计数（Bash 不计）
    let rs = vms.iter().flat_map(reasoning_blocks_of).collect::<Vec<_>>();
    assert!(
        rs.iter().any(|(_, a)| a.as_deref() == Some("read 1 file")),
        "并行工具中的 Read 应计入，实际: {rs:?}"
    );
}

/// R6：段内部分思考无 duration_ms 时，应整体降级（不显示被低估的秒数）。
#[test]
fn test_partial_missing_duration_degrades_whole_segment() {
    // Arrange：两段连续思考——第一段有耗时、第二段无
    let mk = |rid: &str, tid: &str, dur: Option<u64>| BaseMessage::Ai {
        id: cc_agent::messages::MessageId::new(),
        content: MessageContent::blocks(vec![ContentBlock::Reasoning {
            text: rid.to_string(),
            signature: None,
            duration_ms: dur,
        }]),
        tool_calls: vec![ToolCallRequest {
            id: tid.to_string(),
            name: "Read".to_string(),
            arguments: serde_json::json!({ "file_path": "/a.rs" }),
        }],
    };
    let mk_tool = |tid: &str| BaseMessage::Tool {
        id: cc_agent::messages::MessageId::new(),
        tool_call_id: tid.to_string(),
        content: MessageContent::text("x"),
        is_error: false,
    };
    let msgs = vec![
        mk("a", "t1", Some(1000)),
        mk_tool("t1"),
        mk("b", "t2", None), // ← 缺耗时
        mk_tool("t2"),
    ];

    // Act
    let vms = MessagePipeline::messages_to_view_models(&msgs, "/p");
    let rs = vms.iter().flat_map(reasoning_blocks_of).collect::<Vec<_>>();

    // Assert：合并后的段首 duration_ms 应为 None（不低估）
    assert!(
        rs.iter().all(|(d, _)| d.is_none()),
        "任一段缺失耗时时应整体降级为 None，实际: {rs:?}"
    );
}

/// N1 回归：降级为 chars 分支时，`char_count` 必须是**合并后**的字数（非段首旧值）。
#[test]
fn test_merged_char_count_reflects_all_segments_on_fallback() {
    // Arrange：两段思考（第一段有耗时、第二段无 → 触发降级），字数不同
    let mk = |text: &str, tid: &str, dur: Option<u64>| BaseMessage::Ai {
        id: cc_agent::messages::MessageId::new(),
        content: MessageContent::blocks(vec![ContentBlock::Reasoning {
            text: text.to_string(),
            signature: None,
            duration_ms: dur,
        }]),
        tool_calls: vec![ToolCallRequest {
            id: tid.to_string(),
            name: "Read".to_string(),
            arguments: serde_json::json!({ "file_path": "/a.rs" }),
        }],
    };
    let mk_tool = |tid: &str| BaseMessage::Tool {
        id: cc_agent::messages::MessageId::new(),
        tool_call_id: tid.to_string(),
        content: MessageContent::text("x"),
        is_error: false,
    };
    // 段首 2 字，第二段 10 字 → 合并 12 字
    let msgs = vec![
        mk("一二", "t1", Some(1000)),
        mk_tool("t1"),
        mk("三四五六七八九十", "t2", None),
        mk_tool("t2"),
    ];

    // Act
    let vms = MessagePipeline::messages_to_view_models(&msgs, "/p");
    let (dur, chars) = vms
        .iter()
        .find_map(|vm| {
            if let MessageViewModel::AssistantBubble { blocks, .. } = vm {
                blocks.iter().find_map(|b| {
                    if let ContentBlockView::Reasoning {
                        duration_ms,
                        char_count,
                        ..
                    } = b
                    {
                        Some((*duration_ms, *char_count))
                    } else {
                        None
                    }
                })
            } else {
                None
            }
        })
        .expect("应有 Reasoning");

    // Assert：降级为 None，且 char_count 为合并后 12（非段首 2）
    assert_eq!(dur, None, "缺耗时段应降级");
    assert_eq!(chars, 12, "char_count 应为合并后字数（非段首旧值）");
}
