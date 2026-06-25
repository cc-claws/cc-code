//! 100% 证据链：用真实 TUI App::new_headless 走完整数据流，注入 5MB ToolResult
//! 测量每个存储点真实 RSS 增长，对照用户报告"单轮 +32MB"
//!
//! 数据流：
//! 1. push_agent_event(AgentEvent::AssistantChunk with code) → syntect 加载 + 渲染
//! 2. push_agent_event(AgentEvent::StateSnapshot with [Human, Ai, ToolUse, ToolResult(5MB)])
//!    → origin_messages.extend (agent_ops/mod.rs:287)
//!    → pipeline.completed extend (message_pipeline/mod.rs:1039)
//!    → reconcile_tail → view_messages 重建（含 ToolBlock.content: String 深拷贝）
//! 3. flush_rebuild → RenderCache 渲染（Vec<Line<'static>> 含完整字符串）
//! 4. 测量总 RSS 增长
//!
//! 需用 `--features headless` 启用，因为 App::new_headless 是 headless feature gated

#![cfg(all(unix, feature = "headless"))]

use peri_agent::messages::{BaseMessage, ContentBlock, MessageContent};
use peri_tui::app::{AgentEvent, App};

fn current_rss_kb() -> usize {
    if let Ok(status) = std::fs::read_to_string("/proc/self/status") {
        for line in status.lines() {
            if let Some(rest) = line.strip_prefix("VmRSS:") {
                if let Some(num) = rest.split_whitespace().next() {
                    if let Ok(n) = num.parse::<usize>() {
                        return n;
                    }
                }
            }
        }
    }
    0
}

fn mb(kb: usize) -> f64 {
    kb as f64 / 1024.0
}

fn print_rss(label: &str, baseline: &mut usize) {
    let rss = current_rss_kb();
    let delta = rss.saturating_sub(*baseline);
    println!(
        "[{:50}] RSS = {:>8} KB ({:6.2} MB) | +{:>6} KB ({:+.2} MB)",
        label,
        rss,
        mb(rss),
        delta,
        mb(delta)
    );
    *baseline = rss;
}

#[tokio::test]
async fn large_toolresult_full_e2e_real_app() {
    println!("\n=== 端到端测试：真实 App::new_headless + 5MB ToolResult ===\n");

    let (mut app, _handle) = App::new_headless(120, 30).await;

    let mut base = current_rss_kb();
    println!("[基线（App 已创建）]");
    print_rss("App::new_headless 完成", &mut base);

    // === 阶段 1：注入代码块 AssistantChunk（触发 syntect 首次加载）===
    println!("\n--- 阶段 1: AssistantChunk 含代码块（syntect 首次加载）---");
    app.push_agent_event(AgentEvent::AssistantChunk {
        chunk: "I'll show code:\n\n```rust\nfn main() {\n    println!(\"hello\");\n}\n```".into(),
        source_agent_id: None,
    });
    app.process_pending_events();
    app.flush_rebuild();
    tokio::task::yield_now().await;
    tokio::task::yield_now().await;
    print_rss("AssistantChunk 含代码块 + flush_rebuild", &mut base);

    // === 阶段 2：构造 5MB ToolResult ===
    println!("\n--- 阶段 2: 构造 5MB ToolResult 内容 ---");
    let large_content: String = std::iter::repeat_n("X", 5 * 1024 * 1024).collect();
    print_rss("构造 5MB 原始字符串", &mut base);

    // === 阶段 3：StateSnapshot 注入完整消息流 ===
    println!("\n--- 阶段 3: StateSnapshot 注入（含 5MB ToolResult）---");
    let snapshot_msgs = vec![
        BaseMessage::human("read this big file"),
        BaseMessage::ai_from_blocks(vec![
            ContentBlock::text("reading..."),
            ContentBlock::ToolUse {
                id: "toolu_big".to_string(),
                name: "Read".to_string(),
                input: serde_json::json!({ "file_path": "/tmp/big.log" }),
            },
        ]),
        BaseMessage::tool_result(
            "toolu_big".to_string(),
            MessageContent::text(large_content.clone()),
        ),
        BaseMessage::ai("done reading the big file"),
    ];
    print_rss("构造 snapshot_msgs Vec（snapshot 局部变量）", &mut base);

    // 这一步触发：origin_messages.extend + pipeline.completed.extend
    app.push_agent_event(AgentEvent::StateSnapshot(snapshot_msgs));
    app.process_pending_events();
    print_rss("StateSnapshot 处理后（双 extend 完成）", &mut base);

    // === 阶段 4：Done + flush_rebuild（触发 reconcile + RenderCache 填充）===
    println!("\n--- 阶段 4: Done + flush_rebuild（触发 RenderCache 填充）---");
    app.push_agent_event(AgentEvent::Done);
    app.process_pending_events();
    app.flush_rebuild();
    tokio::task::yield_now().await;
    tokio::task::yield_now().await;
    tokio::task::yield_now().await;
    print_rss("Done + flush_rebuild（RenderCache 填充）", &mut base);

    // === 阶段 5：检查 view_messages ===
    println!("\n--- 阶段 5: 检查实际状态 ---");
    let view_count = app.session_mgr.current().messages.view_messages.len();
    let origin_count = app.session_mgr.current().agent.origin_messages.len();
    let completed_count = app
        .session_mgr
        .current()
        .messages
        .pipeline
        .completed_messages()
        .len();
    println!("view_messages:     {} 条 VM", view_count);
    println!("origin_messages:   {} 条 BaseMessage", origin_count);
    println!("pipeline.completed:{} 条 BaseMessage", completed_count);

    // === 阶段 6：drop app ===
    println!("\n--- 阶段 6: drop App（看 jemalloc/system malloc 持有）---");
    drop(app);
    drop(large_content);
    std::thread::sleep(std::time::Duration::from_millis(500));
    print_rss("drop App + 等 500ms", &mut base);
}

#[tokio::test]
async fn varying_toolresult_size_real_app() {
    //! 扫描 1MB / 3MB / 5MB ToolResult，用真实 App 跑端到端
    println!("\n=== 真实 App 端到端：不同 ToolResult 大小的 RSS 增长 ===\n");
    println!("| ToolResult | 阶段1 syntect | 阶段3 双extend | 阶段4 RenderCache | 总增长 |");
    println!("|-----------|---------------|----------------|-------------------|--------|");

    for &size_mb in &[1usize, 3, 5] {
        // 每个 size 跑独立的 App 实例（避免累积）
        let (mut app, _handle) = App::new_headless(120, 30).await;
        let base = current_rss_kb();

        // 阶段 1：syntect
        app.push_agent_event(AgentEvent::AssistantChunk {
            chunk: "```rust\nfn main() {}\n```".into(),
            source_agent_id: None,
        });
        app.process_pending_events();
        app.flush_rebuild();
        tokio::task::yield_now().await;
        let s1 = current_rss_kb().saturating_sub(base);

        // 阶段 2 + 3：构造 ToolResult + StateSnapshot
        let large_content: String = std::iter::repeat_n("X", size_mb * 1024 * 1024).collect();
        let snapshot_msgs = vec![
            BaseMessage::human("read big file"),
            BaseMessage::ai_from_blocks(vec![
                ContentBlock::text("reading"),
                ContentBlock::ToolUse {
                    id: "toolu_big".to_string(),
                    name: "Read".to_string(),
                    input: serde_json::json!({}),
                },
            ]),
            BaseMessage::tool_result(
                "toolu_big".to_string(),
                MessageContent::text(large_content.clone()),
            ),
        ];
        app.push_agent_event(AgentEvent::StateSnapshot(snapshot_msgs));
        app.process_pending_events();
        let s3 = current_rss_kb().saturating_sub(base).saturating_sub(s1);

        // 阶段 4：Done + flush_rebuild
        app.push_agent_event(AgentEvent::Done);
        app.process_pending_events();
        app.flush_rebuild();
        tokio::task::yield_now().await;
        tokio::task::yield_now().await;
        let s4 = current_rss_kb()
            .saturating_sub(base)
            .saturating_sub(s1)
            .saturating_sub(s3);

        let total = s1 + s3 + s4;
        println!(
            "| {} MB | {:>5} KB | {:>7} KB | {:>10} KB | {:>5} KB ({:.2} MB) |",
            size_mb,
            s1,
            s3,
            s4,
            total,
            mb(total)
        );

        drop(app);
        drop(large_content);
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
}

#[tokio::test]
async fn multi_round_accumulation_real_app() {
    //! 模拟用户真实使用：10 轮，其中 2 轮含 3MB 大工具调用
    println!("\n=== 真实 App 端到端：10 轮含 2 次 3MB 大工具调用 ===\n");

    let (mut app, _handle) = App::new_headless(120, 30).await;
    let base = current_rss_kb();
    println!("基线 RSS（App 已创建）: {} KB ({:.2} MB)\n", base, mb(base));
    println!("| 轮次 | RSS | 累计增长 | 备注 |");
    println!("|------|-----|---------|------|");

    let mut prev = base;
    for round in 1..=10 {
        // 模拟用户提交
        app.session_mgr.current_mut().messages.round_start_vm_idx =
            app.session_mgr.current_mut().messages.view_messages.len();

        // AI 回复 chunk
        app.push_agent_event(AgentEvent::AssistantChunk {
            chunk: format!("Round {} response", round),
            source_agent_id: None,
        });
        app.process_pending_events();

        // 第 3 轮和第 7 轮：注入 3MB 大 ToolResult
        let mut snapshot = vec![
            BaseMessage::human(format!("round {}", round)),
            BaseMessage::ai(format!("response {}", round)),
        ];
        if round == 3 || round == 7 {
            let large: String = std::iter::repeat_n("X", 3 * 1024 * 1024).collect();
            snapshot.push(BaseMessage::ai_from_blocks(vec![
                ContentBlock::text("reading big file"),
                ContentBlock::ToolUse {
                    id: format!("toolu_{}", round),
                    name: "Read".to_string(),
                    input: serde_json::json!({}),
                },
            ]));
            snapshot.push(BaseMessage::tool_result(
                format!("toolu_{}", round),
                MessageContent::text(large),
            ));
            snapshot.push(BaseMessage::ai("done"));
        }

        app.push_agent_event(AgentEvent::StateSnapshot(snapshot));
        app.push_agent_event(AgentEvent::Done);
        app.process_pending_events();
        app.flush_rebuild();
        tokio::task::yield_now().await;

        let rss = current_rss_kb();
        let delta = rss.saturating_sub(prev);
        let total = rss.saturating_sub(base);
        let note = if round == 3 || round == 7 {
            "← 含 3MB 大文件"
        } else {
            ""
        };
        println!(
            "| {:>4} | {:>6.2} MB | +{:>5.2} MB (累计 +{:.2} MB) | {} |",
            round,
            mb(rss),
            mb(delta),
            mb(total),
            note
        );
        prev = rss;
    }

    // drop 看持有
    drop(app);
    std::thread::sleep(std::time::Duration::from_millis(500));
    let after_drop = current_rss_kb();
    println!(
        "\n| drop 后 | {:.2} MB | 持有未归还: {:.2} MB |",
        mb(after_drop),
        mb(after_drop.saturating_sub(base))
    );
}
