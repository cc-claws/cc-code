use super::message_pipeline::PipelineAction;
use super::*;
use crate::ui::render_thread::RenderEvent;

impl App {
    /// 发送当前 view_messages 的全量重建到渲染线程
    pub(crate) fn render_rebuild(&self) {
        let session = &self.session_mgr.sessions[self.session_mgr.active];
        let _ = session
            .messages
            .render_tx
            .send(RenderEvent::Rebuild(session.messages.view_messages.clone()));
    }

    /// 发送带滚动锚点的全量重建到渲染线程
    pub(crate) fn render_rebuild_with_anchor(&self, anchor_message_idx: usize) {
        let session = &self.session_mgr.sessions[self.session_mgr.active];
        let _ = session
            .messages
            .render_tx
            .send(RenderEvent::RebuildWithAnchor {
                messages: session.messages.view_messages.clone(),
                anchor_message_idx,
            });
    }

    /// 从 pipeline 规范状态触发 RebuildAll（统一入口）。
    pub(crate) fn request_rebuild(&mut self) {
        let prefix_len = self.session_mgr.sessions[self.session_mgr.active]
            .messages
            .round_start_vm_idx;
        let action = self.session_mgr.sessions[self.session_mgr.active]
            .messages
            .pipeline
            .build_rebuild_all(prefix_len);
        self.apply_pipeline_action(action);
    }

    /// 将 PipelineAction 映射到 view_messages 更新 + RenderEvent 发送
    pub(crate) fn apply_pipeline_action(&mut self, action: PipelineAction) {
        match action {
            PipelineAction::None => {}
            PipelineAction::AddMessage(vm) => {
                self.session_mgr.sessions[self.session_mgr.active]
                    .messages
                    .view_messages
                    .push(vm);
                self.render_rebuild();
            }
            PipelineAction::RebuildAll {
                prefix_len,
                mut tail_vms,
            } => {
                // 保存即将被截断的 ephemeral SystemNote（通过 AddMessage 添加的系统通知）
                let session = &mut self.session_mgr.sessions[self.session_mgr.active];
                let saved_notes: Vec<MessageViewModel> = session
                    .messages
                    .view_messages
                    .drain(prefix_len..)
                    .filter(|vm| matches!(vm, MessageViewModel::SystemNote { .. }))
                    .collect();

                // 去重：如果前缀末尾是 UserBubble 且 tail 首个也是 UserBubble（同一轮 Human 消息被
                // submit_message 的 UserBubble 和 StateSnapshot reconcile 的 UserBubble 重复渲染），
                // 移除 tail 中重复的 UserBubble
                if prefix_len > 0 && !tail_vms.is_empty() {
                    let prefix_last = session.messages.view_messages.get(prefix_len - 1);
                    if let Some(MessageViewModel::UserBubble {
                        content: prefix_content,
                        ..
                    }) = prefix_last
                    {
                        if let Some(MessageViewModel::UserBubble {
                            content: tail_content,
                            ..
                        }) = tail_vms.first()
                        {
                            if prefix_content == tail_content {
                                tail_vms.remove(0);
                            }
                        }
                    }
                }

                session.messages.view_messages.extend(tail_vms);
                session.messages.view_messages.extend(saved_notes);
                let anchor_message_idx = {
                    let cache = self.session_mgr.sessions[self.session_mgr.active]
                        .messages
                        .render_cache
                        .read();
                    let scroll_row = self.session_mgr.sessions[self.session_mgr.active]
                        .ui
                        .scroll_offset as usize;
                    let msg_idx = cache
                        .message_offsets
                        .iter()
                        .enumerate()
                        .find(|(_, &offset)| {
                            offset < cache.wrap_map.len()
                                && cache.wrap_map[offset].visual_row_start as usize >= scroll_row
                        })
                        .map(|(idx, _)| idx)
                        .unwrap_or(prefix_len);
                    msg_idx.min(
                        self.session_mgr.sessions[self.session_mgr.active]
                            .messages
                            .view_messages
                            .len(),
                    )
                };
                self.render_rebuild_with_anchor(anchor_message_idx);
            }
        }
    }
}
