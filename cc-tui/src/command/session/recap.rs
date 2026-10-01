use crate::{app::App, command::Command};

/// /recap 命令 —— 生成会话回顾（一句话目标 + 任务 + 下一步）
///
/// Immediate 类型：由 ACP 层的 RecapCommand 拦截处理。
/// TUI 侧只需将完整命令作为普通 prompt 提交给 ACP。
pub struct RecapCommand;

impl Command for RecapCommand {
    fn name(&self) -> &str {
        "recap"
    }

    fn description(&self, _lc: &crate::i18n::LcRegistry) -> String {
        "Generate a 1-2 sentence recap of the conversation".to_string()
    }

    fn aliases(&self) -> Vec<&str> {
        vec!["away", "catchup"]
    }

    fn execute(&self, app: &mut App, args: &str) {
        let prompt = if args.trim().is_empty() {
            "/recap".to_string()
        } else {
            format!("/recap {}", args.trim())
        };
        app.submit_message(prompt);
    }
}
