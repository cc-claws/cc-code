use std::sync::Arc;

use parking_lot::RwLock;
use peri_agent::interaction::channel_types::ChannelNotification;
use tokio::sync::{mpsc, Notify};

use crate::ui::{
    message_view::MessageViewModel,
    render_thread::{RenderCache, RenderEvent},
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

    pub(crate) fn content(&self) -> peri_agent::messages::MessageContent {
        use peri_agent::messages::{ContentBlock, MessageContent};
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
            channel_notification_rx: None,
        }
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
}
