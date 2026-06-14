//! 用 /proc/self/status + 单独 jemalloc epoch 测 syntect 精确分配字节
//! 避开 peri alloc_config 的 sysinfo bug

#![cfg(unix)]

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

fn current_vsize_kb() -> usize {
    if let Ok(status) = std::fs::read_to_string("/proc/self/status") {
        for line in status.lines() {
            if let Some(rest) = line.strip_prefix("VmSize:") {
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

/// Advance jemalloc epoch（必需，否则 stats 不刷新）
fn advance_epoch() {
    let _ = tikv_jemalloc_ctl::epoch::advance();
}

/// 直接调用 jemalloc ctl 读 stats
fn jemalloc_stats() -> Option<(usize, usize, usize, usize)> {
    advance_epoch();
    let allocated = tikv_jemalloc_ctl::stats::allocated::read().ok()?;
    let active = tikv_jemalloc_ctl::stats::active::read().ok()?;
    let resident = tikv_jemalloc_ctl::stats::resident::read().ok()?;
    let metadata = tikv_jemalloc_ctl::stats::metadata::read().ok()?;
    Some((allocated, active, resident, metadata))
}

fn print_stats(label: &str, baseline_rss: &mut usize, baseline_alloc: &mut usize) {
    let rss = current_rss_kb();
    let vsz = current_vsize_kb();
    let (alloc, active, resident, metadata) = jemalloc_stats().unwrap_or((0, 0, 0, 0));
    let rss_delta = rss.saturating_sub(*baseline_rss);
    let alloc_delta = alloc.saturating_sub(*baseline_alloc);
    println!("\n[{}]", label);
    println!("  VmRSS:        {:>8} KB ({:6.2} MB) | +{:6} KB ({:+.2} MB)",
        rss, rss as f64 / 1024.0, rss_delta, rss_delta as f64 / 1024.0);
    println!("  VmSize:       {:>8} KB ({:6.2} MB)", vsz, vsz as f64 / 1024.0);
    println!("  jemalloc allocated: {:>8} KB ({:6.2} MB) | +{:6} KB ({:+.2} MB)",
        alloc / 1024, alloc as f64 / 1024.0 / 1024.0,
        alloc_delta / 1024, alloc_delta as f64 / 1024.0 / 1024.0);
    println!("  jemalloc active:    {:>8} KB ({:6.2} MB)",
        active / 1024, active as f64 / 1024.0 / 1024.0);
    println!("  jemalloc resident:  {:>8} KB ({:6.2} MB)",
        resident / 1024, resident as f64 / 1024.0 / 1024.0);
    println!("  jemalloc metadata:  {:>8} KB ({:6.2} MB)",
        metadata / 1024, metadata as f64 / 1024.0 / 1024.0);
    *baseline_rss = rss;
    *baseline_alloc = alloc;
}

#[test]
fn measure_syntect_with_jemalloc_precise() {
    use peri_widgets::markdown::{parse_markdown, DefaultMarkdownTheme, MarkdownTheme};

    println!("\n=== jemalloc 精确测量 syntect 加载 ===");

    let mut rss_base = current_rss_kb();
    let mut alloc_base = 0usize;
    if let Some((a, _, _, _)) = jemalloc_stats() {
        alloc_base = a;
    }
    print_stats("基线（test 进程已加载 tokio runtime）", &mut rss_base, &mut alloc_base);

    let theme = DefaultMarkdownTheme;
    let _t1 = parse_markdown(
        "# 简单\n\n纯文本，无代码块。",
        &theme as &dyn MarkdownTheme, 120);
    print_stats("纯文本 markdown（无代码块）", &mut rss_base, &mut alloc_base);

    let _t2 = parse_markdown(
        "# 含代码\n\n```rust\nfn hello() {\n    println!(\"hi\");\n}\n```\n",
        &theme as &dyn MarkdownTheme, 120);
    print_stats("含 ```rust 代码块（syntect 已加载）", &mut rss_base, &mut alloc_base);

    let _t3 = parse_markdown(
        "# 多语言\n\n```python\nprint('a')\n```\n```go\nfunc main() {{}}\n```\n",
        &theme as &dyn MarkdownTheme, 120);
    print_stats("更多代码块（验证稳定）", &mut rss_base, &mut alloc_base);

    println!("\n=== 结论 ===");
    println!("syntect 加载贡献（jemalloc allocated）= 步骤 3 的 allocated - 基线");
    println!("syntect 加载贡献（RSS 增量）= 步骤 3 的 RSS - 基线");

    std::mem::forget(_t1);
    std::mem::forget(_t2);
    std::mem::forget(_t3);
}

/// 验证：调 jemalloc epoch 但 peri-tui 没设全局 allocator 时，
/// jemalloc stats 是否有效（无效说明测试 binary 不用 jemalloc）
#[test]
fn verify_jemalloc_is_global_allocator() {
    println!("\n=== 验证 test binary 是否使用 jemalloc ===");
    let stats1 = jemalloc_stats();
    println!("初始 jemalloc stats: {:?}", stats1);

    // 分配 10 MB
    let big: Vec<u8> = vec![0; 10 * 1024 * 1024];
    let stats2 = jemalloc_stats();
    println!("分配 10MB 后 jemalloc stats: {:?}", stats2);

    // 释放
    drop(big);
    advance_epoch();
    let stats3 = jemalloc_stats();
    println!("释放后 jemalloc stats: {:?}", stats3);

    if let (Some(s1), Some(s2)) = (stats1, stats2) {
        let delta = s2.0.saturating_sub(s1.0);
        println!("\n分配 10MB 后 allocated 增量: {} KB ({:.2} MB)",
            delta / 1024, delta as f64 / 1024.0 / 1024.0);
        if delta > 5 * 1024 * 1024 {
            println!("✓ jemalloc 是全局 allocator（增量 > 5MB）");
        } else if delta > 0 {
            println!("⚠ jemalloc stats 部分有效（增量 {}）", delta);
        } else {
            println!("✗ jemalloc stats 无效（增量 0）→ test binary 使用系统 malloc");
        }
    }
}
