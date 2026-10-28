//! 增长与全屏布局共用的内容测量，所有高度上限以宿主终端为准。

use super::{active_panel_height, bg_agent_bar, message_area, queued_messages, status_bar};
use crate::{
    app::App,
    ui::{
        render_thread::{PreparedWidth, RenderEvent, WidthPreparationRequest},
        welcome,
    },
};
use ratatui::layout::{Rect, Size};

/// 只在目标布局就绪后提交视口计划，等待期间继续保留上一帧。
pub fn prepare_viewport(
    app: &mut App,
    size: Size,
) -> Option<crate::terminal_backend::viewport::ViewportPlan> {
    let previous_size = app.global_ui.viewport.terminal_size;
    if previous_size != Size::default() && previous_size != size {
        crate::terminal_backend::viewport::mark_screen_reflow();
        app.global_ui.terminal_reflow_pending = true;
    }
    app.global_ui.viewport.terminal_size = size;
    app.global_ui.viewport.sync_welcome(
        app.session_mgr.current().metadata.session_id,
        !app.session_mgr.current().messages.view_messages.is_empty(),
    );
    let fullscreen = app.global_ui.viewport.fullscreen_latched
        || app
            .session_mgr
            .current()
            .messages
            .width_preparation
            .entering_fullscreen;
    if !try_prepare_width(app, size.width, fullscreen) {
        return None;
    }
    let required = required_rows(app, Rect::new(0, 0, size.width, size.height));
    let mut next_viewport = app.global_ui.viewport.clone();
    let plan = next_viewport.prepare(required, size);
    if plan.enter_fullscreen && !fullscreen {
        // 窄宽首帧就绪前不能锁定全屏或改变真实终端模式。
        app.session_mgr
            .current_mut()
            .messages
            .width_preparation
            .entering_fullscreen = true;
        if !try_prepare_width(app, size.width, true) {
            return None;
        }
    }
    app.session_mgr
        .current_mut()
        .messages
        .width_preparation
        .entering_fullscreen = false;
    app.global_ui.viewport = next_viewport;
    Some(plan)
}

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
        // 审批弹窗激活时（对齐参考实现）：输入框、状态栏、后台栏全部让位，
        // 只在弹窗底部保留 `Esc to cancel · Tab to amend` 一行由弹窗自身绘制。
        let approval_active = matches!(
            app.session_mgr.current().agent.interaction_prompt,
            Some(crate::app::InteractionPrompt::Approval(_))
        );
        if approval_active {
            // 欢迎卡（logo + 提示行）是消息区顶部固定块，没有被裁切后自动滚走的余地：
            // 面板高度必须给它留出完整行数，否则首行边框会被吃掉。
            let reserved = u16::try_from(welcome_rows(app, terminal.width)).unwrap_or(0);
            let panel = active_panel_height(app, terminal.height, terminal.width)
                .min(terminal.height.saturating_sub(reserved));
            return Self {
                attachment: 0,
                panel,
                queued: 0,
                input: 0,
                status: 0,
                background: 0,
            };
        }
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
        if cache.width == width {
            // 虚拟列表物化新窗口后会修正估计高度，同宽时直接采用最新测量。
            cache.total_lines
        } else {
            session
                .ui
                .full_width_message_rows
                .filter(|(measured_width, revision, _)| {
                    *measured_width == width && *revision == session.messages.layout_revision()
                })
                .map(|(_, _, rows)| rows)
                .unwrap_or(cache.total_lines)
        }
    };
    cached_rows.saturating_add(message_area::visual_line_count(
        &message_area::visible_tail(app),
        width,
    ))
}

pub(super) fn content_rows(app: &App, width: u16) -> usize {
    message_rows(app, width).saturating_add(welcome_rows(app, width))
}

#[derive(Clone, PartialEq, Eq)]
struct WidthContext {
    request: WidthPreparationRequest,
    epoch: u64,
    thread: Option<cc_agent::thread::ThreadId>,
    focused_instance: Option<String>,
    theme: u64,
}

struct PendingWidth {
    revision: u64,
    ready: tokio::sync::oneshot::Receiver<PreparedWidth>,
}

#[derive(Default)]
pub struct WidthPreparation {
    context: Option<WidthContext>,
    pending: Option<PendingWidth>,
    revision: Option<u64>,
    width: u16,
    measured_rows: usize,
    /// 全屏计划必须等窄宽首帧就绪后才提交给终端。
    pub entering_fullscreen: bool,
}

/// 只投递/收取布局准备，不等待 worker。稳定布局滚动没有宽度屏障。
pub fn try_prepare_width(app: &mut App, width: u16, fullscreen: bool) -> bool {
    if width == 0 {
        return true;
    }
    let size = app.global_ui.viewport.terminal_size;
    let session = app.session_mgr.current();
    let context = WidthContext {
        request: WidthPreparationRequest {
            width,
            available_rows: size.height.saturating_sub(
                BottomHeights::measure(app, Rect::new(0, 0, width, size.height)).total(),
            ) as usize,
            extra_rows: welcome_rows(app, width).saturating_add(message_area::visual_line_count(
                &message_area::visible_tail(app),
                width,
            )),
            has_messages: !session.current_view_messages().is_empty(),
            fullscreen,
        },
        epoch: session.messages.layout_epoch(),
        thread: session.current_thread_id.clone(),
        focused_instance: session.focused_instance_id.clone(),
        theme: crate::ui::theme::fingerprint(),
    };
    let revision = session.messages.layout_revision();
    let mut state = std::mem::take(&mut app.session_mgr.current_mut().messages.width_preparation);
    if state.context.as_ref() != Some(&context) {
        state.context = Some(context.clone());
        state.pending = None;
        state.revision = None;
        app.session_mgr.current_mut().ui.full_width_message_rows = None;
    }
    let mut completed = false;
    if let Some(pending) = &mut state.pending {
        match pending.ready.try_recv() {
            Ok(prepared) => {
                state.revision = Some(pending.revision);
                state.width = prepared.width;
                state.measured_rows = prepared.measured_rows;
                // 测量只适用于同一批布局，不能把后续 viewport/tick 的发布当成新布局。
                app.session_mgr.current_mut().ui.full_width_message_rows =
                    Some((width, pending.revision, prepared.full_width_rows));
                state.pending = None;
                completed = true;
            }
            Err(tokio::sync::oneshot::error::TryRecvError::Empty) => {
                app.session_mgr.current_mut().messages.width_preparation = state;
                return false;
            }
            Err(tokio::sync::oneshot::error::TryRecvError::Closed) => {
                state.pending = None;
                state.revision = None;
            }
        }
    }
    let (cache_width, cache_rows) = {
        let session = app.session_mgr.current();
        let cache = session.messages.render_cache.read();
        (cache.width, cache.total_lines)
    };
    let request = context.request;
    let fit_changed = !completed
        && request.fullscreen
        && width > 1
        && if cache_width == width {
            let rows = if request.has_messages { cache_rows } else { 0 };
            rows.saturating_add(request.extra_rows) > request.available_rows
        } else if cache_width == width - 1 && cache_rows != state.measured_rows {
            let rows = if request.has_messages { cache_rows } else { 0 };
            let extra = welcome_rows(app, cache_width).saturating_add(
                message_area::visual_line_count(&message_area::visible_tail(app), cache_width),
            );
            rows.saturating_add(extra) <= request.available_rows
        } else {
            false
        };
    // 流式内容可在等待期间推进；先呈现已经完成的同视图布局，下一帧再追最新版本，
    // 避免持续输出让确认永远落后并饿死绘制。历史/模式变化由 context 隔离。
    if (state.revision == Some(revision) || completed) && cache_width == state.width && !fit_changed
    {
        app.session_mgr.current_mut().messages.last_resize_width = Some(state.width);
        app.session_mgr.current_mut().messages.width_preparation = state;
        return true;
    }
    let (reply, ready) = tokio::sync::oneshot::channel();
    if app
        .session_mgr
        .current()
        .messages
        .render_tx
        .try_send(RenderEvent::PrepareWidth {
            request: context.request,
            reply,
        })
        .is_ok()
    {
        state.pending = Some(PendingWidth { revision, ready });
    }
    // 通道满时不记为已投递，下一次绘制期限重试。
    app.session_mgr.current_mut().messages.width_preparation = state;
    false
}

/// Headless 调用方可显式等待；生产主循环只调用 try_prepare_width。
#[cfg(any(test, feature = "headless"))]
pub async fn prepare_width(app: &mut App, width: u16) {
    while !try_prepare_width(app, width, app.global_ui.viewport.fullscreen_latched) {
        tokio::task::yield_now().await;
    }
}

#[cfg(test)]
#[path = "width_preparation_test.rs"]
mod width_preparation_tests;

#[cfg(test)]
#[path = "width_recovery_test.rs"]
mod width_recovery_tests;
