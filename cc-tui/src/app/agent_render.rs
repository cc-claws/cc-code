use super::{message_pipeline::PipelineAction, *};
use crate::ui::render_thread::RenderEvent;

impl App {
    /// 发送当前 view_messages 的全量重建到渲染线程。
    ///
    /// 当前视口直接读取独立管线的内存快照，渲染期间不阻塞查询 SQLite。
    pub(crate) fn render_rebuild(&self) {
        let session = &self.session_mgr.current();
        let vms = self.resolve_render_vms(session);
        let _ = session
            .messages
            .render_tx
            .try_send(RenderEvent::Rebuild(vms));
    }

    /// 发送带滚动锚点的全量重建到渲染线程。
    ///
    /// 聚焦后台 agent 时忽略锚点（直接 Rebuild），因为 agent 视图与主视图
    /// 的滚动位置不共享。
    pub(crate) fn render_rebuild_with_anchor(&self, anchor_message_idx: usize) {
        let session = &self.session_mgr.current();
        if session.focused_instance_id.is_some() {
            // 子视口有独立滚动位置，不采用主视图锚点。
            let vms = self.resolve_render_vms(session);
            let _ = session
                .messages
                .render_tx
                .try_send(RenderEvent::Rebuild(vms));
        } else {
            let vms = session.messages.view_messages.clone();
            let adjusted_anchor = anchor_message_idx.min(vms.len().saturating_sub(1));
            let _ = session
                .messages
                .render_tx
                .try_send(RenderEvent::RebuildWithAnchor {
                    messages: vms,
                    anchor_message_idx: adjusted_anchor,
                });
        }
    }

    /// 根据聚焦状态决定渲染数据源。
    ///
    /// - 未聚焦：返回内存中的 view_messages
    /// - 聚焦后台 agent：读取该子线程的独立内存视图
    fn resolve_render_vms(&self, session: &ChatSession) -> Vec<MessageViewModel> {
        session.current_view_messages().to_vec()
    }

    /// 从 pipeline 规范状态触发 RebuildAll（统一入口）。
    pub(crate) fn request_rebuild(&mut self) {
        let prefix_len = self.session_mgr.current().messages.round_start_vm_idx;
        let action = self
            .session_mgr
            .current()
            .messages
            .pipeline
            .build_rebuild_all(prefix_len);
        self.apply_pipeline_action(action);
    }

    /// 用规范消息列表**全量**重建视图模型并安装（主题切换 / compact / rewind 共用）。
    ///
    /// 与 [`request_rebuild`](Self::request_rebuild) 的区别：后者只重建
    /// `round_start_vm_idx` 之后的 tail，历史 VM 直接复用；这里连历史一起重建，
    /// 因此 markdown 的 `rendered` 快照会按当前主题/宽度重新生成。
    ///
    /// `extra` 用于在末尾追加一条临时 VM（如 rewind 提示）。
    pub(crate) fn rebuild_vms_from_all(
        &mut self,
        messages: &[cc_agent::messages::BaseMessage],
        extra: Option<MessageViewModel>,
    ) {
        // 历史/主题替换立即作废待复制快照，不能等异步渲染发布后才推进代际。
        let ui = &mut self.session_mgr.current_mut().ui;
        ui.text_selection.clear();
        ui.pending_text_copy = None;
        ui.presented_selection_snapshot = None;
        let cwd = self.services.cwd.clone();
        let mut view_msgs =
            message_pipeline::MessagePipeline::messages_to_view_models(messages, &cwd);
        if let Some(vm) = extra {
            view_msgs.push(vm);
        }
        self.session_mgr.current_mut().messages.round_start_vm_idx = 0;
        self.apply_pipeline_action(PipelineAction::RebuildAll {
            prefix_len: 0,
            tail_vms: view_msgs,
        });
    }

    /// 添加系统通知并记录锚点位置，随后请求一次重建使通知立即可见。
    ///
    /// 面板代码和中断处理等路径 B 调用点应使用此方法，而非直接 push 到 view_messages。
    /// 重绘请求内置于此：提示只写进 `view_messages` 而不发渲染事件的话，要等到下一次
    /// 用户输入（或其它事件）才会出现在屏幕上（issue #572）。
    pub(crate) fn push_system_note(&mut self, content: String) {
        self.session_mgr
            .current_mut()
            .messages
            .push_system_note(content);
        self.request_rebuild();
    }

    /// 将 PipelineAction 映射到 view_messages 更新 + RenderEvent 发送
    pub(crate) fn apply_pipeline_action(&mut self, action: PipelineAction) {
        match action {
            PipelineAction::None => {}
            PipelineAction::AddMessage(vm) => {
                let vm = *vm;
                let session = self.session_mgr.current_mut();
                let anchor = session.messages.view_messages.len();
                session.messages.ephemeral_notes.push((anchor, vm.clone()));
                session.messages.view_messages.push(vm);
                self.render_rebuild();
            }
            PipelineAction::RebuildAll {
                prefix_len,
                mut tail_vms,
            } => {
                let session = self.session_mgr.current_mut();
                // 防御性边界检查：prefix_len 可能因 pipeline 内部 RebuildAll
                // (如 ToolStart 的 throttle flush) 导致 view_messages 缩短后仍然
                // 保持旧值，此时 drain 会 panic。
                let view_len = session.messages.view_messages.len();
                let prefix_len = if prefix_len > view_len {
                    tracing::error!(
                        prefix_len,
                        view_len,
                        round_start_vm_idx = session.messages.round_start_vm_idx,
                        "RebuildAll prefix_len 越界，已钳位到 view_messages.len()"
                    );
                    view_len
                } else {
                    prefix_len
                };

                // 保存 ephemeral_notes 中锚点在 tail 范围内的（锚点 < prefix_len 的已过期，丢弃）。
                // 注意：UserBubble 也通过 AddMessage 进入了 ephemeral_notes，但 UserBubble
                // 不应被视为 ephemeral——RebuildAll 应能彻底移除它们。
                let mut saved_notes: Vec<(usize, MessageViewModel)> = session
                    .messages
                    .ephemeral_notes
                    .drain(..)
                    .filter(|(anchor, _)| *anchor >= prefix_len)
                    .filter(|(_, vm)| !matches!(vm, MessageViewModel::UserBubble { .. }))
                    .collect();

                // drain 尾部
                session.messages.view_messages.drain(prefix_len..);

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

                // 活跃任务的启动结果由树形摘要统一呈现；内部内容只存在于子管线。
                // 后台分组卡片已在 `MessagePipeline::build_rebuild_all`（活跃轮次尾部的
                // 唯一产地）剔除，此处不再重复过滤——否则会连 `/rewind` 这类
                // 「全量时间线替换」里的归档卡片一起删掉。
                session.messages.view_messages.extend(tail_vms);

                // 按锚点位置插入 saved_notes，然后重新注册锚点
                saved_notes.sort_by_key(|(anchor, _)| *anchor);
                for (anchor, vm) in saved_notes {
                    let tail_len = session.messages.view_messages.len() - prefix_len;
                    let insert_pos = (anchor - prefix_len).min(tail_len) + prefix_len;
                    session
                        .messages
                        .view_messages
                        .insert(insert_pos, vm.clone());
                    // 重新注册到 ephemeral_notes，更新锚点为实际插入位置
                    session.messages.ephemeral_notes.push((insert_pos, vm));
                }

                let anchor_message_idx = {
                    let cache = self.session_mgr.current().messages.render_cache.read();
                    let ui = &self.session_mgr.current().ui;
                    // #650：scroll_offset 含欢迎前缀行，先换算回消息空间；
                    // #629：虚拟布局下按其视觉偏移查锚点。
                    let scroll_row = ui.scroll_offset.saturating_sub(ui.message_prefix_rows);
                    let msg_idx = if let Some(layout) = &cache.virtual_layout {
                        cache
                            .viewport_anchor_at(scroll_row)
                            .map(|anchor| anchor.message_index)
                            .unwrap_or_else(|| {
                                layout
                                    .message_visual_offsets
                                    .partition_point(|&row| row <= scroll_row)
                                    .saturating_sub(1)
                            })
                    } else {
                        cache
                            .message_offsets
                            .iter()
                            .enumerate()
                            .find(|(_, &offset)| {
                                offset < cache.wrap_map.len()
                                    && cache.wrap_map[offset].visual_row_start >= scroll_row
                            })
                            .map(|(idx, _)| idx)
                            .unwrap_or(prefix_len)
                    };
                    msg_idx.min(self.session_mgr.current().messages.view_messages.len())
                };
                self.render_rebuild_with_anchor(anchor_message_idx);
            }
        }
    }
}
