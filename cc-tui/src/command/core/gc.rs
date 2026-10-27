use cc_agent::messages::{BaseMessage, ContentBlock, MessageContent};
use cc_widgets::diff::{diff_cache_stats, render_cache_stats};

use crate::{app::App, command::Command};

pub struct GcCommand;

/// `/gc` 的 size class 表最多列多少行，其余合并成一行合计（避免 232 个 class 淹没有效信息）。
const SIZE_CLASS_TOP_N: usize = 12;

impl Command for GcCommand {
    fn name(&self) -> &str {
        "gc"
    }

    fn description(&self, lc: &crate::i18n::LcRegistry) -> String {
        lc.tr("cmd-gc-desc")
    }

    fn aliases(&self) -> Vec<&str> {
        vec![]
    }

    fn execute(&self, app: &mut App, _args: &str) {
        let lc = &app.services.lc;
        let stats_before = crate::alloc_config::query_stats();
        let alloc_name = crate::alloc_config::allocator_name();

        // ── 诊断：各数据结构大小 ──
        let active = app.active();
        let origin_count = active.agent.origin_messages.len();
        let origin_bytes = estimate_messages_heap(&active.agent.origin_messages);
        let (completed_count, completed_bytes) = active.messages.pipeline.completed_stats();
        let vm_count = active.messages.view_messages.len();
        let vm_bytes = estimate_view_messages_heap(&active.messages.view_messages);

        // ── Markdown 缓存诊断：单次加锁快照，不克隆缓存内容 ──
        let md_cache = cc_widgets::markdown::cache::MarkdownCache::global().stats();

        // ── 渲染线程缓存 / diff 缓存（此前未纳入统计，是「未识别」的主要盲区）──
        let (render_cache_bytes, render_lines, wrap_entries, virtual_stats) = {
            let rc = active.messages.render_cache.read();
            (
                estimate_render_cache_heap(&rc),
                rc.lines.len(),
                rc.wrap_map.len(),
                rc.virtual_layout
                    .as_ref()
                    .map(|layout| layout.diagnostics.clone()),
            )
        };
        let diff_stats = diff_cache_stats();
        let render_stats = render_cache_stats();

        let mut lines = Vec::new();

        crate::alloc_config::alloc_collect();

        let stats_after = crate::alloc_config::query_stats();
        // breakdown 由下方两处共用（allocator 明细段 + 已知/未识别估算段），只取一次快照，
        // 保证两段引用的是同一组读数。
        let breakdown = crate::alloc_config::query_breakdown();

        // ── RSS 汇总 ──
        match (stats_before, stats_after) {
            (Some(before), Some(after)) => {
                // 方向语义：`after - before`，增加为正。此前误用 `before - after`
                // 且 `delta >= 0` 才加 `+`，导致内存**下降**时反而显示 `+N`（方向颠倒）。
                let delta = after.current_rss as isize - before.current_rss as isize;
                lines.push(lc.tr_args(
                    "cmd-gc-rss",
                    &[
                        ("before".into(), fmt_bytes(before.current_rss).into()),
                        ("after".into(), fmt_bytes(after.current_rss).into()),
                        ("delta".into(), fmt_signed_delta(delta, fmt_bytes).into()),
                    ],
                ));
                let alloc_delta = after.current_allocated as isize - after.current_rss as isize;
                // allocated == 0 表示分配器读数不可用（Windows stats 读不到或计数失真），
                // 不是「分配器空闲」：不输出 0 B 及其派生差值（#758）。
                if alloc_delta != 0 && after.current_allocated > 0 {
                    // alloc_delta = allocated - RSS（有向差，带符号输出）：
                    // - 为正（allocated 更大）：多为两者采样时刻/记账口径差（先读 RSS 后读
                    //   allocated，其间 /gc 自身仍在分配），不解读为「超出物理内存」。
                    // - 为负（RSS 更大）：可能包含分配器空闲驻留页、页舍入，
                    //   以及栈、映射文件等；仅凭此差值无法归因。
                    let hint = if alloc_delta > 0 {
                        lc.tr("cmd-gc-allocated-rss-hint-positive")
                    } else {
                        lc.tr("cmd-gc-allocated-rss-hint-negative")
                    };
                    lines.push(lc.tr_args(
                        "cmd-gc-allocated-rss",
                        &[
                            (
                                "allocated".into(),
                                fmt_bytes(after.current_allocated).into(),
                            ),
                            (
                                "delta".into(),
                                fmt_signed_delta(alloc_delta, fmt_bytes).into(),
                            ),
                            ("allocator".into(), alloc_name.into()),
                            ("hint".into(), hint.into()),
                        ],
                    ));
                }
            }
            _ => lines.push(lc.tr("cmd-gc-rss-unavailable")),
        }

        // 说明：不再单列 `OS RSS: A → B`。它与上面的 `RSS: A → B` 是同一个读数
        // （都来自 sysinfo `process.memory()`），只是 fmt_bytes / 整数 MB 两种格式，
        // 且整数截断后再相减会凭空丢掉 <1 MB 的差额。

        // ── 数据结构诊断 ──
        lines.push(String::new());
        lines.push(lc.tr("cmd-gc-structures-heading"));
        lines.push(lc.tr_args(
            "cmd-gc-origin-messages",
            &[
                ("count".into(), origin_count.to_string().into()),
                ("bytes".into(), fmt_bytes(origin_bytes).into()),
            ],
        ));
        lines.push(lc.tr_args(
            "cmd-gc-completed-messages",
            &[
                ("count".into(), completed_count.to_string().into()),
                ("bytes".into(), fmt_bytes(completed_bytes).into()),
            ],
        ));
        lines.push(lc.tr_args(
            "cmd-gc-view-messages",
            &[
                ("count".into(), vm_count.to_string().into()),
                ("bytes".into(), fmt_bytes(vm_bytes).into()),
            ],
        ));

        // 只比较条数，不推断内容相同或是否泄漏。
        if origin_count > 0 && completed_count > 0 {
            let overlap = if origin_count == completed_count {
                lc.tr("cmd-gc-message-counts-equal")
            } else {
                lc.tr("cmd-gc-message-counts-different")
            };
            lines.push(lc.tr_args(
                "cmd-gc-message-comparison",
                &[
                    ("overlap".into(), overlap.into()),
                    ("origin".into(), origin_count.to_string().into()),
                    ("completed".into(), completed_count.to_string().into()),
                ],
            ));
        }

        // ── 渲染缓存诊断 ──
        lines.push(String::new());
        lines.push(lc.tr("cmd-gc-caches-heading"));
        lines.push(lc.tr_args(
            "cmd-gc-markdown-cache",
            &[
                ("entries".into(), md_cache.entries.to_string().into()),
                ("capacity".into(), md_cache.capacity.to_string().into()),
                (
                    "bytes".into(),
                    fmt_bytes(md_cache.estimated_heap_bytes).into(),
                ),
                ("budget".into(), fmt_bytes(md_cache.budget).into()),
            ],
        ));
        lines.push(
            lc.tr_args(
                "cmd-gc-markdown-cache-details",
                &[
                    (
                        "average".into(),
                        fmt_bytes(
                            md_cache
                                .estimated_heap_bytes
                                .checked_div(md_cache.entries)
                                .unwrap_or(0),
                        )
                        .into(),
                    ),
                    (
                        "largest".into(),
                        fmt_bytes(md_cache.largest_entry_heap_bytes).into(),
                    ),
                    ("lines".into(), md_cache.rendered_lines.to_string().into()),
                    ("spans".into(), md_cache.rendered_spans.to_string().into()),
                ],
            ),
        );
        lines.push(lc.tr("cmd-gc-cache-estimate-note"));
        lines.push(lc.tr_args(
            "cmd-gc-render-cache",
            &[
                ("lines".into(), render_lines.to_string().into()),
                ("wraps".into(), wrap_entries.to_string().into()),
                ("bytes".into(), fmt_bytes(render_cache_bytes).into()),
            ],
        ));
        if let Some(stats) = virtual_stats {
            lines.push(lc.tr_args(
                "cmd-gc-virtual-render",
                &[
                    (
                        "segments".into(),
                        stats.resident_segments.to_string().into(),
                    ),
                    ("bytes".into(), fmt_bytes(stats.resident_bytes).into()),
                    ("budget".into(), fmt_bytes(stats.budget_bytes).into()),
                    (
                        "estimated".into(),
                        stats.estimated_segments.to_string().into(),
                    ),
                    ("evictions".into(), stats.evictions.to_string().into()),
                    (
                        "largest".into(),
                        fmt_bytes(stats.largest_segment_bytes).into(),
                    ),
                ],
            ));
        }
        lines.push(lc.tr_args(
            "cmd-gc-diff-cache",
            &[
                ("entries".into(), diff_stats.entries.to_string().into()),
                ("capacity".into(), diff_stats.capacity.to_string().into()),
                ("bytes".into(), fmt_bytes(diff_stats.bytes).into()),
                ("budget".into(), fmt_bytes(diff_stats.budget).into()),
            ],
        ));
        lines.push(lc.tr_args(
            "cmd-gc-diff-render-cache",
            &[
                ("entries".into(), render_stats.entries.to_string().into()),
                ("capacity".into(), render_stats.capacity.to_string().into()),
                ("bytes".into(), fmt_bytes(render_stats.bytes).into()),
                ("budget".into(), fmt_bytes(render_stats.budget).into()),
            ],
        ));
        tracing::info!(
            entries = md_cache.entries,
            capacity = md_cache.capacity,
            budget = md_cache.budget,
            estimated_heap_bytes = md_cache.estimated_heap_bytes,
            largest_entry_heap_bytes = md_cache.largest_entry_heap_bytes,
            rendered_lines = md_cache.rendered_lines,
            rendered_spans = md_cache.rendered_spans,
            "markdown cache memory snapshot",
        );

        // ── 分配器 breakdown（关键：allocated vs active vs resident）──
        if let Some(bd) = breakdown {
            // active/mapped/retained 的语义**分平台**：
            // - jemalloc（macOS/Linux）：active = 真实活跃页；mapped/retained = 虚拟地址配额
            // - mimalloc（Windows）：active = page_committed（别名 "touched"，历史触及高水位，
            //   mi_collect 后不减）；mapped/retained = reserved 虚拟地址保留量
            // 因此 Windows 上 "active" 与由其派生的"碎片"均**不可用于判断当前占用**。
            let is_mimalloc = alloc_name == "mimalloc";
            lines.push(String::new());
            lines.push(lc.tr_args(
                "cmd-gc-allocator-heading",
                &[("allocator".into(), alloc_name.into())],
            ));
            lines.push(lc.tr_args(
                "cmd-gc-allocated",
                &[("bytes".into(), fmt_bytes(bd.allocated).into())],
            ));
            if is_mimalloc {
                lines.push(lc.tr_args(
                    "cmd-gc-active-mimalloc",
                    &[("bytes".into(), fmt_bytes(bd.active).into())],
                ));
            } else {
                lines.push(lc.tr_args(
                    "cmd-gc-active",
                    &[("bytes".into(), fmt_bytes(bd.active).into())],
                ));
            }
            if is_mimalloc {
                lines.push(lc.tr_args(
                    "cmd-gc-resident-mimalloc",
                    &[("bytes".into(), fmt_bytes(bd.resident).into())],
                ));
                lines.push(lc.tr("cmd-gc-metadata-mimalloc"));
            } else {
                lines.push(lc.tr_args(
                    "cmd-gc-resident",
                    &[("bytes".into(), fmt_bytes(bd.resident).into())],
                ));
                lines.push(lc.tr_args(
                    "cmd-gc-metadata",
                    &[("bytes".into(), fmt_bytes(bd.metadata).into())],
                ));
            }
            if is_mimalloc {
                lines.push(lc.tr_args(
                    "cmd-gc-mapped-mimalloc",
                    &[("bytes".into(), fmt_bytes(bd.mapped).into())],
                ));
                lines.push(lc.tr_args(
                    "cmd-gc-retained-mimalloc",
                    &[("bytes".into(), fmt_bytes(bd.retained).into())],
                ));
            } else {
                lines.push(lc.tr_args(
                    "cmd-gc-mapped",
                    &[("bytes".into(), fmt_bytes(bd.mapped).into())],
                ));
                if crate::alloc_config::retained_metric_applicable() {
                    lines.push(lc.tr_args(
                        "cmd-gc-retained",
                        &[("bytes".into(), fmt_bytes(bd.retained).into())],
                    ));
                } else {
                    // opt.retain=false 时该 mallctl 恒为 0，渲染成读数就是恒零噪音。
                    lines.push(lc.tr("cmd-gc-retained-na"));
                }
            }
            // 关键指标：碎片仅在 active 有意义时（jemalloc）成立
            let frag = bd.active.saturating_sub(bd.allocated);
            if is_mimalloc {
                lines.push(lc.tr_args(
                    "cmd-gc-fragmentation-mimalloc",
                    &[("fragmentation".into(), fmt_bytes(frag).into())],
                ));
            } else {
                let waste = bd.resident.saturating_sub(bd.active);
                lines.push(lc.tr_args(
                    "cmd-gc-fragmentation",
                    &[
                        ("fragmentation".into(), fmt_bytes(frag).into()),
                        ("waste".into(), fmt_bytes(waste).into()),
                    ],
                ));
            }
            // Windows resident 本身就是全进程 RSS，不能相减推断非分配器占用。
            if !is_mimalloc {
                if let Some(ref s) = stats_after {
                    // 有符号差：resident > RSS 是**正常**的（resident 含内核 RSS 已不再
                    // 计入的 madvise 页），此时真差是负值。此前用 `saturating_sub`
                    // 钳到 0，渲染成 `= 0 B`，等式算术不成立且掩盖了真实差额。
                    let os_gap = signed_gap(s.current_rss, bd.resident);
                    lines.push(lc.tr_args(
                        "cmd-gc-os-gap",
                        &[
                            ("rss".into(), fmt_bytes(s.current_rss).into()),
                            ("resident".into(), fmt_bytes(bd.resident).into()),
                            ("gap".into(), fmt_signed_delta(os_gap, fmt_bytes).into()),
                            ("allocator".into(), alloc_name.into()),
                        ],
                    ));
                    if os_gap < 0 {
                        lines.push(lc.tr("cmd-gc-os-gap-inverted"));
                    }
                }
            }
        }

        // ── size class 分布：把 allocated 按分配尺寸精确分解 ──
        // jemalloc 的 stats.allocated 本身就是各 arena `Σ curregs×reg_size +
        // Σ curlextents×size`，因此这张表能把「未识别」的字节按尺寸定位。
        if let Some(classes) = crate::alloc_config::query_size_classes() {
            let allocated = breakdown.map_or(0, |b| b.allocated);
            let covered: usize = classes.iter().map(|c| c.bytes()).sum();
            // 各项占比统一以 allocated 为分母（表内合计在正常情况等于 allocated）；
            // allocated 读不到时退化为「占表内合计」，不显示 0%。
            let denom = if allocated > 0 { allocated } else { covered };
            lines.push(String::new());
            lines.push(lc.tr("cmd-gc-size-classes-heading"));
            for c in classes.iter().take(SIZE_CLASS_TOP_N) {
                lines.push(lc.tr_args(
                    "cmd-gc-size-class-row",
                    &[
                        ("size".into(), fmt_bytes_exact(c.size).into()),
                        ("objects".into(), fmt_grouped(c.live_objects).into()),
                        ("bytes".into(), fmt_bytes(c.bytes()).into()),
                        ("share".into(), fmt_share(c.bytes(), denom).into()),
                    ],
                ));
            }
            let rest = &classes[SIZE_CLASS_TOP_N.min(classes.len())..];
            if !rest.is_empty() {
                let rest_bytes: usize = rest.iter().map(|c| c.bytes()).sum();
                lines.push(lc.tr_args(
                    "cmd-gc-size-class-rest",
                    &[
                        ("classes".into(), rest.len().to_string().into()),
                        ("bytes".into(), fmt_bytes(rest_bytes).into()),
                        ("share".into(), fmt_share(rest_bytes, denom).into()),
                    ],
                ));
            }
            // 覆盖率校验：逐 class 求和应精确等于 stats.allocated（同一份记账）。
            // 不等时用有符号差如实显示，不做钳位（钳成 0 会掩盖口径问题）。
            lines.push(lc.tr_args(
                "cmd-gc-size-class-coverage",
                &[
                    ("covered".into(), fmt_bytes(covered).into()),
                    ("allocated".into(), fmt_bytes(allocated).into()),
                    ("share".into(), fmt_share(covered, allocated).into()),
                    (
                        "gap".into(),
                        fmt_signed_delta(signed_gap(covered, allocated), fmt_bytes).into(),
                    ),
                ],
            ));
            lines.push(lc.tr("cmd-gc-size-class-note"));
        }

        // ── 分配器全量 stats → tracing ──
        {
            lines.push(String::new());
            lines.push(lc.tr_args(
                "cmd-gc-full-stats-heading",
                &[("allocator".into(), alloc_name.into())],
            ));
            tracing::info!("=== /gc {alloc_name} full stats dump ===");
            crate::alloc_config::dump_stats();
            tracing::info!("=== /gc {alloc_name} full stats end ===");
        }

        // ── 已知 vs 未识别 ──
        if let Some(bd) = breakdown {
            // 消息/VM/Markdown/渲染线程缓存/diff 缓存各自持有独立副本，可分别计入估算。
            let known_bytes = origin_bytes
                + completed_bytes
                + vm_bytes
                + md_cache.estimated_heap_bytes
                + render_cache_bytes
                + diff_stats.bytes
                + render_stats.bytes;
            // 有符号差，不用 saturating_sub：估算项之间可能重叠（同一份渲染结果既被
            // VM 持有又被 markdown 缓存克隆），且按 capacity 估算含冗余，因此已知合计
            // 超过 allocated 是**可能**的；钳成 0 会把「估算溢出」伪装成「完全识别」。
            let gap = signed_gap(bd.allocated, known_bytes);
            lines.push(lc.tr_args(
                "cmd-gc-known-estimate",
                &[
                    ("known".into(), fmt_bytes(known_bytes).into()),
                    (
                        "messages".into(),
                        fmt_bytes(origin_bytes + completed_bytes).into(),
                    ),
                    ("views".into(), fmt_bytes(vm_bytes).into()),
                    (
                        "markdown".into(),
                        fmt_bytes(md_cache.estimated_heap_bytes).into(),
                    ),
                    ("render".into(), fmt_bytes(render_cache_bytes).into()),
                    (
                        "diff".into(),
                        fmt_bytes(diff_stats.bytes + render_stats.bytes).into(),
                    ),
                    ("gap".into(), fmt_signed_delta(gap, fmt_bytes).into()),
                ],
            ));
            if gap < 0 {
                lines.push(lc.tr("cmd-gc-known-estimate-overflow"));
            }
            lines.push(String::new());
            lines.push(lc.tr("cmd-gc-estimate-note"));
        }

        if alloc_name == "mimalloc" {
            lines.push(lc.tr("cmd-gc-collection-scope"));
        }

        app.push_system_note(lines.join("\n"));
        app.render_rebuild();
    }
}

// ── 内存估算 ──────────────────────────────────────────────────────────────────

/// 估算 BaseMessage slice 的堆内存占用（字节）
pub fn estimate_messages_heap(msgs: &[BaseMessage]) -> usize {
    let mut total = 0usize;
    for msg in msgs {
        total += estimate_message_content_heap(msg.message_content());
        // tool_calls
        if let BaseMessage::Ai { tool_calls, .. } = msg {
            for tc in tool_calls {
                total += tc.id.len() + tc.name.len() + estimate_json_heap(&tc.arguments);
            }
        }
        // tool_call_id
        if let BaseMessage::Tool { tool_call_id, .. } = msg {
            total += tool_call_id.len();
        }
        total += std::mem::size_of::<BaseMessage>(); // enum 本身
    }
    total
}

fn estimate_message_content_heap(mc: &MessageContent) -> usize {
    match mc {
        MessageContent::Text(s) => s.len(),
        MessageContent::Blocks(blocks) => {
            let mut size = blocks.capacity() * std::mem::size_of::<ContentBlock>();
            for b in blocks {
                size += estimate_content_block_heap(b);
            }
            size
        }
        MessageContent::Raw(vals) => {
            vals.capacity() * std::mem::size_of::<serde_json::Value>()
                + vals.iter().map(estimate_json_heap).sum::<usize>()
        }
    }
}

fn estimate_content_block_heap(b: &ContentBlock) -> usize {
    match b {
        ContentBlock::Text { text } => text.len(),
        ContentBlock::Image { source } => match source {
            cc_agent::messages::ImageSource::Base64 { media_type, data } => {
                media_type.len() + data.len()
            }
            cc_agent::messages::ImageSource::Url { url } => url.len(),
        },
        ContentBlock::Document { source, title } => {
            let src = match source {
                cc_agent::messages::DocumentSource::Base64 { media_type, data } => {
                    media_type.len() + data.len()
                }
                cc_agent::messages::DocumentSource::Url { url } => url.len(),
                cc_agent::messages::DocumentSource::Text { text } => text.len(),
            };
            src + title.as_ref().map_or(0, |t| t.len())
        }
        ContentBlock::ToolUse { id, name, input } => {
            id.len() + name.len() + estimate_json_heap(input)
        }
        ContentBlock::ToolResult {
            content,
            tool_use_id,
            ..
        } => {
            tool_use_id.len()
                + content.capacity() * std::mem::size_of::<ContentBlock>()
                + content
                    .iter()
                    .map(estimate_content_block_heap)
                    .sum::<usize>()
        }
        ContentBlock::Reasoning {
            text, signature, ..
        } => text.len() + signature.as_ref().map_or(0, |s| s.len()),
        ContentBlock::Unknown(v) => estimate_json_heap(v),
    }
}

fn estimate_json_heap(v: &serde_json::Value) -> usize {
    match v {
        serde_json::Value::String(s) => s.len(),
        serde_json::Value::Object(map) => map
            .iter()
            .map(|(k, v)| k.len() + estimate_json_heap(v))
            .sum(),
        serde_json::Value::Array(arr) => {
            arr.capacity() * std::mem::size_of::<serde_json::Value>()
                + arr.iter().map(estimate_json_heap).sum::<usize>()
        }
        _ => 0,
    }
}

/// 估算渲染线程 `RenderCache` 的近似堆占用（字节）。
///
/// 覆盖整段对话渲染行（`lines`）、逐行换行映射（`wrap_map`，含**纯文本副本**与
/// 字符宽度序列）、消息偏移与链接命中区。此前 `/gc` 完全未统计该结构，
/// 是「未识别」内存的主要盲区之一。
fn estimate_render_cache_heap(rc: &crate::ui::render_thread::RenderCache) -> usize {
    use std::borrow::Cow;
    let mut total = rc.lines.capacity() * std::mem::size_of::<ratatui::text::Line<'static>>();
    for line in &rc.lines {
        total += line.spans.capacity() * std::mem::size_of::<ratatui::text::Span<'static>>();
        for span in &line.spans {
            if let Cow::Owned(s) = &span.content {
                total += s.capacity();
            }
        }
    }
    total += rc.message_offsets.capacity() * std::mem::size_of::<usize>();
    total +=
        rc.wrap_map.capacity() * std::mem::size_of::<crate::ui::render_thread::WrappedLineInfo>();
    for w in &rc.wrap_map {
        total += w.plain_text.capacity() + w.char_widths.capacity();
    }
    total += rc.links.capacity() * std::mem::size_of::<cc_widgets::markdown::LinkHit>();
    for link in &rc.links {
        total += link.url.capacity();
    }
    if let Some(layout) = &rc.virtual_layout {
        total += std::mem::size_of_val(layout.as_ref());
        total += layout.message_visual_offsets.capacity() * std::mem::size_of::<usize>();
        total += std::mem::size_of::<Vec<usize>>();
        total += layout.line_origins.capacity() * std::mem::size_of::<(usize, usize)>();
        total += layout.message_versions.capacity() * std::mem::size_of::<u64>();
        total += std::mem::size_of::<Vec<u64>>();
    }
    total
}

/// 估算渲染视图模型 `view_messages` 的堆内存占用（字节）。
///
/// 覆盖 `MessageViewModel` 各变体，重点包含内嵌的 `Text<'static>`（markdown 渲染结果）、
/// 工具输出字符串、diff 输入、SubAgent 滑窗子 VM 等。
/// 此前 `estimate_messages_heap` 未覆盖这些视图数据；纳入后仍不包含所有堆分配，
/// 不能将 `allocated` 与已知估算的差值直接解释为某一项或泄漏。
pub fn estimate_view_messages_heap(vms: &[crate::ui::message_view::MessageViewModel]) -> usize {
    // 入参为切片，拿不到 Vec 的 capacity()，用 len() 估算（略低估 Vec 冗余，诊断可接受）。
    // 每个 VM 的内联枚举尺寸由 estimate_vm_heap 计入，此处不再重复累加。
    vms.iter().map(estimate_vm_heap).sum::<usize>()
}

fn estimate_vm_heap(vm: &crate::ui::message_view::MessageViewModel) -> usize {
    use crate::ui::message_view::MessageViewModel as Vm;
    let base = std::mem::size_of::<Vm>();
    match vm {
        Vm::UserBubble {
            content,
            rendered,
            rendered_links,
            expanded_content,
            ..
        } => {
            base + content.capacity()
                + estimate_text_heap(rendered)
                + estimate_links_heap(rendered_links)
                + expanded_content.as_ref().map_or(0, |s| s.capacity())
        }
        Vm::AssistantBubble { blocks, .. } => {
            // estimate_block_heap 已含每个 block 的内联尺寸，此处不重复加 capacity*sizeof
            base + blocks.iter().map(estimate_block_heap).sum::<usize>()
        }
        Vm::ToolBlock {
            display_name,
            args_display,
            content,
            diff_input,
            tool_call_id,
            tool_name,
            ..
        } => {
            base + tool_name.capacity()
                + tool_call_id.capacity()
                + display_name.capacity()
                + args_display.as_ref().map_or(0, |s| s.capacity())
                + content.capacity()
                + diff_input.as_ref().map_or(0, |d| {
                    d.file_path.capacity()
                        + d.old_content.capacity()
                        + d.new_content.capacity()
                        + std::mem::size_of::<cc_widgets::DiffInput>()
                })
        }
        Vm::ShellCommand {
            id,
            command,
            cwd,
            stdin,
            stdout,
            stderr,
            ..
        } => {
            base + id.capacity()
                + command.capacity()
                + cwd.capacity()
                + stdin.capacity() * std::mem::size_of::<String>()
                + stdin.iter().map(|s| s.capacity()).sum::<usize>()
                + stdout.capacity()
                + stderr.capacity()
        }
        Vm::SystemNote { content, .. } | Vm::CacheWarning { content, .. } => {
            base + content.capacity()
        }
        Vm::ToolCallGroup {
            tools,
            standalone_action,
            ..
        } => {
            base + tools.capacity() * std::mem::size_of::<crate::ui::message_view::ToolEntry>()
                + tools
                    .iter()
                    .map(|t| {
                        t.tool_name.capacity()
                            + t.display_name.capacity()
                            + t.args_display.as_ref().map_or(0, |s| s.capacity())
                            + t.content.capacity()
                    })
                    .sum::<usize>()
                + standalone_action.as_ref().map_or(0, |s| s.capacity())
        }
        Vm::SubAgentGroup {
            agent_id,
            task_preview,
            recent_messages,
            final_result,
            batch_agents,
            bg_hash,
            ..
        } => {
            base + agent_id.capacity()
                + task_preview.capacity()
                + bg_hash.as_ref().map_or(0, |s| s.capacity())
                + final_result.as_ref().map_or(0, |s| s.capacity())
                // estimate_vm_heap 已含每个子 VM 的内联尺寸，此处不重复累加
                + recent_messages.iter().map(estimate_vm_heap).sum::<usize>()
                + batch_agents.capacity()
                    * std::mem::size_of::<crate::ui::message_view::AgentSummary>()
                + batch_agents
                    .iter()
                    .map(|a| {
                        a.agent_id.capacity()
                            + a.task_preview.capacity()
                            + a.final_result.as_ref().map_or(0, |s| s.capacity())
                    })
                    .sum::<usize>()
        }
    }
}

fn estimate_block_heap(b: &crate::ui::message_view::ContentBlockView) -> usize {
    use crate::ui::message_view::ContentBlockView as B;
    let base = std::mem::size_of::<B>();
    match b {
        B::Text {
            raw,
            rendered,
            rendered_links,
            ..
        } => {
            base + raw.capacity()
                + estimate_text_heap(rendered)
                + estimate_links_heap(rendered_links)
        }
        B::Reasoning {
            text,
            action_summary,
            ..
        } => base + text.capacity() + action_summary.as_ref().map_or(0, |s| s.capacity()),
        B::ToolUse { name } => base + name.capacity(),
    }
}

/// 估算 ratatui `Text<'static>` 的堆占用：每行 `Line` 的每个 `Span` 内容字符串。
///
/// 注意：`Span.content` 是 `Cow<'a, str>`，无 `capacity()`；对 `Borrowed` 变体
/// 其字节本就内联在 `Text` 里（不计），对 `Owned` 变体按 `len()` 计其堆分配。
fn estimate_text_heap(text: &ratatui::text::Text<'static>) -> usize {
    use ratatui::text::Line;
    use std::borrow::Cow;
    let mut total = text.lines.capacity() * std::mem::size_of::<Line<'static>>();
    for line in &text.lines {
        total += line.spans.capacity() * std::mem::size_of::<ratatui::text::Span<'static>>();
        for span in &line.spans {
            if let Cow::Owned(s) = &span.content {
                total += s.capacity();
            }
        }
    }
    total
}

fn estimate_links_heap(links: &[cc_widgets::markdown::LinkHit]) -> usize {
    std::mem::size_of_val(links) + links.iter().map(|l| l.url.capacity()).sum::<usize>()
}

// ── 格式化 ────────────────────────────────────────────────────────────────────

/// 把带符号的变化量格式化为 `+1.3 MB` / `-1.3 MB` / `±0 B`。
///
/// `delta` 的符号语义统一为「增加为正」（即 `after - before`）。此前 gc 汇总
/// 误用 `before - after` 且仅在 `>= 0` 时加 `+`，导致内存下降被显示为 `+N`。
fn fmt_signed_delta(delta: isize, fmt: impl Fn(usize) -> String) -> String {
    if delta == 0 {
        return format!("±{}", fmt(0));
    }
    let sign = if delta > 0 { "+" } else { "-" };
    format!("{sign}{}", fmt(delta.unsigned_abs()))
}

/// 两个读数的有符号差（`left - right`）。
///
/// 不用 `saturating_sub`：本文件里被减的一方并不保证更小（jemalloc resident 可以
/// 大于内核 RSS，已知估算合计也可以大于 allocated），钳到 0 会让「差值为负」与
/// 「两边完全相等」不可区分，而这两种情况的诊断含义相反。
fn signed_gap(left: usize, right: usize) -> isize {
    left as isize - right as isize
}

/// 百分比（保留一位小数）。分母为 0 时返回 `—`：读不到总量不应显示成 `0.0%`。
fn fmt_share(part: usize, whole: usize) -> String {
    if whole == 0 {
        return "—".to_string();
    }
    format!("{:.1}%", part as f64 * 100.0 / whole as f64)
}

/// 整数加千位分隔（`4576` → `4,576`），用于对象计数。
fn fmt_grouped(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

/// size class 尺寸列：**精确**字节数（`1280` → `1,280 B`）。
///
/// 不能沿用 `fmt_bytes`：1280 B 和 1024 B 都会被四舍五入成 `1 KB`，同一张表里
/// 出现两行相同尺寸会让「哪一档吃了多少」彻底不可读。
fn fmt_bytes_exact(bytes: usize) -> String {
    format!("{} B", fmt_grouped(bytes))
}

fn fmt_bytes(bytes: usize) -> String {
    const KB: usize = 1024;
    const MB: usize = 1024 * KB;
    if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.0} KB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} B")
    }
}

#[cfg(test)]
mod tests {
    include!("gc_test.rs");
}
