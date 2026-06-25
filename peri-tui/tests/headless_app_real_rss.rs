//! 用真实 App::new_headless 跑多轮 AgentEvent，测真实 TUI App 的 RSS 增长
//! 这是定位 TUI 层内存大头的最关键测试
//!
//! 需用 `--features headless` 启用，因为 App::new_headless 是 headless feature gated

#![cfg(feature = "headless")]

use peri_agent::messages::BaseMessage;
use peri_tui::app::{events::AgentEvent, App};
use peri_tui::ui::main_ui;

#[cfg(unix)]
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn measure_headless_app_30_rounds_rss() {
    let (mut app, mut handle) = App::new_headless(120, 40).await;
    // 给渲染线程时间初始化
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let baseline = current_rss_kb();
    println!("\n=== Headless App 30 轮真实 RSS 增长 ===");
    println!(
        "基线 RSS（含 App + 渲染线程）: {} KB ({:.2} MB)",
        baseline,
        baseline as f64 / 1024.0
    );
    println!("| 轮 | RSS (KB) | 本轮+ (KB) | 累计+ (KB) | view_messages |");
    println!("|----|----------|------------|------------|---------------|");

    let mut prev = baseline;
    for i in 0..30 {
        // 模拟一轮：AssistantChunk（流式）+ StateSnapshot + Done
        let ai_text = format!(
            r#"## 第 {i} 轮分析

### 主要发现

1. **性能问题**：嵌套循环导致 O(n²) 复杂度
2. **错误处理**：unwrap() 可能 panic

```rust
fn optimized(data: &[u8]) -> Option<usize> {{
    let mut index: HashMap<u8, usize> = HashMap::new();
    for (i, &b) in data.iter().enumerate() {{
        if let Some(&prev) = index.get(&b) {{
            return Some(prev);
        }}
        index.insert(b, i);
    }}
    None
}}
```

第 {i} 轮优化完成。"#
        );

        // 模拟流式 chunk
        let chunk_size = 80;
        for chunk in ai_text.as_bytes().chunks(chunk_size) {
            if let Ok(s) = std::str::from_utf8(chunk) {
                app.push_agent_event(AgentEvent::AssistantChunk {
                    chunk: s.to_string(),
                    source_agent_id: None,
                });
            }
        }

        // StateSnapshot
        let user_msg = BaseMessage::human(format!("第 {i} 轮问题"));
        let ai_msg = BaseMessage::ai(ai_text.clone());
        app.push_agent_event(AgentEvent::StateSnapshot(vec![user_msg, ai_msg]));

        // Done
        app.push_agent_event(AgentEvent::Done);

        // 处理事件
        app.process_pending_events();
        app.flush_rebuild();

        // 让渲染线程工作
        tokio::task::yield_now().await;
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;

        // 强制一次绘制（与真实 TUI 主循环一致）
        let _ = handle.terminal.draw(|f| main_ui::render(f, &mut app));

        let rss = current_rss_kb();
        let delta = rss.saturating_sub(prev);
        let total = rss.saturating_sub(baseline);
        let vm_len = app.session_mgr.current().messages.view_messages.len();
        if i == 0 || (i + 1) % 3 == 0 {
            println!(
                "| {:2} | {:8} | {:10} | {:10} | {:13} |",
                i + 1,
                rss,
                delta,
                total,
                vm_len
            );
            prev = rss;
        }
    }

    // 等待任何后台清理
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    let final_rss = current_rss_kb();
    let total = final_rss.saturating_sub(baseline);
    println!("\n=== 完成 30 轮 ===");
    println!(
        "最终 RSS: {} KB ({:.2} MB)",
        final_rss,
        final_rss as f64 / 1024.0
    );
    println!("累计增长: {} KB ({:.2} MB)", total, total as f64 / 1024.0);
    println!("平均每轮: {:.2} KB", total as f64 / 30.0);
    println!(
        "view_messages len: {}",
        app.session_mgr.current().messages.view_messages.len()
    );
    println!(
        "origin_messages len: {}",
        app.session_mgr.current().agent.origin_messages.len()
    );

    // 最终一次绘制 + 测量
    let _ = handle.terminal.draw(|f| main_ui::render(f, &mut app));
    tokio::task::yield_now().await;
    let after_draw = current_rss_kb();
    println!("最终 draw 后 RSS: {} KB", after_draw);

    // 防止 app 提前 drop 影响测量
    std::mem::forget(app);
    std::mem::forget(handle);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn measure_headless_app_with_tool_calls() {
    // 测含工具调用的真实场景（tool_use + tool_result）
    let (mut app, mut handle) = App::new_headless(120, 40).await;
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let baseline = current_rss_kb();
    println!("\n=== Headless App 含工具调用 30 轮 ===");
    println!(
        "基线 RSS: {} KB ({:.2} MB)",
        baseline,
        baseline as f64 / 1024.0
    );
    println!("| 轮 | RSS (KB) | 累计+ (KB) |");
    println!("|----|----------|------------|");

    for i in 0..30 {
        // ToolStart
        app.push_agent_event(AgentEvent::ToolStart {
            tool_call_id: format!("toolu_{i}"),
            name: "Read".into(),
            display: "ReadFile".into(),
            args: format!("{{\"file_path\":\"/tmp/test_{i}.txt\"}}"),
            input: serde_json::json!({ "file_path": format!("/tmp/test_{i}.txt") }),
            source_agent_id: None,
        });

        // ToolEnd（模拟读小文件）
        let output = format!(
            "1: line content {i}\n2: another line\n3: third line with some text content to make it realistic"
        );
        app.push_agent_event(AgentEvent::ToolEnd {
            tool_call_id: format!("toolu_{i}"),
            name: "Read".into(),
            output,
            is_error: false,
            source_agent_id: None,
        });

        // AssistantChunk（AI 总结）
        let ai_text = format!("第 {i} 轮：读取了文件，内容包含三行测试数据。");
        app.push_agent_event(AgentEvent::AssistantChunk {
            chunk: ai_text,
            source_agent_id: None,
        });

        // StateSnapshot
        let user_msg = BaseMessage::human(format!("第 {i} 轮问题"));
        let ai_msg = BaseMessage::ai(format!("第 {i} 轮：读取了文件，内容包含三行测试数据。"));
        app.push_agent_event(AgentEvent::StateSnapshot(vec![user_msg, ai_msg]));
        app.push_agent_event(AgentEvent::Done);

        app.process_pending_events();
        app.flush_rebuild();
        tokio::task::yield_now().await;
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        let _ = handle.terminal.draw(|f| main_ui::render(f, &mut app));

        let rss = current_rss_kb();
        let total = rss.saturating_sub(baseline);
        if i == 0 || (i + 1) % 5 == 0 {
            println!("| {:2} | {:8} | {:10} |", i + 1, rss, total);
        }
    }

    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    let final_rss = current_rss_kb();
    let total = final_rss.saturating_sub(baseline);
    println!("\n=== 完成 30 轮（含工具调用） ===");
    println!(
        "最终 RSS: {} KB ({:.2} MB)",
        final_rss,
        final_rss as f64 / 1024.0
    );
    println!("累计增长: {} KB ({:.2} MB)", total, total as f64 / 1024.0);
    println!("平均每轮: {:.2} KB", total as f64 / 30.0);

    std::mem::forget(app);
    std::mem::forget(handle);
}
