use cc_widgets::BorderedPanel;
use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::{
    app::{App, InteractionPrompt},
    ui::{message_render::truncate_to_display_width, theme},
};

#[path = "hitl_content.rs"]
mod content;

/// 按完整参数、选项与快捷键的实际显示行数计算面板高度。
pub(crate) fn hitl_popup_height(app: &App, width: u16, max_height: u16) -> u16 {
    let Some(model) = content::build_content(app, usize::from(width)) else {
        return 0;
    };
    let content_rows = model.sections.iter().map(Vec::len).sum::<usize>();
    let fixed_rows = model.summary.len() + model.choices.len() + model.hints.len() + 2;
    let full_rows = content_rows + fixed_rows;
    if full_rows <= usize::from(max_height) {
        return u16::try_from(full_rows).unwrap_or(max_height);
    }
    let cursor = match &app.session_mgr.current().agent.interaction_prompt {
        Some(InteractionPrompt::Approval(prompt)) => prompt.cursor,
        _ => 0,
    };
    let current_rows = model.sections.get(cursor).map(Vec::len).unwrap_or(0);
    let hidden_rows = content_rows.saturating_sub(current_rows);
    let notice_rows = content::hidden_notice(app, hidden_rows, usize::from(width)).len();
    let compact_rows = current_rows + notice_rows + fixed_rows;
    u16::try_from(compact_rows)
        .unwrap_or(u16::MAX)
        .min(max_height)
}

/// HITL 底部审批区：方向键只选择审批项，参数没有独立滚动区。
pub(crate) fn render_hitl_popup(f: &mut Frame, app: &mut App, area: Rect) {
    let Some(InteractionPrompt::Approval(prompt)) =
        &app.session_mgr.current().agent.interaction_prompt
    else {
        return;
    };
    let title = app.services.lc.tr(if prompt.items.len() == 1 {
        "hitl-single-title"
    } else {
        "hitl-batch-title"
    });
    let cursor = prompt.cursor.min(prompt.items.len().saturating_sub(1));
    let inner = BorderedPanel::new(Span::styled(
        truncate_to_display_width(&title, usize::from(area.width)),
        Style::default()
            .fg(theme::THINKING)
            .add_modifier(Modifier::BOLD),
    ))
    .border_style(Style::default().fg(theme::WARNING))
    .render(f, area);
    let Some(mut model) = content::build_content(app, usize::from(inner.width)) else {
        return;
    };
    let height = usize::from(inner.height);
    if height == 0 {
        return;
    }
    // 极矮窗口先移除说明与批量摘要，始终优先保留三项选择和操作提示。
    if model.choices.len() + model.summary.len() + model.hints.len() + 2 > height {
        model.choices.truncate(3);
        model.summary.clear();
    }
    let fixed_rows = model.summary.len() + model.choices.len() + model.hints.len();
    let available = height.saturating_sub(fixed_rows);
    let total_rows = model.sections.iter().map(Vec::len).sum::<usize>();
    let mut lines = if total_rows <= available {
        model.sections.into_iter().flatten().collect::<Vec<_>>()
    } else {
        // 批量超高时只展示当前工具；不利用旧 scroll_offset 改变参数视口。
        let section = model.sections.get(cursor).cloned().unwrap_or_default();
        let mut shown = Vec::new();
        if available > 0 {
            let notice = content::hidden_notice(app, total_rows, usize::from(inner.width));
            let notice_rows = notice.len().min(available);
            let shown_count = available.saturating_sub(notice_rows).min(section.len());
            shown.extend(section.into_iter().take(shown_count));
            let hidden = total_rows.saturating_sub(shown_count);
            shown.extend(
                content::hidden_notice(app, hidden, usize::from(inner.width))
                    .into_iter()
                    .take(available.saturating_sub(shown.len())),
            );
        }
        shown
    };
    lines.resize_with(available, || Line::from(""));
    lines.extend(model.summary);
    lines.extend(model.choices);
    lines.extend(model.hints);
    // 小于可操作最小尺寸时只裁剪最终行；不进行滚动，也不改变审批状态。
    f.render_widget(Paragraph::new(lines), inner);
}

#[cfg(test)]
mod tests {
    use crate::app::{App, HitlBatchPrompt, InteractionPrompt};
    use cc_middlewares::hitl::BatchItem;
    include!("hitl_test.rs");
}
