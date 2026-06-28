use crate::{app::App, command::Command};

/// /commit 命令 —— 一键 Git Commit
/// Passthrough 类型：由 ACP 层的 CommitCommand 拦截处理，
/// 实际执行 git status/diff 收集上下文并生成 commit。
/// TUI 侧只需将完整命令（含参数）作为普通 prompt 提交给 ACP。
pub struct CommitCommand;

impl Command for CommitCommand {
    fn name(&self) -> &str {
        "commit"
    }

    fn description(&self, _lc: &crate::i18n::LcRegistry) -> String {
        "Create a git commit".to_string()
    }

    fn aliases(&self) -> Vec<&str> {
        vec!["ci"]
    }

    fn execute(&self, app: &mut App, args: &str) {
        // /commit 是 Passthrough 命令，实际由 ACP 层的 CommitCommand 处理
        // TUI 侧只需将 "/commit" + 参数作为普通 prompt 提交
        let prompt = if args.trim().is_empty() {
            "/commit".to_string()
        } else {
            format!("/commit {}", args.trim())
        };
        app.submit_message(prompt);
    }
}
