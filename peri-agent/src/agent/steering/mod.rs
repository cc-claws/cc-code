//! 本轮执行中的用户补充。只有安全边界写入 state 后才确认接收。

use std::sync::Arc;

use parking_lot::Mutex;
use tokio::sync::oneshot;

use crate::messages::MessageContent;

#[derive(Clone, Default)]
pub struct SteeringQueue(Arc<Mutex<QueueState>>);

#[derive(Default)]
struct QueueState {
    closed: bool,
    pending: Vec<PendingSteering>,
}

pub(crate) struct PendingSteering {
    pub content: MessageContent,
    pub consumed: oneshot::Sender<()>,
}

impl SteeringQueue {
    /// 返回的接收端成功表示已写入消息历史；关闭表示本轮未接收，调用方保留原消息。
    pub fn enqueue(&self, content: MessageContent) -> Option<oneshot::Receiver<()>> {
        let mut queue = self.0.lock();
        if queue.closed {
            return None;
        }
        let (consumed, receipt) = oneshot::channel();
        queue.pending.push(PendingSteering { content, consumed });
        Some(receipt)
    }

    pub(crate) fn drain(&self) -> Vec<PendingSteering> {
        std::mem::take(&mut self.0.lock().pending)
    }

    /// 最终回答与新请求共用同一把锁：取到消息则继续，否则封口，避免结束竞态丢消息。
    pub(crate) fn drain_or_close(&self) -> Vec<PendingSteering> {
        let mut queue = self.0.lock();
        if queue.pending.is_empty() {
            queue.closed = true;
        }
        std::mem::take(&mut queue.pending)
    }

    pub fn close(&self) {
        let mut queue = self.0.lock();
        queue.closed = true;
        queue.pending.clear();
    }

    pub fn close_on_drop(&self) -> SteeringGuard {
        SteeringGuard(self.clone())
    }
}

/// 包括取消 future、工具错误、初始化失败在内的所有退出路径均拒绝未消费消息。
pub struct SteeringGuard(SteeringQueue);

impl Drop for SteeringGuard {
    fn drop(&mut self) {
        self.0.close();
    }
}

#[cfg(test)]
#[path = "mod_test.rs"]
mod tests;
