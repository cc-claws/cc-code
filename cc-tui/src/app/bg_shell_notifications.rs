//! 后台 shell 通知：运行中走安全插入，空闲时合并，明确拒绝后保留重试。

use std::time::Instant;

use cc_acp::{
    session::steering::{STEERING_NOT_CONSUMED, STEERING_NOT_ENQUEUED},
    transport::types::AcpError,
};
use cc_agent::messages::MessageContent;
use tokio::sync::oneshot;

use super::{App, PendingBgShellNotification};

type Attempt = (Option<String>, Option<Instant>);

struct InFlightNotification {
    notification: PendingBgShellNotification,
    attempt: Attempt,
    receipt: oneshot::Receiver<Result<(), AcpError>>,
}

#[derive(Default)]
pub(crate) struct BgShellSteering {
    in_flight: Vec<InFlightNotification>,
    /// 断连等情况下不能证明是否消费，保留原文但不自动重投。
    unconfirmed: Vec<PendingBgShellNotification>,
    /// 同一轮已拒绝的通知等到下一轮或空闲再尝试，避免每帧请求空队列。
    last_attempt: Option<Attempt>,
}

impl App {
    fn bg_shell_notification_owner(&self) -> Option<String> {
        self.session_mgr
            .current()
            .current_thread_id
            .clone()
            .or_else(|| {
                self.acp_client
                    .as_ref()
                    .and_then(|client| client.session_id())
            })
    }

    fn bg_shell_notification_attempt(&self) -> Attempt {
        (
            self.bg_shell_notification_owner(),
            self.session_mgr.current().agent.task_start_time,
        )
    }

    /// 同步入口只发起请求；消费回执由每帧轮询收取，不阻塞 TUI。
    pub(super) fn inject_bg_shell_notifications(
        &mut self,
        notifications: Vec<PendingBgShellNotification>,
    ) {
        if notifications.is_empty() {
            return;
        }
        let owner = self.bg_shell_notification_owner();
        for mut notification in notifications {
            // 发送前绑定原会话，迟到的回执不能把 None 当作新会话的通配符。
            if notification.owner_session_id.is_none() {
                notification.owner_session_id = owner.clone();
            }
            self.session_mgr
                .current_mut()
                .pending_bg_shell_notifications
                .push_back(notification);
        }
        self.dispatch_pending_bg_shell_notifications(true);
    }

    /// 取走当前会话滞留的后台完成通知，交由用户下一次主动提交一并送出。
    ///
    /// 滞留来源有两类：运行中 steer 被明确拒绝（`-32001`/`-32002`）后回队，以及中断/报错收尾后
    /// 不再自动唤醒新一轮的保留。随用户主动提交送出，既不起额外一轮 turn，也不会重复投递
    /// ——取出即离开队列，此后不会再被 steer 或空闲合并消费。
    pub(super) fn take_pending_bg_shell_notifications(&mut self) -> Option<String> {
        let owner = self.bg_shell_notification_owner();
        let mut content = Vec::new();
        self.session_mgr
            .current_mut()
            .pending_bg_shell_notifications
            .retain(|notification| {
                if notification
                    .owner_session_id
                    .as_deref()
                    .is_none_or(|id| Some(id) == owner.as_deref())
                {
                    content.push(notification.content.clone());
                    false
                } else {
                    // 其它会话的通知留给对应会话消费
                    true
                }
            });
        if content.is_empty() {
            None
        } else {
            Some(content.join("\n\n"))
        }
    }

    pub(crate) fn poll_bg_shell_notifications(&mut self) -> bool {
        let mut updated = false;
        let requests =
            std::mem::take(&mut self.session_mgr.current_mut().bg_shell_steering.in_flight);
        for mut request in requests {
            let result = match request.receipt.try_recv() {
                Ok(result) => result,
                Err(oneshot::error::TryRecvError::Empty) => {
                    self.session_mgr
                        .current_mut()
                        .bg_shell_steering
                        .in_flight
                        .push(request);
                    continue;
                }
                Err(oneshot::error::TryRecvError::Closed) => Err(AcpError::new(
                    -32603,
                    "background steering request cancelled",
                )),
            };
            updated = true;
            match result {
                Ok(()) => {
                    if request.attempt == self.bg_shell_notification_attempt() {
                        // 已写入的通知不能被本轮中断时的输入回滚截掉。
                        self.session_mgr.current_mut().messages.last_submitted_text = None;
                    }
                }
                Err(error)
                    if matches!(error.code, STEERING_NOT_ENQUEUED | STEERING_NOT_CONSUMED) =>
                {
                    self.session_mgr
                        .current_mut()
                        .pending_bg_shell_notifications
                        .push_back(request.notification);
                }
                Err(error) => {
                    tracing::warn!(%error, owner = ?request.notification.owner_session_id,
                        "background notification delivery unknown; retained without automatic retry");
                    self.session_mgr
                        .current_mut()
                        .bg_shell_steering
                        .unconfirmed
                        .push(request.notification);
                }
            }
        }
        self.dispatch_pending_bg_shell_notifications(false) || updated
    }

    fn dispatch_pending_bg_shell_notifications(&mut self, new_notifications: bool) -> bool {
        if self
            .session_mgr
            .current()
            .pending_bg_shell_notifications
            .is_empty()
        {
            return false;
        }
        let loading = self.session_mgr.current().ui.loading;
        let attempt = self.bg_shell_notification_attempt();
        if loading {
            if !new_notifications
                && self
                    .session_mgr
                    .current()
                    .bg_shell_steering
                    .last_attempt
                    .as_ref()
                    == Some(&attempt)
            {
                return false;
            }
            let Some(client) = self.acp_client.as_ref() else {
                return false;
            };
            let Some(session_id) = client.session_id() else {
                return false;
            };
            // 会话切换的异步握手尚未完成时，不发往 client 仍指向的旧会话。
            if attempt
                .0
                .as_deref()
                .is_some_and(|owner| owner != session_id)
            {
                return false;
            }
        } else if self.session_mgr.current().agent.reconcile_already_done {
            // 中断/报错后保留到下一次主动提交，不以通知自动恢复被终止的工作。
            return false;
        }
        let owner = attempt.0.as_deref();
        let mut notifications = Vec::new();
        self.session_mgr
            .current_mut()
            .pending_bg_shell_notifications
            .retain(|notification| {
                if notification
                    .owner_session_id
                    .as_deref()
                    .is_none_or(|id| Some(id) == owner)
                {
                    notifications.push(notification.clone());
                    false
                } else {
                    true
                }
            });
        if notifications.is_empty() {
            return false;
        }
        if loading {
            self.session_mgr
                .current_mut()
                .bg_shell_steering
                .last_attempt = Some(attempt.clone());
            for notification in notifications {
                self.spawn_bg_shell_notification(notification, attempt.clone());
            }
        } else {
            self.submit_message(
                notifications
                    .into_iter()
                    .map(|notification| notification.content)
                    .collect::<Vec<_>>()
                    .join("\n\n"),
            );
        }
        true
    }

    fn spawn_bg_shell_notification(
        &mut self,
        notification: PendingBgShellNotification,
        attempt: Attempt,
    ) {
        let Some(client) = self.acp_client.clone() else {
            return;
        };
        let Some(session_id) = client.session_id() else {
            return;
        };
        let content = MessageContent::text(notification.content.clone());
        let (tx, receipt) = oneshot::channel();
        self.session_mgr
            .current_mut()
            .bg_shell_steering
            .in_flight
            .push(InFlightNotification {
                notification,
                attempt,
                receipt,
            });
        tokio::spawn(async move {
            let _ = tx.send(client.steer_with_receipt(session_id, content).await);
        });
    }
}

#[cfg(test)]
#[path = "bg_shell_notifications_test.rs"]
mod tests;
