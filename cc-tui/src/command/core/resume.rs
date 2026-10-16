use crate::{app::App, command::Command};

/// 打开历史**对话**浏览面板（选 thread 恢复）。
///
/// 刻意与 `/history`（输入历史）分成两个命令：`history` 在 CLI 语境里默认指
/// "你敲过的输入"，用它指会话列表会让人误以为能查输入记录。
pub struct ResumeCommand;

impl Command for ResumeCommand {
    fn name(&self) -> &str {
        "resume"
    }

    fn description(&self, lc: &crate::i18n::LcRegistry) -> String {
        lc.tr("command-resume-description")
    }

    fn execute(&self, app: &mut App, _args: &str) {
        // 运行中不可打开面板——该判定现已统一由 `blocked_while_busy`（默认 true）
        // 在输入入口拦截，命令内不再自查，避免逐命令各写一套。
        app.open_thread_browser();
    }
}
