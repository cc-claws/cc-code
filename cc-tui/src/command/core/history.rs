use crate::{app::App, command::Command};

pub struct HistoryCommand;

impl Command for HistoryCommand {
    fn name(&self) -> &str {
        "history"
    }

    fn description(&self, _lc: &crate::i18n::LcRegistry) -> String {
        _lc.tr("command-history-description")
    }

    fn execute(&self, app: &mut App, _args: &str) {
        // 运行中不可打开面板——该判定现已统一由 `blocked_while_busy`（默认 true）
        // 在输入入口拦截，命令内不再自查，避免逐命令各写一套。
        app.open_input_history_panel();
    }
}
