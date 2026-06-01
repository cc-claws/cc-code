use crate::{app::App, command::Command};

pub struct TasksCommand;

impl Command for TasksCommand {
    fn name(&self) -> &str {
        "tasks"
    }

    fn description(&self, _lc: &crate::i18n::LcRegistry) -> String {
        _lc.tr("command-tasks-description")
    }

    fn execute(&self, app: &mut App, _args: &str) {
        app.open_tasks_panel();
    }
}
