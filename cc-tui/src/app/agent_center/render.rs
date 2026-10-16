use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Clear, Paragraph, Wrap},
    Frame,
};
use unicode_width::UnicodeWidthStr;

use super::{AgentCenter, App, Bucket, CenterRow};
use crate::ui::main_ui::truncate_spans_to_width;
use crate::ui::{
    markdown::parse_markdown,
    message_render::{truncate_to_display_width, wrap_line_spans},
    theme,
};

/// 详情面板中不参与高度分配的固定行数（标题块 + Model/Project/Branch 块 + 标签 + 状态说明）。
const FIXED_DETAIL_LINES: usize = 18;
/// `Prompt` 预览最多占用的行数，其余高度全部让给 `Last message`。
const PREVIEW_PROMPT_MAX: usize = 3;

pub(crate) fn render(frame: &mut Frame, app: &mut App) {
    let Some(mut center) = app.global_ui.agent_center.take() else {
        return;
    };
    let area = frame.area();
    frame.render_widget(Clear, area);
    if area.width < 4 || area.height < 5 {
        app.global_ui.agent_center = Some(center);
        return;
    }
    let outer = area.inner(ratatui::layout::Margin {
        horizontal: 1,
        vertical: 0,
    });
    let parts = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(2),
        ])
        .split(outer);
    let palette = theme::cur();
    let muted = Style::default().fg(palette.inactive);
    let title = format!(
        "Agent command center  Group: {}  g",
        center.grouping.label()
    );
    line(
        frame,
        parts[0],
        &title,
        Style::default().add_modifier(Modifier::BOLD),
    );
    let labels = ["All", "Needs you", "Working", "Ready", "Inactive"];
    let counts = center.counts(app);
    let mut tabs = Vec::new();
    for (index, label) in labels.iter().enumerate() {
        let style = if center.filter == index {
            Style::default()
                .fg(palette.claude)
                .add_modifier(Modifier::BOLD)
        } else {
            muted
        };
        tabs.push(Span::styled(format!(" {label} {}  ", counts[index]), style));
    }
    let hint = "tab/shift+tab filter";
    let tabs_width: usize = tabs.iter().map(|span| span.content.width()).sum();
    frame.render_widget(Paragraph::new(Line::from(tabs)), parts[1]);
    if usize::from(parts[1].width) > tabs_width + hint.width() {
        line(
            frame,
            Rect::new(
                parts[1].right().saturating_sub(hint.width() as u16),
                parts[1].y,
                hint.width() as u16,
                1,
            ),
            hint,
            muted,
        );
    }
    // 全页唯一横线，标题和页脚均不画边框。
    line(frame, parts[2], &"─".repeat(parts[2].width as usize), muted);
    let body = parts[3];
    let (list, detail) = if body.width >= 90 {
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Min(46),
                Constraint::Length(3),
                Constraint::Length(38),
            ])
            .split(body);
        for y in body.y..body.bottom() {
            line(frame, Rect::new(columns[1].x + 1, y, 1, 1), "│", muted);
        }
        (columns[0], Some(columns[2]))
    } else {
        (body, None)
    };
    render_list(frame, app, &mut center, list);
    if let Some(detail) = detail {
        render_detail(frame, app, &center, detail);
    }
    let target = center
        .selected_meta(app)
        .map(|meta| meta.cwd.as_str())
        .unwrap_or(&app.services.cwd);
    let others = center
        .catalog
        .iter()
        .filter(|meta| meta.is_root() && meta.cwd != app.services.cwd)
        .count();
    let footer = if let Some(title) = &center.rename {
        format!(
            "{} {title}▏  enter / esc",
            app.services.lc.tr("agent-center-rename")
        )
    } else if center.search || !center.query.is_empty() {
        format!(
            "/ {}▏  {}",
            center.query,
            app.services.lc.tr("agent-center-search")
        )
    } else {
        format!("? help  esc back  ↑/↓ move  enter open  n new ({target})")
    };
    line(
        frame,
        Rect::new(parts[4].x, parts[4].y, parts[4].width, 1),
        &footer,
        muted,
    );
    let notice = center.error.clone().unwrap_or_else(|| {
        if center.page_rx.is_some() {
            app.services.lc.tr("agent-center-loading")
        } else {
            app.services.lc.tr_args(
                "agent-center-other-projects",
                &[("count".into(), (others as i64).into())],
            )
        }
    });
    line(
        frame,
        Rect::new(parts[4].x, parts[4].y + 1, parts[4].width, 1),
        &notice,
        if center.error.is_some() {
            Style::default().fg(palette.error)
        } else {
            muted
        },
    );
    if let Some(id) = &center.confirm_delete {
        let title = center
            .threads
            .iter()
            .find(|meta| &meta.id == id)
            .and_then(|meta| meta.title.as_deref())
            .unwrap_or("Unknown");
        let title = truncate_to_display_width(
            &title.replace(['\r', '\n'], " "),
            area.width.saturating_sub(4).min(72) as usize,
        );
        let text = format!(
            "{}\n\n{title}\n\n{}",
            app.services.lc.tr("agent-center-delete-question"),
            app.services.lc.tr("agent-center-delete-confirm")
        );
        overlay(frame, area, &text, palette.error);
    }
    if center.help {
        render_help(frame, app, area);
    }
    app.global_ui.agent_center = Some(center);
}

fn render_list(frame: &mut Frame, app: &App, center: &mut AgentCenter, area: Rect) {
    let palette = theme::cur();
    let muted = Style::default().fg(palette.inactive);
    let status_width = if area.width >= 42 { 13 } else { 0 };
    let updated_width = if area.width >= 56 { 11 } else { 0 };
    let title_width = area.width.saturating_sub(status_width + updated_width);
    let header = format!(
        "{:<width$}{:<status$}{}",
        "Tasks",
        if status_width > 0 { "Status" } else { "" },
        if updated_width > 0 { "Updated" } else { "" },
        width = title_width as usize,
        status = status_width as usize
    );
    line(
        frame,
        Rect::new(area.x, area.y, area.width, 1),
        &header,
        muted,
    );
    let rows = center.rows(app);
    center.selected = center.selected.min(rows.len().saturating_sub(1));
    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut previous_group = None;
    let mut selected_line = 0;
    for (row_index, row) in rows.iter().enumerate() {
        if let CenterRow::Thread(index) = row {
            let meta = &center.threads[*index];
            let group = center.group_key(app, meta);
            if previous_group.as_ref() != Some(&group) {
                let count = rows.iter().filter(|row| matches!(row, CenterRow::Thread(other) if center.group_key(app, &center.threads[*other]) == group)).count();
                lines.push(Line::from(Span::styled(
                    truncate_to_display_width(&format!("{group}  {count}"), area.width as usize),
                    muted,
                )));
                previous_group = Some(group);
            }
        }
        if row_index == center.selected {
            selected_line = lines.len();
        }
        let selected = row_index == center.selected;
        match row {
            CenterRow::More => {
                lines.push(Line::from(Span::styled(
                    format!(
                        "{} {}",
                        if selected { "›" } else { " " },
                        app.services.lc.tr("agent-center-show-more")
                    ),
                    if selected {
                        Style::default()
                            .fg(palette.claude)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        muted
                    },
                )));
            }
            CenterRow::Thread(index) => {
                let meta = &center.threads[*index];
                let bucket = app.agent_center_bucket(meta);
                let current =
                    app.session_mgr.current().current_thread_id.as_deref() == Some(&meta.id);
                let symbol = if current || matches!(bucket, Bucket::Working | Bucket::NeedsYou) {
                    "●"
                } else {
                    "○"
                };
                let title = format!(
                    "{} {symbol} {}{}",
                    if selected { "›" } else { " " },
                    meta.title.as_deref().unwrap_or("(untitled)"),
                    if current { " (current)" } else { "" }
                );
                let mut title = truncate_to_display_width(
                    &title.replace(['\n', '\r'], " "),
                    title_width as usize,
                );
                title.push_str(&" ".repeat((title_width as usize).saturating_sub(title.width())));
                let style = if selected {
                    Style::default()
                        .fg(palette.claude)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                let mut spans = vec![Span::styled(title, style)];
                if status_width > 0 {
                    let label = if bucket == Bucket::NeedsYou {
                        "Needs input"
                    } else {
                        bucket.label()
                    };
                    spans.push(Span::styled(
                        format!(
                            "{label:<status_width$}",
                            status_width = status_width as usize
                        ),
                        Style::default().fg(bucket_color(bucket)),
                    ));
                }
                if updated_width > 0 {
                    spans.push(Span::styled(updated(meta.updated_at), muted));
                }
                lines.push(Line::from(spans));
            }
        }
    }
    if lines.is_empty() {
        lines.push(Line::from(app.services.lc.tr("agent-center-empty")));
    }
    let height = area.height.saturating_sub(1) as usize;
    keep_visible(&mut center.scroll, selected_line, height);
    center.scroll = center.scroll.min(lines.len().saturating_sub(height));
    frame.render_widget(
        Paragraph::new(
            lines
                .into_iter()
                .skip(center.scroll)
                .take(height)
                .collect::<Vec<_>>(),
        ),
        Rect::new(area.x, area.y + 1, area.width, height as u16),
    );
}

pub(super) fn keep_visible(scroll: &mut usize, selected_line: usize, height: usize) {
    if selected_line < *scroll {
        *scroll = selected_line;
    } else if height > 0 && selected_line >= scroll.saturating_add(height) {
        *scroll = selected_line + 1 - height;
    }
}

fn render_detail(frame: &mut Frame, app: &App, center: &AgentCenter, area: Rect) {
    let palette = theme::cur();
    let label_style = Style::default().fg(palette.inactive);
    let Some(meta) = center.selected_meta(app) else {
        line(frame, area, "Task details", Style::default());
        return;
    };
    let unknown = "Unknown";
    let none = app.services.lc.tr("agent-center-none");
    let loading = app.services.lc.tr("agent-center-loading");
    let detail = center
        .detail
        .as_ref()
        .filter(|_| center.detail_id.as_deref() == Some(&meta.id));
    let last = detail
        .map(|detail| detail.last.as_str())
        .unwrap_or(&loading);
    let prompt = detail
        .map(|detail| detail.prompt.as_str())
        .unwrap_or(&loading);
    let width = area.width as usize;
    let bucket = app.agent_center_bucket(meta);

    // 版式对齐详情面板参考稿：短字段（Model / Reasoning）标签与值同行；
    // 长字段（Project / Branch / Last message / Prompt）标签独占一行、值另起一行，
    // 块之间留一行空行分隔。配色沿用状态栏（模型青、项目黄、git 分支青）。
    let mut lines: Vec<Line<'static>> = vec![
        Line::from(Span::styled(
            "Task details",
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::default(),
        Line::from(truncate_spans_to_width(
            vec![Span::styled(
                detail_text(meta.title.clone().unwrap_or_else(|| unknown.into())),
                Style::default().fg(palette.text),
            )],
            width,
        )),
        Line::from(Span::styled(
            format!("○ {}", bucket.label()),
            Style::default().fg(bucket_color(bucket)),
        )),
        Line::default(),
        detail_kv_line(
            "Model: ",
            meta.model.as_deref().unwrap_or(unknown),
            Style::default().fg(Color::Cyan),
            label_style,
            width,
        ),
        detail_kv_line(
            "Reasoning: ",
            meta.reasoning_effort.as_deref().unwrap_or(unknown),
            Style::default().fg(palette.text),
            label_style,
            width,
        ),
        Line::default(),
        Line::from(Span::styled("Project", label_style)),
        detail_value_line(&meta.cwd, Style::default().fg(Color::Yellow), width),
        Line::default(),
        Line::from(Span::styled("Branch", label_style)),
        detail_value_line(
            meta.branch.as_deref().unwrap_or(unknown),
            Style::default().fg(Color::Cyan),
            width,
        ),
        Line::default(),
        Line::from(Span::styled("Last message", label_style)),
    ];
    // 固定行数：以上 15 行 + `Prompt` 前的空行 + `Prompt` 标签 + 状态说明 = [`FIXED_DETAIL_LINES`] 行。
    // 余下高度几乎全给 Last message；Prompt 只预留最多 [`PREVIEW_PROMPT_MAX`] 行，
    // 因为它通常是「继续」这类短指令（对齐参考稿里 18 : 1 的占比）。
    let available = (area.height as usize).saturating_sub(FIXED_DETAIL_LINES);
    let last_text = if last.trim().is_empty() { &none } else { last };
    let prompt_text = if prompt.trim().is_empty() {
        &none
    } else {
        prompt
    };
    let prompt_rows = preview_lines(prompt_text, width);
    let prompt_lines = prompt_rows
        .len()
        .min(PREVIEW_PROMPT_MAX)
        .min(available.saturating_sub(1));
    let last_lines = available.saturating_sub(prompt_lines);
    lines.extend(preview_lines(last_text, width).into_iter().take(last_lines));
    lines.push(Line::default());
    lines.push(Line::from(Span::styled("Prompt", label_style)));
    lines.extend(prompt_rows.into_iter().take(prompt_lines));
    lines.push(Line::from(Span::styled(
        app.services.lc.tr("agent-center-status-note"),
        Style::default().fg(palette.subtle),
    )));
    frame.render_widget(
        Paragraph::new(
            lines
                .into_iter()
                .take(area.height as usize)
                .collect::<Vec<_>>(),
        ),
        area,
    );
}

/// 预览正文：按消息区同款链路渲染 markdown（粗体/列表/链接/代码），
/// 再按显示列宽折行（markdown 普通段落不预折行，需显式折行）。
fn preview_lines(text: &str, width: usize) -> Vec<Line<'static>> {
    let width = width.max(1);
    parse_markdown(text, width)
        .lines
        .into_iter()
        .flat_map(|line| wrap_line_spans(line, width))
        .collect()
}

/// 详情面板单行文本：剥离控制字符（标题源自用户消息，含换行会破坏单行布局）。
fn detail_text(text: impl Into<String>) -> String {
    text.into().chars().filter(|ch| !ch.is_control()).collect()
}

/// `Label: value` 单行：标签 muted、值按定位配色，超宽按显示列宽省略。
fn detail_kv_line(
    label: &str,
    value: &str,
    value_style: Style,
    label_style: Style,
    width: usize,
) -> Line<'static> {
    Line::from(truncate_spans_to_width(
        vec![
            Span::styled(label.to_string(), label_style),
            Span::styled(detail_text(value), value_style),
        ],
        width,
    ))
}

/// 块式字段的值行（标签已在上一行独占）：值着色后按显示列宽省略。
fn detail_value_line(value: &str, value_style: Style, width: usize) -> Line<'static> {
    Line::from(truncate_spans_to_width(
        vec![Span::styled(detail_text(value), value_style)],
        width,
    ))
}

fn render_help(frame: &mut Frame, app: &App, area: Rect) {
    overlay(
        frame,
        area,
        &app.services.lc.tr("agent-center-help"),
        theme::cur().claude,
    );
}

fn overlay(frame: &mut Frame, area: Rect, text: &str, color: Color) {
    let width = area.width.saturating_sub(4).min(72);
    let height = area.height.saturating_sub(2).min(17);
    let rect = Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    );
    frame.render_widget(Clear, rect);
    // 底色用 `popup_bg`（弹层专用：暗色主题→黑、亮色主题→白）。
    // 不能用 `background` —— 那是各主题的品牌色（暗色下为青绿 0,188,212），铺满整块弹层过于刺眼。
    frame.render_widget(
        Paragraph::new(text)
            .style(Style::default().fg(color).bg(theme::cur().popup_bg))
            .wrap(Wrap { trim: false }),
        rect,
    );
}

fn line(frame: &mut Frame, area: Rect, text: &str, style: Style) {
    frame.render_widget(
        Paragraph::new(truncate_to_display_width(text, area.width as usize)).style(style),
        area,
    );
}

fn bucket_color(bucket: Bucket) -> Color {
    match bucket {
        Bucket::NeedsYou => theme::cur().error,
        Bucket::Working => theme::cur().success,
        Bucket::Ready => Color::Cyan,
        Bucket::Inactive => theme::cur().inactive,
    }
}

fn updated(time: chrono::DateTime<chrono::Utc>) -> String {
    let seconds = chrono::Utc::now()
        .signed_duration_since(time)
        .num_seconds()
        .max(0);
    if seconds < 60 {
        "now".into()
    } else if seconds < 3600 {
        format!("{}m ago", seconds / 60)
    } else if seconds < 86400 {
        format!("{}h ago", seconds / 3600)
    } else {
        format!("{}d ago", seconds / 86400)
    }
}
