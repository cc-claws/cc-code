//! 用 dhat 做堆 profile：定位 TUI 会话里"谁持有内存"，特别是 96 KB–512 KB 的大块。
//!
//! 运行：
//!   cargo test -p cc-tui --features headless --test heap_profile_dhat -- --nocapture
//!
//! 产物：`C:/temp/dhat-heap.json`（可用 dh_view.html 打开），终端同时打印各阶段 HeapStats。
//! 负载刻意贴近实测会话：220 条左右消息、每条 markdown 1–2 KB、工具输出多数几 KB、
//! 极少数 96 KB–448 KB 大块（对应 /gc 里那 12 个吃掉 4 MB 页的对象）。

#![cfg(feature = "headless")]

use cc_agent::messages::{BaseMessage, ContentBlock, MessageContent};
use cc_tui::app::events::AgentEvent;
use cc_tui::app::App;

#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

/// 造一个目标大小的可打印字符串（内容重复，便于识别来源）。
fn payload(bytes: usize, tag: &str) -> String {
    let mut s = String::with_capacity(bytes);
    s.push_str(tag);
    s.push(':');
    let unit = "lorem ipsum dolor sit amet consectetur adipiscing elit sed do eiusmod\n";
    while s.len() + unit.len() < bytes {
        s.push_str(unit);
    }
    s
}

fn report(label: &str) {
    let st = dhat::HeapStats::get();
    println!(
        "[{label:>28}] curr={:>9} B ({:6.2} MB) blocks={:>7} | total={:>10} B | peak={:>9} B ({:6.2} MB)",
        st.curr_bytes,
        st.curr_bytes as f64 / (1024.0 * 1024.0),
        st.curr_blocks,
        st.total_bytes,
        st.max_bytes,
        st.max_bytes as f64 / (1024.0 * 1024.0),
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn heap_profile_session_shape() {
    let _profiler = dhat::Profiler::builder()
        .file_name("C:/temp/dhat-heap.json")
        .trim_backtraces(None)
        .build();

    let (mut app, _handle) = App::new_headless(120, 40).await;
    report("app created");

    // 触发 syntect 懒加载
    app.push_agent_event(AgentEvent::AssistantChunk {
        chunk: "```rust\nfn main() { println!(\"hi\"); }\n```".into(),
        source_agent_id: None,
    });
    app.process_pending_events();
    app.flush_rebuild();
    tokio::task::yield_now().await;
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    report("syntect loaded");

    for round in 0..60usize {
        let text = format!(
            "## Round {round}\n\n{}\n\n```rust\nfn f{round}() -> i32 {{ {round} * 2 }}\n```\n\n- 要点 A\n- 要点 B\n\n完成。",
            "说明文字 ".repeat(30)
        );
        app.push_agent_event(AgentEvent::AssistantChunk {
            chunk: text.clone(),
            source_agent_id: None,
        });

        // 绝大多数工具输出很小，只有极少数大块（对应 /gc 的 96 KB–448 KB bin）
        let out_bytes = if round % 29 == 0 {
            448 * 1024
        } else if round % 17 == 0 {
            128 * 1024
        } else {
            4 * 1024
        };
        let tool_out = payload(out_bytes, &format!("tool-{round}"));

        let msgs = vec![
            BaseMessage::human(format!("第 {round} 轮用户问题")),
            BaseMessage::ai_from_blocks(vec![
                ContentBlock::text(text),
                ContentBlock::ToolUse {
                    id: format!("tu_{round}"),
                    name: "Read".to_string(),
                    input: serde_json::json!({ "file_path": "D:/code/peri/cc-tui/src/main.rs" }),
                },
            ]),
            BaseMessage::tool_result(format!("tu_{round}"), MessageContent::text(tool_out)),
        ];
        app.push_agent_event(AgentEvent::StateSnapshot(msgs));
        app.push_agent_event(AgentEvent::Done);
        app.process_pending_events();

        if round % 5 == 0 {
            app.flush_rebuild();
            tokio::task::yield_now().await;
        }
        if round % 20 == 0 {
            report(&format!("round {round}"));
        }
    }

    app.flush_rebuild();
    tokio::task::yield_now().await;
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    report("final (after render)");

    let session = app.session_mgr.current();
    println!(
        "origin={} completed={} view_messages={}",
        session.agent.origin_messages.len(),
        session.messages.pipeline.completed_messages().len(),
        session.messages.view_messages.len(),
    );
}
