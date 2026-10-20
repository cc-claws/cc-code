use crate::{app::App, command::Command, ui::theme::ThemeName};

pub struct ThemeCommand;

impl Command for ThemeCommand {
    fn name(&self) -> &str {
        "theme"
    }

    fn description(&self, lc: &crate::i18n::LcRegistry) -> String {
        lc.tr("command-theme-description")
    }

    fn execute(&self, app: &mut App, args: &str) {
        let arg = args.trim();
        if arg.is_empty() {
            app.open_theme_panel();
        } else if let Some(name) = ThemeName::from_setting(arg) {
            app.apply_theme(name);
        } else {
            let note = app
                .services
                .lc
                .tr_args("app-theme-unknown", &[("name".into(), arg.into())]);
            app.session_mgr
                .current_mut()
                .messages
                .push_system_note(note);
        }
    }
}
