use ratatui::text::Line;

use super::{fill_word_diffs, render_diff_result, DiffHunk, DiffLine, DiffResult};
use crate::theme::Theme;

/// 剥离 ANSI 转义字符序列（例如从带有 `--color` 的 git diff 中剥离颜色标记）。
pub fn strip_ansi(s: &str) -> std::borrow::Cow<'_, str> {
    if !s.contains('\x1b') {
        return std::borrow::Cow::Borrowed(s);
    }
    let mut out = String::with_capacity(s.len());
    let mut in_escape = false;
    for c in s.chars() {
        if c == '\x1b' {
            in_escape = true;
        } else if in_escape {
            if c.is_ascii_alphabetic() {
                in_escape = false;
            }
        } else {
            out.push(c);
        }
    }
    std::borrow::Cow::Owned(out)
}

/// 快速判定文本是否为 Unified Diff 补丁（如 `git diff` 输出）。
///
/// 判定特征：在前 100 行内同时命中 `diff --git `（或 `--- a/` / `+++ b/`）与至少一个 hunk 标记 `@@ `。
pub fn is_patch_text(text: &str) -> bool {
    let clean = strip_ansi(text);
    let mut has_file_header = false;
    let mut has_hunk_header = false;

    for line in clean.lines().take(100) {
        let trimmed = line.trim();
        if trimmed.starts_with("diff --git ")
            || (trimmed.starts_with("--- a/") || trimmed.starts_with("--- /dev/null"))
        {
            has_file_header = true;
        }
        if trimmed.starts_with("@@ -") && trimmed.contains(" @@") {
            has_hunk_header = true;
        }
        if has_file_header && has_hunk_header {
            return true;
        }
    }
    has_file_header && has_hunk_header
}

/// 解析 hunk 标头：`@@ -old_start[,old_lines] +new_start[,new_lines] @@`
fn parse_hunk_header(line: &str) -> Option<(u32, u32, u32, u32)> {
    let s = line.strip_prefix("@@ -")?;
    let at_end = s.find(" @@")?;
    let core = &s[..at_end];
    let (old_part, new_part) = core.split_once(" +")?;

    let (old_start, old_lines) = if let Some((start, count)) = old_part.split_once(',') {
        (start.parse().ok()?, count.parse().ok()?)
    } else {
        (old_part.parse().ok()?, 1)
    };

    let (new_start, new_lines) = if let Some((start, count)) = new_part.split_once(',') {
        (start.parse().ok()?, count.parse().ok()?)
    } else {
        (new_part.parse().ok()?, 1)
    };

    Some((old_start, old_lines, new_start, new_lines))
}

/// 从 `diff --git a/... b/...` 行提取规范化的文件路径。
fn extract_file_path(line: &str) -> Option<String> {
    let trimmed = line.trim();
    if let Some(rest) = trimmed.strip_prefix("diff --git ") {
        let parts: Vec<&str> = rest.split_whitespace().collect();
        if parts.len() >= 2 {
            let b_path = parts.last().unwrap_or(&"");
            let clean = b_path.trim_matches('"');
            if clean == "b//dev/null" || clean == "/dev/null" {
                // 删除文件，取 a 路径
                let a_path = parts.first().unwrap_or(&"").trim_matches('"');
                return Some(a_path.strip_prefix("a/").unwrap_or(a_path).to_string());
            }
            return Some(clean.strip_prefix("b/").unwrap_or(clean).to_string());
        }
    }
    if let Some(rest) = trimmed.strip_prefix("+++ ") {
        let clean = rest.trim_whitespace_and_quotes();
        if clean != "/dev/null" {
            return Some(clean.strip_prefix("b/").unwrap_or(clean).to_string());
        }
    }
    None
}

trait TrimWhitespaceAndQuotes {
    fn trim_whitespace_and_quotes(&self) -> &str;
}

impl TrimWhitespaceAndQuotes for str {
    fn trim_whitespace_and_quotes(&self) -> &str {
        self.trim().trim_matches('"')
    }
}

/// 解析单文件或多文件的 Unified Diff 文本。
pub fn parse_patch(patch_text: &str) -> Vec<(String, DiffResult)> {
    let clean = strip_ansi(patch_text);
    let mut results = Vec::new();

    let mut current_file: Option<String> = None;
    let mut is_new_file = false;
    let mut is_deleted_file = false;
    let mut is_binary = false;
    let mut hunks: Vec<DiffHunk> = Vec::new();

    let mut current_hunk: Option<(u32, u32, u32, u32, Vec<DiffLine>)> = None;
    let mut old_line_cur = 0u32;
    let mut new_line_cur = 0u32;

    let flush_hunk = |current_hunk: &mut Option<(u32, u32, u32, u32, Vec<DiffLine>)>,
                      hunks: &mut Vec<DiffHunk>| {
        if let Some((old_start, old_lines, new_start, new_lines, mut lines)) = current_hunk.take() {
            fill_word_diffs(&mut lines);
            hunks.push(DiffHunk {
                old_start,
                old_lines,
                new_start,
                new_lines,
                lines,
            });
        }
    };

    let flush_file = |current_file: &mut Option<String>,
                      is_new_file: &mut bool,
                      is_deleted_file: &mut bool,
                      is_binary: &mut bool,
                      hunks: &mut Vec<DiffHunk>,
                      results: &mut Vec<(String, DiffResult)>| {
        if let Some(path) = current_file.take() {
            results.push((
                path,
                DiffResult {
                    hunks: std::mem::take(hunks),
                    is_new_file: *is_new_file,
                    is_deleted_file: *is_deleted_file,
                    is_binary: *is_binary,
                    is_truncated: false,
                },
            ));
            *is_new_file = false;
            *is_deleted_file = false;
            *is_binary = false;
        }
    };

    for line in clean.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with("diff --git ") {
            flush_hunk(&mut current_hunk, &mut hunks);
            flush_file(
                &mut current_file,
                &mut is_new_file,
                &mut is_deleted_file,
                &mut is_binary,
                &mut hunks,
                &mut results,
            );
            current_file = extract_file_path(trimmed);
            continue;
        }

        if trimmed.starts_with("new file mode ") {
            is_new_file = true;
            continue;
        }
        if trimmed.starts_with("deleted file mode ") {
            is_deleted_file = true;
            continue;
        }
        if trimmed.starts_with("Binary files ") && trimmed.ends_with(" differ") {
            is_binary = true;
            continue;
        }
        if trimmed.starts_with("--- ")
            || trimmed.starts_with("+++ ")
            || trimmed.starts_with("index ")
        {
            if current_file.is_none() && trimmed.starts_with("+++ ") {
                current_file = extract_file_path(trimmed);
            }
            continue;
        }

        if trimmed.starts_with("@@ -") && trimmed.contains(" @@") {
            flush_hunk(&mut current_hunk, &mut hunks);
            if let Some((old_start, old_lines, new_start, new_lines)) = parse_hunk_header(trimmed) {
                old_line_cur = old_start;
                new_line_cur = new_start;
                current_hunk = Some((
                    old_start,
                    old_lines,
                    new_start,
                    new_lines,
                    vec![DiffLine::HunkHeader {
                        text: trimmed.to_string(),
                    }],
                ));
            }
            continue;
        }

        if let Some((_, _, _, _, ref mut lines)) = current_hunk {
            if let Some(content) = line.strip_prefix('+') {
                lines.push(DiffLine::Add {
                    text: content.to_string(),
                    line_num: new_line_cur,
                    word_diff: None,
                });
                new_line_cur += 1;
            } else if let Some(content) = line.strip_prefix('-') {
                lines.push(DiffLine::Remove {
                    text: content.to_string(),
                    line_num: old_line_cur,
                    word_diff: None,
                });
                old_line_cur += 1;
            } else if let Some(content) = line.strip_prefix(' ') {
                lines.push(DiffLine::Context {
                    text: content.to_string(),
                    old_line_num: old_line_cur,
                    new_line_num: new_line_cur,
                });
                old_line_cur += 1;
                new_line_cur += 1;
            } else if line.is_empty() {
                // 兼容空行当做 context 行
                lines.push(DiffLine::Context {
                    text: String::new(),
                    old_line_num: old_line_cur,
                    new_line_num: new_line_cur,
                });
                old_line_cur += 1;
                new_line_cur += 1;
            }
        }
    }

    flush_hunk(&mut current_hunk, &mut hunks);
    flush_file(
        &mut current_file,
        &mut is_new_file,
        &mut is_deleted_file,
        &mut is_binary,
        &mut hunks,
        &mut results,
    );

    results
}

/// 提取 DiffResult 首行用于 shebang 探测
fn first_line_of_diff_result(result: &DiffResult) -> Option<&str> {
    for hunk in &result.hunks {
        for dl in &hunk.lines {
            match dl {
                DiffLine::Context { text, .. }
                | DiffLine::Add { text, .. }
                | DiffLine::Remove { text, .. }
                    if !text.is_empty() =>
                {
                    return Some(text.as_str());
                }
                _ => {}
            }
        }
    }
    None
}

/// 将 Unified Diff 文本解析并渲染为带有语法高亮、词级对比、柔和背景与行号 Gutter 的 Line 列表。
///
/// 若文本不符合 Unified Diff 格式则返回 None。
pub fn render_patch(
    patch_text: &str,
    width: usize,
    theme: &dyn Theme,
) -> Option<Vec<Line<'static>>> {
    if !is_patch_text(patch_text) {
        return None;
    }
    let parsed = parse_patch(patch_text);
    if parsed.is_empty() {
        return None;
    }

    let mut out = Vec::new();
    for (idx, (file_path, diff_result)) in parsed.iter().enumerate() {
        if idx > 0 && !out.is_empty() {
            out.push(Line::raw(""));
        }
        let first_line = first_line_of_diff_result(diff_result);
        let rendered = render_diff_result(file_path, diff_result, width, theme, first_line);
        out.extend(rendered);
    }

    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

#[cfg(test)]
#[path = "patch_test.rs"]
mod patch_test;
