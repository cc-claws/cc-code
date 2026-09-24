//! `/recap` 命令的 TUI 事件处理。
//!
//! 参照 [`super::agent_compact`] 的模式：将 recap 生成结果渲染为系统消息。

use super::{message_pipeline::PipelineAction, *};

impl App {
    /// 处理 [`AgentEvent::RecapCompleted`]，渲染 recap 摘要文本。
    pub(crate) fn handle_recap_completed(&mut self, text: String) -> (bool, bool, bool) {
        self.set_loading(false);
        self.auto_recap.on_recap_done();
        self.session_mgr.current_mut().latest_recap = Some(text);
        self.request_rebuild();
        (true, false, false)
    }

    /// 处理 [`AgentEvent::RecapError`]，渲染错误消息。
    pub(crate) fn handle_recap_error(&mut self, msg: String) -> (bool, bool, bool) {
        self.set_loading(false);
        // 区分正常终止与真正失败：NoTurn（无历史）和 Aborted（取消）不重试
        let is_normal_end = msg.contains("no history") || msg.contains("cancelled");
        if is_normal_end {
            self.auto_recap.on_recap_done();
        } else {
            self.auto_recap.on_recap_failed();
        }
        let vm = MessageViewModel::system(
            self.services
                .lc
                .tr_args("app-recap-failed", &[("error".into(), msg.into())]),
        );
        self.apply_pipeline_action(PipelineAction::AddMessage(vm));
        (true, false, false)
    }

    /// 自动触发 recap：通过 ACP client 发送 `/recap` 命令。
    ///
    /// 守卫：不在 loading 状态（避免与主轮次抢 executor 的 prompt_lock）。
    /// 复用现有 `/recap` 命令分发链路，ACP/agent 层的生成逻辑不变。
    pub fn trigger_auto_recap(&mut self, revision: u64) {
        // 不在 loading 状态才触发（避免与主轮次抢 executor）
        if self.session_mgr.current().ui.loading {
            tracing::debug!("auto_recap: loading 中，推迟触发");
            self.auto_recap.on_trigger_deferred();
            return;
        }

        tracing::info!(revision = revision, "auto_recap: 触发自动会话回顾");
        if let Some(ref client) = self.acp_client {
            let client = client.clone();
            tokio::spawn(async move {
                let content = peri_agent::messages::MessageContent::text("/recap");
                if let Err(e) = client.prompt(&content).await {
                    tracing::error!(error = %e, "auto_recap: ACP prompt 失败");
                }
            });
        } else {
            tracing::warn!("auto_recap: acp_client 未初始化，跳过");
            self.auto_recap.on_trigger_deferred();
        }
    }
}
