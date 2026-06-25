use super::hitl_prompt::PendingAttachment;

/// 会话元数据：低频访问的会话状态。
pub struct SessionMetadata {
    pub session_id: uuid::Uuid,
    pub pending_attachments: Vec<PendingAttachment>,
    pub last_human_message: Option<String>,
    pub pre_submit_state_len: usize,
    /// 下一个分配给 textarea 占位符 `[Image #N]` 的稳定 ID。
    /// 单调递增，不随附件增删回退——避免 textarea 中残留文本与附件错位。
    pub next_image_id: usize,
}

impl SessionMetadata {
    pub fn new() -> Self {
        Self {
            session_id: uuid::Uuid::new_v7(uuid::Timestamp::now(uuid::NoContext)),
            pending_attachments: Vec::new(),
            last_human_message: None,
            pre_submit_state_len: 0,
            next_image_id: 1,
        }
    }

    /// 分配下一个 image_id 并自增计数器。
    pub fn alloc_image_id(&mut self) -> usize {
        let id = self.next_image_id;
        self.next_image_id += 1;
        id
    }
}

impl Default for SessionMetadata {
    fn default() -> Self {
        Self::new()
    }
}
