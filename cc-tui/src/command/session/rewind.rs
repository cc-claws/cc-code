use crate::{app::App, command::Command};

/// /rewind 命令 —— 打开对话回滚选择器（别名 `/undo`）。
///
/// 只负责「打开选择器」这一步：无参 `/rewind` 由本命令拦截并弹出回滚面板。
/// 用户在面板中确认后，TUI 再向 ACP 发送带 JSON 参数的
/// `/rewind {target_message_id, revert_files}` 执行实际回退。
pub struct RewindCommand;

impl Command for RewindCommand {
    fn name(&self) -> &str {
        "rewind"
    }

    fn description(&self, lc: &crate::i18n::LcRegistry) -> String {
        lc.tr("command-rewind-description")
    }

    fn aliases(&self) -> Vec<&str> {
        vec!["undo"]
    }

    fn execute(&self, app: &mut App, _args: &str) {
        app.open_rewind_prompt();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    include!("rewind_test.rs");
}
