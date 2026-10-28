use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};

use cc_agent::interaction::channel_types::ChannelNotification;
use parking_lot::RwLock;
use tokio::sync::{mpsc, Notify};

use crate::ui::{
    message_view::MessageViewModel,
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

/// 消息状态：会话级的消息管线、渲染通道、消息列表。
pub struct MessageState {
    pub view_messages: Vec<MessageViewModel>,
    pub round_start_vm_idx: usize,
    pub pipeline: MessagePipeline,
    pub render_tx: mpsc::Sender<RenderEvent>,
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
    pub last_viewport: Option<(usize, usize, bool, u16, Option<ViewportAnchor>)>,
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
    pub(crate) fn send_render_event(
        &self,
        event: RenderEvent,
    ) -> Result<(), mpsc::error::TrySendError<RenderEvent>> {
        let replaces_layout = matches!(
            event,
            RenderEvent::Clear
                | RenderEvent::ToggleDetail(_)
                | RenderEvent::ToggleDiff(_)
                | RenderEvent::ToggleToolMessages(_)
                | RenderEvent::SetDetailStrategy(_)
        );
        self.render_tx.try_send(event)?;
        self.layout_revision.fetch_add(1, Ordering::Relaxed);
        if replaces_layout {
            self.invalidate_layout();
        }
        Ok(())
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

    /// 添加带详细模式内容的系统通知并记录锚点位置。
    pub fn push_system_note_with_detail(&mut self, content: String, detail_content: String) {
        let anchor = self.view_messages.len();
        let vm = MessageViewModel::system_with_detail(content, detail_content);
        self.ephemeral_notes.push((anchor, vm.clone()));
        self.view_messages.push(vm);
    }
}
