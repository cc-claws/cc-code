//! 精准模拟"单轮涨 32MB"路径：单次大工具结果在多个存储点的累积
//!
//! 假设用户场景：随便对话 1 次，其中 LLM 调用 Read 读取了一个大文件（5-10MB）
//! 或调用 Bash 跑了命令输出了大体积 stdout。
//!
//! 追踪路径（每步都是 deep clone，BaseMessage 无 Arc 共享）：
//! 1. 工具返回 → AgentState.messages 写入第 1 份
//! 2. StateSnapshot emit → agent_ops/mod.rs:287 extend 到 origin_messages（第 2 份）
//! 3. message_pipeline/mod.rs:1039 set_completed → pipeline.completed（第 3 份）
//! 4. messages_to_view_models → 渲染 Text<'static> 字符串（第 4 份）
//! 5. RenderCache 缓存（第 5 份）
//!
//! 5MB 工具结果 × 5 个存储 = 25MB RSS 增长，足以解释 +32MB 单轮暴涨。

#![cfg(unix)]

use peri_agent::messages::{BaseMessage, ContentBlock, MessageContent};

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

#[test]
fn large_toolresult_full_path_attribution() {
    println!("\n=== 单次大工具结果（5MB）经过 5 个存储点的真实 RSS 成本 ===\n");

    let baseline = current_rss_kb();
    let mut base = baseline;
    println!("[基线] RSS = {} KB ({:.2} MB)\n", base, mb(base));

    // === 5MB 大文件内容（模拟 Read 了一个 5MB 文件）===
    let large_content: String = std::iter::repeat_n("X", 5 * 1024 * 1024).collect();
    let base_with_string = current_rss_kb();
    println!(
        "[第 0 步：构造 5MB 原始字符串] RSS = {} KB (+{} KB = +{:.2} MB)",
        base_with_string,
        base_with_string.saturating_sub(base),
        mb(base_with_string.saturating_sub(base))
    );
    base = base_with_string;

    // === 第 1 份：写入 BaseMessage（ContentBlock::ToolResult）===
    let msg1 = BaseMessage::tool_result(
        "toolu_large_1".to_string(),
        MessageContent::text(large_content.clone()),
    );
    let after_msg1 = current_rss_kb();
    println!(
        "[第 1 步：BaseMessage::tool_result（AgentState 写入）] RSS = {} KB (+{} KB = +{:.2} MB)",
        after_msg1,
        after_msg1.saturating_sub(base),
        mb(after_msg1.saturating_sub(base))
    );
    base = after_msg1;

    // === 第 2 份：StateSnapshot → origin_messages extend（agent_ops/mod.rs:287）===
    let snapshot_msgs: Vec<BaseMessage> = vec![msg1.clone()];
    let mut origin_messages: Vec<BaseMessage> = Vec::new();
    origin_messages.extend(snapshot_msgs.iter().cloned());
    let after_origin = current_rss_kb();
    println!("[第 2 步：origin_messages.extend() — agent_ops/mod.rs:287] RSS = {} KB (+{} KB = +{:.2} MB)",
        after_origin, after_origin.saturating_sub(base), mb(after_origin.saturating_sub(base)));
    base = after_origin;

    // === 第 3 份：pipeline.completed extend（message_pipeline/mod.rs:1039）===
    let mut completed: Vec<BaseMessage> = Vec::new();
    completed.extend(snapshot_msgs.iter().cloned());
    let after_completed = current_rss_kb();
    println!("[第 3 步：pipeline.completed extend — message_pipeline/mod.rs:1039] RSS = {} KB (+{} KB = +{:.2} MB)",
        after_completed, after_completed.saturating_sub(base), mb(after_completed.saturating_sub(base)));
    base = after_completed;

    // === 第 4 份：view_messages 渲染（Text<'static>，含字符串 clone）===
    let view_strings: Vec<String> = completed.iter().map(|m| m.content().to_string()).collect();
    let after_view = current_rss_kb();
    println!(
        "[第 4 步：view_messages content() clone → 字符串] RSS = {} KB (+{} KB = +{:.2} MB)",
        after_view,
        after_view.saturating_sub(base),
        mb(after_view.saturating_sub(base))
    );
    base = after_view;

    // === 第 5 份：RenderCache 缓存（预渲染 ratatui Text<'static>）===
    let mut rendered_texts: Vec<String> = Vec::new();
    for s in &view_strings {
        // 模拟 markdown 渲染会复制字符串
        rendered_texts.push(s.clone());
        rendered_texts.push(s.clone()); // 第二份表示 paragraph + line 内部各自一份
    }
    let after_cache = current_rss_kb();
    println!(
        "[第 5 步：RenderCache 渲染 ×2（paragraph + line）] RSS = {} KB (+{} KB = +{:.2} MB)",
        after_cache,
        after_cache.saturating_sub(base),
        mb(after_cache.saturating_sub(base))
    );

    let total_delta = after_cache.saturating_sub(baseline);

    println!("\n=== 结论 ===");
    println!(
        "5MB 单次工具结果经过完整存储路径后，总 RSS 增长 = {:.2} MB",
        mb(total_delta)
    );

    drop(msg1);
    drop(snapshot_msgs);
    drop(origin_messages);
    drop(completed);
    drop(view_strings);
    drop(rendered_texts);
    drop(large_content);
    std::thread::sleep(std::time::Duration::from_millis(500));
    let after_drop = current_rss_kb();
    println!("drop 全部 + 等 500ms: RSS = {} KB", after_drop);
}

#[test]
fn varying_toolresult_size_scan() {
    //! 扫描不同 ToolResult 大小（1MB / 5MB / 10MB / 20MB）下的完整路径 RSS 成本
    println!("\n=== 不同 ToolResult 大小的完整存储路径成本扫描 ===\n");
    println!("| ToolResult 大小 | 第1份 | 第2份 | 第3份 | 第4份 | 第5份 | 总增长 | 倍率 |");
    println!("|-----------------|-------|-------|-------|-------|-------|--------|------|");

    for &size_mb in &[1usize, 5, 10, 20] {
        let mut base = current_rss_kb();
        let content: String = std::iter::repeat_n("X", size_mb * 1024 * 1024).collect();
        let _ = current_rss_kb();

        let msg = BaseMessage::tool_result(
            format!("toolu_{}", size_mb),
            MessageContent::text(content.clone()),
        );
        let s1 = current_rss_kb().saturating_sub(base);
        base = current_rss_kb();

        let origin: Vec<BaseMessage> = vec![msg.clone()];
        let s2 = current_rss_kb().saturating_sub(base);
        base = current_rss_kb();

        let completed: Vec<BaseMessage> = vec![msg.clone()];
        let s3 = current_rss_kb().saturating_sub(base);
        base = current_rss_kb();

        let view: Vec<String> = completed.iter().map(|m| m.content().to_string()).collect();
        let s4 = current_rss_kb().saturating_sub(base);
        base = current_rss_kb();

        let cache: Vec<String> = view
            .iter()
            .flat_map(|s| vec![s.clone(), s.clone()])
            .collect();
        let s5 = current_rss_kb().saturating_sub(base);
        base = current_rss_kb();

        let total = s1 + s2 + s3 + s4 + s5;
        let ratio = total as f64 / (size_mb * 1024) as f64;

        println!("| {:>10} MB | {:>4} KB | {:>4} KB | {:>4} KB | {:>4} KB | {:>4} KB | {:>5} KB | {:.1}x |",
            size_mb, s1, s2, s3, s4, s5, total, ratio);

        drop(content);
        drop(msg);
        drop(origin);
        drop(completed);
        drop(view);
        drop(cache);
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

#[test]
fn real_world_conversation_simulation() {
    //! 真实场景：用户对话 10 轮，其中 2 轮包含大工具结果
    println!("\n=== 真实场景：10 轮对话，含 2 次大工具调用 ===\n");

    let mut base = current_rss_kb();
    println!("基线 RSS: {} KB ({:.2} MB)\n", base, mb(base));

    let mut origin_messages: Vec<BaseMessage> = Vec::new();
    let mut completed: Vec<BaseMessage> = Vec::new();

    for round in 1..=10 {
        // 普通轮次：4 条小消息
        let mut round_msgs = vec![
            BaseMessage::human(format!("第 {round} 轮问题")),
            BaseMessage::ai(format!("第 {round} 轮回答，包含一些技术细节...")),
        ];

        // 第 3 轮和第 7 轮：包含 3MB 大工具结果
        if round == 3 || round == 7 {
            let large_content: String = std::iter::repeat_n("X", 3 * 1024 * 1024).collect();
            round_msgs.push(BaseMessage::ai_from_blocks(vec![ContentBlock::ToolUse {
                id: format!("toolu_big_{round}"),
                name: "Read".to_string(),
                input: serde_json::json!({ "file_path": format!("/tmp/big_{round}.log") }),
            }]));
            round_msgs.push(BaseMessage::tool_result(
                format!("toolu_big_{round}"),
                MessageContent::text(large_content),
            ));
            round_msgs.push(BaseMessage::ai(format!(
                "第 {round} 轮分析完成，文件很大。"
            )));
        }

        // 模拟双存储 extend
        origin_messages.extend(round_msgs.iter().cloned());
        completed.extend(round_msgs.iter().cloned());

        let rss = current_rss_kb();
        let delta = rss.saturating_sub(base);
        println!(
            "轮 {round:>2}: RSS = {} KB ({:>6.2} MB)  累计 +{} KB ({:.2} MB){}",
            rss,
            mb(rss),
            delta,
            mb(delta),
            if round == 3 || round == 7 {
                " ← 含 3MB 大文件"
            } else {
                ""
            }
        );
        base = rss;
    }

    println!("\n=== drop 全部后 RSS（看 jemalloc 持有） ===");
    drop(origin_messages);
    drop(completed);
    std::thread::sleep(std::time::Duration::from_millis(500));
    let after = current_rss_kb();
    println!("drop 后 RSS: {} KB ({:.2} MB)\n", after, mb(after));
}
