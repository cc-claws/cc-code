//! #307 压测：compact 失败/跳过时消息历史无上限增长
//!
//! 生产代码证据：
//!  1. `peri-agent/src/agent/state.rs:209` `add_message`：`self.messages.push(message)`，
//!     无任何上限，仅每 100 条打一条 warn（:216-222）。
//!  2. `peri-middlewares/src/compact_middleware.rs:126-184` `do_full_compact`：
//!     - model 为 None 时静默跳过（:127-136），消息原样保留；
//!     - 失败/取消时 `state.messages_mut().extend(own_messages)`（:158-160,:168-177），
//!       全部消息恢复，无任何降级修剪。
//!
//! 本测试真实分配 100MB 级别的消息历史（模拟 compact 持续失败的长会话），
//! 确认消息数量与内存占用无上限线性增长。

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

use peri_agent::agent::{AgentState, State};
use peri_agent::messages::{BaseMessage, MessageContent, MessageId};

// ─── 计数分配器 ────────────────────────────────────────────────────────────

struct CountingAlloc;

static LIVE_BYTES: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: 直接委托给 System，只做计数
        let ptr = System.alloc(layout);
        if !ptr.is_null() {
            LIVE_BYTES.fetch_add(layout.size(), Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: 与 alloc 配对，直接委托给 System
        System.dealloc(ptr, layout);
        LIVE_BYTES.fetch_sub(layout.size(), Ordering::Relaxed);
    }
}

#[global_allocator]
static GLOBAL: CountingAlloc = CountingAlloc;

fn live_bytes() -> usize {
    LIVE_BYTES.load(Ordering::Relaxed)
}

// ─── 压测 ──────────────────────────────────────────────────────────────────

#[test]
fn stress_307_message_history_grows_without_bound() {
    const NUM_MSGS: usize = 20_000;
    const BYTES_PER_MSG: usize = 5 * 1024; // 5KB（模拟 tool result 文本）

    let mut state = AgentState::new("/tmp/stress-307");

    let before = live_bytes();

    // 模拟 compact 持续失败的长会话：不断追加消息
    for i in 0..NUM_MSGS {
        state.add_message(BaseMessage::Human {
            id: MessageId::new(),
            content: MessageContent::text("T".repeat(BYTES_PER_MSG)),
        });
        std::hint::black_box(i);
    }

    let after = live_bytes();
    let grown = after.saturating_sub(before);
    let msg_count = state.messages().len();

    println!(
        "messages={} grown={}MB (cap={})",
        msg_count,
        grown / 1024 / 1024,
        peri_agent::agent::state::MAX_MESSAGES,
    );

    // 断言（#307 修复后）：硬上限生效，20000 条被截断到 MAX_MESSAGES，
    // 内存有界（≤ MAX_MESSAGES * 单条大小 + 余量），不再无界增长。
    assert_eq!(
        msg_count,
        peri_agent::agent::state::MAX_MESSAGES,
        "#307 修复未生效：期望消息被硬上限截断到 {} 条，实际 {msg_count} 条",
        peri_agent::agent::state::MAX_MESSAGES,
    );

    // 内存上界：10000 条 * 5KB = ~50MB，留 20% 余量
    let max_expected = peri_agent::agent::state::MAX_MESSAGES * BYTES_PER_MSG * 12 / 10;
    assert!(
        grown <= max_expected,
        "#307 修复未生效：内存增长 {}MB 超过上限 {}MB",
        grown / 1024 / 1024,
        max_expected / 1024 / 1024,
    );

    drop(state);
}
