use crate::app::App;
use crate::git::commit::FileStatus;
use crate::ui::toolbar;
use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph, Wrap},
    Frame,
};

pub fn draw(f: &mut Frame, area: Rect, app: &mut App) {
    let block = Block::default()
        .borders(ratatui::widgets::Borders::ALL)
        .title(" Detail ");
    f.render_widget(block, area);
    let inner = area.inner(ratatui::layout::Margin::new(1, 1));

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
    let content_area = Rect::new(inner.x, content_y, inner.width, content_height);

    // 记录 detail 面板信息
    app.detail_content_y = content_y;
    app.detail_viewport = content_height;

    if let Some(detail) = &app.selected_detail {
        let mut lines: Vec<Line> = Vec::new();

        lines.push(Line::from(vec![
            Span::styled("Hash: ", Style::default().fg(Color::Gray)),
            Span::styled(
                detail.short_hash.clone(),
                Style::default().fg(Color::Magenta),
            ),
        ]));
        lines.push(Line::from(vec![
            Span::styled("Author: ", Style::default().fg(Color::Gray)),
            Span::styled(
                format!("{} <{}>", detail.author_name, detail.author_email),
                Style::default().fg(Color::White),
            ),
        ]));
        lines.push(Line::from(""));
        for msg_line in detail.message.lines() {
            lines.push(Line::styled(
                msg_line.to_string(),
                Style::default().fg(Color::White),
            ));
        }

        if !detail.branches.is_empty() {
            lines.push(Line::from(""));
            let branch_spans: Vec<Span> = detail
                .branches
                .iter()
                .flat_map(|b| {
                    vec![
                        Span::styled(
                            format!(" {} ", b),
                            Style::default().fg(Color::White).bg(Color::Magenta),
                        ),
                        Span::raw(" "),
                    ]
                })
                .collect();
            lines.push(Line::from(branch_spans));
        }
        if !detail.tags.is_empty() {
            let tag_spans: Vec<Span> = detail
                .tags
                .iter()
                .flat_map(|t| {
                    vec![
                        Span::styled(
                            format!(" {} ", t),
                            Style::default().fg(Color::Black).bg(Color::Cyan),
                        ),
                        Span::raw(" "),
                    ]
                })
                .collect();
            lines.push(Line::from(tag_spans));
        }

        if let Some(stats) = &app.selected_diff_stats {
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled(
                    format!("{} files changed", stats.files_changed),
                    Style::default().fg(Color::White),
                ),
                Span::raw("  "),
                Span::styled(
                    format!("+{}", stats.insertions),
                    Style::default().fg(Color::Green),
                ),
                Span::raw("  "),
                Span::styled(
                    format!("-{}", stats.deletions),
                    Style::default().fg(Color::Red),
                ),
            ]));
            for file in &stats.files {
                let (status_ch, status_color) = match file.status {
                    FileStatus::Added => ('A', Color::Green),
                    FileStatus::Deleted => ('D', Color::Red),
                    _ => ('M', Color::Yellow),
                };
                lines.push(Line::from(vec![
                    Span::styled(
                        format!("{} ", status_ch),
                        Style::default().fg(status_color),
                    ),
                    Span::styled(file.path.clone(), Style::default().fg(Color::Gray)),
                ]));
            }
        }

        let para = Paragraph::new(lines).wrap(Wrap { trim: true }).scroll((app.detail_scroll, 0));
        // 计算实际行数用于滚动上限
        let line_count = para.line_count(content_area.width) as u16;
        app.detail_total_lines = line_count;
        f.render_widget(para, content_area);
    }
}
