use crate::{app::App, command::Command};

#[allow(dead_code)] // /tasks 入口已由命令中心替代，验收后独立 PR 清理。
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
