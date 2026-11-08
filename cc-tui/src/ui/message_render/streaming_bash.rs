use super::*;
use crate::ui::message_view::ToolOutputPreview;

/// 非详细模式实时输出只显示最后几行。
const LIVE_OUTPUT_COLLAPSED_LINES: usize = 5;

pub(crate) fn render_live_output(
    output: &ToolOutputPreview,
    width: usize,
    detail: bool,
) -> Vec<Line<'static>> {
    let mut error_header = String::new();
    // 折叠态先截断源行再美化（issue #645）：只显示最后 5 行，对尾部 5 个
    // 源行做 JSON 美化即可覆盖可见后缀（逐行变换独立保序，后缀逐字节不变）。
    let window = if detail {
        BodyWindow::All
    } else {
        BodyWindow::TailLines(LIVE_OUTPUT_COLLAPSED_LINES)
    };
    let display = transform_bash_output_lines(&output.tail, None, &mut error_header, window);
    let mut lines: Vec<_> = display
        .lines
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
    let total_visual = lines.len();
    if !detail && lines.len() > LIVE_OUTPUT_COLLAPSED_LINES {
        lines.drain(..lines.len() - LIVE_OUTPUT_COLLAPSED_LINES);
    }
    // 省略行数 ≈ 更早滚动行 + 窗口外源行 + 窗口内未显示的展开/折行视觉行
    // （折行与 JSON 展开计入视觉省略行数；issue #645 先截断后美化拿不到
    // 全文展开总数，窗口外行数用源行口径补齐）。
    let omitted = earlier_lines
        .saturating_add(display.source_total.saturating_sub(display.source_taken))
        .saturating_add(total_visual.saturating_sub(lines.len()));
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
    let elapsed = format_elapsed_secs(started_at.elapsed().as_secs().max(output.elapsed_ms / 1000));
    let bytes = format_output_bytes(output.total_bytes);
    match shell_timeout_label(timeout_ms) {
        Some(timeout) => format!("({elapsed} · {timeout} · {bytes})"),
        None => format!("({elapsed} · {bytes})"),
    }
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
