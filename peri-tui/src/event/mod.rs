// ── Event module ──────────────────────────────────────────────────────────────
// Split from the original monolithic event.rs (1447 lines) into:
//   mouse.rs   — mouse coordinate helpers + clipboard functions
//   keyboard.rs — key event handler
//   macros.rs  — panel dispatch macros (with_global_panels!, with_session_panels!)
//   mod.rs     — Action, event loop, dispatcher, OAuth handling

pub mod keyboard;
mod macros;
pub mod mouse;

use crate::{with_global_panels, with_session_panels};

use anyhow::Result;
use ratatui::crossterm::event::{
    self, Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind,
};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tui_textarea::{Input, Key};

use crate::app::{
    panel_manager::{EventResult, PanelKind},
    App, MessageScrollbarMetrics,
};

// ── Mouse event liveness tracking ────────────────────────────────────────────
// Tracks the timestamp (millis since epoch) of the last mouse event processed
// by `handle_event`. The main event loop reads this to detect prolonged mouse
// silence and refresh ConPTY mouse tracking if needed.
static LAST_MOUSE_EVENT_MS: AtomicU64 = AtomicU64::new(0);

fn record_mouse_event() {
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    LAST_MOUSE_EVENT_MS.store(ms, Ordering::Relaxed);
}

/// Returns millis since epoch of the last mouse event, or 0 if none recorded.
pub fn last_mouse_event_ms() -> u64 {
    LAST_MOUSE_EVENT_MS.load(Ordering::Relaxed)
}

// ── Action ──────────────────────────────────────────────────────────────────

pub enum Action {
    Quit,
    Submit(String),
    RunShellCommand(String),
    Redraw,
}

// ── Event loop ──────────────────────────────────────────────────────────────

pub async fn next_event(app: &mut App) -> Result<Option<Action>> {
    // Quit-pending state auto-expires after 2s; trigger redraw so the shortcut bar
    // returns to normal.  Must match the window used by handle_ctrl_c().
    if let Some(since) = app.global_ui.quit_pending_since {
        if since.elapsed() >= std::time::Duration::from_secs(2) {
            app.global_ui.quit_pending_since = None;
            return Ok(Some(Action::Redraw));
        }
    }

    // 全屏 alternate screen + EnableMouseCapture 后鼠标必然可用，无需探测。
    if !event::poll(Duration::from_millis(50))? {
        return Ok(None);
    }

    let ev = event::read()?;

    // Drag event coalescing: keep scrollbar/text-selection dragging responsive
    // without discarding mouse wheel distance.
    let ev = coalesce_drag_events(ev);

    // Simulated-paste detection: on terminals without bracketed paste support
    // (Windows), multi-line paste arrives as a rapid burst of key events.
    // Detect this pattern and convert to Event::Paste so the normal paste
    // handler inserts the full text into the textarea.
    let ev = detect_simulated_paste(ev);

    handle_event(app, ev).await
}

// ── Mouse drag coalescing ────────────────────────────────────────────────

/// Coalesces rapid-fire left-drag events from the crossterm queue.
///
/// Mouse wheel events intentionally bypass this path: a crossterm
/// `ScrollUp`/`ScrollDown` event does not carry a repeat count, so draining
/// several wheel events into one event makes fast scrolling move only one
/// three-line step.
fn coalesce_drag_events(ev: Event) -> Event {
    // Only activate coalescing for left-drag mouse events.
    match &ev {
        Event::Mouse(m) => match m.kind {
            MouseEventKind::Drag(MouseButton::Left) => {}
            _ => return ev,
        },
        _ => return ev,
    }

    let mut last_ev = ev;

    // Drain all queued drag events, keeping only the last one.
    // Non-drag events terminate the drain and become the result
    // so they are not lost.
    while event::poll(Duration::ZERO).unwrap_or(false) {
        let next = match event::read() {
            Ok(e) => e,
            Err(_) => break,
        };
        match &next {
            Event::Mouse(m) => match m.kind {
                MouseEventKind::Drag(MouseButton::Left) => {
                    last_ev = next;
                }
                // Other mouse events (click, release, move): stop draining,
                // return this event instead so it's handled normally.
                _ => {
                    last_ev = next;
                    break;
                }
            },
            // Non-mouse events: stop draining, return this event
            _ => {
                last_ev = next;
                break;
            }
        }
    }

    last_ev
}

// ── Simulated-paste detection (Windows) ───────────────────────────────

/// On terminals that do not support bracketed paste (e.g. Windows cmd.exe,
/// legacy PowerShell), multi-line paste is simulated as a rapid burst of
/// individual Key events — each character becomes a Char event and each
/// newline becomes a bare Enter event.
///
/// This function detects that pattern from the first key in a burst. Waiting
/// until Enter is too late: the first pasted line has already been inserted as
/// normal typing, which splits one external paste into raw text plus multiple
/// placeholders.
///
/// A 1 ms start window is too short for human typing to trigger in practice.
/// Once a burst is detected, a small idle window lets slower Windows terminals
/// deliver the rest of the paste without fragmenting it at every newline.
fn detect_simulated_paste(ev: Event) -> Event {
    const START_WINDOW: Duration = Duration::from_millis(1);
    const IDLE_WINDOW: Duration = Duration::from_millis(15);

    if !is_simulated_paste_start(&ev) {
        return ev;
    }

    // Quick probe: any queued event within 1 ms?
    if !event::poll(START_WINDOW).unwrap_or(false) {
        return ev; // No queued events → manual typing / manual Enter
    }

    let original_ev = ev.clone();
    let mut text = String::new();
    let _ = key_event_to_text(ev, &mut text);
    let mut meaningful_after_first = false;

    while event::poll(IDLE_WINDOW).unwrap_or(false) {
        match event::read() {
            Ok(next) => {
                meaningful_after_first |= key_event_to_text(next, &mut text);
            }
            Err(_) => break,
        }
    }

    // A key release queued behind the press is not a paste. It is safe that the
    // release event was consumed because the TUI only acts on key presses.
    if !meaningful_after_first {
        return original_ev;
    }

    Event::Paste(text)
}

fn is_simulated_paste_start(ev: &Event) -> bool {
    match ev {
        Event::Key(k) if k.kind == KeyEventKind::Press => match k.code {
            KeyCode::Char(_) | KeyCode::Tab => {
                !k.modifiers.contains(KeyModifiers::CONTROL)
                    && !k.modifiers.contains(KeyModifiers::ALT)
            }
            KeyCode::Enter => k.modifiers == KeyModifiers::NONE,
            _ => false,
        },
        _ => false,
    }
}

/// Append a single crossterm `Event` into `text` for simulated-paste
/// reconstruction. Key(Char) appends the character; Key(Enter) appends
/// `\n`; Key(Tab) appends `\t`; Key(Backspace) removes the last char;
/// everything else (modifiers, non-printable keys) terminates the drain.
fn key_event_to_text(ev: Event, text: &mut String) -> bool {
    match ev {
        Event::Key(k) if k.kind != KeyEventKind::Release => match k.code {
            KeyCode::Char(c) => {
                // Ctrl+char or Alt+char during paste → stop collecting
                if k.modifiers.contains(KeyModifiers::CONTROL)
                    || k.modifiers.contains(KeyModifiers::ALT)
                {
                    // Flush remaining: stop collecting but don't lose the event.
                    // Since we can't re-inject, treat modifier+char as literal.
                    text.push(c);
                } else {
                    text.push(c);
                }
                true
            }
            KeyCode::Enter => {
                text.push('\n');
                true
            }
            KeyCode::Tab => {
                text.push('\t');
                true
            }
            KeyCode::Backspace => {
                text.pop();
                true
            }
            _ => false, // Ignore other keys (arrows, etc.) during paste
        },
        Event::Mouse(_) | Event::FocusGained | Event::FocusLost | Event::Resize(_, _) => {
            // Non-key events shouldn't appear in a paste burst; stop collecting.
            false
        }
        Event::Paste(p) => {
            // Rare: a real Paste event appeared mid-burst (shouldn't happen).
            text.push_str(&p);
            true
        }
        _ => false,
    }
}

// ── Event dispatcher ────────────────────────────────────────────────────────

fn point_in_rect(column: u16, row: u16, area: ratatui::layout::Rect) -> bool {
    column >= area.x && column < area.x + area.width && row >= area.y && row < area.y + area.height
}

/// 消息区滚动条命中横向容差：滚动条视觉仅 1 列（bar_area.width == 1），
/// 宽屏终端下极难精确点中（280 列里只占 1 列），导致"有时能拖、有时不能"。
/// 向左扩展若干列作为点击命中区——视觉仍是 1 列细线，仅放大可点击范围。
/// 消息区文字为只读 Paragraph、无点击交互，让出几列不影响其他功能。
const MESSAGE_SCROLLBAR_HIT_PAD: u16 = 6;

fn point_in_hit_bar(column: u16, row: u16, bar_area: ratatui::layout::Rect) -> bool {
    let hit_x_start = bar_area.x.saturating_sub(MESSAGE_SCROLLBAR_HIT_PAD);
    column >= hit_x_start
        && column <= bar_area.x
        && row >= bar_area.y
        && row < bar_area.y + bar_area.height
}

fn handle_message_scrollbar_down(app: &mut App, row: u16, column: u16) -> bool {
    let Some(metrics) = app.session_mgr.current().ui.message_scrollbar_metrics else {
        return false;
    };

    if let Some(btn) = metrics.up_btn_area {
        if point_in_rect(column, row, btn) {
            set_message_scroll_offset(app, 0);
            return true;
        }
    }

    if let Some(btn) = metrics.down_btn_area {
        if point_in_rect(column, row, btn) {
            set_message_scroll_offset(app, metrics.max_offset);
            return true;
        }
    }

    if point_in_hit_bar(column, row, metrics.bar_area) && metrics.max_offset > 0 {
        let new_offset = message_scrollbar_offset_for_row(metrics, row);
        set_message_scroll_offset(app, new_offset);
        app.session_mgr.current_mut().ui.message_scrollbar_dragging = true;
        return true;
    }

    false
}

fn handle_message_scrollbar_drag(app: &mut App, row: u16) -> bool {
    let dragging = app.session_mgr.current().ui.message_scrollbar_dragging;
    if !dragging {
        return false;
    }

    let Some(metrics) = app.session_mgr.current().ui.message_scrollbar_metrics else {
        app.session_mgr.current_mut().ui.message_scrollbar_dragging = false;
        return true;
    };

    let new_offset = message_scrollbar_offset_for_row(metrics, row);
    set_message_scroll_offset(app, new_offset);
    true
}

fn message_scrollbar_offset_for_row(metrics: MessageScrollbarMetrics, row: u16) -> usize {
    let bar_inner_height = metrics.bar_area.height.saturating_sub(2);
    if bar_inner_height == 0 || metrics.max_offset == 0 {
        return 0;
    }

    let rel_y = row
        .saturating_sub(metrics.bar_area.y.saturating_add(1))
        .min(bar_inner_height);
    ((metrics.max_offset as u128 * rel_y as u128) / bar_inner_height as u128)
        .min(usize::MAX as u128) as usize
}

fn set_message_scroll_offset(app: &mut App, offset: usize) {
    let ui = &mut app.session_mgr.current_mut().ui;
    let max_scroll = ui.scrollbar_max_offset;
    let min_scroll = ui.scrollbar_min_offset.min(max_scroll);
    let offset = offset.clamp(min_scroll, max_scroll);
    ui.scroll_offset = offset;
    ui.scroll_follow = offset >= max_scroll;
}

/// Core event-handling logic (extracted from `next_event` to avoid duplicating
/// the probe and normal paths).
async fn handle_event(app: &mut App, ev: Event) -> Result<Option<Action>> {
    match ev {
        Event::FocusGained => {
            app.focused = true;
            return Ok(Some(Action::Redraw));
        }
        Event::FocusLost => {
            app.focused = false;
            return Ok(Some(Action::Redraw));
        }
        Event::Resize(_, _) => {
            // Width sync is now driven by render_messages (compares cache.width vs text_area.width)
            app.session_mgr.current_mut().ui.text_selection.clear();
        }
        Event::Key(key_event) => {
            return keyboard::handle_key_event(app, key_event);
        }
        Event::Paste(text) => {
            // Paste text handling
            // Some terminals (e.g. VSCode) use \r instead of \n as line separator in bracketed paste
            let text = text.replace('\r', "\n");

            // Setup wizard open — paste into active field
            if let Some(wizard) = &mut app.global_ui.setup_wizard {
                wizard.paste_text(&text);
                return Ok(Some(Action::Redraw));
            }

            // ─── 交互弹窗优先路由（AskUser/HITL/OAuth） ──────────────────
            // 弹窗激活时，Paste（含终端 IME 组合后的中文）应进入弹窗
            // 而非 textarea。仅 AskUser 弹窗有 custom_input 接收文本。
            if app.is_interaction_popup_active() {
                app.paste_to_interaction_popup(&text);
                return Ok(Some(Action::Redraw));
            }

            // ─── PanelManager paste dispatch ────────────────────────────
            {
                // Session panels: Model, Agent, Hooks, Login, Config, ThreadBrowser, CommandPalette
                let session_kind = app.session_mgr.current_mut().session_panels.active_kind();
                if matches!(
                    session_kind,
                    Some(PanelKind::Model)
                        | Some(PanelKind::Agent)
                        | Some(PanelKind::Hooks)
                        | Some(PanelKind::Login)
                        | Some(PanelKind::Config)
                        | Some(PanelKind::ThreadBrowser)
                        | Some(PanelKind::CommandPalette)
                ) {
                    with_session_panels!(app, |sp, ctx| sp.dispatch_paste(&text, &mut ctx));
                    return Ok(Some(Action::Redraw));
                }

                // Global panels: Status, Memory, Mcp, Cron, Plugin, Tasks
                let global_kind = app.global_panels.active_kind();
                if matches!(
                    global_kind,
                    Some(PanelKind::Status)
                        | Some(PanelKind::Memory)
                        | Some(PanelKind::Mcp)
                        | Some(PanelKind::Cron)
                        | Some(PanelKind::Plugin)
                        | Some(PanelKind::Tasks)
                ) {
                    with_global_panels!(app, |pm, ctx| pm.dispatch_paste(&text, &mut ctx));
                    return Ok(Some(Action::Redraw));
                }
            }

            // Fallback: paste into textarea
            // 弹窗激活时不写入 textarea——用户应通过弹窗 UI 交互
            if !app.is_interaction_popup_active() {
                app.paste_text_into_textarea(&text);
            }
        }
        Event::Mouse(mouse) => {
            record_mouse_event();
            match mouse.kind {
            // ── AskUser 弹窗鼠标交互（优先于面板/消息区） ────────────────────────
            MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
                {
                    if let Some(crate::app::InteractionPrompt::Questions(_)) =
                        app.session_mgr.current_mut().agent.interaction_prompt
                    {
                        if let Some(area) = app.session_mgr.current_mut().ui.panel_area {
                            if mouse::mouse_in_rect(&mouse, area) {
                                let delta = if matches!(mouse.kind, MouseEventKind::ScrollUp) {
                                    -3
                                } else {
                                    3
                                };
                                app.ask_user_scroll(delta);
                                return Ok(Some(Action::Redraw));
                            }
                        }
                    }
                }
                // 正常滚动处理
                match mouse.kind {
                    MouseEventKind::ScrollUp => {
                        let panel_area = app.session_mgr.current_mut().ui.panel_area;
                        if let Some(area) = panel_area {
                            if mouse::mouse_in_rect(&mouse, area) {
                                // Session panel takes priority
                                let sp = &app.session_mgr.current_mut().session_panels;
                                if sp.is_any_open() {
                                    let result = with_session_panels!(app, |sp, ctx| {
                                        sp.dispatch_scroll(-3, &mut ctx)
                                    });
                                    if result == EventResult::Consumed {
                                        return Ok(Some(Action::Redraw));
                                    }
                                }
                                // Global panel
                                if app.global_panels.is_any_open() {
                                    let result = with_global_panels!(app, |pm, ctx| {
                                        pm.dispatch_scroll(-3, &mut ctx)
                                    });
                                    if result == EventResult::Consumed {
                                        return Ok(Some(Action::Redraw));
                                    }
                                }
                            }
                        }
                        app.scroll_up();
                        return Ok(Some(Action::Redraw));
                    }
                    MouseEventKind::ScrollDown => {
                        let panel_area = app.session_mgr.current_mut().ui.panel_area;
                        if let Some(area) = panel_area {
                            if mouse::mouse_in_rect(&mouse, area) {
                                // Session panel takes priority
                                let sp = &app.session_mgr.current_mut().session_panels;
                                if sp.is_any_open() {
                                    let result = with_session_panels!(app, |sp, ctx| {
                                        sp.dispatch_scroll(3, &mut ctx)
                                    });
                                    if result == EventResult::Consumed {
                                        return Ok(Some(Action::Redraw));
                                    }
                                }
                                // Global panel
                                if app.global_panels.is_any_open() {
                                    let result = with_global_panels!(app, |pm, ctx| {
                                        pm.dispatch_scroll(3, &mut ctx)
                                    });
                                    if result == EventResult::Consumed {
                                        return Ok(Some(Action::Redraw));
                                    }
                                }
                            }
                        }
                        app.scroll_down();
                        return Ok(Some(Action::Redraw));
                    }
                    _ => unreachable!(),
                }
            }
            MouseEventKind::Down(MouseButton::Left) => {
                // ── AskUser 弹窗滚动条点击（优先于面板滚动条） ──────────────────
                {
                    if let Some(crate::app::InteractionPrompt::Questions(ref p)) =
                        app.session_mgr.current_mut().agent.interaction_prompt
                    {
                        if let Some(metrics) = p.scrollbar_metrics {
                            if mouse.column >= metrics.bar_area.x
                                && mouse.column < metrics.bar_area.x + metrics.bar_area.width
                                && mouse.row >= metrics.bar_area.y
                                && mouse.row < metrics.bar_area.bottom()
                                && metrics.max_offset > 0
                            {
                                let bar_inner_height = metrics.bar_area.height.saturating_sub(2);
                                if bar_inner_height > 0 {
                                    let rel_y = (mouse.row.saturating_sub(metrics.bar_area.y + 1))
                                        .min(bar_inner_height);
                                    let new_offset = ((rel_y as f64 / bar_inner_height as f64)
                                        * metrics.max_offset as f64)
                                        as u16;
                                    let new_offset = new_offset.min(metrics.max_offset);
                                    if let Some(crate::app::InteractionPrompt::Questions(p)) = app
                                        .session_mgr
                                        .current_mut()
                                        .agent
                                        .interaction_prompt
                                        .as_mut()
                                    {
                                        p.scroll_offset = new_offset;
                                    }
                                }
                                return Ok(Some(Action::Redraw));
                            }
                        }
                    }
                }
                if handle_message_scrollbar_down(app, mouse.row, mouse.column) {
                    return Ok(Some(Action::Redraw));
                }

                // Panel scrollbar: ▲/▼ buttons and bar click/drag
                // Must be checked BEFORE dispatch_mouse so scrollbar clicks
                // aren't consumed by panel content area handlers.
                {
                    let session = &mut app.session_mgr.current_mut();
                    if let Some(ref metrics) = session.ui.panel_scrollbar_metrics {
                        // ▼ button click (scroll to bottom)
                        if let Some(btn) = metrics.down_btn_area {
                            if mouse.column >= btn.x
                                && mouse.column < btn.x + btn.width
                                && mouse.row >= btn.y
                                && mouse.row < btn.y + btn.height
                            {
                                session
                                    .session_panels
                                    .dispatch_set_scroll_offset(metrics.max_offset);
                                session.ui.panel_scroll_offset = metrics.max_offset;
                                return Ok(Some(Action::Redraw));
                            }
                        }
                        // ▲ button click (scroll to top)
                        if let Some(btn) = metrics.up_btn_area {
                            if mouse.column >= btn.x
                                && mouse.column < btn.x + btn.width
                                && mouse.row >= btn.y
                                && mouse.row < btn.y + btn.height
                            {
                                session.session_panels.dispatch_set_scroll_offset(0);
                                session.ui.panel_scroll_offset = 0;
                                return Ok(Some(Action::Redraw));
                            }
                        }
                        // Scrollbar bar click (proportional jump + start drag)
                        if mouse.column == metrics.bar_area.x
                            && mouse.row >= metrics.bar_area.y
                            && mouse.row < metrics.bar_area.bottom()
                            && metrics.max_offset > 0
                        {
                            let bar_inner_height = metrics.bar_area.height.saturating_sub(2);
                            if bar_inner_height > 0 {
                                let rel_y = (mouse.row.saturating_sub(metrics.bar_area.y + 1))
                                    .min(bar_inner_height);
                                let new_offset = ((rel_y as f64 / bar_inner_height as f64)
                                    * metrics.max_offset as f64)
                                    as u16;
                                let new_offset = new_offset.min(metrics.max_offset);
                                session
                                    .session_panels
                                    .dispatch_set_scroll_offset(new_offset);
                                session.ui.panel_scroll_offset = new_offset;
                                session.ui.panel_scrollbar_dragging = true;
                            }
                            return Ok(Some(Action::Redraw));
                        }
                    }
                }
                // Panel area: dispatch mouse click to panel content
                let panel_area = app.session_mgr.current_mut().ui.panel_area;
                let mut click_consumed = false;
                if let Some(area) = panel_area {
                    if mouse::mouse_in_rect(&mouse, area) {
                        // Session panels
                        {
                            let sp = &app.session_mgr.current_mut().session_panels;
                            if sp.is_any_open() {
                                let result = with_session_panels!(app, |sp, ctx| {
                                    sp.dispatch_mouse(mouse, area, &mut ctx)
                                });
                                if result == EventResult::Consumed {
                                    click_consumed = true;
                                }
                            }
                        }
                        // Global panels
                        if !click_consumed && app.global_panels.is_any_open() {
                            let result = with_global_panels!(app, |pm, ctx| {
                                pm.dispatch_mouse(mouse, area, &mut ctx)
                            });
                            if result == EventResult::Consumed {
                                click_consumed = true;
                            }
                        }
                    }
                }
                if click_consumed {
                    return Ok(Some(Action::Redraw));
                }
                // Panel area: start panel selection
                let panel_area = app.session_mgr.current_mut().ui.panel_area;
                if let Some(area) = panel_area {
                    if mouse::mouse_in_rect(&mouse, area) {
                        let content_row = mouse.row - area.y
                            + app.session_mgr.current_mut().ui.panel_scroll_offset;
                        let col = mouse.column - area.x;
                        app.session_mgr
                            .current_mut()
                            .ui
                            .panel_selection
                            .start_drag(content_row, col);
                        app.session_mgr.current_mut().ui.text_selection.clear();
                        // Don't process other-area selections
                        return Ok(Some(Action::Redraw));
                    }
                }
                if let Some(area) = app.session_mgr.current_mut().ui.messages_area {
                    if mouse.row >= area.y
                        && mouse.row < area.y + area.height
                        && mouse.column >= area.x
                        && mouse.column < area.x + area.width
                    {
                        let visual_row = usize::from(mouse.row - area.y)
                            + app.session_mgr.current_mut().ui.scroll_offset;
                        let visual_col = mouse.column - area.x;
                        app.session_mgr
                            .current_mut()
                            .ui
                            .text_selection
                            .start_drag(visual_row, visual_col);
                    }
                }
                // Textarea area: start textarea selection
                // 弹窗激活时跳过——光标不应移到 textarea 内
                if !app.is_interaction_popup_active() {
                    if let Some(area) = app.session_mgr.current_mut().ui.textarea_area {
                        if mouse.row >= area.y
                            && mouse.row < area.y + area.height
                            && mouse.column >= area.x
                            && mouse.column < area.x + area.width
                        {
                            let session = &app.session_mgr.current_mut();
                            let (row, col) =
                                mouse::textarea_mouse_to_cursor(&session.ui.textarea, area, &mouse);
                            app.session_mgr.current_mut().ui.textarea.move_cursor(
                                tui_textarea::CursorMove::Jump(row as u16, col as u16),
                            );
                            app.session_mgr.current_mut().ui.textarea.start_selection();
                        }
                    }
                }
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                // Panel scrollbar drag: update panel scroll offset from mouse Y
                {
                    let session = &mut app.session_mgr.current_mut();
                    if session.ui.panel_scrollbar_dragging {
                        if let Some(ref metrics) = session.ui.panel_scrollbar_metrics {
                            let bar_inner_height = metrics.bar_area.height.saturating_sub(2);
                            if bar_inner_height > 0 {
                                let rel_y = (mouse.row.saturating_sub(metrics.bar_area.y + 1))
                                    .min(bar_inner_height);
                                let new_offset = ((rel_y as f64 / bar_inner_height as f64)
                                    * metrics.max_offset as f64)
                                    as u16;
                                let new_offset = new_offset.min(metrics.max_offset);
                                session
                                    .session_panels
                                    .dispatch_set_scroll_offset(new_offset);
                                session.ui.panel_scroll_offset = new_offset;
                            }
                        }
                        return Ok(Some(Action::Redraw));
                    }
                }
                if handle_message_scrollbar_drag(app, mouse.row) {
                    return Ok(Some(Action::Redraw));
                }
                // Panel selection drag
                if app.session_mgr.current_mut().ui.panel_selection.dragging {
                    if let Some(area) = app.session_mgr.current_mut().ui.panel_area {
                        let content_row = mouse
                            .row
                            .saturating_sub(area.y)
                            .saturating_add(app.session_mgr.current_mut().ui.panel_scroll_offset);
                        let col = mouse.column.saturating_sub(area.x);
                        app.session_mgr
                            .current_mut()
                            .ui
                            .panel_selection
                            .update_drag(content_row, col);
                    }
                }
                if app.session_mgr.current_mut().ui.text_selection.dragging {
                    if let Some(area) = app.session_mgr.current_mut().ui.messages_area {
                        let visual_row = usize::from(mouse.row.saturating_sub(area.y))
                            + app.session_mgr.current_mut().ui.scroll_offset;
                        let visual_col = mouse.column.saturating_sub(area.x);
                        app.session_mgr
                            .current_mut()
                            .ui
                            .text_selection
                            .update_drag(visual_row, visual_col);
                    }
                }
                // Textarea area: extend textarea selection
                if app.session_mgr.current_mut().ui.textarea.is_selecting() {
                    if let Some(area) = app.session_mgr.current_mut().ui.textarea_area {
                        if mouse.row >= area.y && mouse.row < area.y + area.height {
                            let session = &app.session_mgr.current_mut();
                            let (row, col) =
                                mouse::textarea_mouse_to_cursor(&session.ui.textarea, area, &mouse);
                            app.session_mgr.current_mut().ui.textarea.move_cursor(
                                tui_textarea::CursorMove::Jump(row as u16, col as u16),
                            );
                        }
                    }
                }
            }
            MouseEventKind::Up(MouseButton::Left) => {
                // End panel scrollbar drag
                app.session_mgr.current_mut().ui.panel_scrollbar_dragging = false;
                app.session_mgr.current_mut().ui.message_scrollbar_dragging = false;
                // Panel selection released
                if app.session_mgr.current_mut().ui.panel_selection.dragging {
                    app.session_mgr.current_mut().ui.panel_selection.end_drag();
                    let sel = &app.session_mgr.current_mut().ui.panel_selection;
                    if let (Some(start), Some(end)) = (sel.start, sel.end) {
                        let text = crate::app::text_selection::extract_panel_text(
                            start,
                            end,
                            &app.session_mgr.current_mut().ui.panel_plain_lines,
                        );
                        app.session_mgr
                            .current_mut()
                            .ui
                            .panel_selection
                            .set_selected_text(text);
                    }
                    mouse::copy_panel_selection_to_clipboard(app);
                }
                if app.session_mgr.current_mut().ui.text_selection.dragging {
                    app.session_mgr.current_mut().ui.text_selection.end_drag();
                    let ts = &app.session_mgr.current_mut().ui.text_selection;
                    if let (Some(start), Some(end)) = (ts.start, ts.end) {
                        let usable_width = app
                            .session_mgr
                            .current_mut()
                            .ui
                            .messages_area
                            .map(|a| a.width)
                            .unwrap_or(0);
                        let cache = app.session_mgr.current_mut().messages.render_cache.read();
                        let text = crate::app::text_selection::extract_selected_text(
                            start,
                            end,
                            &cache.wrap_map,
                            usable_width,
                        );
                        drop(cache);
                        app.session_mgr
                            .current_mut()
                            .ui
                            .text_selection
                            .set_selected_text(text);
                    }
                    mouse::copy_selection_to_clipboard(app);
                }
                // textarea selection on mouse up: no extra handling; tui_textarea maintains
                // its own selection state
            }
            _ => {}
        }
        },
    }

    Ok(Some(Action::Redraw))
}

// ── OAuth prompt ────────────────────────────────────────────────────────────

fn handle_oauth_prompt(app: &mut App, input: Input) {
    use crate::app::handle_edit_key;
    let prompt = match app.global_ui.oauth_prompt.as_mut() {
        Some(p) => p,
        None => return,
    };
    match input {
        Input {
            key: Key::Enter, ..
        } => {
            if prompt.submit() {
                app.global_ui.oauth_prompt = None;
            }
        }
        Input {
            key: Key::Char('o'),
            ctrl: true,
            ..
        } => {
            let url = prompt.authorization_url.clone();
            #[cfg(unix)]
            let _ = std::process::Command::new("open").arg(&url).spawn();
            #[cfg(windows)]
            let _ = std::process::Command::new("cmd")
                .args(["/C", "start", &url])
                .spawn();
        }
        Input { key: Key::Esc, .. } => {
            app.global_ui.oauth_prompt = None;
        }
        Input {
            key: Key::Char('c'),
            ctrl: true,
            ..
        } => {
            // Ctrl+C in OAuth popup: ignore (no quit)
        }
        _ => {
            prompt.error_message = None;
            handle_edit_key(&mut prompt.input, &mut prompt.cursor, input);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::crossterm::event::KeyEvent;
    use ratatui::layout::Rect;

    fn make_key(code: KeyCode) -> Event {
        Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
    }

    #[test]
    fn test_key_event_to_text_simulated_paste_includes_first_line() {
        let mut text = String::new();

        assert!(key_event_to_text(make_key(KeyCode::Char('b')), &mut text));
        assert!(key_event_to_text(make_key(KeyCode::Char('u')), &mut text));
        assert!(key_event_to_text(make_key(KeyCode::Enter), &mut text));
        assert!(key_event_to_text(make_key(KeyCode::Char('i')), &mut text));
        assert!(key_event_to_text(make_key(KeyCode::Char('d')), &mut text));

        assert_eq!(
            text, "bu\nid",
            "模拟粘贴重建必须从第一个字符开始，不能等到 Enter 后才收集"
        );
    }

    #[test]
    fn test_message_scrollbar_offset_for_row_maps_track_to_range() {
        let metrics = MessageScrollbarMetrics {
            bar_area: Rect::new(79, 5, 1, 12),
            max_offset: 100,
            up_btn_area: Some(Rect::new(79, 5, 1, 1)),
            down_btn_area: Some(Rect::new(79, 16, 1, 1)),
        };

        assert_eq!(message_scrollbar_offset_for_row(metrics, 6), 0);
        assert_eq!(message_scrollbar_offset_for_row(metrics, 11), 50);
        assert_eq!(message_scrollbar_offset_for_row(metrics, 16), 100);
    }

    #[test]
    fn test_message_scrollbar_offset_for_row_handles_large_offsets() {
        let metrics = MessageScrollbarMetrics {
            bar_area: Rect::new(10, 0, 1, 22),
            max_offset: usize::MAX - 10,
            up_btn_area: None,
            down_btn_area: None,
        };

        assert_eq!(
            message_scrollbar_offset_for_row(metrics, 21),
            usize::MAX - 10
        );
    }

    /// 运行时证据：用当前 ratatui（0.30）真实渲染 thumb，读出 thumb 实际占据的行，
    /// 然后用我们的点击映射 `message_scrollbar_offset_for_row` 去「点」thumb 中点，
    /// 再用算出的 offset 重新渲染 —— 断言 thumb 会跑离鼠标点击的行。
    ///
    /// 若该测试通过（漂移非空），即 100% 证明：当前代码下点击可见的滑块，offset 必然
    /// 跳变、滑块必然从鼠标下跑走，与「点滑块抓不住 / 拖动乱跳」现象一致。
    /// oracle 是 ratatui 自己的渲染结果，不依赖任何手工公式复刻。
    #[test]
    fn thumb_click_drift_runtime_proof() {
        use peri_widgets::unified_vertical_scrollbar;
        use ratatui::{
            buffer::Buffer,
            layout::{Position, Rect},
            prelude::StatefulWidget,
            widgets::ScrollbarState,
        };

        let h: u16 = 20;
        // 复刻 message_area.rs 渲染滚动条时的 area（整个消息区内层，高 h）
        let area = Rect::new(0, 0, 1, h);

        // 多个内容长度 × 多个位置采样，统计「点击 thumb 中点后滑块漂移」的情况
        let mut total_drifted = 0usize;
        let mut total_sampled = 0usize;
        let mut global_max_drift = 0i32;
        eprintln!("=== thumb 点击漂移运行时证据（H={h}，oracle = ratatui 真实渲染）===");
        eprintln!("  max_scroll | 漂移采样/总数 | 最大漂移(行)");
        for &max_scroll in &[50usize, 100, 200, 500, 1000, 2000] {
            let mut drifted_in_case = 0usize;
            let mut sampled = 0usize;
            let mut case_max_drift = 0i32;
            for &pct in &[10usize, 30, 50, 70, 90] {
                let orig_offset = max_scroll * pct / 100;
                sampled += 1;

                // 1. ratatui 真实渲染 thumb
                let mut buf = Buffer::empty(area);
                let mut state = ScrollbarState::new(max_scroll).position(orig_offset);
                unified_vertical_scrollbar().render(area, &mut buf, &mut state);

                // 2. 读出 thumb 实际占据的行（thumb symbol = block::FULL = "█"）
                let thumb_rows: Vec<u16> = (0..h)
                    .filter(|&r| {
                        buf.cell(Position::new(0, r))
                            .is_some_and(|c| c.symbol() == "█")
                    })
                    .collect();
                assert!(
                    !thumb_rows.is_empty(),
                    "max_scroll={max_scroll} offset={orig_offset} 未渲染出 thumb，测试前提不成立"
                );
                let thumb_mid = thumb_rows[thumb_rows.len() / 2];

                // 3. 用我们的点击映射算「点 thumb 中点」得到的 offset
                let metrics = MessageScrollbarMetrics {
                    bar_area: Rect::new(0, 0, 1, h),
                    max_offset: max_scroll,
                    up_btn_area: None,
                    down_btn_area: None,
                };
                let clicked_offset = message_scrollbar_offset_for_row(metrics, thumb_mid);

                // 4. 用 clicked_offset 重新渲染，看 thumb 跑到哪
                let mut buf2 = Buffer::empty(area);
                let mut state2 = ScrollbarState::new(max_scroll).position(clicked_offset);
                unified_vertical_scrollbar().render(area, &mut buf2, &mut state2);
                let new_thumb_rows: Vec<u16> = (0..h)
                    .filter(|&r| {
                        buf2.cell(Position::new(0, r))
                            .is_some_and(|c| c.symbol() == "█")
                    })
                    .collect();
                let new_mid = new_thumb_rows[new_thumb_rows.len() / 2];

                let drift = (thumb_mid as i32 - new_mid as i32).abs();
                if drift > 0 {
                    drifted_in_case += 1;
                    case_max_drift = case_max_drift.max(drift);
                }
            }
            total_drifted += drifted_in_case;
            total_sampled += sampled;
            global_max_drift = global_max_drift.max(case_max_drift);
            eprintln!(
                "  {max_scroll:>10} |   {drifted_in_case}/{sampled}      |   {case_max_drift}"
            );
        }

        assert!(
            total_drifted > 0,
            "所有 {total_sampled} 个采样点点击 thumb 都未漂移 —— bug 不成立"
        );
        eprintln!(
            "\n  合计：{total_drifted}/{total_sampled} 个采样点点击 thumb 后滑块跑离鼠标，最大漂移 {global_max_drift} 行"
        );
        eprintln!("  结论：点击可见滑块时 offset 间歇性跳变 —— 即「点滑块抓不住 / 拖动乱跳」必然发生。");
    }
}
