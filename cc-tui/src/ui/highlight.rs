//! 提示文案里的 slash 命令着色。
//!
//! tips、welcome、队列提示、`/help` 输出里都会出现 `/help`、`/clear` 这类命令名，
//! 统一用这里把命令 token 染成 SUGGESTION（与输入框里的 `/command` 一致），
//! 其余文字保持调用方给的基础色。

use ratatui::style::Style;
use ratatui::text::Span;

/// 找出文本中 slash 命令的字符区间（半开区间，按 `char` 下标计）。
///
/// 判定规则（按顺序）：
/// 1. `/` 前一个字符不能是字母/数字、`_`、`-`、`.`、`:`、`/`、`\\`、`~`
///    —— 排除 `.claude/skills/`、`C:/Users`、`https://` 这类路径；
/// 2. 命令体只取 ASCII 字母/数字/`_`/`-`，且后面不能紧跟 `/` 或 `\\`
///    —— 排除 `/home/user`；
/// 3. 单独的 `/`（如 `按 / 输入命令`）只有在后一个非空白字符不是 ASCII 字母/数字时才算命令
///    —— 避免把 `Ctrl+P / Alt+P` 中间的分隔符染色。
pub fn slash_command_ranges(text: &str) -> Vec<(usize, usize)> {
    let chars: Vec<char> = text.chars().collect();
    let mut ranges = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] != '/' || is_path_like_prefix(chars.get(i.wrapping_sub(1)).copied()) {
            i += 1;
            continue;
        }
        let mut j = i + 1;
        while j < chars.len() && is_command_char(chars[j]) {
            j += 1;
        }
        let bare = j == i + 1;
        let followed_by_path_sep = chars.get(j).is_some_and(|c| matches!(c, '/' | '\\'));
        let next_non_space = chars[j..].iter().find(|c| !c.is_whitespace()).copied();
        let followed_by_word = next_non_space.is_some_and(|c| c.is_ascii_alphanumeric());
        let accepted = if bare {
            !followed_by_word
        } else {
            !followed_by_path_sep
        };
        if accepted {
            ranges.push((i, j));
        }
        i = j.max(i + 1);
    }
    ranges
}

/// 把文本切成 span：slash 命令用 `accent`，其余用 `base`。
pub fn slash_command_spans(text: &str, base: Style, accent: Style) -> Vec<Span<'static>> {
    let ranges = slash_command_ranges(text);
    if ranges.is_empty() {
        return vec![Span::styled(text.to_string(), base)];
    }
    let chars: Vec<char> = text.chars().collect();
    let mut spans = Vec::new();
    let mut cursor = 0;
    for (start, end) in ranges {
        if start > cursor {
            spans.push(Span::styled(
                chars[cursor..start].iter().collect::<String>(),
                base,
            ));
        }
        spans.push(Span::styled(
            chars[start..end].iter().collect::<String>(),
            accent,
        ));
        cursor = end;
    }
    if cursor < chars.len() {
        spans.push(Span::styled(
            chars[cursor..].iter().collect::<String>(),
            base,
        ));
    }
    spans
}

/// `/` 的前一个字符若属于这些，说明它是路径/URL 的一部分，不是命令。
fn is_path_like_prefix(prev: Option<char>) -> bool {
    prev.is_some_and(|c| {
        c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | ':' | '/' | '\\' | '~')
    })
}

/// 命令体允许的字符（`/resume`、`/loop`、`/compact` 等）。
fn is_command_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '-')
}

#[cfg(test)]
#[path = "highlight_test.rs"]
mod highlight_test;
