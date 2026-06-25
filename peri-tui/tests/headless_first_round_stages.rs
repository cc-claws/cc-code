//! 精细分割 TUI 首轮的各个阶段，找出每个操作的具体 RSS 增量
//! 100% 证明 syntect 加载发生在哪个具体调用点
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

#[cfg(unix)]
fn print_rss_delta(label: &str, baseline: &mut usize) {
    let now = current_rss_kb();
    let delta = now.saturating_sub(*baseline);
    println!(
        "    {:40} | RSS = {:7} KB | +{:6} KB ({:+.2} MB)",
        label,
        now,
        delta,
        delta as f64 / 1024.0
    );
    *baseline = now;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stage_breakdown_first_round() {
    println!("\n=== TUI 首轮各阶段 RSS 精细分割 ===\n");

    let mut baseline = current_rss_kb();
    println!(
        "初始 RSS: {} KB ({:.2} MB)",
        baseline,
        baseline as f64 / 1024.0
    );

    // ── 阶段 1：创建 headless App ──
    println!("\n[阶段 1] 创建 App::new_headless");
    let (mut app, mut handle) = App::new_headless(120, 40).await;
    print_rss_delta("App 创建完成", &mut baseline);

    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    print_rss_delta("等待 300ms（渲染线程初始化）", &mut baseline);

    // ── 阶段 2：第一个 AssistantChunk（触发首次 markdown 渲染）──
    println!("\n[阶段 2] 第一个 AssistantChunk");
    app.push_agent_event(AgentEvent::AssistantChunk {
        chunk: "## 分析开始\n\n我开始处理您的请求。".to_string(),
        source_agent_id: None,
    });
    print_rss_delta("push AssistantChunk（纯文本）", &mut baseline);

    app.process_pending_events();
    print_rss_delta("process_pending_events", &mut baseline);

    app.flush_rebuild();
    print_rss_delta("flush_rebuild（触发首次渲染）", &mut baseline);

    tokio::task::yield_now().await;
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    print_rss_delta("等待渲染线程工作 50ms", &mut baseline);

    let _ = handle.terminal.draw(|f| main_ui::render(f, &mut app));
    print_rss_delta("首次 terminal.draw", &mut baseline);

    // ── 阶段 3：含代码块的 chunk（触发 syntect 加载）──
    println!("\n[阶段 3] 含代码块的 chunk（应触发 syntect 懒加载）");
    let code_chunk =
        "查看这段代码：\n\n```rust\nfn example() -> i32 {\n    let x = 42;\n    x * 2\n}\n```\n";
    app.push_agent_event(AgentEvent::AssistantChunk {
        chunk: code_chunk.to_string(),
        source_agent_id: None,
    });
    print_rss_delta("push 含 ```rust 代码块 chunk", &mut baseline);

    app.process_pending_events();
    print_rss_delta("process_pending_events", &mut baseline);

    app.flush_rebuild();
    print_rss_delta("flush_rebuild", &mut baseline);

    tokio::task::yield_now().await;
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    print_rss_delta("等待 100ms", &mut baseline);

    let _ = handle.terminal.draw(|f| main_ui::render(f, &mut app));
    print_rss_delta("terminal.draw", &mut baseline);

    // ── 阶段 4：StateSnapshot + Done ──
    println!("\n[阶段 4] StateSnapshot + Done");
    let user_msg = BaseMessage::human("用户问题");
    let ai_msg = BaseMessage::ai(
        "## 分析\n\n```rust\nfn example() -> i32 {\n    let x = 42;\n    x * 2\n}\n```\n",
    );
    app.push_agent_event(AgentEvent::StateSnapshot(vec![user_msg, ai_msg]));
    print_rss_delta("push StateSnapshot", &mut baseline);

    app.push_agent_event(AgentEvent::Done);
    print_rss_delta("push Done", &mut baseline);

    app.process_pending_events();
    print_rss_delta("process_pending_events", &mut baseline);

    app.flush_rebuild();
    print_rss_delta("flush_rebuild", &mut baseline);

    tokio::task::yield_now().await;
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    print_rss_delta("等待 100ms", &mut baseline);

    let _ = handle.terminal.draw(|f| main_ui::render(f, &mut app));
    print_rss_delta("terminal.draw", &mut baseline);

    // ── 阶段 5：第二轮（应该几乎零成本）──
    println!("\n[阶段 5] 第二轮（验证已稳定）");
    for chunk in ["第二轮开始\n\n```python\nprint('hi')\n```\n", "结束"] {
        app.push_agent_event(AgentEvent::AssistantChunk {
            chunk: chunk.to_string(),
            source_agent_id: None,
        });
    }
    let user_msg2 = BaseMessage::human("用户问题 2");
    let ai_msg2 = BaseMessage::ai("第二轮回复\n\n```python\nprint('hi')\n```\n");
    app.push_agent_event(AgentEvent::StateSnapshot(vec![user_msg2, ai_msg2]));
    app.push_agent_event(AgentEvent::Done);
    app.process_pending_events();
    app.flush_rebuild();
    tokio::task::yield_now().await;
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    let _ = handle.terminal.draw(|f| main_ui::render(f, &mut app));
    print_rss_delta("第二轮完整流程", &mut baseline);

    println!("\n=== 总结 ===");
    let final_rss = current_rss_kb();
    let initial = baseline;
    println!(
        "本轮测试从 {} KB 涨到 {} KB（{:.2} MB）",
        initial,
        final_rss,
        (final_rss.saturating_sub(initial)) as f64 / 1024.0
    );

    std::mem::forget(app);
    std::mem::forget(handle);
}

/// 反证测试：在不创建 App 的情况下，直接调用 markdown 解析（含 syntect 触发）
/// 证明 12.59 MB 完全由 syntect 加载贡献
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn isolate_syntect_load_via_markdown() {
    use peri_widgets::markdown::{parse_markdown, DefaultMarkdownTheme, MarkdownTheme};

    println!("\n=== 反证：直接调 parse_markdown 测 syntect 加载 ===\n");
    let mut baseline = current_rss_kb();
    println!(
        "初始 RSS: {} KB ({:.2} MB)",
        baseline,
        baseline as f64 / 1024.0
    );

    let theme = DefaultMarkdownTheme;
    let md_simple = "# 简单标题\n\n这是纯文本，无代码块。\n";
    let _text1 = parse_markdown(md_simple, &theme as &dyn MarkdownTheme, 120);
    print_rss_delta("parse_markdown（纯文本，无代码块）", &mut baseline);

    let md_with_code = "# 含代码块\n\n```rust\nfn hello() {\n    println!(\"hi\");\n}\n```\n";
    let _text2 = parse_markdown(md_with_code, &theme as &dyn MarkdownTheme, 120);
    print_rss_delta("parse_markdown（含 ```rust 代码块）", &mut baseline);

    let md_with_python = "# Python\n\n```python\nprint('hi')\n```\n";
    let _text3 = parse_markdown(md_with_python, &theme as &dyn MarkdownTheme, 120);
    print_rss_delta("parse_markdown（含 ```python 代码块）", &mut baseline);

    let md_many_lang = "# 多语言\n\n```go\nfunc main() {{}}\n```\n```js\nconst x = 1;\n```\n```cpp\nint main() {{}}\n```\n";
    let _text4 = parse_markdown(md_many_lang, &theme as &dyn MarkdownTheme, 120);
    print_rss_delta("parse_markdown（多语言代码块）", &mut baseline);

    println!("\n=== 总结 ===");
    println!(
        "纯文本 markdown 解析: {:.2} MB",
        (current_rss_kb().saturating_sub(baseline)) as f64 / 1024.0
    );
    println!("首次代码块触发 syntect 加载: 见上方 +delta");

    std::mem::forget(_text1);
    std::mem::forget(_text2);
    std::mem::forget(_text3);
    std::mem::forget(_text4);
}
