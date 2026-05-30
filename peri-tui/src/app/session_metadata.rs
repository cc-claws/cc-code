use super::hitl_prompt::PendingAttachment;

/// 会话元数据：低频访问的会话状态。
pub struct SessionMetadata {
    pub session_id: uuid::Uuid,
    pub pending_attachments: Vec<PendingAttachment>,
    pub last_human_message: Option<String>,
    pub pre_submit_state_len: usize,
}

impl SessionMetadata {
    pub fn new() -> Self {
        Self {
            session_id: uuid::Uuid::new_v7(uuid::Timestamp::now(uuid::NoContext)),
            pending_attachments: Vec::new(),
            last_human_message: None,
            pre_submit_state_len: 0,
        }
    }
}

impl Default for SessionMetadata {
    fn default() -> Self {
        Self::new()
    }
}
