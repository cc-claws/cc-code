use cc_widgets::BorderedPanel;
use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};
use unicode_width::UnicodeWidthStr;

use crate::{
    app::{worktree_ops::same_directory, worktree_panel::WorktreePanel, App},
    ui::theme,
};

pub(crate) fn render_worktree_panel(
    f: &mut Frame,
    panel: &mut WorktreePanel,
    app: &mut App,
    area: Rect,
) {
    let inner = BorderedPanel::new(Span::styled(
        " Worktrees ",
        Style::default()
            .fg(theme::cur().thinking)
            .add_modifier(Modifier::BOLD),
    ))
    .border_style(Style::default().fg(theme::cur().prompt_border))
    .render(f, area);
    app.session_mgr.current_mut().ui.panel_area = Some(inner);
    let height = inner.height as usize;
    panel.list.ensure_visible(inner.height);
    let offset = panel.list.scroll_offset() as usize;
    // 使用同一个滚动起点供键盘导航和鼠标命中计算。
    let lines: Vec<_> = panel
        .list
        .items()
        .iter()
        .enumerate()
        .skip(offset)
        .take(height)
        .map(|(index, tree)| {
            let label = match tree {
                None => "+ Create worktree (new conversation, detached HEAD)".to_string(),
                Some(tree) => {
                    let branch = crate::ui::message_render::truncate_to_display_width(
                        tree.branch.as_deref().unwrap_or("detached"),
                        (inner.width as usize / 4).clamp(4, 24),
                    );
                    let prefix = format!(
                        "{}. {} {}{}{}  ",
                        index,
                        branch,
                        if same_directory(&tree.path, &panel.current_cwd) {
                            "✔"
                        } else {
                            ""
                        },
                        if tree.locked { " [locked]" } else { "" },
                        if tree.prunable || tree.bare {
                            " [unavailable]"
                        } else {
                            ""
                        }
                    );
                    let path = directory_suffix(
                        &tree.path,
                        (inner.width as usize).saturating_sub(3 + prefix.width()),
                    );
                    format!("{prefix}{path}")
                }
            };
            let focused = index == panel.list.cursor();
            Line::from(Span::styled(
                format!(
                    " {} {label}",
                    if focused { theme::SYMBOL_CURSOR } else { " " }
                ),
                Style::default().fg(if focused {
                    theme::cur().thinking
                } else {
                    theme::cur().text
                }),
            ))
        })
        .collect();
    f.render_widget(Paragraph::new(lines), inner);
}

fn directory_suffix(path: &str, width: usize) -> String {
    let path = crate::app::tool_display::sanitize_display_text(path);
    if path.width() <= width {
        return path;
    }
    if width == 0 {
        return String::new();
    }
    // 优先保留目录末尾的唯一标识，避免长路径都只剩相同的存储目录前缀。
    let mut used = 0;
    let tail: String = path
        .chars()
        .rev()
        .take_while(|ch| {
            used += unicode_width::UnicodeWidthChar::width(*ch).unwrap_or(0);
            used < width
        })
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    format!("…{tail}")
}
