use crate::{app::App, command::Command};

/// /init 命令 —— 生成或优化项目 CLAUDE.md 知识库
/// Passthrough 类型：内容直接发给 Agent 执行
pub struct InitCommand;

impl Command for InitCommand {
    fn name(&self) -> &str {
        "init"
    }

    fn description(&self, lc: &crate::i18n::LcRegistry) -> String {
        lc.tr("command-init-description")
    }

    fn aliases(&self) -> Vec<&str> {
        vec![]
    }

    fn execute(&self, app: &mut App, _args: &str) {
        // /init 是 Passthrough 命令，实际由 ACP 层的 InitCommand 处理
        // TUI 侧只需将 "/init" 作为普通 prompt 提交
        app.submit_message("/init".to_string());
    }
}
