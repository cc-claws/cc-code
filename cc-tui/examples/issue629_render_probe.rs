//! #629 收益探针：调用真实 RenderEvent → worker → RenderCache 管线。
//! 不访问外部服务、不写配置、不改生产行为；计时是本机 debug 构建口径。
use std::{
    borrow::Cow,
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    io::Write,
    sync::Arc,
    time::{Duration, Instant},
};

use cc_agent::messages::{BaseMessage, ContentBlock, MessageContent};
use cc_tui::ui::{
    message_view::MessageViewModel,
    render_thread::{spawn_render_thread, RenderCache, RenderEvent, ViewportAnchor},
};
use parking_lot::RwLock;
use serde_json::{json, Value};
use tokio::sync::{mpsc, Notify};

fn make_messages(count: usize) -> Vec<MessageViewModel> {
    (0..count)
        .map(|index| match index % 10 {
            0 => MessageViewModel::user(format!(
                "请检查模块 {index} 的行为，并保持中文、链接和折行一致。"
            )),
            1 | 2 => {
                let message = BaseMessage::ai(MessageContent::blocks(vec![
                    ContentBlock::Reasoning {
                        text: "分析输入、接口与边界，逐项核对实现。\n".repeat(30),
                        signature: None,
                        duration_ms: None,
                    },
                    ContentBlock::text(format!(
                        "## 模块 {index}\n\n{}\n\n- 校验输入\n- 保留已有交互\n",
                        "检查渲染缓存与事件发布，定位全量复制的成本。".repeat(20)
                    )),
                ]));
                MessageViewModel::from_base_message_with_cwd(&message, &[], None)
            }
            3..=6 => make_tool(index, "Bash", 400),
            7 | 8 => make_tool(index, "Read", 120),
            _ => MessageViewModel::system_with_detail(
                format!("任务 {index}"),
                "Prompt 正文示例，保留详细态分支。\n".repeat(12),
            ),
        })
        .collect()
}

fn make_tool(index: usize, name: &str, rows: usize) -> MessageViewModel {
    let mut message = MessageViewModel::tool_block_with_id(
        format!("probe-{index}"),
        name.to_owned(),
        name.to_owned(),
        Some(format!("module_{index}")),
        false,
    );
    if let MessageViewModel::ToolBlock { content, .. } = &mut message {
        *content = (0..rows)
            .map(|row| {
                format!(
                    "module_{index} output {row:04}: compiling foo::bar processing item done ok\n"
                )
            })
            .collect();
    }
    message.recompute_hash();
    message
}

fn summarize(cache: &RenderCache) -> Value {
    let mut bytes = cache.lines.first().map_or(0, |line| {
        cache.lines.capacity() * std::mem::size_of_val(line)
    });
    let mut fingerprint = DefaultHasher::new();
    for line in &cache.lines {
        if let Some(span) = line.spans.first() {
            bytes += line.spans.capacity() * std::mem::size_of_val(span);
        }
        for span in &line.spans {
            span.content.hash(&mut fingerprint);
            span.style.hash(&mut fingerprint);
            if let Cow::Owned(text) = &span.content {
                bytes += text.capacity();
            }
        }
        0xffff_u16.hash(&mut fingerprint);
    }
    bytes += cache.message_offsets.capacity() * std::mem::size_of::<usize>();
    if let Some(wrap) = cache.wrap_map.first() {
        bytes += cache.wrap_map.capacity() * std::mem::size_of_val(wrap);
    }
    for wrap in &cache.wrap_map {
        bytes += wrap.plain_text.capacity() + wrap.char_widths.capacity();
        wrap.plain_text.hash(&mut fingerprint);
        wrap.visual_row_start.hash(&mut fingerprint);
        wrap.visual_row_end.hash(&mut fingerprint);
    }
    if let Some(link) = cache.links.first() {
        bytes += cache.links.capacity() * std::mem::size_of_val(link);
    }
    json!({ "logical_lines": cache.lines.len(), "visual_rows": cache.total_lines, "wrap_entries": cache.wrap_map.len(), "message_offsets": cache.message_offsets.len(), "shared_cache_estimated_bytes": bytes, "virtual": cache.virtual_layout.as_ref().map(|v| json!({"resident_segments":v.diagnostics.resident_segments, "resident_bytes":v.diagnostics.resident_bytes, "budget_bytes":v.diagnostics.budget_bytes, "estimated_segments":v.diagnostics.estimated_segments, "metadata_bytes": v.message_visual_offsets.capacity()*std::mem::size_of::<usize>() + v.message_versions.capacity()*std::mem::size_of::<u64>() + v.line_origins.capacity()*std::mem::size_of::<(usize,usize)>(), "evictions":v.diagnostics.evictions})), "fingerprint": fingerprint.finish() })
}

async fn send_and_measure(
    tx: &mpsc::Sender<RenderEvent>,
    cache: &Arc<RwLock<RenderCache>>,
    notify: &Arc<Notify>,
    event: RenderEvent,
) -> (f64, Value) {
    let before = cache.read().version;
    let started = Instant::now();
    tx.send(event).await.expect("发送探针事件失败");
    loop {
        if cache.read().version > before {
            break;
        }
        tokio::time::timeout(Duration::from_secs(300), notify.notified())
            .await
            .expect("等待真实渲染发布超时");
    }
    let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
    (elapsed_ms, summarize(&cache.read()))
}

async fn measure_case(count: usize) -> Value {
    let prepare_started = Instant::now();
    let messages = make_messages(count);
    let prepare_ms = prepare_started.elapsed().as_secs_f64() * 1000.0;
    let clone_started = Instant::now();
    let initial = messages.clone();
    let snapshot_clone_ms = clone_started.elapsed().as_secs_f64() * 1000.0;
    let (tx, cache, notify) = spawn_render_thread(120);
    let strategy =
        std::env::var("PERI_RENDER_PROBE_STRATEGY").unwrap_or_else(|_| "cache".to_owned());
    tx.send(RenderEvent::SetDetailStrategy(strategy.clone()))
        .await
        .expect("设置策略失败");
    tx.send(RenderEvent::Viewport {
        top: 0,
        height: 40,
        follow_tail: true,
        anchor: None,
    })
    .await
    .expect("设置视口失败");
    let (folded_ms, folded) =
        send_and_measure(&tx, &cache, &notify, RenderEvent::Rebuild(initial)).await;
    let (cold_ms, cold) =
        send_and_measure(&tx, &cache, &notify, RenderEvent::ToggleDetail(true)).await;
    let (off_ms, off) =
        send_and_measure(&tx, &cache, &notify, RenderEvent::ToggleDetail(false)).await;
    let (hot_ms, hot) =
        send_and_measure(&tx, &cache, &notify, RenderEvent::ToggleDetail(true)).await;
    assert_eq!(
        folded["fingerprint"], off["fingerprint"],
        "关闭详细模式后内容、样式和换行摘要必须与初始折叠态一致"
    );
    assert_eq!(
        cold["fingerprint"], hot["fingerprint"],
        "热开后的内容、样式和换行摘要必须与冷开一致"
    );
    let mut streamed = messages.clone();
    streamed.push(MessageViewModel::user(
        "尾部追加一条消息，验证完整快照更新。".to_owned(),
    ));
    let (append_ms, appended) =
        send_and_measure(&tx, &cache, &notify, RenderEvent::Rebuild(streamed)).await;
    let scroll = if cache.read().virtual_layout.is_some() {
        Some(measure_windows(count, &tx, &cache, &notify).await)
    } else {
        None
    };
    let result = json!({ "strategy":strategy, "messages": count, "width": 120, "fixture_prepare_ms": prepare_ms, "app_snapshot_clone_ms": snapshot_clone_ms, "folded_publish_ms": folded_ms, "cold_detail_publish_ms": cold_ms, "off_publish_ms": off_ms, "hot_detail_publish_ms": hot_ms, "append_publish_ms": append_ms, "folded": folded, "detail": cold, "scroll":scroll, "appended_visual_rows": appended["visual_rows"], "contracts": "folded==off; cold==hot, including text/style/wrap; fingerprint is diagnostic, not per-line diff" });
    drop(tx);
    tokio::task::yield_now().await;
    result
}

async fn measure_windows(
    count: usize,
    tx: &mpsc::Sender<RenderEvent>,
    cache: &Arc<RwLock<RenderCache>>,
    notify: &Arc<Notify>,
) -> Value {
    let mut misses = Vec::new();
    let mut hits = Vec::new();
    let mut max_bytes = 0;
    for index in (3..count).step_by(10) {
        for (logical_line, samples) in [(0, &mut misses), (1, &mut hits)] {
            let top = cache
                .read()
                .virtual_layout
                .as_ref()
                .expect("应有虚拟元数据")
                .message_visual_offsets[index]
                + logical_line;
            let (elapsed, _) = send_and_measure(
                tx,
                cache,
                notify,
                RenderEvent::Viewport {
                    top,
                    height: 40,
                    follow_tail: false,
                    anchor: Some(ViewportAnchor {
                        message_index: index,
                        logical_line,
                        row_in_line: 0,
                    }),
                },
            )
            .await;
            samples.push(elapsed);
            let read = cache.read();
            let layout = read.virtual_layout.as_ref().expect("应有虚拟元数据");
            assert!(
                layout
                    .line_origins
                    .iter()
                    .any(|(source, _)| *source == index),
                "新窗口必须物化目标消息"
            );
            assert!(
                layout.diagnostics.resident_bytes <= layout.diagnostics.budget_bytes,
                "普通小段不得突破缓存预算"
            );
            max_bytes = max_bytes.max(layout.diagnostics.resident_bytes);
        }
    }
    fn percentiles(mut values: Vec<f64>) -> Value {
        if values.is_empty() {
            return Value::Null;
        }
        values.sort_by(f64::total_cmp);
        let n = values.len();
        json!({"samples":n,"p50_ms":values[(n-1)/2],"p95_ms":values[(n*95).div_ceil(100)-1],"max_ms":values[n-1]})
    }
    json!({"new_window":percentiles(misses),"resident_window":percentiles(hits),"max_resident_bytes":max_bytes,"final_cache":summarize(&cache.read())})
}

async fn measure_resize_control_sequence() -> Value {
    let (tx, cache, notify) = spawn_render_thread(120);
    let _ = send_and_measure(
        &tx,
        &cache,
        &notify,
        RenderEvent::Rebuild(make_messages(10)),
    )
    .await;
    let before_version = cache.read().version;
    // 单线程探针中两条 try_send 之间没有 await，确保控制事件相邻入队。
    tx.try_send(RenderEvent::Resize(100))
        .expect("发送 resize 失败");
    tx.try_send(RenderEvent::ToggleDetail(true))
        .expect("发送 detail 失败");
    loop {
        if cache.read().version > before_version {
            break;
        }
        tokio::time::timeout(Duration::from_secs(30), notify.notified())
            .await
            .expect("等待 resize 发布超时");
    }
    tokio::task::yield_now().await;
    let after_queued_version = cache.read().version;
    let after_queued_rows = cache.read().total_lines;
    let _ = send_and_measure(&tx, &cache, &notify, RenderEvent::ToggleDetail(true)).await;
    let explicit_detail_rows = cache.read().total_lines;
    let result = json!({ "before_version": before_version, "after_queued_version": after_queued_version, "queued_resize_then_detail_visual_rows": after_queued_rows, "explicit_detail_visual_rows": explicit_detail_rows, "queued_toggle_lost": after_queued_rows != explicit_detail_rows });
    drop(tx);
    tokio::task::yield_now().await;
    result
}

fn main() {
    let counts: Vec<usize> = std::env::args()
        .skip(1)
        .map(|argument| argument.parse().expect("消息数必须为整数"))
        .collect();
    let counts = if counts.is_empty() {
        vec![100, 500, 1000]
    } else {
        counts
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("创建探针运行时失败");
    let (results, resize_sequence) = runtime.block_on(async {
        let mut results = Vec::new();
        for count in counts {
            results.push(measure_case(count).await);
        }
        (results, measure_resize_control_sequence().await)
    });
    let report = json!({ "baseline_commit": "60e3f795dbb8072677915bda5d96c2c9c0d54fee", "implementation": "working-tree #629", "release_tag": "npm-v0.6.168", "build": "debug; Windows; single-thread probe runtime", "scope": "real rendering worker; synthetic fixture; one timing sample per toggle, scroll distributions per case. Virtual global rows include offscreen estimates; visible windows use exact rendering. Bytes estimate shared window and resident segments separately, excluding source VM, allocator and RSS.", "results": results, "resize_control_sequence": resize_sequence });
    std::io::stdout()
        .write_all(
            serde_json::to_string_pretty(&report)
                .expect("序列化探针结果失败")
                .as_bytes(),
        )
        .expect("输出探针结果失败");
}
