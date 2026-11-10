use ratatui::crossterm::event::KeyCode;

use super::{SHORTCUT_BG_BAR, SHORTCUT_COMMAND_PALETTE, SHORTCUT_KERNEL_PANEL};
use crate::app::panel_manager::PanelKind;
use crate::app::App;

use super::super::Action;

/// 处理全局快捷键：BackTab（权限循环）、Ctrl+B（bg bar）、Ctrl+P（内核选择）、
/// Alt+P（命令面板）、Ctrl+O（详细模式切换）。
/// 内核切换走 Ctrl+P 内核面板，Provider / Model / Effort 选择走 Alt+P 命令面板。
pub(super) fn handle_shortcuts(
    app: &mut App,
    key_event: &ratatui::crossterm::event::KeyEvent,
) -> Option<Action> {
    // Shift+Tab (BackTab): cycle permission mode
    if matches!(key_event.code, KeyCode::BackTab) {
        app.services.permission_mode.cycle();
        return Some(Action::Redraw);
    }

    // Ctrl+O: toggle detail mode (only when OAuth popup is NOT active)
    if key_event
        .modifiers
        .contains(ratatui::crossterm::event::KeyModifiers::CONTROL)
        && matches!(key_event.code, KeyCode::Char('o'))
    {
        if app.global_ui.oauth_prompt.is_none() {
            app.toggle_detail_mode();
        }
        return Some(Action::Redraw);
    }

    // Ctrl+B: 有前台 shell 时先后台化（进程不中断），然后聚焦底部 shell 入口；
    // 已有后台 shell 时也只聚焦入口，Enter 再打开面板；否则聚焦 bg agent bar。
    if SHORTCUT_BG_BAR.matches(key_event) {
        // 面板内交给选中项处理，不能再次走全局分支重置选择。
        if app.global_panels.is_active(PanelKind::BackgroundTasks) {
            return None;
        }
        let has_foreground = app.session_mgr.current().shell_pool.is_running();
        if has_foreground {
            if app.background_foreground() {
                focus_background_tasks_bar(app);
            }
        } else if app.background_agent_foreground() {
            if !app.global_panels.is_active(PanelKind::BackgroundTasks) {
                focus_background_tasks_bar(app);
            }
        } else if app.has_running_background_shell_tasks() {
            focus_background_tasks_bar(app);
        } else if !app.session_mgr.current().background_agents.is_empty() {
            app.session_mgr.current_mut().ui.bg_bar_cursor = Some(0);
        }
        return Some(Action::Redraw);
    }

    // Ctrl+P: toggle 内核选择面板（cc / dsh / pi）
    if SHORTCUT_KERNEL_PANEL.matches(key_event) {
        if app
            .session_mgr
            .current()
            .session_panels
            .is_active(PanelKind::Kernel)
        {
            app.session_mgr
                .current_mut()
                .session_panels
                .close_if(PanelKind::Kernel);
        } else {
            app.open_kernel_panel();
        }
        return Some(Action::Redraw);
    }

    // Alt+P: toggle 命令面板（Provider & Model 选择）
    if SHORTCUT_COMMAND_PALETTE.matches(key_event) {
        if app
            .session_mgr
            .current()
            .session_panels
            .is_active(PanelKind::CommandPalette)
        {
            app.session_mgr
                .current_mut()
                .session_panels
                .close_if(PanelKind::CommandPalette);
        } else {
            app.open_command_palette();
        }
        return Some(Action::Redraw);
    }

    None
}

fn focus_background_tasks_bar(app: &mut App) {
    app.session_mgr
        .current_mut()
        .ui
        .background_tasks_bar_focused = true;
}

#[cfg(test)]
#[path = "shortcuts_test.rs"]
mod tests;
