use crate::app::App;
use crate::git::commit::FileStatus;
use crate::ui::toolbar;
use peri_widgets::Theme as _;
use ratatui::{
    layout::{Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap},
    Frame,
};

pub fn draw(f: &mut Frame, area: Rect, app: &mut App) {
    let theme = &app.theme;
    let block = Block::default()
        .borders(ratatui::widgets::Borders::ALL)
        .title(" Detail ")
        .border_style(Style::default().fg(theme.border()));
    f.render_widget(block, area);
    let inner = area.inner(Margin::new(1, 1));

    // 工具栏占顶部 1 行
    if inner.height > 1 {
        let toolbar_area = Rect::new(inner.x, inner.y, inner.width, 1);
        let buttons = toolbar::commit_buttons(app);
        toolbar::draw_toolbar(f, toolbar_area, &buttons, &mut app.toolbar_state);
    }

    // 详情内容从 toolbar 下方开始
    let content_y = inner.y + 1;
    let content_height = inner.height.saturating_sub(1);
    if content_height == 0 {
        return;
    }
    // 为 scrollbar 预留最右 1 列
    let content_width = inner.width.saturating_sub(1);
    let content_area = Rect::new(inner.x, content_y, content_width, content_height);

    app.detail_content_y = content_y;
    app.detail_viewport = content_height;

    if let Some(detail) = &app.selected_detail {
        let mut lines: Vec<Line> = Vec::new();

        // Hash 行
        lines.push(Line::from(vec![
            Span::styled(
                " Hash   ",
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Rgb(100, 60, 140)),
            ),
            Span::raw(" "),
            Span::styled(
                detail.short_hash.clone(),
                Style::default()
                    .fg(Color::Magenta)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));

        // Author 行
        lines.push(Line::from(vec![
            Span::styled(
                " Author ",
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Rgb(60, 100, 140)),
            ),
            Span::raw(" "),
            Span::styled(
                format!("{} <{}>", detail.author_name, detail.author_email),
                Style::default().fg(Color::White),
            ),
        ]));

        // commit message
        lines.push(Line::from(""));
        for msg_line in detail.message.lines() {
            lines.push(Line::styled(
                msg_line.to_string(),
                Style::default().fg(Color::White),
            ));
        }

        // 分支徽章（白字 + 彩色背景 + BOLD，与 Graph 面板一致）
        if !detail.branches.is_empty() {
            lines.push(Line::from(""));
            let branch_spans: Vec<Span> = detail
                .branches
                .iter()
                .flat_map(|b| {
                    vec![
                        Span::styled(
                            format!(" {} ", b),
                            Style::default()
                                .fg(Color::White)
                                .bg(Color::Magenta)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::raw(" "),
                    ]
                })
                .collect();
            lines.push(Line::from(branch_spans));
        }

        // 标签徽章（黑字 + 青色背景 + BOLD，与 Graph 面板一致）
        if !detail.tags.is_empty() {
            let tag_spans: Vec<Span> = detail
                .tags
                .iter()
                .flat_map(|t| {
                    vec![
                        Span::styled(
                            format!(" {} ", t),
                            Style::default()
                                .fg(Color::Black)
                                .bg(Color::Cyan)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::raw(" "),
                    ]
                })
                .collect();
            lines.push(Line::from(tag_spans));
        }

        // diff 统计
        if let Some(stats) = &app.selected_diff_stats {
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled(
                    format!(" {} files ", stats.files_changed),
                    Style::default().fg(Color::White).bg(Color::Rgb(60, 60, 80)),
                ),
                Span::raw(" "),
                Span::styled(
                    format!(" +{} ", stats.insertions),
                    Style::default()
                        .fg(Color::White)
                        .bg(Color::Rgb(30, 100, 30)),
                ),
                Span::raw(" "),
                Span::styled(
                    format!(" -{} ", stats.deletions),
                    Style::default()
                        .fg(Color::White)
                        .bg(Color::Rgb(120, 30, 30)),
                ),
            ]));

            // 文件列表（状态徽章样式）
            for file in &stats.files {
                let (status_ch, status_bg) = match file.status {
                    FileStatus::Added => ('A', Color::Rgb(30, 100, 30)),
                    FileStatus::Deleted => ('D', Color::Rgb(120, 30, 30)),
                    _ => ('M', Color::Rgb(120, 100, 20)),
                };
                lines.push(Line::from(vec![
                    Span::styled(
                        format!(" {} ", status_ch),
                        Style::default()
                            .fg(Color::White)
                            .bg(status_bg)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(" "),
                    Span::styled(file.path.clone(), Style::default().fg(Color::Gray)),
                ]));
            }
        }

        // 计算实际行数（wrap 后）并渲染
        let para = Paragraph::new(lines)
            .wrap(Wrap { trim: true })
            .scroll((app.detail_scroll, 0));
        let line_count = para.line_count(content_area.width) as u16;
        app.detail_total_lines = line_count;

        // clamp scroll
        let max_scroll = line_count.saturating_sub(content_height);
        if app.detail_scroll > max_scroll {
            app.detail_scroll = max_scroll;
        }

        f.render_widget(para, content_area);

        // 渲染滚动条
        if line_count > content_height {
            let scrollbar_area = Rect::new(
                inner.x + inner.width.saturating_sub(1),
                content_y,
                1,
                content_height,
            );
            let mut scrollbar_state =
                ScrollbarState::new(max_scroll as usize).position(app.detail_scroll as usize);
            f.render_stateful_widget(
                Scrollbar::new(ScrollbarOrientation::VerticalRight),
                scrollbar_area,
                &mut scrollbar_state,
            );
        }
    }
}
