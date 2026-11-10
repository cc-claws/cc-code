use crate::ansi::strip_ansi;
use crate::dedup::dedup_adjacent_lines;
use crate::guard::{estimate_tokens, never_worse};

#[test]
fn test_strip_ansi_colors() {
    let colored = "\x1b[32mSuccess\x1b[0m: \x1b[1;31mError message\x1b[0m";
    assert_eq!(strip_ansi(colored), "Success: Error message");
}

#[test]
fn test_strip_ansi_carriage_return() {
    let progress = "Downloading 10%\rDownloading 50%\rDownloading 100%\nDone";
    assert_eq!(strip_ansi(progress), "Downloading 100%\nDone");
}

#[test]
fn test_strip_ansi_no_escape_passthrough() {
    let plain = "line one\nline two";
    assert_eq!(strip_ansi(plain), plain);
}

// ── guard：token 估算与「不变差」守卫 ──────────────────────────────

#[test]
fn test_estimate_tokens_rounds_up() {
    // 4 字节 = 1 token；不足 4 字节也计 1
    assert_eq!(estimate_tokens(""), 0);
    assert_eq!(estimate_tokens("abcd"), 1);
    assert_eq!(estimate_tokens("abcde"), 2);
}

#[test]
fn test_never_worse_keeps_smaller_filtered() {
    let raw = "keep this whole block of text intact";
    let filtered = "keep".to_string();
    assert_eq!(never_worse(raw, filtered.clone()), filtered);
}

#[test]
fn test_never_worse_falls_back_when_filtered_grows() {
    // 过滤后变长（小输出被追加标记行）→ 必须回退原文
    let raw = "ok";
    let filtered = "ok [+999 lines omitted]".to_string();
    assert_eq!(never_worse(raw, filtered), raw);
}

#[test]
fn test_never_worse_keeps_filtered_on_equal_size() {
    // 同尺寸时保留过滤结果（无额外成本，且更规整）
    let raw = "abcd";
    let filtered = "efgh".to_string();
    assert_eq!(never_worse(raw, filtered.clone()), filtered);
}

// ── dedup：相邻重复行折叠 ──────────────────────────────────────

#[test]
fn test_dedup_folds_long_adjacent_run() {
    let raw = "warning: same\nwarning: same\nwarning: same\nwarning: same\ndone";
    let out = dedup_adjacent_lines(raw);
    assert_eq!(out, "warning: same [x4]\ndone");
}

#[test]
fn test_dedup_keeps_short_run_untouched() {
    // 2 次重复低于下限，不引入标记
    let raw = "a\nb\nb\nc";
    assert_eq!(dedup_adjacent_lines(raw), raw);
}

#[test]
fn test_dedup_does_not_cross_gaps() {
    // 非相邻的相同行属不同事件，不得合并
    let raw = "x\nother\nx\nother2\nx";
    assert_eq!(dedup_adjacent_lines(raw), raw);
}

#[test]
fn test_dedup_exempts_blank_lines() {
    // 连续空行是排版，不折叠
    let raw = "a\n\n\n\nb";
    assert_eq!(dedup_adjacent_lines(raw), raw);
}

#[test]
fn test_dedup_stops_at_different_content_between_runs() {
    let raw = "same\nsame\nsame\nmid\nsame\nsame\nsame";
    assert_eq!(dedup_adjacent_lines(raw), "same [x3]\nmid\nsame [x3]");
}
