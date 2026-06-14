//! 实测 TUI MessagePipeline 多轮真实 RSS 增长
//!
//! 模拟真实 50 轮对话的事件流：
//! 每轮 = AssistantChunk（流式文本）+ StateSnapshot（消息快照）+ Done
//! 测量每轮 RSS 增长，定位 TUI 层是否为内存大头

use peri_agent::messages::BaseMessage;
use peri_tui::app::events::AgentEvent;
use peri_tui::app::message_pipeline::{MessagePipeline, PipelineAction};

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

#[cfg(not(unix))]
fn current_rss_kb() -> usize {
    0
}

/// 模拟一轮 AI 回复事件流：流式文本 → StateSnapshot → Done
fn feed_one_round(
    pipeline: &mut MessagePipeline,
    round_idx: usize,
    ai_text: &str,
) -> Vec<PipelineAction> {
    let mut all_actions = Vec::new();

    // begin_round (App 在 submit_message 调用)
    pipeline.begin_round();

    // 模拟流式 chunk（每 100 字节一个 chunk，共 10 个）
    let chunk_size = 100;
    let chunks: Vec<&str> = ai_text.as_bytes().chunks(chunk_size).map(|b| {
        std::str::from_utf8(b).unwrap_or("")
    }).collect();
    for chunk in chunks {
        if chunk.is_empty() {
            continue;
        }
        let actions = pipeline.handle_event(AgentEvent::AssistantChunk {
            chunk: chunk.to_string(),
            source_agent_id: None,
        });
        all_actions.extend(actions);
    }

    // StateSnapshot：携带本轮 user + assistant 消息（与真实流一致）
    let user_msg = BaseMessage::human(format!("第 {round_idx} 轮用户问题"));
    let ai_msg = BaseMessage::ai(ai_text.to_string());
    let actions = pipeline.handle_event(AgentEvent::StateSnapshot(vec![user_msg, ai_msg]));
    all_actions.extend(actions);

    // Done
    pipeline.done();

    all_actions
}

#[test]
fn measure_pipeline_50_rounds_rss() {
    let mut pipeline = MessagePipeline::new("/tmp".to_string());
    let baseline = current_rss_kb();

    println!("\n=== TUI MessagePipeline 50 轮真实 RSS 增长 ===");
    println!("基线 RSS: {} KB ({:.2} MB)", baseline, baseline as f64 / 1024.0);
    println!("| 轮 | RSS (KB) | 本轮+ (KB) | 累计+ (KB) | actions 总数 |");
    println!("|----|----------|------------|------------|--------------|");

    let mut prev = baseline;
    let mut max_actions = 0usize;
    for i in 0..50 {
        // 真实 AI 回复：含 markdown 代码块、列表、标题，2-5KB
        let ai_text = format!(
            r#"## 第 {i} 轮分析

我已分析您的问题，结论如下：

### 主要发现

1. **性能问题**：嵌套循环导致 O(n²) 复杂度
2. **可读性**：变量命名不够清晰
3. **错误处理**：unwrap() 可能 panic

### 修复建议

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

### 总结

第 {i} 轮优化完成，预计性能提升 5-10 倍。
"#
        );

        let actions = feed_one_round(&mut pipeline, i, &ai_text);
        let action_count = actions.len();
        max_actions = max_actions.max(action_count);
        // 持有 actions 直到下一轮（模拟 view_messages 累积）
        // 注意：真实 TUI 会把 actions 应用到 view_messages 并保留

        let rss = current_rss_kb();
        let delta = rss.saturating_sub(prev);
        let total = rss.saturating_sub(baseline);
        if i == 0 || (i + 1) % 5 == 0 {
            println!("| {:2} | {:8} | {:10} | {:10} | {:12} |",
                i + 1, rss, delta, total, action_count);
            prev = rss;
        }
        // 模拟 view_messages 累积：保留 actions
        std::mem::forget(actions);
    }

    let final_rss = current_rss_kb();
    let total = final_rss.saturating_sub(baseline);
    println!("\n=== 完成 50 轮 ===");
    println!("最终 RSS: {} KB", final_rss);
    println!("累计增长: {} KB ({:.2} MB)", total, total as f64 / 1024.0);
    println!("平均每轮: {:.2} KB", total as f64 / 50.0);
    println!("最大单轮 actions 数: {}", max_actions);
    let (completed_count, completed_bytes) = pipeline.completed_stats();
    println!("pipeline.completed: {} 条, {} bytes", completed_count, completed_bytes);
}

#[test]
fn measure_pipeline_with_view_messages_50_rounds() {
    // 更真实：累积 view_messages（模拟真实 TUI 行为）
    let mut pipeline = MessagePipeline::new("/tmp".to_string());
    let mut view_messages: Vec<peri_tui::ui::message_view::MessageViewModel> = Vec::new();
    let baseline = current_rss_kb();

    println!("\n=== MessagePipeline + view_messages 累积（真实 TUI 模式） ===");
    println!("基线 RSS: {} KB ({:.2} MB)", baseline, baseline as f64 / 1024.0);
    println!("| 轮 | RSS (KB) | 累计+ (KB) | view_messages len |");
    println!("|----|----------|------------|-------------------|");

    for i in 0..50 {
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
"#
        );

        let actions = feed_one_round(&mut pipeline, i, &ai_text);

        // 应用 actions 到 view_messages（模拟真实 TUI）
        for action in actions {
            match action {
                PipelineAction::AddMessage(vm) => {
                    view_messages.push(vm);
                }
                PipelineAction::RebuildAll { prefix_len, tail_vms } => {
                    // 重建：保留前缀，替换尾部
                    view_messages.truncate(prefix_len);
                    view_messages.extend(tail_vms);
                }
                PipelineAction::None => {}
            }
        }

        let rss = current_rss_kb();
        let total = rss.saturating_sub(baseline);
        if i == 0 || (i + 1) % 5 == 0 {
            println!("| {:2} | {:8} | {:10} | {:17} |",
                i + 1, rss, total, view_messages.len());
        }
    }

    let final_rss = current_rss_kb();
    let total = final_rss.saturating_sub(baseline);
    println!("\n=== 完成 50 轮（含 view_messages 累积） ===");
    println!("最终 RSS: {} KB", final_rss);
    println!("累计增长: {} KB ({:.2} MB)", total, total as f64 / 1024.0);
    println!("平均每轮: {:.2} KB", total as f64 / 50.0);
    println!("view_messages len: {}", view_messages.len());
    let (completed_count, completed_bytes) = pipeline.completed_stats();
    println!("pipeline.completed: {} 条, {} bytes", completed_count, completed_bytes);

    // 持有 view_messages 防止 drop
    std::mem::forget(view_messages);
    std::mem::forget(pipeline);

    let after_forget = current_rss_kb();
    println!("(持有未释放，RSS 应保持在 {} KB)", after_forget);
}
