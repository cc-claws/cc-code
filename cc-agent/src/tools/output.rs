//! 工具输出的展示通道：身份由调度器提供，尾缓存与节流不接触 AgentState。

use std::{
    collections::VecDeque,
    future::Future,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::{
    agent::events::{AgentEvent, AgentEventHandler},
    messages::MessageId,
};

const MAX_TAIL_BYTES: usize = 64 * 1024;
const OUTPUT_INTERVAL: Duration = Duration::from_secs(1);

/// 当前调用的可信输出通道。spawn 不继承 task-local，宿主须在 execute 内捕获。
#[derive(Clone)]
pub struct ToolOutputContext {
    message_id: MessageId,
    tool_call_id: String,
    source_agent_id: Option<String>,
    handler: Arc<dyn AgentEventHandler>,
    closed: Arc<Mutex<bool>>,
    cancel: CancellationToken,
}

tokio::task_local! {
    static TOOL_OUTPUT: Option<ToolOutputContext>;
}

impl ToolOutputContext {
    pub fn new(
        message_id: MessageId,
        tool_call_id: String,
        source_agent_id: Option<String>,
        handler: Arc<dyn AgentEventHandler>,
    ) -> Self {
        Self {
            message_id,
            tool_call_id,
            source_agent_id,
            handler,
            closed: Arc::new(Mutex::new(false)),
            cancel: CancellationToken::new(),
        }
    }

    pub fn current() -> Option<Self> {
        TOOL_OUTPUT.try_with(Clone::clone).ok().flatten()
    }

    /// None 可屏蔽外层通道，用于关闭 streaming 或直接后台执行。
    pub async fn scope<F: Future>(context: Option<Self>, future: F) -> F::Output {
        TOOL_OUTPUT.scope(context, future).await
    }

    pub(crate) async fn scope_invocation<F: Future>(context: Option<Self>, future: F) -> F::Output {
        // 即使宿主的 reader 尚未观察到取消，ToolEnd 前也要封闭展示通道。
        let _lifetime = InvocationOutputLifetime(context.clone());
        Self::scope(context, future).await
    }

    pub fn stream(self) -> ToolOutputStream {
        ToolOutputStream {
            state: Arc::new(Mutex::new(OutputState {
                context: self,
                tail: VecDeque::with_capacity(MAX_TAIL_BYTES),
                total_bytes: 0,
                newline_count: 0,
                ends_with_newline: false,
                started_at: Instant::now(),
                last_emitted: None,
                dirty: false,
                stopped: false,
                timer_pending: false,
            })),
            pending: None,
        }
    }

    fn close(&self) {
        // 与 publish 共用锁；close 返回后，不能再出现迟到的 ToolOutput。
        *self
            .closed
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = true;
        self.cancel.cancel();
    }
}

struct InvocationOutputLifetime(Option<ToolOutputContext>);

impl Drop for InvocationOutputLifetime {
    fn drop(&mut self) {
        if let Some(context) = &self.0 {
            context.close();
        }
    }
}

struct OutputState {
    context: ToolOutputContext,
    tail: VecDeque<u8>,
    total_bytes: usize,
    newline_count: usize,
    ends_with_newline: bool,
    started_at: Instant,
    last_emitted: Option<Instant>,
    dirty: bool,
    stopped: bool,
    timer_pending: bool,
}

impl OutputState {
    fn publish(&mut self, now: Instant) {
        let closed = self
            .context
            .closed
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if *closed || self.stopped || !self.dirty {
            return;
        }
        self.dirty = false;
        self.last_emitted = Some(now);
        self.context.handler.on_event(AgentEvent::ToolOutput {
            message_id: self.context.message_id,
            tool_call_id: self.context.tool_call_id.clone(),
            tail: decode_tail(self.tail.make_contiguous()),
            total_lines: self
                .newline_count
                .saturating_add(usize::from(!self.ends_with_newline)),
            total_bytes: self.total_bytes,
            elapsed_ms: now
                .duration_since(self.started_at)
                .as_millis()
                .min(u64::MAX as u128) as u64,
            source_agent_id: self.context.source_agent_id.clone(),
        });
    }
}

/// 每个命令共享一个 stream，stdout/stderr 按到达顺序 push。
///
/// 首次立即发布，随后每秒最多发布一次。被抑制的 burst 在下一秒发布一次最新值；
/// 没有新内容时不重复发布。stop/Drop 会取消尚未触发的 timer。
pub struct ToolOutputStream {
    state: Arc<Mutex<OutputState>>,
    pending: Option<JoinHandle<()>>,
}

impl ToolOutputStream {
    pub fn push(&mut self, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        if state.stopped || state.context.cancel.is_cancelled() {
            return;
        }
        state.total_bytes = state.total_bytes.saturating_add(bytes.len());
        state.newline_count = state
            .newline_count
            .saturating_add(bytes.iter().filter(|byte| **byte == b'\n').count());
        state.ends_with_newline = bytes.last() == Some(&b'\n');
        // 先丢弃再追加，连瞬时的环形缓存长度也不能超过 64 KiB。
        if bytes.len() >= MAX_TAIL_BYTES {
            state.tail.clear();
            state.tail.extend(&bytes[bytes.len() - MAX_TAIL_BYTES..]);
        } else {
            let remove = state
                .tail
                .len()
                .saturating_add(bytes.len())
                .saturating_sub(MAX_TAIL_BYTES);
            state.tail.drain(..remove);
            state.tail.extend(bytes);
        }
        state.dirty = true;
        let now = Instant::now();
        let due = state.last_emitted.map(|last| last + OUTPUT_INTERVAL);
        if due.is_none_or(|due| now >= due) {
            // 旧 timer 可能已到期却尚未获锁；本次发布取代它，防止重复发同一尾部。
            if let Some(pending) = self.pending.take() {
                pending.abort();
            }
            state.timer_pending = false;
            state.publish(now);
        } else if !state.timer_pending {
            let due = due.expect("已有上次输出时才安排延迟发布");
            let cancel = state.context.cancel.clone();
            let shared = Arc::clone(&self.state);
            state.timer_pending = true;
            self.pending = Some(tokio::spawn(async move {
                tokio::select! {
                    biased;
                    _ = cancel.cancelled() => {
                        let mut state = shared.lock().unwrap_or_else(|error| error.into_inner());
                        state.timer_pending = false;
                        state.dirty = false;
                    }
                    _ = tokio::time::sleep_until(tokio::time::Instant::from_std(due)) => {
                        let mut state = shared.lock().unwrap_or_else(|error| error.into_inner());
                        state.timer_pending = false;
                        state.publish(Instant::now());
                    }
                }
            }));
        }
    }

    /// 宿主移交后台时立即调用；之后保留最终结果通道，停止所有展示输出。
    pub fn stop(&mut self) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        state.stopped = true;
        state.timer_pending = false;
        if let Some(pending) = self.pending.take() {
            pending.abort();
        }
    }
}

impl Drop for ToolOutputStream {
    fn drop(&mut self) {
        self.stop();
    }
}

fn decode_tail(bytes: &[u8]) -> String {
    // 环形缓存可能从 UTF-8 字符中间开始；末端可能尚未收到字符剩余字节。
    // 两种边界都不应触发 Windows GBK 兜底，完整最终结果仍使用原有解码入口。
    let start = bytes
        .iter()
        .take(3)
        .take_while(|byte| **byte & 0xc0 == 0x80)
        .count();
    let candidate = &bytes[start..];
    let text = match std::str::from_utf8(candidate) {
        Ok(text) => text.to_string(),
        Err(error) if error.error_len().is_none() => {
            String::from_utf8_lossy(&candidate[..error.valid_up_to()]).into_owned()
        }
        Err(_) => crate::encoding::decode_output_bytes(bytes),
    };
    if text.len() <= MAX_TAIL_BYTES {
        return text;
    }
    let first = text
        .char_indices()
        .find(|(index, _)| *index >= text.len() - MAX_TAIL_BYTES)
        .map(|(index, _)| index)
        .unwrap_or(text.len());
    text[first..].to_string()
}

#[cfg(test)]
#[path = "output_test.rs"]
mod tests;
