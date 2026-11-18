use std::{
    cell::RefCell,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};

use cc_agent::interaction::channel_types::ChannelNotification;
use parking_lot::RwLock;
use tokio::sync::{mpsc, Notify};

use crate::ui::{
    message_view::{HookRow, HookRowKind, MessageViewModel, NoteTone},
    render_thread::{RenderCache, RenderEvent, ViewportAnchor},
};

use super::message_pipeline::MessagePipeline;
use super::PendingAttachment;

/// 每条排队消息独立持有已经展开的文字和图片，避免读取后续草稿的附件。
pub struct QueuedMessage {
    pub id: uuid::Uuid,
    pub text: String,
    pub attachments: Vec<PendingAttachment>,
    pub sending: bool,
}

impl QueuedMessage {
    pub fn new(text: String, attachments: Vec<PendingAttachment>) -> Self {
        Self {
            id: uuid::Uuid::new_v4(),
            text,
            attachments,
            sending: false,
        }
    }

    pub(crate) fn content(&self) -> cc_agent::messages::MessageContent {
        use cc_agent::messages::{ContentBlock, MessageContent};
        if self.attachments.is_empty() {
            return MessageContent::text(self.text.clone());
        }
        let mut blocks = vec![ContentBlock::text(self.text.clone())];
        blocks.extend(
            self.attachments
                .iter()
                .map(|image| ContentBlock::image_base64(&image.media_type, &image.base64_data)),
        );
        MessageContent::Blocks(blocks)
    }
}

impl From<String> for QueuedMessage {
    fn from(text: String) -> Self {
        Self::new(text, Vec::new())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QueuedMessageAction {
    Steer(uuid::Uuid),
    Delete(uuid::Uuid),
    PreviousPage,
    NextPage,
}

pub(crate) type SteeringResult = (uuid::Uuid, Result<(), String>);

pub type RenderViewportRequest = (usize, usize, bool, u16, Option<ViewportAnchor>, u64, usize);

/// 消息状态：会话级的消息管线、渲染通道、消息列表。
pub struct MessageState {
    pub view_messages: Vec<MessageViewModel>,
    pub round_start_vm_idx: usize,
    pub pipeline: MessagePipeline,
    pub render_tx: mpsc::Sender<RenderEvent>,
    /// 渲染队列满时保留最新完整快照（重建、滚动锚点或清空），下一轮轮询重试。
    pending_render_rebuild: Option<RenderEvent>,
    /// 每种渲染模式仅保留最新值，最多五种；重试时先同步模式，再发送内容快照。
    pending_render_modes: RefCell<Vec<RenderEvent>>,
    pub render_cache: Arc<RwLock<RenderCache>>,
    pub render_notify: Arc<Notify>,
    pub last_render_version: u64,
    pub pending_messages: Vec<QueuedMessage>,
    pub(crate) steering_result_tx: mpsc::UnboundedSender<SteeringResult>,
    pub(crate) steering_result_rx: mpsc::UnboundedReceiver<SteeringResult>,
    /// 最近一次提交的用户文本（用于 Ctrl+C 中断时恢复到输入框）
    pub last_submitted_text: Option<String>,
    /// 临时系统通知（不在 BaseMessage[] 中），记录 (锚点索引, VM)。
    /// 锚点 = 创建时 view_messages.len()，RebuildAll 时按锚点插入到对应位置。
    pub ephemeral_notes: Vec<(usize, MessageViewModel)>,
    /// 最近一次发送给渲染线程的 resize 宽度（用于去抖，避免每帧重复发送）
    pub last_resize_width: Option<u16>,
    /// 已成功投递的视口，通道满时保留旧值以便下一帧重试。
    pub last_viewport: Option<RenderViewportRequest>,
    /// 内容布局请求独立于视口物化的 cache.version，普通滚动不推进此版本。
    layout_revision: AtomicU64,
    /// 历史替换/模式切换使旧的异步宽度确认失效。
    layout_epoch: AtomicU64,
    pub width_preparation: crate::ui::main_ui::WidthPreparation,
    /// Channel 消息通知接收端
    pub channel_notification_rx: Option<tokio::sync::mpsc::UnboundedReceiver<ChannelNotification>>,
}

impl MessageState {
    pub fn new(
        cwd: String,
        render_tx: mpsc::Sender<RenderEvent>,
        render_cache: Arc<RwLock<RenderCache>>,
        render_notify: Arc<Notify>,
    ) -> Self {
        let (steering_result_tx, steering_result_rx) = mpsc::unbounded_channel();
        Self {
            view_messages: Vec::new(),
            round_start_vm_idx: 0,
            pipeline: MessagePipeline::new(cwd),
            render_tx,
            pending_render_rebuild: None,
            pending_render_modes: RefCell::new(Vec::new()),
            render_cache,
            render_notify,
            last_render_version: 0,
            pending_messages: Vec::new(),
            steering_result_tx,
            steering_result_rx,
            last_submitted_text: None,
            ephemeral_notes: Vec::new(),
            last_resize_width: None,
            last_viewport: None,
            layout_revision: AtomicU64::new(0),
            layout_epoch: AtomicU64::new(0),
            width_preparation: Default::default(),
            channel_notification_rx: None,
        }
    }

    pub(crate) fn layout_revision(&self) -> u64 {
        self.layout_revision.load(Ordering::Relaxed)
    }

    pub(crate) fn layout_epoch(&self) -> u64 {
        self.layout_epoch.load(Ordering::Relaxed)
    }

    pub(crate) fn invalidate_layout(&self) {
        self.layout_epoch.fetch_add(1, Ordering::Relaxed);
    }

    /// 所有内容/模式变更共用此入口；发送失败不把新布局标记为已入队。
    // RenderEvent 是渲染线程的大枚举（≥184B），TrySendError 需携带失败事件供
    // 调用方回收重排；Box 化会波及十余个调用点，故豁免。
    #[allow(clippy::result_large_err)]
    pub(crate) fn send_render_event(
        &self,
        event: RenderEvent,
    ) -> Result<(), mpsc::error::TrySendError<RenderEvent>> {
        let replaces_layout = matches!(
            event.payload(),
            RenderEvent::Clear
                | RenderEvent::ToggleDetail(_)
                | RenderEvent::ToggleDiff(_)
                | RenderEvent::ToggleHookRows(_)
                | RenderEvent::ToggleToolMessages(_)
                | RenderEvent::SetDetailStrategy(_)
        );
        // 模式变更是绝对状态，队列满时同类型的新值覆盖旧值，避免开关永久失步。
        let mode = event.clone_mode();
        let result = self.render_tx.try_send(event);
        if let Some(mode) = mode {
            let mut pending = self.pending_render_modes.borrow_mut();
            pending.retain(|old| {
                std::mem::discriminant(old.payload()) != std::mem::discriminant(mode.payload())
            });
            if matches!(&result, Err(mpsc::error::TrySendError::Full(_))) {
                pending.push(mode);
            }
        }
        result?;
        self.layout_revision.fetch_add(1, Ordering::Relaxed);
        if replaces_layout {
            self.invalidate_layout();
        }
        Ok(())
    }

    pub(crate) fn send_render_snapshot(&mut self, event: RenderEvent) {
        debug_assert!(matches!(
            event.payload(),
            RenderEvent::Rebuild(_) | RenderEvent::RebuildWithAnchor { .. } | RenderEvent::Clear
        ));
        if !self.pending_render_modes.borrow().is_empty() {
            // 锚点必须在新模式下计算，不能越过仍未入队的布局变更。
            self.pending_render_rebuild = Some(event);
            return;
        }
        self.pending_render_rebuild = match self.send_render_event(event) {
            Ok(()) | Err(mpsc::error::TrySendError::Closed(_)) => None,
            Err(mpsc::error::TrySendError::Full(event)) => Some(event),
        };
    }

    pub(crate) fn retry_render_rebuild(&mut self) -> bool {
        let mut updated = false;
        for event in self.pending_render_modes.take() {
            updated |= self.send_render_event(event).is_ok();
        }
        if !self.pending_render_modes.borrow().is_empty() {
            return updated;
        }
        let Some(event) = self.pending_render_rebuild.take() else {
            return updated;
        };
        self.send_render_snapshot(event);
        updated || (self.pending_render_rebuild.is_none() && !self.render_tx.is_closed())
    }

    pub(crate) fn render_updates_pending(&self) -> bool {
        self.pending_render_rebuild.is_some() || !self.pending_render_modes.borrow().is_empty()
    }

    /// 添加系统通知并记录锚点位置。
    ///
    /// 面板代码（通过 PanelContext）和 App 方法均可调用。
    pub fn push_system_note(&mut self, content: String) {
        let anchor = self.view_messages.len();
        let vm = MessageViewModel::system(content);
        self.ephemeral_notes.push((anchor, vm.clone()));
        self.view_messages.push(vm);
    }

    /// 添加正文带语义色的系统通知（如需要用户立刻注意到的警告）。
    pub fn push_toned_system_note(&mut self, content: String, tone: NoteTone) {
        let anchor = self.view_messages.len();
        let vm = MessageViewModel::system_toned(content, tone);
        self.ephemeral_notes.push((anchor, vm.clone()));
        self.view_messages.push(vm);
    }

    /// 添加带详细模式内容的系统通知并记录锚点位置。
    pub fn push_system_note_with_detail(&mut self, content: String, detail_content: String) {
        let anchor = self.view_messages.len();
        let vm = MessageViewModel::system_with_detail(content, detail_content);
        self.ephemeral_notes.push((anchor, vm.clone()));
        self.view_messages.push(vm);
    }

    /// 添加会话级 hook 结果行（`⎿` 前缀、整行按 `tone` 着色）并记录锚点位置。
    ///
    /// `detail_only` 为真时只有详细模式可见：成功汇总、异步完成、hook 自身失败
    /// 都不该在普通模式刷屏；阻断行（工具真的没跑）两种模式都要可见。
    pub fn push_hook_system_note(&mut self, content: String, tone: NoteTone, detail_only: bool) {
        let anchor = self.view_messages.len();
        let vm = MessageViewModel::system_hook_line(content, tone, detail_only);
        self.ephemeral_notes.push((anchor, vm.clone()));
        self.view_messages.push(vm);
    }

    fn round_user_message(&mut self) -> Option<&mut MessageViewModel> {
        let index = self.round_start_vm_idx.checked_sub(1)?;
        self.view_messages.get_mut(index)
    }

    /// 异步开场 hook 可能跨轮完成，开始时在发起它的用户气泡上建立锚点。
    pub(super) fn mark_session_start_user(&mut self) {
        let Some(user) = self.round_user_message() else {
            return;
        };
        if let MessageViewModel::UserBubble {
            session_start_hook_rows,
            system_reminder: false,
            ..
        } = user
        {
            if session_start_hook_rows.is_none() {
                *session_start_hook_rows = Some(Vec::new());
                user.recompute_hash();
            }
        }
    }

    /// 将开场 hook 行合并到发起它的用户气泡，使用气泡自身的消息间距。
    pub(super) fn attach_session_start_hook_row(&mut self, row: HookRow, is_async: bool) -> bool {
        let user = if is_async {
            self.view_messages.iter_mut().rev().find(|vm| {
                matches!(
                    vm,
                    MessageViewModel::UserBubble {
                        session_start_hook_rows: Some(_),
                        ..
                    }
                )
            })
        } else {
            self.round_user_message()
        };
        let Some(user) = user else {
            return false;
        };
        let MessageViewModel::UserBubble {
            session_start_hook_rows,
            system_reminder: false,
            ..
        } = user
        else {
            return false;
        };

        let rows = session_start_hook_rows.get_or_insert_with(Vec::new);
        if let Some(summary) = rows.iter_mut().find(|existing| {
            existing.kind == HookRowKind::Summary && row.kind == HookRowKind::Summary
        }) {
            *summary = row;
        } else {
            rows.push(row);
        }
        user.recompute_hash();
        true
    }

    /// 添加会话级 hook 行到**本轮用户消息下方**（而非视图尾部）。
    ///
    /// 供只在首轮触发一次的事件（SessionStart）使用：这类事件的产物语义上属于
    /// 「会话开场信息」，必须紧贴首个用户消息，而不是随事件到达时刻漂到工具块之后。
    /// 已存在的同组 hook 行按到达顺序排在前，多条上报（plugin + global）依次递增
    /// 锚点，保证 RebuildAll 归位时顺序不颠倒。
    pub fn push_hook_system_note_at_round_start(
        &mut self,
        content: String,
        tone: NoteTone,
        detail_only: bool,
    ) {
        let vm = MessageViewModel::system_hook_line(content, tone, detail_only);
        // 从本轮重建起点（用户消息之后的第一位）开始，跳过已经贴在这里的 hook 行
        let mut pos = self.round_start_vm_idx.min(self.view_messages.len());
        while matches!(
            self.view_messages.get(pos),
            Some(MessageViewModel::SystemNote {
                branch_prefix: true,
                ..
            })
        ) {
            pos += 1;
        }
        // 中间插入会把 pos 之后的条目整体右移一位，已登记的锚点必须同步右移，
        // 否则下一轮 RebuildAll 按旧锚点归位时这些条目会前移一位错位。
        for (anchor, _) in self.ephemeral_notes.iter_mut() {
            if *anchor >= pos {
                *anchor += 1;
            }
        }
        self.ephemeral_notes.push((pos, vm.clone()));
        self.view_messages.insert(pos, vm);
    }
}

#[cfg(test)]
#[path = "message_state_test.rs"]
mod tests;
