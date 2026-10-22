//! 增长与全屏布局共用的内容测量，所有高度上限以宿主终端为准。

use super::{active_panel_height, bg_agent_bar, message_area, queued_messages, status_bar};
use crate::{app::App, ui::welcome};
use ratatui::layout::Rect;

pub(super) struct BottomHeights {
    pub attachment: u16,
    pub panel: u16,
    pub queued: u16,
    pub input: u16,
    pub status: u16,
    pub background: u16,
}

impl BottomHeights {
    pub(super) fn measure(app: &App, terminal: Rect) -> Self {
        let line_count = app.session_mgr.current().ui.textarea.lines().len();
        Self {
            attachment: if app
                .session_mgr
                .current()
                .metadata
                .pending_attachments
                .is_empty()
            {
                0
            } else {
                3
            },
            panel: active_panel_height(app, terminal.height, terminal.width),
            queued: queued_messages::height(app),
            input: (line_count.saturating_add(2).min(u16::MAX as usize) as u16)
                .min(terminal.height.saturating_mul(2) / 5)
                .max(3),
            status: status_bar::status_bar_height(app),
            background: bg_agent_bar::bg_bar_height(app, terminal.height),
        }
    }

    pub(super) fn total(&self) -> u16 {
        [
            self.attachment,
            self.panel,
            self.queued,
            self.input,
            self.status,
            self.background,
        ]
        .into_iter()
        .fold(0, u16::saturating_add)
    }
}

pub(super) fn welcome_visible(app: &App) -> bool {
    app.session_mgr.current().current_view_messages().is_empty()
        || (app.global_ui.viewport.startup_welcome
            && app.session_mgr.current().focused_instance_id.is_none())
}

pub(super) fn welcome_rows(app: &App, width: u16) -> usize {
    if welcome_visible(app) {
        let card_rows = welcome::welcome_height(app, width) as usize;
        // 首条消息与欢迎卡之间留一行，间距同前缀一起测量和滚动。
        card_rows
            + usize::from(
                card_rows > 0 && !app.session_mgr.current().current_view_messages().is_empty(),
            )
    } else {
        0
    }
}

/// 下一帧所有可见持久区域实际需要的总行数。
/// 临时浮层（`/` 命令提示、`@` 候选等）作为 Overlay 在输入框上方已有可用空间内自适应渲染，
/// 不计入 `required_rows`，避免浮层出现和消失导致视口抖动、留白或意外触发全屏。
pub fn required_rows(app: &mut App, terminal: Rect) -> u16 {
    if app.global_ui.setup_wizard.is_some() || app.global_ui.agent_center.is_some() {
        return terminal.height;
    }
    let bottom = BottomHeights::measure(app, terminal);
    let content = content_rows(app, terminal.width).max(1);
    bottom
        .total()
        .saturating_add(content.min(u16::MAX as usize) as u16)
}

#[cfg(test)]
#[path = "viewport_layout_test.rs"]
mod tests;

fn message_rows(app: &App, width: u16) -> usize {
    let cached_rows = if app.session_mgr.current().current_view_messages().is_empty() {
        0
    } else {
        let session = app.session_mgr.current();
        let cache = session.messages.render_cache.read();
        session
            .ui
            .full_width_message_rows
            .filter(|(measured_width, version, _)| {
                *measured_width == width && *version == cache.version
            })
            .map(|(_, _, rows)| rows)
            .unwrap_or(cache.total_lines)
    };
    cached_rows.saturating_add(message_area::visual_line_count(
        &message_area::visible_tail(app),
        width,
    ))
}

pub(super) fn content_rows(app: &App, width: u16) -> usize {
    message_rows(app, width).saturating_add(welcome_rows(app, width))
}

/// 宽度变化时先等后台重排，避免用上一帧的行数判定不可逆的全屏切换。
pub async fn prepare_width(app: &mut App, width: u16) {
    if width == 0 {
        return;
    }
    let size = app.global_ui.viewport.terminal_size;
    // 先处理之前排队的快照；同宽 barrier 不触发重新渲染。
    let cache_width = app.session_mgr.current().messages.render_cache.read().width;
    let initial_width = if app.global_ui.viewport.fullscreen_latched && cache_width > 0 {
        cache_width
    } else {
        width
    };
    wait_for_width(app, initial_width).await;
    let (cache_width, version, rows) = {
        let cache = app.session_mgr.current().messages.render_cache.read();
        (cache.width, cache.version, cache.total_lines)
    };
    let measured = app.session_mgr.current().ui.full_width_message_rows.filter(
        |(measured_width, measured_version, _)| {
            *measured_width == width && *measured_version == version
        },
    );
    let full_rows = if cache_width == width {
        rows
    } else if let Some((_, _, rows)) = measured {
        rows
    } else {
        // 窄缓存不能用于推断全宽高度，尤其跨过吉祥物与折行的临界列宽时。
        wait_for_width(app, width).await;
        app.session_mgr
            .current()
            .messages
            .render_cache
            .read()
            .total_lines
    };
    let mut target = width;
    if app.global_ui.viewport.fullscreen_latched && width > 1 {
        let terminal = Rect::new(0, 0, width, size.height);
        let available = size
            .height
            .saturating_sub(BottomHeights::measure(app, terminal).total());
        let message_rows = if app.session_mgr.current().current_view_messages().is_empty() {
            0
        } else {
            full_rows
        };
        let content = message_rows
            .saturating_add(welcome_rows(app, width))
            .saturating_add(message_area::visual_line_count(
                &message_area::visible_tail(app),
                width,
            ));
        if content > available as usize {
            target -= 1;
        }
    }
    if app.session_mgr.current().messages.render_cache.read().width != target {
        wait_for_width(app, target).await;
    }
    let version = app
        .session_mgr
        .current()
        .messages
        .render_cache
        .read()
        .version;
    app.session_mgr.current_mut().ui.full_width_message_rows = Some((width, version, full_rows));
}

async fn wait_for_width(app: &mut App, target: u16) {
    // 同宽时也等待此前排队的恢复/子视口/折叠快照，首帧就按目标内容决策。
    let (done, ready) = tokio::sync::oneshot::channel();
    let sender = app.session_mgr.current().messages.render_tx.clone();
    if sender
        .send(crate::ui::render_thread::RenderEvent::ResizeAndWait(
            target, done,
        ))
        .await
        .is_ok()
    {
        let _ = ready.await;
        app.session_mgr.current_mut().messages.last_resize_width = Some(target);
    }
}
