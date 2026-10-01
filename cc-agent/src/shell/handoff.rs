use std::sync::Mutex;

use tokio::sync::Notify;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Ownership {
    Foreground,
    Background,
    Settled,
}

/// 命令归属的唯一权威。通知只负责唤醒，不能代替所有权移交。
/// 进程终态独立保存：退出与前台等待结束不必同时发生。
#[derive(Debug)]
pub struct ShellHandoff {
    background_supported: bool,
    ownership: Mutex<Ownership>,
    changed: Notify,
}

impl ShellHandoff {
    pub fn new(background_supported: bool, backgrounded: bool) -> Self {
        Self {
            background_supported,
            ownership: Mutex::new(if backgrounded {
                Ownership::Background
            } else {
                Ownership::Foreground
            }),
            changed: Notify::new(),
        }
    }

    /// 手动和自动后台化使用同一次原子转换；成功后取消前台不得再杀进程。
    pub fn background(&self) -> bool {
        let mut owner = self.ownership.lock().unwrap_or_else(|e| e.into_inner());
        if !self.background_supported || *owner != Ownership::Foreground {
            return false;
        }
        *owner = Ownership::Background;
        drop(owner);
        self.changed.notify_waiters();
        true
    }

    /// 完成/取消领取前台所有权。返回 false 表示已移交，不能再次取消或返回前台结果。
    pub fn settle_foreground(&self) -> bool {
        let mut owner = self.ownership.lock().unwrap_or_else(|e| e.into_inner());
        if *owner != Ownership::Foreground {
            return false;
        }
        *owner = Ownership::Settled;
        true
    }

    pub fn is_backgrounded(&self) -> bool {
        *self.ownership.lock().unwrap_or_else(|e| e.into_inner()) == Ownership::Background
    }

    pub fn is_foreground_pending(&self) -> bool {
        *self.ownership.lock().unwrap_or_else(|e| e.into_inner()) == Ownership::Foreground
    }

    pub async fn wait_for_background(&self) {
        loop {
            let notified = self.changed.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if self.is_backgrounded() {
                return;
            }
            notified.await;
        }
    }
}

#[cfg(test)]
#[path = "handoff_test.rs"]
mod tests;
