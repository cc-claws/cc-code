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
    // chrome：顶部实线 + 标题 + [副标题] + [描述] + 空行 + 提问句
    // 内容区若存在再额外占 2 行（上下虚线）。
    let base_chrome =
        4 + usize::from(model.subtitle.is_some()) + usize::from(model.description.is_some());
    let chrome_rows = base_chrome + model.notice.len();
    let content_rows = model.sections.iter().map(Vec::len).sum::<usize>();
    let hint_gap = usize::from(!model.hints.is_empty());
    let body_rule_rows = if content_rows > 0 { 2 } else { 0 };
    let fixed_rows = model.summary.len()
        + model.choices.len()
        + model.hints.len()
        + chrome_rows
        + body_rule_rows
        + hint_gap;
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
///
/// 布局对齐参考实现：
/// ```text
/// ━━━━━━━━━━━━━━━━━━━━━━━━━━  顶部实线（满宽，thinking 紫）
///  <工具标题>                  缩进 1，thinking 紫 + 粗
/// ╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌  内容区上虚线（满宽）
///  <命令 / diff>               缩进 1
/// ╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌  内容区下虚线（满宽）
///  Do you want to proceed?    缩进 1
///  ❯ 1. Yes                   缩进 1
///    2. …                      缩进 3
/// ```
pub(crate) fn render_hitl_popup(f: &mut Frame, app: &mut App, area: Rect) {
    let Some(InteractionPrompt::Approval(prompt)) =
        &app.session_mgr.current().agent.interaction_prompt
    else {
        return;
    };
    let cursor = prompt.cursor.min(prompt.items.len().saturating_sub(1));
    let width = usize::from(area.width);
    if width == 0 || area.height == 0 {
        return;
    }

    let Some(mut model) = content::build_content(app, width) else {
        return;
    };
    let height = usize::from(area.height);

    // ── 空间预算 ────────────────────────────────────────────────────
    // 必须显示的固定行：顶部实线(1) + 标题(1) + [副标题] + [描述] + 空行(1) + 提问(1)
    let base_chrome =
        4 + usize::from(model.subtitle.is_some()) + usize::from(model.description.is_some());
    let mut budget = height.saturating_sub(base_chrome);

    // 内容区想要的行数与上下虚线
    let want_body: usize = model.sections.iter().map(Vec::len).sum();
    let body_rule = usize::from(want_body > 0) * 2;

    // 各区块想要的行数
    let want_choices = model.choices.len();
    let want_hint_block = if model.hints.is_empty() {
        0
    } else {
        1 + model.hints.len() // 空行 + 提示
    };
    let want_notice = model.notice.len();
    let want_summary = model.summary.len();

    // ── 分配顺序：选项 → 提示 → notice → summary → 内容 ────────────
    // 选项是唯一可交互的东西，必须整组完整显示（缺项会让数字键与实际状态不一致）；
    // 内容再多也可以截断，它只是阅读材料。
    let choice_rows = want_choices.min(budget);
    budget -= choice_rows;

    // 提示（含前置空行）：放不下就整块省略，不占半行
    let hint_rows = if want_hint_block > 0 && budget >= want_hint_block {
        budget -= want_hint_block;
        want_hint_block
    } else {
        0
    };

    let notice_rows = want_notice.min(budget);
    budget -= notice_rows;
    let summary_rows = want_summary.min(budget);
    budget -= summary_rows;

    // 剩余空间全部给内容：够画上下虚线就画，不够就只画内容
    let (body_rows, draw_body_rule) = if want_body == 0 {
        (0, false)
    } else if budget > body_rule {
        (want_body.min(budget - body_rule), true)
    } else {
        (want_body.min(budget), false)
    };

    // 按分配结果裁剪各区块
    model.choices.truncate(choice_rows);
    if hint_rows == 0 {
        model.hints.clear();
    }
    model.notice.truncate(notice_rows);
    model.summary.truncate(summary_rows);

    let available = body_rows;
    let total_rows = model.sections.iter().map(Vec::len).sum::<usize>();
    let body = if total_rows <= available {
        model.sections.into_iter().flatten().collect::<Vec<_>>()
    } else {
        // 批量超高时只展示当前工具。
        let section = model.sections.get(cursor).cloned().unwrap_or_default();
        let mut shown = Vec::new();
        if available > 0 {
            let notice = content::hidden_notice(app, total_rows, width);
            let notice_rows = notice.len().min(available);
            let shown_count = available.saturating_sub(notice_rows).min(section.len());
            shown.extend(section.into_iter().take(shown_count));
            let hidden = total_rows.saturating_sub(shown_count);
            shown.extend(
                content::hidden_notice(app, hidden, width)
                    .into_iter()
                    .take(available.saturating_sub(shown.len())),
            );
        }
        shown
    };
    // 内容区不足 available 行时不补空行：补白只会挤掉选项与提示。
    // 上下虚线会自动贴合实际内容高度。

    // ── 组装最终行序列 ──────────────────────────────────────────
    let mut lines: Vec<Line<'static>> = Vec::with_capacity(height);

    // 1. 顶部实线（满宽，thinking 紫）
    lines.push(solid_rule(width));

    // 2. 标题行（缩进 1）
    lines.push(title_line(&model.title, width));

    // 2b. 副标题行（缩进 1，弱化色）：Edit/Write 显示文件路径
    if let Some(subtitle) = &model.subtitle {
        lines.push(subtitle_line(subtitle, width));
    }

    // 2c. 描述行（缩进 1，正文色）：工具自带 `description` 时说明这一步要做什么，
    //     参考实现把它放在标题下方、虚线之上。
    if let Some(description) = &model.description {
        lines.push(question_line(description, width));
    }

    // 3. 上虚线 + 内容 + 下虚线：内容为空时整块省略，
    //    空间不足时省略虚线，把行数让给内容本身。
    let has_body = !body.is_empty() && body.iter().any(|line| !line.spans.is_empty());
    if has_body {
        if draw_body_rule {
            lines.push(dashed_rule(width));
        }
        lines.extend(body.into_iter().map(|line| indent_line(line, width)));
        if draw_body_rule {
            lines.push(dashed_rule(width));
        }
    }

    // 5a. 审批原因区（对齐参考实现的 decisionReason）
    if !model.notice.is_empty() {
        for (index, text) in model.notice.iter().enumerate() {
            // 第一行加 `│ ` gutter（对齐 CC 的引用块样式）
            if index == 0 {
                lines.push(notice_line(text, width, true));
            } else {
                lines.push(notice_line(text, width, false));
            }
        }
    }

    // 5b. 空行（对齐参考实现：内容区/提问句之间留一空行）
    lines.push(Line::from(""));

    // 5c. 批量摘要（在提问句之前，说明当前是第几项/共几项）
    lines.extend(model.summary);

    // 6. 提问句（缩进 1）
    lines.push(question_line(&model.question, width));

    // 7. 选择项
    lines.extend(model.choices);

    // 7b. 空行（对齐参考实现：选项与底部提示之间留一空行）
    if !model.hints.is_empty() {
        lines.push(Line::from(""));
    }

    // 8. 快捷键提示
    lines.extend(model.hints);

    lines.truncate(height);
    f.render_widget(Paragraph::new(lines), area);
}

/// 顶部实线：`─` 重复满宽。参考实现用 U+2500 light + permission 紫。
fn solid_rule(width: usize) -> Line<'static> {
    Line::from(Span::styled(
        "─".repeat(width),
        Style::default().fg(theme::cur().thinking),
    ))
}

/// 内容区上下虚线：`╌`（U+254C）重复满宽，用弱化色。
fn dashed_rule(width: usize) -> Line<'static> {
    Line::from(Span::styled(
        "╌".repeat(width),
        Style::default().fg(theme::cur().inactive),
    ))
}

/// 标题行：缩进 1，thinking 紫 + 粗。
fn title_line(title: &str, width: usize) -> Line<'static> {
    Line::from(Span::styled(
        format!(
            " {}",
            truncate_to_display_width(title, width.saturating_sub(1))
        ),
        Style::default()
            .fg(theme::cur().thinking)
            .add_modifier(Modifier::BOLD),
    ))
}

/// 审批原因行：第一行带 `│ ` 引用块 gutter，其余缩进 1 格。
fn notice_line(text: &str, width: usize, gutter: bool) -> Line<'static> {
    let body = truncate_to_display_width(text, width.saturating_sub(3));
    if gutter {
        Line::from(vec![
            Span::raw(" "),
            Span::styled("│ ", Style::default().fg(theme::cur().inactive)),
            Span::styled(body, Style::default().fg(theme::cur().text_soft)),
        ])
    } else {
        Line::from(Span::styled(
            format!(" {body}"),
            Style::default().fg(theme::cur().text_soft),
        ))
    }
}

/// 副标题行：缩进 1，正文色（对齐参考实现的文件路径行）。
fn subtitle_line(subtitle: &str, width: usize) -> Line<'static> {
    Line::from(Span::styled(
        format!(
            " {}",
            truncate_to_display_width(subtitle, width.saturating_sub(1))
        ),
        Style::default().fg(theme::cur().text_soft),
    ))
}

/// 提问句：缩进 1，正文色。
fn question_line(question: &str, width: usize) -> Line<'static> {
    Line::from(Span::styled(
        format!(
            " {}",
            truncate_to_display_width(question, width.saturating_sub(1))
        ),
        Style::default().fg(theme::cur().text_soft),
    ))
}

/// 内容行统一缩进 1 格。
fn indent_line(line: Line<'static>, width: usize) -> Line<'static> {
    let mut spans = Vec::with_capacity(line.spans.len() + 1);
    spans.push(Span::raw(" "));
    spans.extend(line.spans);
    let _ = width;
    Line::from(spans)
}

#[cfg(test)]
mod tests {
    use crate::app::{App, HitlBatchPrompt, InteractionPrompt};
    use cc_middlewares::hitl::BatchItem;
    include!("hitl_test.rs");
}
