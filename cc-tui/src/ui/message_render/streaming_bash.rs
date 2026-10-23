use super::*;
use crate::ui::message_view::ToolOutputPreview;

pub(crate) fn render_live_output(
    output: &ToolOutputPreview,
    width: usize,
    detail: bool,
) -> Vec<Line<'static>> {
    let mut error_header = String::new();
    let display = transform_bash_output_lines(&output.tail, None, &mut error_header);
    let mut lines: Vec<_> = display
        .iter()
        .flat_map(|item| {
            format_prefixed_text(
                item.text.as_ref(),
                item.prefix,
                Style::default().fg(theme::cur().inactive),
                Style::default().fg(theme::cur().text_soft),
                BranchPrefix::content_width(width),
            )
        })
        .collect();
    // 缓存外只能知道原始行数；缓存内按与完成输出相同的 JSON/折行规则计数。
    let earlier_lines = output
        .total_lines
        .saturating_sub(output.tail.lines().count());
    let total = earlier_lines.saturating_add(lines.len());
    if !detail && lines.len() > 5 {
        lines.drain(..lines.len() - 5);
    }
    let omitted = total.saturating_sub(lines.len());
    if let Some(prefix) = lines.first_mut().and_then(|line| line.spans.first_mut()) {
        prefix.content = BranchPrefix::PREFIX.into();
    }
    if omitted > 0 {
        lines.push(Line::from(vec![
            Span::raw(BranchPrefix::HANG),
            Span::styled(
                format!(
                    "… +{omitted} {}",
                    if omitted == 1 { "line" } else { "lines" }
                ),
                Style::default().fg(theme::cur().subtle),
            ),
        ]));
    }
    lines
}

pub(crate) fn shell_output_status_text(
    started_at: std::time::Instant,
    timeout_ms: Option<u64>,
    output: &ToolOutputPreview,
) -> String {
    let elapsed = humanize_secs(started_at.elapsed().as_secs().max(output.elapsed_ms / 1000));
    let timeout = shell_timeout_text(timeout_ms)
        .map(|text| format!(" · {}", text.trim_start_matches('(').trim_end_matches(')')))
        .unwrap_or_default();
    format!(
        "({elapsed}{timeout} · {})",
        format_output_bytes(output.total_bytes)
    )
}

fn format_output_bytes(bytes: usize) -> String {
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut value = bytes as f64 / 1024.0;
    let mut unit = "KB";
    for next in ["MB", "GB", "TB"] {
        if value < 1024.0 {
            break;
        }
        value /= 1024.0;
        unit = next;
    }
    format!("{value:.1} {unit}")
}

#[cfg(test)]
#[path = "streaming_bash_test.rs"]
mod tests;
