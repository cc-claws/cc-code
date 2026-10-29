use super::{message_pipeline::PipelineAction, *};
use crate::ui::branch_prefix::BranchPrefix;
use cc_agent::{agent::events::CompactFileInfo, messages::BaseMessage};

/// 视图历史上限（条）。
///
/// 自动 compact 改为保留历史后，长会话的 `view_messages` 会无上限增长——每条都带着
/// rendered 快照、思考正文与工具输出。超限时裁掉最老的一段并插一行占位提示；
/// 被裁的只是屏幕历史，模型上下文（pipeline 的规范消息）不受影响。
///
/// 取值口径：一屏约 20-40 行，一条 VM 通常占 1-3 行；1 万条约合 1-3 万行屏幕历史
/// （数百到上千轮工具调用），正常会话几乎不可能触发，只在渲染快照确实堆积时才兜底。
pub(crate) const MAX_VIEW_MESSAGES: usize = 10_000;

impl App {
    /// 视图历史超限时裁剪最老的一段。
    ///
    /// 需要同步三处下标：`round_start_vm_idx`（本轮重建起点）、`ephemeral_notes`
    /// 的锚点、以及文本选区（跨裁剪会错位，直接作废）。
    pub(crate) fn trim_view_history(&mut self, keep: usize) {
        let session = self.session_mgr.current_mut();
        let len = session.messages.view_messages.len();
        if len <= keep {
            return;
        }
        let drop_n = len - keep;
        session.messages.view_messages.drain(..drop_n);
        session.messages.view_messages.insert(
            0,
            MessageViewModel::system(format!(
                "… 更早的 {drop_n} 条历史已省略（视图上限 {keep} 条，模型上下文不受影响）"
            )),
        );

        // 锚点平移：被裁掉的丢弃，其余整体左移 drop_n，再让过第 0 行的占位
        session
            .messages
            .ephemeral_notes
            .retain(|(anchor, _)| *anchor >= drop_n);
        for (anchor, _) in session.messages.ephemeral_notes.iter_mut() {
            *anchor = anchor.saturating_sub(drop_n).saturating_add(1);
        }
        // 本轮起点同样平移；若起点已被裁掉，落到占位行之后
        let view_len = session.messages.view_messages.len();
        session.messages.round_start_vm_idx = session
            .messages
            .round_start_vm_idx
            .saturating_sub(drop_n)
            .saturating_add(1)
            .min(view_len);

        session.ui.text_selection.clear();
        session.ui.pending_text_copy = None;
        session.ui.presented_selection_snapshot = None;

        self.render_rebuild();
    }

    pub(crate) fn handle_compact_started(&mut self, manual: bool) -> (bool, bool, bool) {
        // 退出聚焦模式（如有）
        self.session_mgr.current_mut().focused_instance_id = None;
        self.session_mgr.current_mut().ui.bg_bar_cursor = None;

        // 仅在手动 compact（/compact 命令）时置位，handle_compact_completed 依赖此标志结束 loading。
        // auto-compact 发生于 ReAct 循环内部，后续仍需继续执行，不能在压缩完成时结束 loading。
        self.session_mgr.current_mut().agent.compact_manual = manual;

        // 显示 loading 状态（spinner + 禁用输入）
        self.set_loading(true);
        // 压缩期间不往消息流里插「正在进行」提示：参考实现把这段进度挂在 spinner
        // 上，历史里只留一条完成通知。这里把 spinner 文案临时切到「正在压缩上下文…」，
        // 完成/失败时由 restore_spinner_after_compact() 还原随机动词。
        let label = self.services.lc.tr("app-compact-started");
        self.session_mgr
            .current_mut()
            .spinner_state
            .set_mode_with_label(cc_widgets::SpinnerMode::Thinking, Some(label));
        (true, false, false)
    }

    /// 压缩结束后还原 spinner 文案。
    ///
    /// 压缩期间 spinner 的 verb 被临时替换成「正在压缩上下文…」（见
    /// [`Self::handle_compact_started`]），这里重新选一个本轮动词，
    /// 避免压缩结束后整轮都停在那句话上。
    fn restore_spinner_after_compact(&mut self) {
        self.session_mgr
            .current_mut()
            .spinner_state
            .pick_round_verb();
    }

    pub(crate) fn handle_compact_completed(
        &mut self,
        _summary: String,
        files: Vec<CompactFileInfo>,
        skills: Vec<String>,
        micro_cleared: usize,
        messages: Vec<BaseMessage>,
    ) -> (bool, bool, bool) {
        self.restore_spinner_after_compact();
        if micro_cleared > 0 {
            // Micro-compact: 更新内部状态，保留 pipeline 显示
            self.session_mgr.current_mut().agent.origin_messages = messages;
            let vm = MessageViewModel::system(self.services.lc.tr_args(
                "app-compact-auto-cleared",
                &[("count".into(), (micro_cleared as i64).into())],
            ));
            self.apply_pipeline_action(PipelineAction::add_message(vm));
            return (true, false, false);
        }

        // Full compact: 清理 pipeline + 更新内部状态
        // Auto-compact 在 ReAct 循环内执行，agent 即将 resubmit，spinner 持续到 Done。
        // Manual compact 是独立操作，无后续 Done 事件，必须在此结束 loading。
        let is_manual = self.session_mgr.current_mut().agent.compact_manual;
        if is_manual {
            self.set_loading(false);
            self.session_mgr.current_mut().agent.compact_manual = false;
        }
        let mut label_lines = vec![format!("✻ {}", self.services.lc.tr("app-compact-done"))];
        for f in &files {
            label_lines.push(format!(
                "{}Read {} ({} lines)",
                BranchPrefix::PREFIX,
                f.path,
                f.lines
            ));
        }
        if !skills.is_empty() {
            label_lines.push(format!(
                "{}Skill: {}",
                BranchPrefix::PREFIX,
                skills.join(", ")
            ));
        }
        let compact_label = label_lines.join("\n");

        // 更新内部状态消息（供下一次 prompt 使用）
        self.session_mgr.current_mut().agent.origin_messages = messages.clone();

        // 清空 pipeline 内部状态 + 用压缩后消息恢复（手动/自动都需要：
        // 压缩后的上下文就是后续轮次的唯一事实来源）
        self.session_mgr.current_mut().messages.pipeline.clear();
        self.session_mgr
            .current_mut()
            .messages
            .pipeline
            .restore_completed(messages);

        if !is_manual {
            // 自动 compact：用户没有主动要求重置视图，**已显示的历史必须留在屏幕上**，
            // 只追加一条压缩通知。把 round_start_vm_idx 推到当前视图末尾，
            // 后续 request_rebuild() 只重建新增部分，历史前缀原样复用。
            self.apply_pipeline_action(PipelineAction::add_message(MessageViewModel::system(
                compact_label,
            )));
            let end = self.session_mgr.current().messages.view_messages.len();
            self.session_mgr.current_mut().messages.round_start_vm_idx = end;
            // 历史只增不减会吃满内存：超限时裁掉最老的一段（仅视图，不动模型上下文）
            self.trim_view_history(MAX_VIEW_MESSAGES);
            return (true, false, false);
        }

        // 手动 /compact：用户主动发起压缩，视图按压缩后上下文重置，只留一条通知
        // 清除 ephemeral_notes，防止 compact 前的系统通知残留
        self.session_mgr
            .current_mut()
            .messages
            .ephemeral_notes
            .clear();

        // 清空 view_messages，只显示 compact 通知
        let view_msgs = vec![MessageViewModel::system(compact_label)];
        // resubmit 后 view_messages 被清空重建，round_start_vm_idx 必须重置
        // 否则第二轮 agent 事件的 request_rebuild() 使用旧值会越界并导致 VM 累积
        self.session_mgr.current_mut().messages.round_start_vm_idx = 0;
        self.apply_pipeline_action(PipelineAction::RebuildAll {
            prefix_len: 0,
            tail_vms: view_msgs,
        });

        (true, false, false)
    }

    pub(crate) fn handle_compact_error(&mut self, msg: String) -> (bool, bool, bool) {
        self.restore_spinner_after_compact();
        let is_manual = self.session_mgr.current_mut().agent.compact_manual;
        if is_manual {
            self.set_loading(false);
            self.session_mgr.current_mut().agent.compact_manual = false;
        }
        let vm = MessageViewModel::system(
            self.services
                .lc
                .tr_args("app-compact-failed", &[("error".into(), msg.into())]),
        );
        self.apply_pipeline_action(PipelineAction::add_message(vm));

        (true, false, false)
    }

    /// Rewind 完成：更新消息历史，显示回退通知。
    ///
    /// 与 compact 不同：保留到目标用户消息（含），不显示文件/skill 信息，
    /// 直接使用 rewind 命令传入的摘要文本。
    pub(crate) fn handle_rewind_completed(
        &mut self,
        summary: String,
        messages: Vec<BaseMessage>,
    ) -> (bool, bool, bool) {
        // 更新内部状态消息
        self.session_mgr.current_mut().agent.origin_messages = messages.clone();

        // 清空 pipeline + 用回退后消息恢复
        self.session_mgr.current_mut().messages.pipeline.clear();
        self.session_mgr
            .current_mut()
            .messages
            .pipeline
            .restore_completed(messages.clone());
        self.session_mgr
            .current_mut()
            .messages
            .ephemeral_notes
            .clear();

        // 将保留的消息渲染为 VMs，追加 rewind 通知
        let label = format!("↩ {summary}");
        self.rebuild_vms_from_all(&messages, Some(MessageViewModel::system(label)));

        (true, false, false)
    }
}

#[cfg(test)]
#[path = "agent_compact_test.rs"]
mod tests;
