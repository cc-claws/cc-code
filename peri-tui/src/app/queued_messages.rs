use super::{App, QueuedMessage, QueuedMessageAction};

impl App {
    pub(crate) fn queue_user_message(&mut self, text: String) {
        let text = self.expand_pasted_text(&text);
        let ids = crate::clipboard::image_placeholder::parse_placeholders(&text);
        let session = self.session_mgr.current_mut();
        let mut attachments = std::mem::take(&mut session.metadata.pending_attachments);
        attachments.retain(|image| ids.contains(&image.image_id));
        session
            .messages
            .pending_messages
            .push(QueuedMessage::new(text, attachments));
        session.ui.textarea = super::build_textarea(true);
        self.clear_pasted_text_blocks();
        self.update_textarea_hint();
    }

    /// 自动发送队列时保留尚未提交的输入、粘贴块和图片。
    pub(crate) fn submit_queued_message(&mut self, message: QueuedMessage) {
        let session = self.session_mgr.current_mut();
        let draft = std::mem::replace(&mut session.ui.textarea, super::build_textarea(false));
        let draft_blocks = std::mem::take(&mut session.ui.pasted_text_blocks);
        let draft_block_id = session.ui.next_pasted_text_id;
        let draft_images = std::mem::replace(
            &mut session.metadata.pending_attachments,
            message.attachments,
        );
        self.submit_message(message.text);
        let session = self.session_mgr.current_mut();
        session.ui.textarea = draft;
        session.ui.pasted_text_blocks = draft_blocks;
        session.ui.next_pasted_text_id = draft_block_id;
        session.metadata.pending_attachments = draft_images;
    }

    pub(crate) fn handle_queued_message_action(&mut self, action: QueuedMessageAction) {
        match action {
            QueuedMessageAction::Steer(id) => self.steer_queued_message(id),
            QueuedMessageAction::Delete(id) => {
                self.session_mgr
                    .current_mut()
                    .messages
                    .pending_messages
                    .retain(|message| message.id != id || message.sending);
            }
            QueuedMessageAction::PreviousPage | QueuedMessageAction::NextPage => {
                let session = self.session_mgr.current_mut();
                let height = session
                    .ui
                    .queued_messages_area
                    .map_or(0, |area| area.height as usize);
                let paginated = session.messages.pending_messages.len() > height.min(3);
                let page_size = height.saturating_sub(usize::from(paginated)).min(3);
                if matches!(action, QueuedMessageAction::PreviousPage) {
                    session.ui.queued_messages_offset =
                        session.ui.queued_messages_offset.saturating_sub(page_size);
                } else if session.ui.queued_messages_offset + page_size
                    < session.messages.pending_messages.len()
                {
                    session.ui.queued_messages_offset += page_size;
                }
            }
        }
        // 删除后旧坐标不再代表原消息，等下一帧重新登记。
        self.session_mgr
            .current_mut()
            .ui
            .queued_message_actions
            .clear();
    }

    fn steer_queued_message(&mut self, id: uuid::Uuid) {
        let session = self.session_mgr.current_mut();
        let Some(index) = session
            .messages
            .pending_messages
            .iter()
            .position(|message| message.id == id && !message.sending)
        else {
            return;
        };
        if !session.ui.loading {
            if session
                .messages
                .pending_messages
                .iter()
                .any(|message| message.sending)
            {
                return;
            }
            let message = session.messages.pending_messages.remove(index);
            self.submit_queued_message(message);
            return;
        }
        let Some(client) = self.acp_client.clone() else {
            self.push_system_note(self.services.lc.tr("queue-unavailable"));
            return;
        };
        // 点击时固定会话，避免异步任务启动前切换会话而误发到新对话。
        let Some(session_id) = client.session_id() else {
            self.push_system_note(self.services.lc.tr("queue-unavailable"));
            return;
        };
        let message = &mut session.messages.pending_messages[index];
        let content = message.content();
        message.sending = true;
        let tx = session.messages.steering_result_tx.clone();
        tokio::spawn(async move {
            let result = client.steer(session_id, content).await;
            let _ = tx.send((id, result));
        });
    }

    pub(crate) fn poll_steering_results(&mut self) -> bool {
        let mut updated = false;
        while let Ok((id, result)) = self
            .session_mgr
            .current_mut()
            .messages
            .steering_result_rx
            .try_recv()
        {
            let Some(index) = self
                .session_mgr
                .current()
                .messages
                .pending_messages
                .iter()
                .position(|message| message.id == id)
            else {
                continue;
            };
            updated = true;
            match result {
                Ok(()) => {
                    let message = self
                        .session_mgr
                        .current_mut()
                        .messages
                        .pending_messages
                        .remove(index);
                    self.push_input_history(message.text);
                    // 已插入新的用户消息，后续中断不能再回滚最初那条输入。
                    self.session_mgr.current_mut().messages.last_submitted_text = None;
                }
                Err(error) => {
                    self.session_mgr.current_mut().messages.pending_messages[index].sending = false;
                    tracing::warn!(%error, "queued message steering failed; kept in queue");
                    self.push_system_note(self.services.lc.tr("queue-steer-failed"));
                }
            }
        }
        if updated {
            self.session_mgr
                .current_mut()
                .ui
                .queued_message_actions
                .clear();
            if !self.session_mgr.current().ui.loading {
                self.flush_pending_messages();
            }
        }
        updated
    }
}

#[cfg(test)]
#[path = "queued_messages_test.rs"]
mod tests;
