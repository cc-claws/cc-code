use crate::{app::App, command::Command};

pub struct PsCommand;

impl Command for PsCommand {
    fn name(&self) -> &str {
        "ps"
    }

    fn description(&self, lc: &crate::i18n::LcRegistry) -> String {
        lc.tr("command-ps-description")
    }

    fn execute_while_busy(&self) -> bool {
        true
    }

    fn execute(&self, app: &mut App, _args: &str) {
        app.open_background_tasks_panel();
    }
}
