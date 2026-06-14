//! 测真实 markdown 渲染 + 缓存占用的 RSS 代价
//! 这是验证"MarkdownCache 是大头"的关键测试

#![cfg(feature = "markdown")]

use peri_widgets::markdown::{cache::MarkdownCache, parse_markdown, DefaultMarkdownTheme};

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

#[test]
fn measure_markdown_render_real_cost() {
    let theme = DefaultMarkdownTheme;
    let baseline = current_rss_kb();
    println!("\n=== Markdown 渲染真实 RSS 占用 ===");
    println!("基线 RSS: {} KB ({:.2} MB)", baseline, baseline as f64 / 1024.0);

    // 模拟 50 条典型 AI 回复（含代码块 + 列表 + 标题）
    let mut all_texts: Vec<_> = Vec::new();
    for i in 0..50 {
        let markdown = format!(
            r#"## 第 {i} 轮分析结果

以下是第 {i} 轮的代码审查结论：

### 主要问题

1. **性能瓶颈**：嵌套循环导致 O(n²) 复杂度
2. **内存浪费**：每次迭代都 clone 整个 Vec
3. **错误处理**：unwrap() 可能 panic

### 建议修复

```rust
fn optimized(data: &[u8]) -> Option<Result<()>> {{
    // 用 HashMap 索引避免 O(n²)
    let mut index: HashMap<u8, usize> = HashMap::new();
    for (i, &b) in data.iter().enumerate() {{
        if let Some(&prev) = index.get(&b) {{
            return Some(Ok((prev, i)));
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
        let text = parse_markdown(&markdown, &theme, 120);
        all_texts.push(text);
    }

    let after_50 = current_rss_kb();
    let delta_50 = after_50.saturating_sub(baseline);
    println!(
        "50 条 markdown 渲染后: RSS = {} KB (+{} KB = {:.2} MB, 即 {:.2} KB/条)",
        after_50, delta_50, delta_50 as f64 / 1024.0, delta_50 as f64 / 50.0,
    );

    // 填满到 1024 条（缓存容量上限）
    for i in 50..1024 {
        let markdown = format!(
            "## 第 {i} 轮\n\n简短内容 {i}：包含一些技术说明和代码片段\n\n```rust\nfn x() -> i32 {{ {i} }}\n```\n"
        );
        let text = parse_markdown(&markdown, &theme, 120);
        all_texts.push(text);
    }

    let after_1024 = current_rss_kb();
    let delta_1024 = after_1024.saturating_sub(after_50);
    println!(
        "再加 974 条（共 1024，达到缓存上限）: RSS = {} KB (+{} KB = {:.2} MB, 即 {:.2} KB/条)",
        after_1024, delta_1024, delta_1024 as f64 / 1024.0, delta_1024 as f64 / 974.0,
    );

    drop(all_texts);
    std::thread::sleep(std::time::Duration::from_millis(500));
    let after_drop = current_rss_kb();
    println!(
        "drop 全部 + 等 500ms: RSS = {} KB ({:+} KB 相对基线)\n",
        after_drop,
        after_drop as isize - baseline as isize,
    );
}

#[test]
fn measure_markdown_cache_full_1024_entries() {
    // 模拟 TUI 实际行为：通过 MarkdownCache 全局单例填充 1024 条
    // 这接近真实 TUI 长会话的内存占用
    let cache = MarkdownCache::global();
    let theme = DefaultMarkdownTheme;
    let baseline = current_rss_kb();
    println!("\n=== MarkdownCache 全局单例填满 1024 条 ===");
    println!("基线 RSS: {} KB", baseline);
    println!("cache cap: {}", cache.capacity());

    // 先 clear
    cache.clear();

    // 填充 1024 条不同内容（模拟 1024 个不同消息）
    for i in 0..1024 {
        let markdown = format!(
            "## 消息 {i}\n\n内容 {i}：这是一段相对真实的 AI 回复，包含一些技术说明、列表和代码片段。\n\n- 列表项 1\n- 列表项 2\n\n```rust\nfn example_{i}() -> i32 {{\n    let x = {i};\n    x * 2\n}}\n```\n"
        );
        // 渲染并放入 cache
        let text = parse_markdown(&markdown, &theme, 120);
        cache.put(&markdown, 120, text);
    }

    let after_fill = current_rss_kb();
    let delta = after_fill.saturating_sub(baseline);
    println!(
        "填满 1024 条后: RSS = {} KB (+{} KB = {:.2} MB)",
        after_fill, delta, delta as f64 / 1024.0,
    );
    println!("cache 当前 len: {}", cache.len());

    cache.clear();
    std::thread::sleep(std::time::Duration::from_millis(500));
    let after_clear = current_rss_kb();
    println!(
        "cache.clear() + 等 500ms: RSS = {} KB ({:+} KB 相对基线)\n",
        after_clear,
        after_clear as isize - baseline as isize,
    );

    // 不强断言 delta > 0：CI 环境（jemalloc + 缓存复用）下小数据量可能不增长 RSS。
    let _ = delta;
}
