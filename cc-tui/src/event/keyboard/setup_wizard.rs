use tui_textarea::{Input, Key};

use crate::app::{App, MessageViewModel};
use cc_agent::messages::BaseMessage;

use super::super::Action;

/// Setup wizard 模式下拦截所有按键
pub(super) fn handle_setup_wizard(app: &mut App, input: &Input) -> Option<Action> {
    if app.global_ui.setup_wizard.is_none() {
        return None;
    }

    // Ctrl+C: exit flow (matching normal-mode behaviour)
    if matches!(
        input,
        Input {
            key: Key::Char('c'),
            ctrl: true,
            ..
        }
    ) {
        return Some(super::normal_keys::handle_ctrl_c(app).unwrap_or(Action::Redraw));
    }

    let input_clone = input.clone();
    if let Some(ref mut wizard) = app.global_ui.setup_wizard {
        if let Some(action) = crate::app::setup_wizard::handle_setup_wizard_key(wizard, input_clone)
        {
            match action {
                crate::app::setup_wizard::SetupWizardAction::SaveAndClose => {
                    let wizard = app
                        .global_ui
                        .setup_wizard
                        .take()
                        .expect("setup_wizard must be Some (checked above)");
                    match crate::app::setup_wizard::save_setup(&wizard) {
                        Ok(cfg) => app.refresh_after_setup(cfg),
                        Err(e) => {
                            let msg = MessageViewModel::from_base_message(
                                &BaseMessage::system(format!("配置保存失败: {}", e)),
                                &[],
                            );
                            app.session_mgr
                                .current_mut()
                                .messages
                                .view_messages
                                .push(msg);
                            app.render_rebuild();
                        }
                    }
                }
                crate::app::setup_wizard::SetupWizardAction::Skip => {
                    let from_command = app
                        .global_ui
                        .setup_wizard
                        .as_ref()
                        .map(|w| w.from_command)
                        .unwrap_or(false);
                    app.global_ui.setup_wizard = None;
                    if !from_command {
                        return Some(Action::Quit);
                    }
                }
                crate::app::setup_wizard::SetupWizardAction::SetLanguage(lang) => {
                    let _ = app.services.lc.switch(&lang);
                }
                crate::app::setup_wizard::SetupWizardAction::Redraw => {}
            }
        }
    }
    Some(Action::Redraw)
}
