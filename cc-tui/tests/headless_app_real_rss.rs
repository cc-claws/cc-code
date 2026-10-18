//! 用真实 App::new_headless 跑多轮 AgentEvent，测真实 TUI App 的 RSS 增长
//! 这是定位 TUI 层内存大头的最关键测试
//!
//! 需用 `--features headless` 启用，因为 App::new_headless 是 headless feature gated

#![cfg(feature = "headless")]

use cc_agent::messages::BaseMessage;
use cc_tui::app::{events::AgentEvent, App};
use cc_tui::ui::main_ui;

mod common;
use common::current_rss_kb;

/// 探测默认开启：不设 `PERI_RSS_CONTENT_X` 时自动扫这些「每轮内容倍数」档，
/// 一次跑完即可得到「内容量 → 每轮常驻增量」的曲线与线性拟合。
/// 设了 `PERI_RSS_CONTENT_X` 则只跑该档并打印逐轮明细。
const DEFAULT_CONTENT_STEPS: [usize; 4] = [1, 8, 20, 30];

/// 跑一档测量（`mult` = 每轮内容倍数），返回该档「累计增量（KB）」。
///
/// `verbose` 为 true 时打印基线与逐轮明细；`leak` 为 true 时有意不析构 App 与渲染句柄
/// —— 保持该测试原本的语义：最后一档不触发 drop，避免析构开销干扰测量。
async fn measure_tier(mult: usize, rounds: usize, verbose: bool, leak: bool) -> usize {
    let (mut app, mut handle) = App::new_headless(120, 40).await;
    // 给渲染线程时间初始化
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let baseline = current_rss_kb();
    if verbose {
        println!(
            "基线 RSS（含 App + 渲染线程）: {} KB ({:.2} MB)",
            baseline,
            baseline as f64 / 1024.0
        );
        println!("| 轮 | RSS (KB) | 本轮+ (KB) | 累计+ (KB) | view_messages |");
        println!("|----|----------|------------|------------|---------------|");
    }

    let mut prev = baseline;
    for i in 0..rounds {
        // 模拟一轮：AssistantChunk（流式）+ StateSnapshot + Done
        let mut ai_text = format!(
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
        for k in 0..mult.saturating_sub(1) {
            ai_text.push_str(&format!(
                "\n\n### 附加段 {k}\n\n- 说明：第 {i} 轮第 {k} 段\n\n```rust\nfn extra_{k}(data: &[u8]) -> usize {{\n    data.iter().filter(|b| **b > 0).count()\n}}\n```\n"
            ));
        }

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
        if verbose && (i == 0 || (i + 1) % 3 == 0) {
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
    if verbose {
        println!("\n=== 完成 {rounds} 轮 ===");
        println!(
            "最终 RSS: {} KB ({:.2} MB)",
            final_rss,
            final_rss as f64 / 1024.0
        );
        println!("累计增长: {} KB ({:.2} MB)", total, total as f64 / 1024.0);
        println!("平均每轮: {:.2} KB", total as f64 / rounds as f64);
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
    }

    if leak {
        std::mem::forget(app);
        std::mem::forget(handle);
    }

    total
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn measure_headless_app_30_rounds_rss() {
    // 每档轮数（PERI_RSS_ROUNDS 覆盖，用于跑到缓存上限找平台期）
    let rounds: usize = std::env::var("PERI_RSS_ROUNDS")
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|v| *v > 0)
        .unwrap_or(30);
    // 内容倍数档位：默认全扫；显式指定则只跑一档（此时打印逐轮明细）
    let steps: Vec<usize> = match std::env::var("PERI_RSS_CONTENT_X")
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|v: &usize| *v > 0)
    {
        Some(x) => vec![x],
        None => DEFAULT_CONTENT_STEPS.to_vec(),
    };
    let verbose = steps.len() == 1;
    println!("\n=== Headless App 内存探测：每档 {rounds} 轮 ===");
    if steps.len() > 1 {
        println!("注意：同一进程内多档连跑，后面的档会复用前面档已提交的分配器页 → 数值偏低；");
        println!("      需要干净数值时用 PERI_RSS_CONTENT_X=<倍数> 单跑一档（新进程）。");
    }
    println!("| 内容倍数 | 累计增量 (KB) | 每轮 (KB) |");
    println!("|----------|---------------|-----------|");
    let mut points: Vec<(f64, f64)> = Vec::new();

    for (step_idx, &mult) in steps.iter().enumerate() {
        // 最后一档不析构（与原实现语义一致）；中间各档正常析构，保证每档从干净状态起跑
        let leak = step_idx + 1 == steps.len();
        let total = measure_tier(mult, rounds, verbose, leak).await;
        // 汇总一行（扫描模式下每档一行）
        println!(
            "| {:>7}× | {:>13} | {:>9.1} |",
            mult,
            total,
            total as f64 / rounds as f64
        );
        points.push((mult as f64, total as f64 / rounds as f64));
    }

    // 线性拟合：每轮增量 ≈ a + b × 内容倍数
    if points.len() >= 2 {
        let n = points.len() as f64;
        let sx: f64 = points.iter().map(|p| p.0).sum();
        let sy: f64 = points.iter().map(|p| p.1).sum();
        let sxx: f64 = points.iter().map(|p| p.0 * p.0).sum();
        let sxy: f64 = points.iter().map(|p| p.0 * p.1).sum();
        let denom = n * sxx - sx * sx;
        if denom.abs() > f64::EPSILON {
            let b = (n * sxy - sx * sy) / denom;
            let a = (sy - b * sx) / n;
            println!("\n拟合: 每轮 ≈ {a:.0} KB（固定）+ {b:.1} KB × 内容倍数");
            for (x, y) in &points {
                println!("  {x:.0}× 实测 {y:.1} KB / 拟合 {:.1} KB", a + b * x);
            }
        }
    }
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
