//! #306 压测：每次 LLM 调用深拷贝整个消息历史两次
//!
//! 生产代码路径（逐行对应）：
//!  1. `cc-agent/src/agent/executor/llm_step.rs:26`
//!     `AgentEvent::LlmCallStart { messages: Arc::new(state.messages().to_vec()), .. }`
//!     —— 深拷贝 #1（Vec<BaseMessage> 整体复制，base64 String 等非 Arc 内容逐字节复制）
//!  2. `cc-acp/src/session/executor.rs:319`
//!     `tracer.lock().on_llm_start(*step, messages, tools)`（messages 经 Deref 为 &[BaseMessage]）
//!  3. `cc-acp/src/langfuse/tracer.rs:348` `messages.to_vec()`
//!     —— 深拷贝 #2（存入 generation_data，直到 on_llm_end 才释放）
//!
//! 本测试用计数分配器真实分配 40MB（含 base64 图片）的消息历史，
//! 精确测量上述两步新增的常驻内存，确认是否为 ~2x 载荷。

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use langfuse_client::{BackpressurePolicy, Batcher, BatcherConfig, LangfuseClient};
use cc_acp::langfuse::{LangfuseSession, LangfuseTracer};
use cc_agent::messages::{BaseMessage, ContentBlock, ImageSource, MessageContent, MessageId};

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

// ─── 辅助 ──────────────────────────────────────────────────────────────────

fn make_tracer() -> LangfuseTracer {
    let client = LangfuseClient::new("pk-test", "sk-test", "http://127.0.0.1:1", 0);
    let config = BatcherConfig {
        max_events: 1000,
        flush_interval: std::time::Duration::from_secs(600),
        backpressure: BackpressurePolicy::DropNew,
        max_retries: 0,
    };
    let batcher = Arc::new(Batcher::new(client, config));
    let session = Arc::new(LangfuseSession {
        client: Arc::new(LangfuseClient::new("pk", "sk", "http://127.0.0.1:1", 0)),
        batcher,
    });
    LangfuseTracer::new(session, "stress-306".to_string())
}

/// 构造含 base64 图片的消息历史（模拟真实多模态会话）。
/// 每条消息携带 1MB base64 数据（`ImageSource::Base64.data: String` 非 Arc，真深拷贝）。
fn make_image_history(num_messages: usize, bytes_per_image: usize) -> Vec<BaseMessage> {
    (0..num_messages)
        .map(|_| BaseMessage::Human {
            id: MessageId::new(),
            content: MessageContent::blocks(vec![ContentBlock::Image {
                source: ImageSource::Base64 {
                    media_type: "image/png".to_string(),
                    // 精确 bytes_per_image 字节，确保断言阈值准确
                    data: "A".repeat(bytes_per_image),
                },
            }]),
        })
        .collect()
}

// ─── 压测 ──────────────────────────────────────────────────────────────────

#[tokio::test]
async fn stress_306_llm_call_copies_message_history_twice() {
    const NUM_MSGS: usize = 40;
    const BYTES_PER_IMAGE: usize = 1024 * 1024; // 1MB
    let payload_bytes = NUM_MSGS * BYTES_PER_IMAGE; // ~40MB

    // 构造消息历史（预热分配器，排除构造期干扰）
    let msgs = make_image_history(NUM_MSGS, BYTES_PER_IMAGE);
    assert!(msgs.len() == NUM_MSGS);

    let mut tracer = make_tracer();

    // ── 测量生产代码路径 ──
    let before = live_bytes();

    // llm_step.rs:26 — 深拷贝 #1
    let arc = Arc::new(msgs.to_vec());

    // executor.rs:319 -> tracer.rs:348 — 深拷贝 #2（on_llm_start 内部 to_vec）
    tracer.on_llm_start(0, &arc, &[]);

    let after = live_bytes();
    let copied = after.saturating_sub(before);

    // 清理（on_llm_end 移除 generation_data 并序列化）
    tracer.on_llm_end(0, "stress-model", "stress-provider", "ok", None);
    drop(arc);
    drop(msgs);

    // 断言（#306 修复后）：只有 llm_step.rs 的一次快照拷贝，tracer 共享 Arc 不再复制。
    // 期望 ≈ 1x 载荷。若回到两次深拷贝，copied ≈ 2x，断言失败。
    // 阈值取 1.3x 上限（含 Vec/Arc/HashMap 元数据噪声），下限 0.8x（确保拷贝真实发生）。
    let ratio = copied as f64 / payload_bytes as f64;
    println!(
        "payload={}MB copied={}MB (ratio {:.2}x)",
        payload_bytes / 1024 / 1024,
        copied / 1024 / 1024,
        ratio
    );
    assert!(
        ratio < 1.3,
        "#306 回归：tracer 又在深拷贝消息历史，ratio={ratio:.2}x（期望 <1.3x）",
    );
    assert!(
        ratio > 0.8,
        "压测异常：拷贝未发生（ratio={ratio:.2}x），测试本身可能失效",
    );
}
