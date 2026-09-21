use ratatui::{layout::Rect, style::Style, widgets::Paragraph, Frame};
use unicode_width::UnicodeWidthStr;

use crate::{
    app::{App, QueuedMessageAction},
    ui::{message_render::truncate_to_display_width, theme},
};

pub(super) fn height(app: &App) -> u16 {
    let count = app.session_mgr.current().messages.pending_messages.len();
    count.min(3) as u16 + u16::from(count > 3)
}

pub(super) fn render(f: &mut Frame, app: &mut App, area: Rect) {
    let lc = &app.services.lc;
    let session = app.session_mgr.current_mut();
    session.ui.queued_message_actions.clear();
    session.ui.queued_messages_area = None;
    let messages = &session.messages.pending_messages;
    if messages.is_empty() {
        session.ui.queued_messages_offset = 0;
        return;
    }
    if area.height == 0 || area.width == 0 {
        return;
    }
    session.ui.queued_messages_area = Some(area);
    // 分页行计入布局高度；小终端只使用实际分配到的行数。
    let paginated = messages.len() > area.height.min(3) as usize;
    let page_size = area.height.saturating_sub(u16::from(paginated)).min(3) as usize;
    if page_size == 0 {
        return;
    }
    let last_page = (messages.len() - 1) / page_size * page_size;
    let offset = (session.ui.queued_messages_offset / page_size * page_size).min(last_page);
    session.ui.queued_messages_offset = offset;
    let visible_count = page_size.min(messages.len() - offset);
    let actions = &mut session.ui.queued_message_actions;
    let pending_style = Style::default().fg(theme::MUTED).bg(theme::USER_BG);
    let active_style = Style::default().fg(theme::ACCENT).bg(theme::USER_BG);
    let queue_label = lc.tr("queue-label");
    for (row, message) in messages.iter().skip(offset).take(visible_count).enumerate() {
        let row_area = Rect::new(area.x, area.y + row as u16, area.width, 1);
        f.render_widget(Paragraph::new("").style(pending_style), row_area);
        let steer_label = format!(
            "[{}]",
            lc.tr(if message.sending {
                "queue-sending"
            } else {
                "queue-steer"
            })
        );
        let delete_label = "[×]";
        let delete_width = UnicodeWidthStr::width(delete_label) as u16;
        let steer_width = UnicodeWidthStr::width(steer_label.as_str()) as u16;
        // 保留删除按钮空间；窄屏时缩短补充按钮，避免命中区越界。
        let delete_width = delete_width.min(row_area.width);
        let delete_area = Rect::new(row_area.right() - delete_width, row_area.y, delete_width, 1);
        let steer_width = steer_width.min(row_area.width.saturating_sub(delete_width + 1));
        let steer_area = Rect::new(
            delete_area
                .x
                .saturating_sub(steer_width + 1)
                .max(row_area.x),
            row_area.y,
            steer_width,
            1,
        );
        let control_style = if message.sending {
            pending_style
        } else {
            active_style
        };
        f.render_widget(
            Paragraph::new(truncate_to_display_width(
                &steer_label,
                steer_width as usize,
            ))
            .style(control_style),
            steer_area,
        );
        f.render_widget(
            Paragraph::new(delete_label).style(control_style),
            delete_area,
        );
        if !message.sending {
            if steer_area.width > 0 {
                actions.push((steer_area, QueuedMessageAction::Steer(message.id)));
            }
            if delete_area.width > 0 {
                actions.push((delete_area, QueuedMessageAction::Delete(message.id)));
            }
        }
        let image_count = if message.attachments.is_empty() {
            String::new()
        } else {
            format!(
                " [{}]",
                lc.tr_args(
                    "queue-attachments",
                    &[("count".into(), (message.attachments.len() as i64).into())]
                )
            )
        };
        let text = message
            .text
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        let preview = format!("{queue_label} {}{image_count}: {text}", offset + row + 1);
        let left_padding = 2.min(row_area.width);
        let preview_area = Rect::new(
            row_area.x + left_padding,
            row_area.y,
            steer_area.x.saturating_sub(row_area.x + left_padding + 1),
            1,
        );
        f.render_widget(
            Paragraph::new(truncate_to_display_width(
                &preview,
                preview_area.width as usize,
            ))
            .style(pending_style),
            preview_area,
        );
    }
    if paginated {
        let row = Rect::new(area.x, area.bottom() - 1, area.width, 1);
        let previous = Rect::new(row.x, row.y, row.width.min(3), 1);
        let next = Rect::new(
            row.right().saturating_sub(3).max(row.x),
            row.y,
            row.width.min(3),
            1,
        );
        f.render_widget(
            Paragraph::new("[‹]").style(if offset > 0 {
                active_style
            } else {
                pending_style
            }),
            previous,
        );
        if next.x >= previous.right() {
            f.render_widget(
                Paragraph::new("[›]").style(if offset + visible_count < messages.len() {
                    active_style
                } else {
                    pending_style
                }),
                next,
            );
            if offset > 0 {
                actions.push((previous, QueuedMessageAction::PreviousPage));
            }
            if offset + visible_count < messages.len() {
                actions.push((next, QueuedMessageAction::NextPage));
            }
        }
        let summary_area = Rect::new(
            previous.right(),
            row.y,
            next.x.saturating_sub(previous.right()),
            1,
        );
        let summary = format!(
            " {}–{}/{}",
            offset + 1,
            offset + visible_count,
            messages.len()
        );
        f.render_widget(
            Paragraph::new(truncate_to_display_width(
                &summary,
                summary_area.width as usize,
            ))
            .style(pending_style),
            summary_area,
        );
    }
}

#[cfg(test)]
#[path = "queued_messages_test.rs"]
mod tests;
