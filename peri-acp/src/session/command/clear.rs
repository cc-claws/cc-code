//! `/clear` 命令 — 清空对话历史。

use super::{AgentCommand, CommandContext, CommandKind, CommandResult};
use crate::session::executor::PromptStopReason;

/// 清空历史命令。
pub struct ClearCommand;

impl ClearCommand {
    pub const NAME: &'static str = "clear";
}

#[async_trait::async_trait]
impl AgentCommand for ClearCommand {
    fn name(&self) -> &str {
        Self::NAME
    }

    fn aliases(&self) -> Vec<&str> {
        vec!["cls", "reset"]
    }

    fn description(&self) -> &str {
        "清空当前会话的对话历史"
    }

    fn kind(&self) -> CommandKind {
        CommandKind::Immediate
    }

    async fn execute(&self, _ctx: CommandContext) -> CommandResult {
        CommandResult {
            messages: Vec::new(),
            stop_reason: PromptStopReason::EndTurn,
        }
    }
}
