//! 独立读取终端输入，不让渲染/Agent 阻塞控制台消费；仅合并相邻的悬停事件。
use parking_lot::{Condvar, Mutex, MutexGuard};
use ratatui::crossterm::event::{self, Event, MouseEventKind};
use std::{
    collections::VecDeque,
    io,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

// 外部编辑器持有此锁期间，读取线程不能碰终端输入。
static INPUT_GATE: Mutex<()> = Mutex::new(());
const POLL_INTERVAL: Duration = Duration::from_millis(10);
const MAX_QUEUED_EVENTS: usize = 4096;

pub fn pause_input() -> MutexGuard<'static, ()> {
    INPUT_GATE.lock()
}

#[derive(Default)]
struct Queue {
    events: VecDeque<Event>,
    error: Option<io::Error>,
}

impl Queue {
    fn coalesce_hover(&mut self, event: &Event) -> bool {
        if matches!(event, Event::Mouse(m) if m.kind == MouseEventKind::Moved)
            && matches!(self.events.back(), Some(Event::Mouse(m)) if m.kind == MouseEventKind::Moved)
        {
            *self.events.back_mut().expect("已有悬停事件") = event.clone();
            true
        } else {
            false
        }
    }
}

#[derive(Default)]
struct Shared {
    queue: Mutex<Queue>,
    changed: Condvar,
    stop: AtomicBool,
}

pub(super) struct InputPump {
    shared: Arc<Shared>,
    worker: Option<JoinHandle<()>>,
}

impl InputPump {
    pub fn start() -> io::Result<Self> {
        Self::with_source(|timeout| {
            if event::poll(timeout)? {
                event::read().map(Some)
            } else {
                Ok(None)
            }
        })
    }

    fn with_source(
        mut source: impl FnMut(Duration) -> io::Result<Option<Event>> + Send + 'static,
    ) -> io::Result<Self> {
        let shared = Arc::new(Shared::default());
        let worker_shared = shared.clone();
        let worker = thread::Builder::new()
            .name("tui-input".into())
            .spawn(move || {
                let shared = worker_shared;
                while !shared.stop.load(Ordering::Acquire) {
                    let result = {
                        // 有界等待也保证暂停/退出能及时拿到锁。
                        let Some(_gate) = INPUT_GATE.try_lock_for(POLL_INTERVAL) else {
                            continue;
                        };
                        if shared.stop.load(Ordering::Acquire) {
                            break;
                        }
                        source(POLL_INTERVAL)
                    };
                    let event = match result {
                        Ok(Some(event)) => event,
                        Ok(None) => continue,
                        Err(error) => {
                            shared.queue.lock().error = Some(error);
                            shared.changed.notify_all();
                            break;
                        }
                    };
                    let mut queue = shared.queue.lock();
                    if queue.coalesce_hover(&event) {
                        continue;
                    }
                    // 非悬停事件不能丢弃；队列上限防止意外无限增长。
                    while queue.events.len() >= MAX_QUEUED_EVENTS
                        && !shared.stop.load(Ordering::Acquire)
                    {
                        shared.changed.wait_for(&mut queue, POLL_INTERVAL);
                    }
                    if shared.stop.load(Ordering::Acquire) {
                        break;
                    }
                    queue.events.push_back(event);
                    shared.changed.notify_all();
                }
            })?;
        Ok(Self {
            shared,
            worker: Some(worker),
        })
    }

    pub fn next(&self, timeout: Duration) -> io::Result<Option<Event>> {
        let deadline = Instant::now() + timeout;
        let mut queue = self.shared.queue.lock();
        loop {
            if let Some(event) = queue.events.pop_front() {
                self.shared.changed.notify_all();
                return Ok(Some(event));
            }
            if let Some(error) = queue.error.take() {
                return Err(error);
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Ok(None);
            }
            self.shared.changed.wait_for(&mut queue, remaining);
        }
    }
    pub fn stop(&mut self) {
        self.shared.stop.store(true, Ordering::Release);
        self.shared.changed.notify_all();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

impl Drop for InputPump {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
#[path = "input_pump_test.rs"]
mod tests;
