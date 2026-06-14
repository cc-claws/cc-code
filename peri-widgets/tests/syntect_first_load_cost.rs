//! 测 syntect SyntaxSet::load_defaults_newlines() 的真实 RSS 代价
//! 验证这是 TUI 首轮暴涨 +12MB 的根因

#![cfg(feature = "markdown-highlight")]

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
fn measure_syntaxset_load_cost() {
    let baseline = current_rss_kb();
    println!("\n=== syntect SyntaxSet::load_defaults_newlines() 真实成本 ===");
    println!("基线 RSS: {} KB ({:.2} MB)", baseline, baseline as f64 / 1024.0);

    // 单独加载 SyntaxSet（通过 peri_widgets::markdown::highlight 的公开静态变量）
    use peri_widgets::markdown::highlight::{SYNTAX_SET, THEME_SET};
    let ss = &*SYNTAX_SET;
    let after_ss = current_rss_kb();
    let delta_ss = after_ss.saturating_sub(baseline);
    println!("\n[1] SYNTAX_SET 懒加载（load_defaults_newlines）");
    println!("    加载后 RSS: {} KB (+{} KB = {:.2} MB)",
        after_ss, delta_ss, delta_ss as f64 / 1024.0);
    println!("    SyntaxSet 加载的语言数: {}", ss.syntaxes().len());

    // 再加载 ThemeSet
    let _ts = &*THEME_SET;
    let after_ts = current_rss_kb();
    let delta_ts = after_ts.saturating_sub(after_ss);
    println!("\n[2] THEME_SET 懒加载（load_defaults）");
    println!("    加载后 RSS: {} KB (+{} KB = {:.2} MB)",
        after_ts, delta_ts, delta_ts as f64 / 1024.0);
    println!("    ThemeSet 加载的主题数: {}", THEME_SET.themes.len());

    // 总计
    let total = after_ts.saturating_sub(baseline);
    println!("\n=== 总计 ===");
    println!("SyntaxSet + ThemeSet: RSS = {} KB (+{} KB = {:.2} MB)",
        after_ts, total, total as f64 / 1024.0);

    // 列出所有支持的语言（前 30 个）
    println!("\n加载的语言列表（前 30 个）:");
    for (i, syntax) in ss.syntaxes().iter().take(30).enumerate() {
        println!("  {:2}. {} (extensions: {:?})", i + 1, syntax.name, syntax.file_extensions);
    }
    if ss.syntaxes().len() > 30 {
        println!("  ... 共 {} 个语言", ss.syntaxes().len());
    }
}

#[test]
fn measure_full_highlight_first_call_cost() {
    // 测第一次 highlight_code_block 调用的真实成本（含懒加载）
    let baseline = current_rss_kb();
    println!("\n=== 第一次 highlight_code_block 调用的真实成本 ===");
    println!("基线 RSS: {} KB ({:.2} MB)", baseline, baseline as f64 / 1024.0);

    // 模拟一段典型 Rust 代码块
    let code = vec![
        "fn optimized(data: &[u8]) -> Option<usize> {".to_string(),
        "    let mut index: HashMap<u8, usize> = HashMap::new();".to_string(),
        "    for (i, &b) in data.iter().enumerate() {".to_string(),
        "        if let Some(&prev) = index.get(&b) {".to_string(),
        "            return Some(prev);".to_string(),
        "        }".to_string(),
        "        index.insert(b, i);".to_string(),
        "    }".to_string(),
        "    None".to_string(),
        "}".to_string(),
    ];

    // 第一次调用（会触发懒加载）
    let result = peri_widgets::markdown::highlight::highlight_code_block("rust", &code);
    let after_first = current_rss_kb();
    let delta_first = after_first.saturating_sub(baseline);
    println!("\n第一次 highlight_code_block(\"rust\", 10 行):");
    println!("    RSS = {} KB (+{} KB = {:.2} MB)",
        after_first, delta_first, delta_first as f64 / 1024.0);
    println!("    返回行数: {}", result.as_ref().map(|v| v.len()).unwrap_or(0));

    // 第二次调用（懒加载已完成，应该几乎零成本）
    let result2 = peri_widgets::markdown::highlight::highlight_code_block("rust", &code);
    let after_second = current_rss_kb();
    let delta_second = after_second.saturating_sub(after_first);
    println!("\n第二次 highlight_code_block(\"rust\", 10 行):");
    println!("    RSS = {} KB (+{} KB = {:.2} MB)",
        after_second, delta_second, delta_second as f64 / 1024.0);
    println!("    返回行数: {}", result2.as_ref().map(|v| v.len()).unwrap_or(0));

    // 调用其他语言
    let python_code = vec!["def hello():\n    print('hi')".to_string()];
    let _ = peri_widgets::markdown::highlight::highlight_code_block("python", &python_code);
    let after_python = current_rss_kb();
    let delta_python = after_python.saturating_sub(after_second);
    println!("\n调用 python（应零成本，因 SyntaxSet 已加载）:");
    println!("    RSS = {} KB (+{} KB)",
        after_python, delta_python);

    std::mem::forget(result);
    std::mem::forget(result2);

    let _ = delta_python;
    println!("\n=== 结论 ===");
    println!("syntect SyntaxSet + ThemeSet 首次加载 = {:.2} MB",
        delta_first as f64 / 1024.0);
    println!("这是 TUI 首轮 RSS 暴涨的根因");
}
