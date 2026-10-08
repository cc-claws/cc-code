use cc_agent::messages::{BaseMessage, ContentBlock, MessageContent};

use crate::{app::App, command::Command};

pub struct GcCommand;

impl Command for GcCommand {
    fn name(&self) -> &str {
        "gc"
    }

    fn description(&self, _lc: &crate::i18n::LcRegistry) -> String {
        "手动触发内存回收并显示 RSS 变化和数据结构诊断".to_string()
    }

    fn aliases(&self) -> Vec<&str> {
        vec![]
    }

    fn execute(&self, app: &mut App, _args: &str) {
        let stats_before = crate::alloc_config::query_stats();
        let os_rss_before = crate::alloc_config::os_rss_mb();
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

        let mut lines = Vec::new();

        crate::alloc_config::alloc_collect();

        let stats_after = crate::alloc_config::query_stats();
        let os_rss_after = crate::alloc_config::os_rss_mb();

        // ── RSS 汇总 ──
        match (stats_before, stats_after) {
            (Some(before), Some(after)) => {
                // 方向语义：`after - before`，增加为正。此前误用 `before - after`
                // 且 `delta >= 0` 才加 `+`，导致内存**下降**时反而显示 `+N`（方向颠倒）。
                let delta = after.current_rss as isize - before.current_rss as isize;
                lines.push(format!(
                    "RSS: {} → {} ({})",
                    fmt_bytes(before.current_rss),
                    fmt_bytes(after.current_rss),
                    fmt_signed_delta(delta, fmt_bytes),
                ));
                let alloc_delta = after.current_allocated as isize - after.current_rss as isize;
                if alloc_delta != 0 {
                    // alloc_delta = allocated - RSS（有向差，带符号输出）：
                    // - 为正（allocated 更大）：多为两者采样时刻/记账口径差（先读 RSS 后读
                    //   allocated，其间 /gc 自身仍在分配），不解读为「超出物理内存」。
                    // - 为负（RSS 更大）：RSS 含非分配器占用（栈、映射文件等）。
                    let hint = if alloc_delta > 0 {
                        "allocated 大于当时 RSS（采样时刻/记账口径差，仅供参考）"
                    } else {
                        "RSS 更大 = 栈/映射文件等非分配器占用"
                    };
                    lines.push(format!(
                        "{alloc_name} allocated: {} (allocated-RSS = {}；{hint})",
                        fmt_bytes(after.current_allocated),
                        fmt_signed_delta(alloc_delta, fmt_bytes),
                    ));
                }
            }
            _ => lines.push("RSS: 不可用（分配器 stats 读取失败）".to_string()),
        }

        if let (Some(before), Some(after)) = (os_rss_before, os_rss_after) {
            let delta = after as isize - before as isize;
            lines.push(format!(
                "OS RSS: {} → {} ({})",
                fmt_mb(before),
                fmt_mb(after),
                fmt_signed_delta(delta, fmt_mb_from_usize),
            ));
        }

        // ── 数据结构诊断 ──
        lines.push(String::new());
        lines.push("── 数据结构诊断 ──".to_string());
        lines.push(format!(
            "origin_messages:  {} 条, ~{}",
            origin_count,
            fmt_bytes(origin_bytes),
        ));
        lines.push(format!(
            "pipeline.completed: {} 条, ~{}",
            completed_count,
            fmt_bytes(completed_bytes),
        ));
        lines.push(format!(
            "view_messages:     {} 条 VM, ~{}",
            vm_count,
            fmt_bytes(vm_bytes)
        ));

        // 检查重复
        if origin_count > 0 && completed_count > 0 {
            let overlap = if origin_count == completed_count {
                "完全相同（设计冗余：origin 为 agent 权威历史，completed 为渲染管线基线，非泄漏）"
            } else {
                "部分重叠"
            };
            lines.push(format!(
                "origin vs completed: {} ({}/{} 条)",
                overlap, origin_count, completed_count,
            ));
        }

        // ── 渲染缓存诊断 ──
        lines.push(String::new());
        lines.push("── 渲染缓存 ──".to_string());
        lines.push(format!(
            "markdown_cache: {}/{} 条, 数据堆估算 ~{}",
            md_cache.entries,
            md_cache.capacity,
            fmt_bytes(md_cache.estimated_heap_bytes),
        ));
        lines.push(format!(
            "  平均条目 ~{} | 最大条目 ~{} | {} 行 / {} Span",
            fmt_bytes(
                md_cache
                    .estimated_heap_bytes
                    .checked_div(md_cache.entries)
                    .unwrap_or(0)
            ),
            fmt_bytes(md_cache.largest_entry_heap_bytes),
            md_cache.rendered_lines,
            md_cache.rendered_spans,
        ));
        lines.push(
            "  口径：按容器/自有字符串 capacity 估算，不含 LRU/分配器开销，非 RSS。".to_string(),
        );
        tracing::info!(
            entries = md_cache.entries,
            capacity = md_cache.capacity,
            estimated_heap_bytes = md_cache.estimated_heap_bytes,
            largest_entry_heap_bytes = md_cache.largest_entry_heap_bytes,
            rendered_lines = md_cache.rendered_lines,
            rendered_spans = md_cache.rendered_spans,
            "markdown cache memory snapshot",
        );

        // ── 分配器 breakdown（关键：allocated vs active vs resident）──
        if let Some(bd) = crate::alloc_config::query_breakdown() {
            // active/mapped/retained 的语义**分平台**：
            // - jemalloc（macOS/Linux）：active = 真实活跃页；mapped/retained = 虚拟地址配额
            // - mimalloc（Windows）：active = page_committed（别名 "touched"，历史触及高水位，
            //   mi_collect 后不减）；mapped/retained = reserved 虚拟地址保留量
            // 因此 Windows 上 "active" 与由其派生的"碎片"均**不可用于判断当前占用**。
            let is_mimalloc = alloc_name == "mimalloc";
            lines.push(String::new());
            lines.push(format!("── {alloc_name} 明细 ──"));
            lines.push(format!(
                "allocated: {} (应用实际分配)",
                fmt_bytes(bd.allocated)
            ));
            if is_mimalloc {
                lines.push(format!(
                    "active:    {} (mimalloc page_committed＝历史触及高水位，非当前占用，勿用于诊断)",
                    fmt_bytes(bd.active)
                ));
            } else {
                lines.push(format!("active:    {} (活跃页)", fmt_bytes(bd.active)));
            }
            lines.push(format!(
                "resident:  {} (物理驻留，真实占用)",
                fmt_bytes(bd.resident)
            ));
            lines.push(format!(
                "metadata:  {} (分配器元数据)",
                fmt_bytes(bd.metadata)
            ));
            if is_mimalloc {
                lines.push(format!(
                    "mapped:    {} (reserved 虚拟地址保留量，Windows 上不占物理内存)",
                    fmt_bytes(bd.mapped)
                ));
                lines.push(format!(
                    "retained:  {} (保留未归还 OS 的虚拟地址，非物理内存)",
                    fmt_bytes(bd.retained)
                ));
            } else {
                lines.push(format!("mapped:    {} (映射)", fmt_bytes(bd.mapped)));
                lines.push(format!(
                    "retained:  {} (保留未归还 OS)",
                    fmt_bytes(bd.retained)
                ));
            }
            // 关键指标：碎片仅在 active 有意义时（jemalloc）成立
            let frag = bd.active.saturating_sub(bd.allocated);
            if is_mimalloc {
                lines.push(format!(
                    "碎片: active-allocated={} （⚠ 基于 mimalloc touched 高水位，非真实碎片，忽略）",
                    fmt_bytes(frag)
                ));
            } else {
                let waste = bd.resident.saturating_sub(bd.active);
                lines.push(format!(
                    "碎片: active-allocated={} | resident-active={}",
                    fmt_bytes(frag),
                    fmt_bytes(waste),
                ));
            }
            // OS RSS vs allocator resident
            if let Some(ref s) = stats_after {
                let os_gap = s.current_rss.saturating_sub(bd.resident);
                lines.push(format!(
                    "OS RSS({}) - {alloc_name} resident({}) = {}",
                    fmt_bytes(s.current_rss),
                    fmt_bytes(bd.resident),
                    fmt_bytes(os_gap),
                ));
            }
        }

        // ── 分配器全量 stats → tracing ──
        {
            lines.push(String::new());
            lines.push(format!("── {alloc_name} 全量统计（见日志）──"));
            tracing::info!("=== /gc {alloc_name} full stats dump ===");
            crate::alloc_config::dump_stats();
            tracing::info!("=== /gc {alloc_name} full stats end ===");
        }

        // ── 已知 vs 未识别 ──
        if let Some(bd) = crate::alloc_config::query_breakdown() {
            // Markdown 缓存与 VM 当前持有独立的解析产物副本，可分别计入估算。
            let known_bytes =
                origin_bytes + completed_bytes + vm_bytes + md_cache.estimated_heap_bytes;
            let gap = bd.allocated.saturating_sub(known_bytes);
            lines.push(format!(
                "已知合计估算: {} (消息 {} + VM {} + Markdown 缓存 {}) | allocated 内未识别估算: {}",
                fmt_bytes(known_bytes),
                fmt_bytes(origin_bytes + completed_bytes),
                fmt_bytes(vm_bytes),
                fmt_bytes(md_cache.estimated_heap_bytes),
                fmt_bytes(gap),
            ));
            lines.push(String::new());
            lines.push(
                "注：估算未覆盖后台渲染缓存/Diff 缓存/ACP 缓冲/tokio/tracing 等；余量来源待定位，不能据此判断是否泄漏。"
                    .to_string(),
            );
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

/// 估算渲染视图模型 `view_messages` 的堆内存占用（字节）。
///
/// 覆盖 `MessageViewModel` 各变体，重点包含内嵌的 `Text<'static>`（markdown 渲染结果）、
/// 工具输出字符串、diff 输入、SubAgent 滑窗子 VM 等。
/// 这是此前 `estimate_messages_heap` 完全遗漏的部分——`/gc` 报告的
/// "allocated 内未识别" 主因即在此，纳入后诊断数字才有意义。
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

fn fmt_mb(mb: u64) -> String {
    if mb >= 1024 {
        format!("{:.1} GB", mb as f64 / 1024.0)
    } else {
        format!("{mb} MB")
    }
}

fn fmt_mb_from_usize(mb: usize) -> String {
    fmt_mb(mb as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 回归：RSS 变化的方向符号不得颠倒。
    ///
    /// 此前 gc 汇总用 `before - after` 计算 delta 且仅在 `>= 0` 时加 `+`，
    /// 导致内存**下降**（197.9 → 196.6）被显示为 `+1.3 MB`。
    #[test]
    fn fmt_signed_delta_direction() {
        const MB: isize = 1024 * 1024;
        // 下降 1.3 MB：after - before = -1.3MB → "-1.3 MB"
        assert_eq!(
            fmt_signed_delta(-13 * MB / 10, fmt_bytes),
            "-1.3 MB",
            "内存下降应显示负号"
        );
        // 上升 1.3 MB → "+1.3 MB"
        assert_eq!(fmt_signed_delta(13 * MB / 10, fmt_bytes), "+1.3 MB");
        // 无变化 → "±0 B"
        assert_eq!(fmt_signed_delta(0, fmt_bytes), "±0 B");
    }

    /// 回归：OS RSS（MB 整数）走同一符号约定。
    #[test]
    fn fmt_signed_delta_mb_direction() {
        assert_eq!(fmt_signed_delta(-1, fmt_mb_from_usize), "-1 MB");
        assert_eq!(fmt_signed_delta(3, fmt_mb_from_usize), "+3 MB");
        assert_eq!(fmt_signed_delta(0, fmt_mb_from_usize), "±0 MB");
    }
}
