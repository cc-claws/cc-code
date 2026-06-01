use ratatui::{
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph},
    Frame,
};

use crate::{app::plugin_panel::PluginPanel, ui::theme};

/// 渲染搜索框到固定区域（不参与滚动）
pub(crate) fn render_discover_search_box(f: &mut Frame, panel: &PluginPanel, area: Rect) {
    if area.width < 4 || area.height < 3 {
        return;
    }

    let search_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(if panel.discover_searching {
            theme::ACCENT
        } else {
            theme::DIM
        }));

    let search_inner = search_block.inner(area);

    let query_val = panel.discover_search.value();
    let content_line = if query_val.is_empty() && !panel.discover_searching {
        Line::from(vec![
            Span::styled(" \u{2315} ", Style::default().fg(theme::MUTED)),
            Span::styled("Search plugins\u{2026}", Style::default().fg(theme::DIM)),
        ])
    } else {
        let mut spans = vec![
            Span::styled(" \u{2315} ", Style::default().fg(theme::MUTED)),
            Span::styled(
                panel.discover_search.display_text('\u{2022}'),
                Style::default().fg(theme::TEXT),
            ),
        ];
        if panel.discover_searching {
            spans.push(Span::styled("\u{2588}", Style::default().fg(theme::TEXT)));
        }
        Line::from(spans)
    };

    let search_para = Paragraph::new(content_line);
    f.render_widget(search_block, area);
    f.render_widget(search_para, search_inner);
}
