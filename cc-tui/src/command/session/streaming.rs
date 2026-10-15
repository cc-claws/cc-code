use crate::{app::App, command::Command};

/// /streaming 命令 —— 查看或切换时间线渲染模式（`streaming` / `block` / `none`）。
///
/// 此前只在 `submit_message` 内部做前缀特例拦截，于是它只能通过「排队后消费」这条
/// 路径触达：空闲时键入反而报「未知命令」，运行中键入则被排队后延迟生效。现提升为
/// 正式命令，`submit_message` 里的特例已移除，空闲与延后两条路径共用同一份派发。
pub struct StreamingCommand;

impl Command for StreamingCommand {
    fn name(&self) -> &str {
        "streaming"
    }

    fn description(&self, _lc: &crate::i18n::LcRegistry) -> String {
        "View or switch timeline rendering mode (streaming/block/none)".to_string()
    }

    /// 只改本地渲染模式，不依赖 Agent 是否在跑，可安全延后执行。
    fn blocked_while_busy(&self) -> bool {
        false
    }

    fn execute(&self, app: &mut App, args: &str) {
        app.handle_streaming_command(args.trim());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_streaming_command_reports_current_mode() {
        let (mut app, _handle) = App::new_headless(80, 24).await;
        StreamingCommand.execute(&mut app, "");
        let text = format!("{:?}", app.session_mgr.current_mut().messages.view_messages);
        assert!(
            text.contains("当前渲染模式"),
            "无参数应回报当前渲染模式，实际: {text}"
        );
    }
}
