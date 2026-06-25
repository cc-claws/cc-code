use tui_textarea::{Input, Key};

use crate::{
    app::{
        panel_manager::{EventResult, PanelKind},
        App,
    },
    with_global_panels, with_session_panels,
};

use super::super::Action;

/// PanelManager 分发：先处理 session panels，再处理 global panels
pub(super) fn handle_panels(app: &mut App, input: &Input) -> Option<Action> {
    // Ctrl+C 是全局退出/中断快捷键，优先于任何面板的按键处理，直接穿透到
    // 后续 Stage（normal_keys → handle_ctrl_c）。多数面板用 `_ => Consumed`
    // 兜底吞掉未识别按键，若不在此拦截，Ctrl+C 永远到不了 handle_ctrl_c。
    // （详见 spec/issues/2026-06-24-panel-swallow-ctrl-c.md）
    if input.ctrl && matches!(input.key, Key::Char('c')) {
        return None;
    }

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
        let result = with_session_panels!(app, |sp, ctx| {
            let result = sp.dispatch_key(input.clone(), &mut ctx);
            match result {
                EventResult::ClosePanel => {
                    sp.close();
                    app.session_mgr.current_mut().ui.panel_selection.clear();
                    app.session_mgr.current_mut().ui.panel_area = None;
                }
                EventResult::OpenThread(thread_id) => {
                    sp.close();
                    app.session_mgr.current_mut().ui.panel_selection.clear();
                    app.session_mgr.current_mut().ui.panel_area = None;
                    // with_session_panels! macro puts sp back at closure end,
                    // but OpenThread needs to put back first then call open_thread_with_feedback
                    app.session_mgr.current_mut().session_panels = sp;
                    // Early return prevents macro from putting back again
                    app.open_thread_with_feedback(thread_id);
                    return Some(Action::Redraw);
                }
                _ => {}
            }
            result
        });
        // 只有面板真正消费了按键（或改变了面板状态）才返回 Redraw；
        // NotConsumed 时穿透到下一 Stage，避免吞掉未处理的按键。
        if matches!(result, EventResult::NotConsumed) {
            return None;
        }
        return Some(Action::Redraw);
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
        let result = with_global_panels!(app, |pm, ctx| {
            let result = pm.dispatch_key(input.clone(), &mut ctx);
            match result {
                EventResult::ClosePanel => {
                    pm.close();
                    app.session_mgr.current_mut().ui.panel_selection.clear();
                    app.session_mgr.current_mut().ui.panel_area = None;
                }
                EventResult::OpenPanel(PanelKind::Memory) => {
                    app.global_panels = pm;
                    if let Err(e) = app.memory_panel_open_editor() {
                        tracing::error!("Failed to open editor: {}", e);
                    }
                    return Some(Action::Redraw);
                }
                _ => {}
            }
            result
        });
        // 只有面板真正消费了按键（或改变了面板状态）才返回 Redraw；
        // NotConsumed 时穿透到下一 Stage，避免吞掉未处理的按键。
        if matches!(result, EventResult::NotConsumed) {
            return None;
        }
        return Some(Action::Redraw);
    }

    None
}

#[cfg(test)]
mod tests {
    use super::handle_panels;
    use crate::app::panel_manager::PanelKind;
    use crate::app::App;
    use crate::event::Action;
    use tui_textarea::{Input, Key};

    #[tokio::test]
    async fn test_ctrl_c_passes_through_when_panel_open() {
        // ModelPanel 用 `_ => Consumed` 兜底吞掉未识别按键（含 Ctrl+C），
        // 是验证「Ctrl+C 穿透」最严格的用例：即使面板本身会吞，Ctrl+C 也必须穿透
        // 到 normal_keys（handle_ctrl_c），否则面板打开时无法退出/中断。
        let (mut app, _handle) = App::new_headless(80, 24).await;
        app.open_model_panel();
        assert!(
            app.session_mgr
                .current()
                .session_panels
                .is_active(PanelKind::Model),
            "前置条件：ModelPanel 应已打开"
        );

        let ctrl_c = Input {
            key: Key::Char('c'),
            ctrl: true,
            alt: false,
            shift: false,
        };
        let result = handle_panels(&mut app, &ctrl_c);

        assert!(
            result.is_none(),
            "面板打开时 Ctrl+C 必须穿透到 normal_keys，不得被拦截"
        );
    }

    #[tokio::test]
    async fn test_consumed_key_still_returns_redraw_when_panel_open() {
        // 回归保护：面板真正消费的按键（如 Up 导航）仍应返回 Redraw，
        // 不因 NotConsumed 穿透机制而被误传到 normal_keys。
        let (mut app, _handle) = App::new_headless(80, 24).await;
        app.open_model_panel();

        let up = Input {
            key: Key::Up,
            ctrl: false,
            alt: false,
            shift: false,
        };
        let result = handle_panels(&mut app, &up);

        assert!(
            matches!(result, Some(Action::Redraw)),
            "面板消费的按键应返回 Redraw，不被穿透"
        );
    }
}
