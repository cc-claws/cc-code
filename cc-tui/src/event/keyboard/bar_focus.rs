use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::app::App;

use super::super::Action;

/// Bar 焦点模式拦截：bg_bar_cursor 有值时，所有按键转发到 handle_bar_key_event
pub(super) fn handle_bar_focus(
    app: &mut App,
    key_event: &ratatui::crossterm::event::KeyEvent,
) -> Option<Action> {
    if app.session_mgr.current().ui.background_tasks_bar_focused {
        if app.global_panels.is_any_open() || app.session_mgr.current().session_panels.is_any_open()
        {
            app.session_mgr
                .current_mut()
                .ui
                .background_tasks_bar_focused = false;
            return None;
        }
        if app.has_running_background_shell_tasks() {
            return Some(handle_background_tasks_bar_key_event(app, *key_event));
        }
        app.session_mgr
            .current_mut()
            .ui
            .background_tasks_bar_focused = false;
    }

    let cursor = app.session_mgr.current().ui.bg_bar_cursor;
    if cursor.is_some() {
        // 后台 Agent 全部结束后游标可能残留，此时抽屉已收起，不能继续吞按键。
        if !app.session_mgr.current().drawer_nav_active() {
            app.session_mgr.current_mut().ui.bg_bar_cursor = None;
            return None;
        }
        if drawer_type_to_exit(key_event) {
            app.session_mgr.current_mut().ui.bg_bar_cursor = None;
            return None;
        }
        return Some(handle_bar_key_event(app, *key_event));
    }
    None
}

/// 抽屉导航态「打字即交还输入框」（对齐 CC `PromptInput.tsx` 的 type-to-exit footer）。
///
/// 导航键（`↑`/`↓`/`Enter`/`Esc`）与抽屉自己的 `x`（停止该 Agent）留在抽屉；
/// 其余可打印字符先把焦点交还输入框，再走正常输入路径。没有这一条，用户按
/// `↓` 看一眼后想直接说话时字符会被静默吞掉，只能再按 `Enter` 落回输入框。
fn drawer_type_to_exit(key_event: &KeyEvent) -> bool {
    matches!(key_event.code, KeyCode::Char(c) if !c.is_control() && c != 'x')
        && !key_event
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
}

fn handle_background_tasks_bar_key_event(
    app: &mut App,
    key_event: ratatui::crossterm::event::KeyEvent,
) -> Action {
    if key_event.kind == KeyEventKind::Release {
        return Action::Redraw;
    }

    match key_event.code {
        KeyCode::Esc => {
            app.session_mgr
                .current_mut()
                .ui
                .background_tasks_bar_focused = false;
            Action::Redraw
        }
        KeyCode::Enter => {
            app.session_mgr
                .current_mut()
                .ui
                .background_tasks_bar_focused = false;
            app.open_background_tasks_panel();
            Action::Redraw
        }
        _ => Action::Redraw,
    }
}

/// 聚焦只读模式拦截：focused_instance_id 有值时，仅 Esc 退出
pub(super) fn handle_focused_only(
    app: &mut App,
    key_event: &ratatui::crossterm::event::KeyEvent,
) -> Option<Action> {
    if let Some(sub) = app.session_mgr.current().active_subagent() {
        let active = sub.status.is_active();
        if matches!(key_event.code, KeyCode::Esc) {
            app.switch_subagent_viewport(None);
            return Some(Action::Redraw);
        }
        // 子 Agent 视图是受限上下文：Shift+Tab（权限模式循环）属于主会话操作，
        // 在这里按下只会误切主会话权限模式，直接吞掉。
        if matches!(key_event.code, KeyCode::BackTab) {
            return Some(Action::Redraw);
        }
        if key_event.code == KeyCode::Enter
            && !key_event
                .modifiers
                .intersects(KeyModifiers::SHIFT | KeyModifiers::ALT)
        {
            if active {
                let text = app.session_mgr.current().ui.textarea.lines().join("\n");
                app.send_subagent_input(text);
            }
            return Some(Action::Redraw);
        }
        // 已结束视口仍允许滚动、思考折叠和抽屉导航，输入保持只读。
        if !active
            && !matches!(
                key_event.code,
                KeyCode::Up
                    | KeyCode::Down
                    | KeyCode::PageUp
                    | KeyCode::PageDown
                    | KeyCode::Home
                    | KeyCode::End
            )
            && !(key_event.code == KeyCode::Char('o')
                && key_event.modifiers.contains(KeyModifiers::CONTROL))
        {
            return Some(Action::Redraw);
        }
    }
    None
}

/// Bar 焦点模式下的键盘处理
pub(super) fn handle_bar_key_event(
    app: &mut App,
    key_event: ratatui::crossterm::event::KeyEvent,
) -> Action {
    if key_event.kind == KeyEventKind::Release {
        return Action::Redraw;
    }

    let agents_len = app.session_mgr.current_mut().background_agents.len();
    let total_items = 1 + agents_len;

    let raw_cursor = app.session_mgr.current_mut().ui.bg_bar_cursor.unwrap_or(0);
    // Clamp cursor to valid range (agents may have been removed)
    let cursor = raw_cursor.min(total_items.saturating_sub(1));

    match key_event.code {
        KeyCode::Esc => {
            app.session_mgr.current_mut().ui.bg_bar_cursor = None;
            Action::Redraw
        }
        KeyCode::Up => {
            let new_cursor = if cursor > 0 {
                cursor - 1
            } else {
                total_items - 1
            };
            app.session_mgr.current_mut().ui.bg_bar_cursor = Some(new_cursor);
            Action::Redraw
        }
        KeyCode::Down => {
            let new_cursor = (cursor + 1) % total_items;
            app.session_mgr.current_mut().ui.bg_bar_cursor = Some(new_cursor);
            Action::Redraw
        }
        KeyCode::Enter => {
            let id = cursor
                .checked_sub(1)
                .and_then(|index| app.session_mgr.current().background_agents.get(index))
                .map(|agent| agent.instance_id.clone());
            app.switch_subagent_viewport(id);
            Action::Redraw
        }
        KeyCode::Char('x') if key_event.modifiers.is_empty() => {
            if let Some(id) = cursor
                .checked_sub(1)
                .and_then(|index| app.session_mgr.current().background_agents.get(index))
                .map(|agent| agent.instance_id.clone())
            {
                app.cancel_subagent(&id);
            }
            Action::Redraw
        }
        _ => Action::Redraw,
    }
}

/// 在抽屉、输入框和审批浮层之前处理任务控制键，避免焦点吞掉中断。
pub(super) fn handle_subagent_controls(
    app: &mut App,
    key: &ratatui::crossterm::event::KeyEvent,
) -> Option<Action> {
    if app.session_mgr.current().subagent_sessions.is_empty() {
        return None;
    }
    let now = std::time::Instant::now();
    let control = key.modifiers.contains(KeyModifiers::CONTROL);
    if control && key.code == KeyCode::Char('x') {
        app.session_mgr.current_mut().subagent_control_prefix = Some(now);
        return Some(Action::Redraw);
    }
    if let Some(prefix) = app.session_mgr.current_mut().subagent_control_prefix.take() {
        if control && key.code == KeyCode::Char('k') && now.duration_since(prefix).as_secs() < 2 {
            app.cancel_all_subagents();
            return Some(Action::Redraw);
        }
    }
    if control && key.code == KeyCode::Char('c') {
        if !app.session_mgr.current().ui.loading
            && !app.session_mgr.current().has_running_subagents()
            && app.session_mgr.current().focused_instance_id.is_none()
        {
            return None;
        }
        let double = app
            .session_mgr
            .current()
            .subagent_interrupt_at
            .is_some_and(|time| now.duration_since(time).as_millis() <= 500);
        app.session_mgr.current_mut().subagent_interrupt_at = Some(now);
        if double {
            app.cancel_all_subagents();
            app.interrupt();
        } else if let Some(id) = app.session_mgr.current().focused_instance_id.clone() {
            app.cancel_subagent(&id);
        } else if app.session_mgr.current().ui.loading {
            app.interrupt();
        }
        return Some(Action::Redraw);
    }
    if key.code == KeyCode::Down
        && key.modifiers.is_empty()
        && app.session_mgr.current().ui.bg_bar_cursor.is_none()
        && app
            .session_mgr
            .current()
            .ui
            .textarea
            .lines()
            .iter()
            .all(|line| line.is_empty())
        && !app.session_mgr.current().background_agents.is_empty()
        && !app.is_interaction_popup_active()
        && !app.session_mgr.current().session_panels.is_any_open()
        && !app.global_panels.is_any_open()
    {
        let cursor = app
            .session_mgr
            .current()
            .focused_instance_id
            .as_ref()
            .and_then(|id| {
                app.session_mgr
                    .current()
                    .background_agents
                    .iter()
                    .position(|agent| &agent.instance_id == id)
            })
            .map(|index| index + 1)
            .unwrap_or(0);
        app.session_mgr.current_mut().ui.bg_bar_cursor = Some(cursor);
        return Some(Action::Redraw);
    }
    None
}
