use std::time::{Duration, Instant};

/// 输入和后台更新共用一个重画期限；连续请求不能推迟已排定的尾帧。
pub struct RedrawScheduler {
    interval: Duration,
    dirty: bool,
    next_attempt: Instant,
}

impl RedrawScheduler {
    pub fn new(interval: Duration, now: Instant) -> Self {
        Self {
            interval,
            dirty: true,
            next_attempt: now,
        }
    }

    pub fn request_redraw(&mut self) {
        self.dirty = true;
    }

    pub fn is_due(&self, now: Instant) -> bool {
        self.dirty && now >= self.next_attempt
    }

    pub fn wait_timeout(&self, now: Instant, idle_timeout: Duration) -> Duration {
        if self.dirty {
            idle_timeout.min(self.next_attempt.saturating_duration_since(now))
        } else {
            idle_timeout
        }
    }

    /// 期限从绘制实际完成时开始计算；布局未就绪或输出失败时保留 dirty，避免忙等。
    pub fn finish_draw(&mut self, completed_at: Instant, rendered: bool) {
        self.dirty = !rendered;
        self.next_attempt = completed_at + self.interval;
    }
}

#[cfg(test)]
#[path = "redraw_scheduler_test.rs"]
mod tests;
