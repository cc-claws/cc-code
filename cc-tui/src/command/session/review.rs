use crate::{app::App, command::Command};

/// /review 命令 —— PR Code Review
/// Passthrough 类型：由 ACP 层的 ReviewCommand 拦截处理，
//  调用 gh CLI 完成 PR 审查。TUI 侧透传完整命令（含 PR 编号）给 ACP。
pub struct ReviewCommand;

impl Command for ReviewCommand {
    fn name(&self) -> &str {
        "review"
    }

    fn description(&self, _lc: &crate::i18n::LcRegistry) -> String {
        "Review a pull request".to_string()
    }

    fn aliases(&self) -> Vec<&str> {
        vec!["pr"]
    }

    /// execute 只是把 `/review` 提交给 ACP，运行中可安全入队延后执行。
    fn blocked_while_busy(&self) -> bool {
        false
    }

    fn execute(&self, app: &mut App, args: &str) {
        // /review 是 Passthrough 命令，实际由 ACP 层的 ReviewCommand 处理
        // TUI 侧只需将 "/review" + 参数（如 PR 编号）作为普通 prompt 提交
        let prompt = if args.trim().is_empty() {
            "/review".to_string()
        } else {
            format!("/review {}", args.trim())
        };
        app.submit_message(prompt);
    }
}
