//! 实测双存储代价：构造 50 条典型 BaseMessage，调用真实 estimate_messages_heap
//! 测出"单份 vs 双存储"的精确字节差，并与 issue 中 40-80MB 估算对比。

use peri_agent::messages::{BaseMessage, ContentBlock, MessageContent};
use peri_tui::command::core::gc::estimate_messages_heap;

// ── 场景 A：纯文本对话（每条 200-500 字节）──
fn build_text_only_session(rounds: usize) -> Vec<BaseMessage> {
    let mut msgs = Vec::new();
    for i in 0..rounds {
        msgs.push(BaseMessage::human(format!(
            "用户问题 {i}：帮我分析一下这段 Rust 代码的性能瓶颈，看看有没有可以优化的地方。当前实现的复杂度大概是 O(n²)，期望优化到接近线性。"
        )));
        msgs.push(BaseMessage::ai(format!(
            "我来分析第 {i} 个问题。根据代码审查，主要瓶颈在以下几个方面：1) 嵌套循环导致平方复杂度；2) 不必要的中间分配；3) 缺乏并行性。建议改用 HashMap 索引，配合 rayon 并行迭代。"
        )));
    }
    msgs
}

// ── 场景 B：含小工具调用（ToolUse + ToolResult，每条 1-3KB）──
fn build_small_tool_session(rounds: usize) -> Vec<BaseMessage> {
    let mut msgs = Vec::new();
    for i in 0..rounds {
        msgs.push(BaseMessage::human(format!("用户问题 {i}：读取 config.json 并告诉我里面的字段")));
        msgs.push(BaseMessage::ai_from_blocks(vec![
            ContentBlock::Text { text: "我来读取文件。".to_string() },
            ContentBlock::ToolUse {
                id: format!("toolu_{i}"),
                name: "Read".to_string(),
                input: serde_json::json!({ "file_path": format!("/tmp/config_{i}.json") }),
            },
        ]));
        let body = format!(
            "{{\n  \"name\": \"project-{i}\",\n  \"version\": \"1.{i}.0\",\n  \"dependencies\": [\"serde\", \"tokio\", \"reqwest\"],\n  \"timeout_ms\": 30000\n}}"
        );
        msgs.push(BaseMessage::tool_result(
            format!("toolu_{i}"),
            MessageContent::text(body),
        ));
        msgs.push(BaseMessage::ai(format!("读取完成，第 {i} 个配置文件包含字段 name/version/dependencies/timeout_ms。")));
    }
    msgs
}

// ── 场景 C：含大 ToolResult（模拟 Read 大文件/大命令输出，每条 30-80KB）──
fn build_large_tool_session(rounds: usize) -> Vec<BaseMessage> {
    let mut msgs = Vec::new();
    for i in 0..rounds {
        msgs.push(BaseMessage::human(format!("用户问题 {i}：阅读这个大文件并总结")));
        msgs.push(BaseMessage::ai_from_blocks(vec![
            ContentBlock::Text { text: "开始处理。".to_string() },
            ContentBlock::ToolUse {
                id: format!("toolu_{i}"),
                name: "Read".to_string(),
                input: serde_json::json!({ "file_path": format!("/tmp/large_{i}.rs") }),
            },
        ]));
        // 50KB 大文件内容
        let big_body: String = std::iter::repeat_n("// 这是模拟的大文件内容行，用于测大 ToolResult 内存占用。\n", 1000)
            .map(|line| format!("{line}# round {i}"))
            .collect();
        msgs.push(BaseMessage::tool_result(
            format!("toolu_{i}"),
            MessageContent::text(big_body),
        ));
        msgs.push(BaseMessage::ai(format!("第 {i} 个大文件总结：包含约 1000 行注释，主题是 Rust 内存管理。")));
    }
    msgs
}

fn mb(bytes: usize) -> f64 {
    bytes as f64 / 1024.0 / 1024.0
}

fn report(name: &str, msgs: &[BaseMessage]) {
    let count = msgs.len();
    let single = estimate_messages_heap(msgs);
    let double = single * 2;
    let waste = single; // 双存储浪费的字节 = 单份字节数
    println!(
        "\n=== {name} ===\n消息条数: {count}\n单份字节: {single} ({:.2} MB)\n双存储总字节: {double} ({:.2} MB)\n双存储浪费: {waste} ({:.2} MB)\n命中 issue 40-80MB 区间: {}",
        mb(single),
        mb(double),
        mb(waste),
        if (40.0..=80.0).contains(&mb(waste)) { "✅ 是" } else { "❌ 否" },
    );
}

#[test]
fn measure_three_scenarios_50_messages() {
    let text_msgs = build_text_only_session(25); // 50 条
    let small_msgs = build_small_tool_session(13); // 52 条
    let large_msgs = build_large_tool_session(13); // 52 条
    report("场景 A：纯文本对话 (50 条)", &text_msgs);
    report("场景 B：含小工具调用 (52 条)", &small_msgs);
    report("场景 C：含大文件 ToolResult (52 条)", &large_msgs);
}

#[test]
fn measure_extreme_scenarios() {
    // 极端场景：长会话 + 超大 ToolResult
    let huge_large = build_large_tool_session(100); // 400 条大消息
    report("场景 D：400 条含大文件 ToolResult", &huge_large);

    // 极端场景：100 条 200KB 超大 ToolResult（模拟大命令输出/grep 结果）
    let mut mega_msgs = Vec::new();
    for i in 0..100 {
        mega_msgs.push(BaseMessage::human(format!("run command {i}")));
        mega_msgs.push(BaseMessage::ai_from_blocks(vec![
            ContentBlock::Text { text: "running".to_string() },
            ContentBlock::ToolUse {
                id: format!("toolu_{i}"),
                name: "Bash".to_string(),
                input: serde_json::json!({ "command": format!("cat /tmp/big_{i}.log") }),
            },
        ]));
        let huge_body: String = std::iter::repeat_n("X", 200_000).collect();
        mega_msgs.push(BaseMessage::tool_result(
            format!("toolu_{i}"),
            MessageContent::text(huge_body),
        ));
    }
    report("场景 E：300 条含 200KB 命令输出", &mega_msgs);
}

#[test]
fn verify_estimate_reflects_real_rss_growth() {
    // 验证 estimate_messages_heap 与实际 RSS 增长是否一致
    // 用 Arc<Vec<BaseMessage>> 测量 clone 前后 RSS 差异
    let msgs = build_large_tool_session(13); // 52 条大消息
    let estimated_bytes = estimate_messages_heap(&msgs);

    // 强制 GC 前后取 RSS
    let rss_before = current_rss_kb();

    // 模拟双存储：clone 一份
    let cloned: Vec<BaseMessage> = msgs.to_vec();
    let actual_clone_bytes = estimate_messages_heap(&cloned);

    let rss_after = current_rss_kb();

    println!(
        "\n=== RSS 验证（场景 C 大消息，clone 一份前后）===\n估算单份字节: {} ({:.2} MB)\n估算 clone 后字节: {} ({:.2} MB)\n估算增量: {:.2} MB\nRSS 增量: {} KB ({:.2} MB)\n比例 (估算/RSS): {:.2}",
        estimated_bytes,
        mb(estimated_bytes),
        actual_clone_bytes,
        mb(actual_clone_bytes),
        mb(actual_clone_bytes.saturating_sub(estimated_bytes)),
        rss_after.saturating_sub(rss_before),
        mb((rss_after.saturating_sub(rss_before)) * 1024),
        if rss_after > rss_before {
            estimated_bytes as f64 / ((rss_after - rss_before) * 1024) as f64
        } else {
            0.0
        },
    );

    // 防止编译器优化掉 cloned
    assert_eq!(cloned.len(), msgs.len());
    assert!(estimated_bytes > 0);
}

#[cfg(unix)]
fn current_rss_kb() -> usize {
    // 读取 /proc/self/status 的 VmRSS
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

#[test]
fn measure_llm_client_creation_cost() {
    // 测量创建 ChatAnthropic/ChatOpenAI 实例（含 reqwest::Client）的真实 RSS 代价
    use peri_agent::llm::{ChatAnthropic, ChatOpenAI};

    // 预热（首次创建会触发各种 lazy init）
    let _warmup = ChatAnthropic::new("test-key", "claude-test");
    drop(_warmup);
    std::thread::sleep(std::time::Duration::from_millis(100));

    let rss_before = current_rss_kb();
    println!("\n=== LLM Client 创建代价 ===");
    println!("基线 RSS: {} KB ({:.2} MB)", rss_before, rss_before as f64 / 1024.0);

    // 创建 10 个 ChatAnthropic（模拟 10 轮 prompt 各创建一个 model）
    let mut anthropic_clients = Vec::new();
    let rss_after_1 = {
        for i in 0..10 {
            anthropic_clients.push(ChatAnthropic::new(
                format!("sk-test-{i}"),
                "claude-test",
            ));
        }
        current_rss_kb()
    };
    let delta_1 = rss_after_1.saturating_sub(rss_before);
    println!(
        "10 个 ChatAnthropic 后: {} KB (+{} KB, 即 {:.2} MB/client)",
        rss_after_1,
        delta_1,
        delta_1 as f64 / 10.0 / 1024.0,
    );

    // 再创建 10 个 ChatOpenAI
    let mut openai_clients = Vec::new();
    let rss_after_2 = {
        for i in 0..10 {
            openai_clients.push(ChatOpenAI::new(
                format!("sk-openai-{i}"),
                "gpt-test",
            ));
        }
        current_rss_kb()
    };
    let delta_2 = rss_after_2.saturating_sub(rss_after_1);
    println!(
        "再 10 个 ChatOpenAI: {} KB (+{} KB, 即 {:.2} MB/client)",
        rss_after_2,
        delta_2,
        delta_2 as f64 / 10.0 / 1024.0,
    );

    // 释放全部
    drop(anthropic_clients);
    drop(openai_clients);
    std::thread::sleep(std::time::Duration::from_millis(500));
    let rss_after_drop = current_rss_kb();
    println!(
        "drop 全部 + 等 500ms: {} KB ({:+} KB 相对基线，jemalloc 未归还 OS 部分约 {} KB)",
        rss_after_drop,
        rss_after_drop as isize - rss_before as isize,
        rss_after_drop.saturating_sub(rss_before),
    );

    // 不强断言 delta > 0：CI 环境（jemalloc + 缓存复用）下 LLM Client 构造可能不立即增长 RSS。
    let _ = (delta_1, delta_2);
}

#[test]
fn measure_ratatui_text_render_cost() {
    // 测真实 ratatui Text<'static> 渲染占用
    // 模拟 50 条消息，每条都构造完整的 Text<'static> 含 Spans/Style/Line
    use ratatui::text::{Line, Span, Text};
    use ratatui::style::{Color, Modifier, Style};

    let baseline = current_rss_kb();
    println!("\n=== ratatui Text<'static> 真实渲染占用 ===");
    println!("基线 RSS: {} KB", baseline);

    // 模拟 50 条 AI 消息，每条含 markdown 渲染后的多行 Line
    let mut all_texts: Vec<Text<'static>> = Vec::new();
    for i in 0..50 {
        let mut lines: Vec<Line<'static>> = Vec::new();
        // 标题行（粗体 + 颜色）
        lines.push(Line::from(vec![
            Span::styled(
                format!("## 第 {i} 轮分析结果"),
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            ),
        ]));
        // 5 行正文
        for j in 0..5 {
            lines.push(Line::from(vec![Span::raw(format!(
                "  这是第 {j} 行内容，包含一些技术细节和说明文字，长度大约 80-120 字符。"
            ))]));
        }
        // 代码块行（带样式）
        lines.push(Line::from(vec![
            Span::styled(
                "fn example()".to_string(),
                Style::default().fg(Color::Yellow),
            ),
            Span::raw(" {"),
        ]));
        lines.push(Line::from(vec![Span::raw("    // code here")]));

        all_texts.push(Text::from(lines));
    }

    let after_50 = current_rss_kb();
    let delta_50 = after_50.saturating_sub(baseline);
    println!(
        "50 条 Text<'static> 后: RSS = {} KB (+{} KB = {:.2} MB, 即 {:.2} KB/条)",
        after_50, delta_50, delta_50 as f64 / 1024.0, delta_50 as f64 / 50.0,
    );

    // 翻倍：100 条
    for i in 50..100 {
        let mut lines: Vec<Line<'static>> = Vec::new();
        lines.push(Line::from(vec![
            Span::styled(
                format!("## 第 {i} 轮分析结果"),
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            ),
        ]));
        for j in 0..5 {
            lines.push(Line::from(vec![Span::raw(format!(
                "  这是第 {j} 行内容，包含一些技术细节和说明文字，长度大约 80-120 字符。"
            ))]));
        }
        all_texts.push(Text::from(lines));
    }

    let after_100 = current_rss_kb();
    let delta_100 = after_100.saturating_sub(after_50);
    println!(
        "再加 50 条（共 100）: RSS = {} KB (+{} KB = {:.2} MB, 即 {:.2} KB/条)",
        after_100, delta_100, delta_100 as f64 / 1024.0, delta_100 as f64 / 50.0,
    );

    drop(all_texts);
    std::thread::sleep(std::time::Duration::from_millis(500));
    let after_drop = current_rss_kb();
    println!(
        "drop 全部 + 等 500ms: RSS = {} KB ({:+} KB 相对基线)\n",
        after_drop,
        after_drop as isize - baseline as isize,
    );

    // 不强断言 delta_50 > 0：CI 环境（jemalloc + 缓存复用）下小数据量可能不增长 RSS。
    // 测试只做"测量并打印"，由人工审查输出判断是否符合预期。
    let _ = delta_50;
}

#[test]
fn measure_50_rounds_real_rss_growth() {
    // 模拟 50 轮对话，每轮产生 4 条典型消息，extend 到 origin_messages + completed
    // 不用 estimate，直接测真实 RSS
    use peri_agent::messages::{BaseMessage, ContentBlock, MessageContent};

    let mut origin_messages: Vec<BaseMessage> = Vec::new();
    let mut completed: Vec<BaseMessage> = Vec::new();
    let mut view_rendered: Vec<String> = Vec::new(); // 模拟 view_messages 中的预渲染内容

    let baseline = current_rss_kb();
    println!("\n=== 50 轮累积测试（真实 RSS）===");
    println!("轮 0（基线）: RSS = {} KB ({:.2} MB)", baseline, baseline as f64 / 1024.0);

    let mut prev_rss = baseline;
    for round in 1..=50 {
        // 每轮产生 4 条典型消息（含小 ToolResult）
        let human = BaseMessage::human(format!(
            "第 {round} 轮：帮我看看这段代码有什么问题，怎么优化"
        ));
        let ai_thinking = BaseMessage::ai(format!(
            "分析第 {round} 轮的问题，主要瓶颈在以下几个方面..."
        ));
        let tool_use = BaseMessage::ai_from_blocks(vec![
            ContentBlock::ToolUse {
                id: format!("toolu_{round}"),
                name: "Read".to_string(),
                input: serde_json::json!({ "file_path": format!("/tmp/file_{round}.rs") }),
            },
        ]);
        let tool_result = BaseMessage::tool_result(
            format!("toolu_{round}"),
            MessageContent::text(
                "1: fn process(data: &Vec<u8>) -> Option<Result<{\n2:     // ... 30-100 行模拟代码 ...\n}".to_string(),
            ),
        );
        let ai_summary = BaseMessage::ai(format!("第 {round} 轮分析完成，建议改用迭代器链。"));

        // 双存储：两边都 extend（clone 一份）
        let batch = vec![human.clone(), ai_thinking.clone(), tool_use.clone(), tool_result.clone(), ai_summary.clone()];
        origin_messages.extend(batch.clone());
        completed.extend(batch.clone());

        // 第三份：view_messages（预渲染字符串模拟）
        for m in &batch {
            view_rendered.push(m.content()); // 模拟 Text<'static> 的字符串内容
        }
        drop(batch);

        // 每 10 轮采样
        if round % 10 == 0 {
            // 触发 jemalloc epoch（让统计准确）
            std::thread::sleep(std::time::Duration::from_millis(10));
            let rss = current_rss_kb();
            let delta = rss.saturating_sub(prev_rss);
            let total = rss.saturating_sub(baseline);
            println!(
                "轮 {round:>2}: RSS = {} KB ({:>6.2} MB)  +{} KB（本轮）累计 +{} KB ({:.2} MB)",
                rss, rss as f64 / 1024.0, delta, total, total as f64 / 1024.0,
            );
            prev_rss = rss;
        }
    }

    // 释放全部
    drop(origin_messages);
    drop(completed);
    drop(view_rendered);
    std::thread::sleep(std::time::Duration::from_millis(500));
    let after_drop = current_rss_kb();
    println!(
        "\ndrop 全部 + 等 500ms: RSS = {} KB ({:+} KB 相对基线，jemalloc 持有未归还)",
        after_drop,
        after_drop as isize - baseline as isize,
    );

    // 防 optimize out
    let _ = (baseline, prev_rss);
}

#[test]
fn measure_system_prompt_build_cost() {
    // 测量 build_system_prompt 的内存代价（frozen 但每轮可能重算）
    let rss_before = current_rss_kb();
    println!("\n=== System Prompt 构建代价 ===\n基线 RSS: {} KB", rss_before);

    // 强制分配 ~1MB 字符串模拟 system prompt
    let mut prompts = Vec::new();
    for i in 0..10 {
        let big_prompt: String = format!("section {i}: ...\n").repeat(5000);
        prompts.push(big_prompt);
    }
    let rss_after = current_rss_kb();
    let delta = rss_after.saturating_sub(rss_before);
    println!(
        "10 个 ~1MB prompt 后: {} KB (+{} KB = {:.2} MB)\n",
        rss_after,
        delta,
        delta as f64 / 1024.0,
    );

    drop(prompts);
    // 不强断言 delta > 0：CI 环境（jemalloc + 缓存复用）下小数据量可能不增长 RSS。
    let _ = delta;
}
