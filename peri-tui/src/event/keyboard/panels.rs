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
    // Ctrl+C：agent 运行中 → 穿透中断（紧急操作优先，行为不变）；
    // 空闲 + 有面板打开 → 先关闭顶层面板并消费掉，不触发退出；
    // 空闲 + 无面板 → 穿透到后续 Stage（normal_keys → handle_ctrl_c）走双击退出。
    // （2026-10-01 修：此前无条件穿透，面板开着时双击/长按 Ctrl+C 会直接退出整个
    // TUI，而用户预期是先关面板。详见 spec/archive-issues/2026-06-24-panel-swallow-ctrl-c.md
    // 的反方向问题。）
    if input.ctrl && matches!(input.key, Key::Char('c')) {
        if app.session_mgr.current().ui.loading {
            return None;
        }
        if close_top_panel(app) {
            // 关面板这次不计入双击退出：清掉可能已有的 quit-pending，
            // 避免"关面板"被误认为双击退出的第一次按键。
            app.global_ui.quit_pending_since = None;
            return Some(Action::Redraw);
        }
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
                    app.session_mgr.current_mut().ui.screen_selection.clear();
                    app.session_mgr.current_mut().ui.text_selection.clear();
                    app.session_mgr
                        .current_mut()
                        .ui
                        .background_tasks_bar_focused = false;
                    app.session_mgr.current_mut().ui.panel_area = None;
                }
                EventResult::OpenThread(thread_id) => {
                    sp.close();
                    app.session_mgr.current_mut().ui.screen_selection.clear();
                    app.session_mgr.current_mut().ui.text_selection.clear();
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
            | Some(PanelKind::BackgroundTasks)
    ) {
        let result = with_global_panels!(app, |pm, ctx| {
            let result = pm.dispatch_key(input.clone(), &mut ctx);
            match result {
                EventResult::ClosePanel => {
                    pm.close();
                    app.session_mgr.current_mut().ui.screen_selection.clear();
                    app.session_mgr.current_mut().ui.text_selection.clear();
                    app.session_mgr
                        .current_mut()
                        .ui
                        .background_tasks_bar_focused = false;
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

/// 关闭顶层打开的面板（session panels 优先于 global panels，与分发顺序一致）。
/// 返回 true 表示确实关了一个面板。清理逻辑与各面板 ClosePanel 分支保持一致。
fn close_top_panel(app: &mut App) -> bool {
    if app.session_mgr.current().session_panels.is_any_open() {
        app.session_mgr.current_mut().session_panels.close();
    } else if app.global_panels.is_any_open() {
        app.global_panels.close();
    } else {
        return false;
    }
    let ui = &mut app.session_mgr.current_mut().ui;
    ui.screen_selection.clear();
    ui.text_selection.clear();
    ui.background_tasks_bar_focused = false;
    ui.panel_area = None;
    true
}

#[cfg(test)]
mod tests {
    use super::handle_panels;
    use crate::app::panel_manager::PanelKind;
    use crate::app::App;
    use crate::event::Action;
    use tui_textarea::{Input, Key};

    #[tokio::test]
    async fn test_ctrl_c_closes_session_panel_when_idle() {
        // 面板打开 + 空闲时，Ctrl+C 应关闭面板并消费掉，不再穿透到退出逻辑。
        // （2026-10-01 改：此前无条件穿透，面板开着时双击/长按 Ctrl+C 会直接退出 TUI）
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
            matches!(result, Some(Action::Redraw)),
            "关面板应消费按键并返回 Redraw，不穿透"
        );
        assert!(
            !app.session_mgr.current().session_panels.is_any_open(),
            "面板应已关闭"
        );
        assert!(
            app.global_ui.quit_pending_since.is_none(),
            "关面板这次不应计入双击退出"
        );
    }

    #[tokio::test]
    async fn test_ctrl_c_closes_global_panel_when_idle() {
        // Global 面板同样：空闲时 Ctrl+C 关面板，不退出。
        let (mut app, _handle) = App::new_headless(80, 24).await;
        app.open_status_panel(0);
        assert!(
            app.global_panels.is_active(PanelKind::Status),
            "前置条件：StatusPanel 应已打开"
        );

        let ctrl_c = Input {
            key: Key::Char('c'),
            ctrl: true,
            alt: false,
            shift: false,
        };
        let result = handle_panels(&mut app, &ctrl_c);

        assert!(
            matches!(result, Some(Action::Redraw)),
            "关面板应消费按键并返回 Redraw，不穿透"
        );
        assert!(
            !app.global_panels.is_any_open(),
            "Global 面板应已关闭"
        );
    }

    #[tokio::test]
    async fn test_ctrl_c_passes_through_when_loading() {
        // agent 运行中 + 面板打开：Ctrl+C 仍穿透（中断优先），不关面板。
        let (mut app, _handle) = App::new_headless(80, 24).await;
        app.open_model_panel();
        app.session_mgr.current_mut().ui.loading = true;

        let ctrl_c = Input {
            key: Key::Char('c'),
            ctrl: true,
            alt: false,
            shift: false,
        };
        let result = handle_panels(&mut app, &ctrl_c);

        assert!(result.is_none(), "运行中 Ctrl+C 应穿透以中断 agent");
        assert!(
            app.session_mgr
                .current()
                .session_panels
                .is_active(PanelKind::Model),
            "中断优先时面板不应被关闭"
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
