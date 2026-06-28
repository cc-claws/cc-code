use ratatui::crossterm::event::KeyCode;

use super::{SHORTCUT_BG_BAR, SHORTCUT_COMMAND_PALETTE, SHORTCUT_CTRL_CYCLE_MODE, SHORTCUT_CYCLE_MODE};
use crate::app::panel_manager::PanelKind;
use crate::app::{App, MessageViewModel};

use super::super::Action;

/// 处理全局快捷键：BackTab（权限循环）、Ctrl+B（bg bar）、Ctrl+P（命令面板）、
/// Ctrl+T（模型 alias 切换）、Ctrl+O（详细模式切换）。
/// Provider 切换已统一收敛到 Ctrl+P 命令面板，不再单独提供循环快捷键。
pub(super) fn handle_shortcuts(
    app: &mut App,
    key_event: &ratatui::crossterm::event::KeyEvent,
) -> Option<Action> {
    // Shift+Tab (BackTab): cycle permission mode
    if matches!(key_event.code, KeyCode::BackTab) {
        let _new_mode = app.services.permission_mode.cycle();
        app.global_ui.mode_highlight_until =
            Some(std::time::Instant::now() + std::time::Duration::from_millis(1500));
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

    // Ctrl+B: 有前台 shell 时后台化（output_rx 切换到磁盘，进程不中断）；
    // 否则有后台 shell 时打开面板；否则聚焦 bg agent bar
    if SHORTCUT_BG_BAR.matches(key_event) {
        let has_foreground = app.session_mgr.current().shell_pool.is_running();
        if has_foreground {
            app.background_foreground();
        } else if !app.session_mgr.current().background_shells.is_empty() {
            app.open_background_tasks_panel();
        } else if !app.session_mgr.current().background_agents.is_empty() {
            app.session_mgr.current_mut().ui.bg_bar_cursor = Some(0);
        }
        return Some(Action::Redraw);
    }

    // Ctrl+P: toggle 命令面板（Provider & Model 选择）
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

    // Ctrl+T / Alt+M: cycle model aliases（仅切换当前 provider 下非空 alias）
    if SHORTCUT_CTRL_CYCLE_MODE.matches(key_event) || SHORTCUT_CYCLE_MODE.matches(key_event) {
        if let Some(cfg) = app.services.peri_config.as_mut() {
            // 从当前激活 provider 收集非空 alias，避免循环到未配置的空槽位
            let active_provider_id = cfg.config.active_provider_id.as_str();
            let active_provider = cfg
                .config
                .providers
                .iter()
                .find(|p| p.id == active_provider_id);
            let candidates: Vec<&'static str> = match active_provider {
                Some(p) => ["opus", "sonnet", "haiku"]
                    .iter()
                    .filter_map(|&a| {
                        p.models.get_model(a).filter(|m| !m.is_empty()).map(|_| a)
                    })
                    .collect(),
                None => vec!["opus", "sonnet", "haiku"],
            };
            if candidates.len() <= 1 {
                // 当前 provider 只有一个可用 alias，无可循环对象，静默忽略
                return Some(Action::Redraw);
            }
            let current = cfg.config.active_alias.as_str();
            let idx = candidates.iter().position(|&a| a == current).unwrap_or(0);
            let next = candidates[(idx + 1) % candidates.len()];
            cfg.config.active_alias = next.to_string();
            if let Err(e) = App::save_config(cfg, app.services.config_path_override.as_deref()) {
                app.session_mgr
                    .current_mut()
                    .messages
                    .view_messages
                    .push(MessageViewModel::system(format!("配置保存失败: {}", e)));
            }
            if let Some(p) = crate::app::agent::LlmProvider::from_config(cfg) {
                app.services.provider_name = p.display_name().to_string();
                app.services.model_name = p.model_name().to_string();
            }
            if let Some(ref acp_client) = app.acp_client {
                let acp = acp_client.clone();
                let alias = next.to_string();
                tokio::spawn(async move {
                    let _ = acp.set_config_option("model", &alias).await;
                });
            }
            app.global_ui.model_highlight_until =
                Some(std::time::Instant::now() + std::time::Duration::from_millis(1500));
        }
        return Some(Action::Redraw);
    }

    None
}
