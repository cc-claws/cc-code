//! 按显示列宽截断字符串（unicode-width）。

use unicode_width::UnicodeWidthChar;

/// 按显示列宽截断字符串。
/// 若超出 max_width，则截断并追加 '…'（占 1 列宽），确保结果总显示列宽不超过 max_width。
pub fn truncate_to_display_width(s: &str, max_width: usize) -> String {
    if max_width == 0 {
        return String::new();
    }
    let total_width: usize = s.chars().map(|c| c.width().unwrap_or(0)).sum();
    if total_width <= max_width {
        return s.to_string();
    }
    let target_width = max_width.saturating_sub(1);
    let mut cur_width = 0;
    let mut result = String::new();
    for c in s.chars() {
        let cw = c.width().unwrap_or(0);
        if cur_width + cw > target_width {
            break;
        }
        cur_width += cw;
        result.push(c);
    }
    result.push('…');
    result
}
