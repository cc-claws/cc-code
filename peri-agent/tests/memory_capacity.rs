//! 容量实测：500MB 能支持多少条消息（真实 mock，非短消息）
//!
//! Mock 数据来源（全部取自本仓库真实内容）：
//!  - Tool 结果：真实 .rs 文件内容（平均 ~15KB，模拟 Read/Grep 输出）
//!  - User 消息：真实风格的编码任务 prompt（中英混合）
//!  - Assistant 消息：带 tool_calls 的 reasoning 文本
//!  - Image：偶发 base64 截图（模拟多模态会话，~200KB）
//!
//! 测量方式：计数分配器，真实分配到 500MB，数消息条数。

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

use peri_agent::agent::{AgentState, State};
use peri_agent::messages::{
    BaseMessage, ContentBlock, ImageSource, MessageContent, MessageId, ToolCallRequest,
};

// ─── 计数分配器 ────────────────────────────────────────────────────────────

struct CountingAlloc;
static LIVE_BYTES: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = System.alloc(layout);
        if !ptr.is_null() {
            LIVE_BYTES.fetch_add(layout.size(), Ordering::Relaxed);
        }
        ptr
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        System.dealloc(ptr, layout);
        LIVE_BYTES.fetch_sub(layout.size(), Ordering::Relaxed);
    }
}

#[global_allocator]
static GLOBAL: CountingAlloc = CountingAlloc;

fn live_bytes() -> usize {
    LIVE_BYTES.load(Ordering::Relaxed)
}

// ─── 真实 Mock 数据 ────────────────────────────────────────────────────────

/// 从仓库读取真实 .rs 文件内容，作为 Tool 结果 mock
fn load_real_file_contents() -> Vec<String> {
    // 测试工作目录是 peri-agent/，用 CARGO_MANIFEST_DIR 定位到仓库根
    let workspace_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root");
    let mut files = Vec::new();
    // 采样不同大小的真实文件
    let candidates = [
        "peri-agent/src/agent/state.rs",
        "peri-acp/src/langfuse/tracer.rs",
        "peri-middlewares/src/compact_middleware.rs",
        "peri-agent/src/messages/content.rs",
        "peri-acp/src/session/executor.rs",
    ];
    for c in candidates {
        if let Ok(content) = std::fs::read_to_string(workspace_root.join(c)) {
            files.push(content);
        }
    }
    assert!(!files.is_empty(), "无法读取仓库文件");
    files
}

const USER_PROMPTS: &[&str] = &[
    "帮我看一下这个函数的实现有没有内存泄漏，重点检查 Arc 循环引用",
    "这个 clippy 警告怎么修？type_complexity，说我的 HashMap 类型太复杂了",
    "给 compact_middleware 加一个降级策略，失败的时候不要直接恢复全部消息",
    "Review this PR: does the Windows DACL code handle the case where GetNamedSecurityInfoW fails?",
    "把 tracer 的 on_llm_start 改成收 Arc，不要再 to_vec 深拷贝了",
    "这个测试在 Windows CI 上挂了，帮忙看一下是不是时序问题",
];

const ASSISTANT_TEXTS: &[&str] = &[
    "我看了一下，这里的 `to_vec()` 确实做了深拷贝。`ImageSource::Base64` 的 data 是 plain String，不是 Arc，所以每次调用都会复制全部字节。建议改成共享 Arc。",
    "修好了。主要改动：`generation_data` 的类型从 `Vec<BaseMessage>` 改成 `Arc<Vec<BaseMessage>>`，`on_llm_start` 里用 `Arc::clone`。压测从 2.00x 降到 1.00x。",
    "The root cause is that `TempDir` on Windows holds a file lock. The watchdog task keeps getting PermissionDenied when trying to read metadata. We should handle this gracefully instead of looping forever.",
];

/// 生成一条逼真的消息（轮询：user → assistant+tool_call → tool_result → assistant）
fn mock_message(
    seq: usize,
    file_contents: &[String],
    image_data: &str,
) -> BaseMessage {
    match seq % 4 {
        0 => {
            // User：真实风格 prompt
            let prompt = USER_PROMPTS[seq % USER_PROMPTS.len()];
            BaseMessage::Human {
                id: MessageId::new(),
                content: MessageContent::text(format!("{prompt}\n\n补充说明：这是第 {seq} 轮对话。")),
            }
        }
        1 => {
            // Assistant：reasoning 文本 + tool_call
            let text = ASSISTANT_TEXTS[seq % ASSISTANT_TEXTS.len()];
            BaseMessage::Ai {
                id: MessageId::new(),
                content: MessageContent::text(text.to_string()),
                tool_calls: vec![ToolCallRequest {
                    id: format!("tc-{seq}"),
                    name: "Read".to_string(),
                    arguments: serde_json::json!({"file_path": "peri-agent/src/agent/state.rs"}),
                }],
            }
        }
        2 => {
            // Tool：真实文件内容（模拟 Read 结果）
            let content = &file_contents[seq % file_contents.len()];
            // 每 20 条混入一张图片（多模态）
            let blocks = if seq.is_multiple_of(20) {
                vec![
                    ContentBlock::Text { text: content[..content.len().min(2000)].into() },
                    ContentBlock::Image {
                        source: ImageSource::Base64 {
                            media_type: "image/png".to_string(),
                            data: image_data.to_string(),
                        },
                    },
                ]
            } else {
                vec![ContentBlock::Text { text: content.as_str().into() }]
            };
            BaseMessage::Tool {
                id: MessageId::new(),
                tool_call_id: format!("tc-{}", seq - 1),
                content: MessageContent::blocks(blocks),
                is_error: false,
            }
        }
        _ => {
            // Assistant：纯文本回复
            let text = ASSISTANT_TEXTS[seq % ASSISTANT_TEXTS.len()];
            BaseMessage::Ai {
                id: MessageId::new(),
                content: MessageContent::text(format!("{text}\n\n（第 {seq} 轮）")),
                tool_calls: vec![],
            }
        }
    }
}

// ─── 容量测试 ──────────────────────────────────────────────────────────────

#[test]
fn capacity_500mb_how_many_messages() {
    const BUDGET: usize = 500 * 1024 * 1024; // 500MB

    let file_contents = load_real_file_contents();
    let avg_file_kb: usize =
        file_contents.iter().map(|s| s.len()).sum::<usize>() / file_contents.len() / 1024;
    println!("mock 文件平均大小: {avg_file_kb}KB, 采样 {n} 个", n = file_contents.len());

    // 200KB base64 图片（模拟截图）
    let image_data = "B".repeat(200 * 1024);

    let mut state = AgentState::new("/tmp/capacity-test");
    let baseline = live_bytes();

    let mut seq = 0usize;
    loop {
        let msg = mock_message(seq, &file_contents, &image_data);
        // 直接 push 绕开 MAX_MESSAGES 硬上限，测内存的真实容量
        // （带 cap 的实际值见下方注记）
        state.messages_mut().push(msg);
        seq += 1;

        // 每 1000 条检查一次
        if seq.is_multiple_of(1000) {
            let used = live_bytes().saturating_sub(baseline);
            if used >= BUDGET {
                break;
            }
        }
        // 安全上限
        if seq >= 500_000 {
            break;
        }
    }

    let used = live_bytes().saturating_sub(baseline);
    let count = state.messages().len();
    let avg_bytes = used / count.max(1);

    println!("═══════════════════════════════════════");
    println!("500MB 容量实测结果（真实 mock，无 cap）：");
    println!("  消息条数: {count}");
    println!("  实际占用: {}MB", used / 1024 / 1024);
    println!("  平均每条: {}KB", avg_bytes / 1024);
    println!("  构成: user prompt / assistant+tool_call / tool(真实文件~19KB) / assistant 文本，20:1 混入 200KB 图片");
    println!("───────────────────────────────────────");
    println!("注：生产环境受 MAX_MESSAGES={} 硬上限约束，", peri_agent::agent::state::MAX_MESSAGES);
    println!("    按此消息分布约占用 {}MB 即触发上限", 
        peri_agent::agent::state::MAX_MESSAGES * avg_bytes / 1024 / 1024);
    println!("═══════════════════════════════════════");

    // 容量断言：500MB 至少应支持 20000 条真实消息
    assert!(
        count >= 20000,
        "容量不足：500MB 仅支持 {count} 条消息"
    );

    drop(state);
}
