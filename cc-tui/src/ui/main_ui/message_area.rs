use chrono::{DateTime, Local};
use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Clear, Paragraph, ScrollbarState, Wrap},
    Frame,
};

use cc_widgets::unified_vertical_scrollbar;
use unicode_segmentation::UnicodeSegmentation;

use crate::{
    app::{text_selection::PresentedSelectionSnapshot, App, MessageScrollbarMetrics},
    ui::{
        branch_prefix::BranchPrefix,
        render_thread::{RenderCache, RenderEvent},
        theme, welcome,
    },
};
use cc_middlewares::prelude::{TodoItem, TodoStatus};

use super::{
    sticky_header,
    viewport_layout::{content_rows, welcome_rows},
};

/// 消息区底部 spinner 关联 todo 列表最多展示的条目数
pub(crate) const MAX_VISIBLE_TODOS: usize = 5;

/// 计算给定数量 todo items 在 TUI 消息区底部渲染的逻辑行数（含截断行）
#[cfg(test)]
pub(crate) fn todo_render_line_count(total_todos: usize) -> u16 {
    if total_todos == 0 {
        0
    } else if total_todos <= MAX_VISIBLE_TODOS {
        total_todos as u16
    } else {
        (MAX_VISIBLE_TODOS + 1) as u16
    }
}

/// 格式化多余 todo items 的折叠汇总文案（如 `    ... +3 pending`）
pub(crate) fn format_hidden_todos_summary(hidden_items: &[TodoItem]) -> String {
    let pending_count = hidden_items
        .iter()
        .filter(|item| item.status != TodoStatus::Completed)
        .count();
    let completed_count = hidden_items
        .iter()
        .filter(|item| item.status == TodoStatus::Completed)
        .count();

    if completed_count == 0 {
        format!("    ... +{} pending", pending_count)
    } else if pending_count == 0 {
        format!("    ... +{} completed", completed_count)
    } else {
        format!(
            "    ... +{} pending, +{} completed",
            pending_count, completed_count
        )
    }
}

/// `deep in thought` 触发阈值：思考超过 60s（数分钟长思考）
pub(crate) const DEEP_THOUGHT_SECS: u64 = 60;
/// `still thinking` 触发阈值：同一段思考未中断且超过此秒数
pub(crate) const STILL_THINKING_SECS: u64 = 15;
/// `thinking more` 触发秒数：思考超过 5s 加温
pub(crate) const THINKING_MORE_SECS: u64 = 5;

/// spinner 第三字段（状态词）文案，纯按时间流逝阶梯演变。空白表示不显示。
///
/// - `thinking`：0 ~ 5s 初段思考
/// - `thinking more`：5 ~ 15s 持续思考
/// - `still thinking`：15 ~ 60s 超时深思
/// - `deep in thought`：>= 60s 深度沉思（长达数分钟）
/// - `thought for Ns`：非思考段（工具执行/出文本），定格显示上一段耗时
pub(crate) fn thinking_status_word(
    is_thinking: bool,
    thinking_elapsed_ms: u64,
    last_thought_ms: u64,
) -> String {
    if !is_thinking {
        return if last_thought_ms > 0 {
            format!("thought for {}s", (last_thought_ms / 1000).max(1))
        } else {
            String::new()
        };
    }
    let secs = thinking_elapsed_ms / 1000;
    let word = if secs >= DEEP_THOUGHT_SECS {
        "deep in thought"
    } else if secs >= STILL_THINKING_SECS {
        "still thinking"
    } else if secs >= THINKING_MORE_SECS {
        "thinking more"
    } else {
        "thinking"
    };
    word.to_string()
}

/// 状态词配色（对齐 CC 2.1.300 的 thinkingIntensity 机制，见 issue #510）：
///
/// - 状态词不闪烁；思考段超过 10s 后，按 thinkingIntensity（10s 起、再 10s 渐满）
///   从 inactive 向 warning 平滑插值；10s 内恒为 inactive；
/// - `thought for Ns`：已完成态，始终 inactive；
/// - `compact`：特例整体用 thinking 紫色。
pub(crate) fn thinking_status_style(
    status_word: &str,
    thinking_elapsed_ms: u64,
    is_compact: bool,
) -> ratatui::style::Style {
    if is_compact {
        return Style::default().fg(theme::cur().thinking);
    }
    if status_word.starts_with("thought for") {
        return Style::default().fg(theme::cur().inactive);
    }
    let intensity = cc_widgets::spinner::animation::thinking_intensity(thinking_elapsed_ms);
    let color = cc_widgets::spinner::animation::blend_color(
        theme::cur().inactive,
        theme::cur().warning,
        intensity,
    );
    Style::default().fg(color)
}

/// 视口裁剪结果
struct ViewportClip {
    /// 裁剪后的可见行（含 spinner 和选区高亮）
    lines: Vec<Line<'static>>,
    /// 裁剪后的局部滚动偏移（相对于 lines[0] 的视觉行偏移）
    local_offset: u16,
    selection_snapshot: Option<PresentedSelectionSnapshot>,
    presented_offset: usize,
    /// 本帧可见行的超链接命中区，已换算为「视口内行偏移 → 该行列区间」。
    ///
    /// 行偏移相对 `body_area.y`（即视口首行），列区间相对 `body_area.x`。
    /// 换算需要 `cache.wrap_map` 与 `presented_offset`，这两者在 `viewport_clip`
    /// 内才同时可用，故在此处一并算好，输出层只做纯坐标平移与注入。
    link_rows: Vec<(u16, Vec<crate::ui::osc8::LinkColumns>)>,
}

pub(crate) fn render_messages(
    f: &mut Frame,
    app: &mut App,
    header_area: Rect,
    messages_area: Rect,
) {
    // #629：虚拟布局代际/版本变化后，旧锚点选区失效，先清掉再绘制。
    let invalid_selection = {
        let session = app.session_mgr.current();
        let cache = session.messages.render_cache.read();
        [
            session.ui.text_selection.anchor_start,
            session.ui.text_selection.anchor_end,
        ]
        .into_iter()
        .flatten()
        .any(|anchor| {
            cache.virtual_layout.as_ref().is_none_or(|layout| {
                layout.history_epoch != anchor.history_epoch
                    || layout.message_versions.get(anchor.message_index)
                        != Some(&anchor.message_version)
            })
        })
    };
    if invalid_selection {
        let ui = &mut app.session_mgr.current_mut().ui;
        ui.text_selection.clear();
        ui.pending_text_copy = None;
    }

    let empty_messages = app.session_mgr.current().current_view_messages().is_empty();
    let tail = visible_tail(app);
    let inner = messages_area;
    let visible_height = inner.height;

    let visual_total = content_rows(app, inner.width);
    let needs_scrollbar = visual_total > visible_height as usize && inner.width > 1;
    let text_area = if needs_scrollbar {
        Rect {
            width: inner.width.saturating_sub(1),
            ..inner
        }
    } else {
        inner
    };
    app.session_mgr.current_mut().ui.messages_area = Some(text_area);
    let prefix_rows = welcome_rows(app, text_area.width);
    app.session_mgr.current_mut().ui.message_prefix_rows = prefix_rows;

    // 渲染驱动宽度同步：用 last_resize_width 去抖——宽度未变时跳过重复发送，
    // 避免每秒 N 次 resize 事件导致渲染线程队列积压和 CPU 暴涨
    // （参见 spec/issues/2026-05-14-streaming-resize-cpu-spike）。
    {
        let text_area_width = text_area.width;
        let cache_width = app.session_mgr.current().messages.render_cache.read().width;
        let last_resize = app.session_mgr.current().messages.last_resize_width;
        if last_resize != Some(text_area_width)
            && cache_width != text_area_width
            && text_area_width > 0
        {
            app.session_mgr.current_mut().messages.last_resize_width = Some(text_area_width);
            let _ = app
                .session_mgr
                .current_mut()
                .messages
                .render_tx
                .try_send(RenderEvent::Resize(text_area_width));
        }
    }

    // ── 从 RenderCache 读取并计算滚动参数 ──────────────────────────────────
    let (min_scroll, max_scroll, offset) = {
        let mut cache = app.session_mgr.current().messages.render_cache.write();

        // 全屏 alternate screen 模式：无原生 scrollback，所有内容都在 TUI 视口内，
        // base_visual 恒为 0。
        let message_height = if empty_messages { 0 } else { cache.total_lines };
        let visual_total = prefix_rows
            .saturating_add(message_height)
            .saturating_add(visual_line_count(&tail, text_area.width));
        let min_scroll = 0;
        let max_scroll = visual_total
            .saturating_sub(visible_height as usize)
            .max(min_scroll);
        let scroll_follow = app.session_mgr.current().ui.scroll_follow;
        let scroll_offset = app.session_mgr.current().ui.scroll_offset;
        let scroll_anchor = cache.scroll_anchor.take().filter(|_| {
            cache.virtual_layout.as_ref().is_none_or(|layout| {
                layout.requested_top == scroll_offset
                    && layout.requested_height == visible_height as usize
                    && cache.width == text_area.width
            })
        });
        let dragging = app.session_mgr.current().ui.message_scrollbar_dragging;
        let (new_follow, off) = if dragging {
            // dragging 期间跳过 follow/anchor 优先级，强制使用用户拖拽的 offset
            let off = scroll_offset.clamp(min_scroll, max_scroll);
            (off >= max_scroll, off)
        } else if scroll_follow {
            (true, max_scroll)
        } else if scroll_offset < prefix_rows {
            // 消息锚点不包含欢迎卡；浏览前缀时重建不能把用户推到消息起点。
            let off = scroll_offset.min(max_scroll);
            (off >= max_scroll, off)
        } else if let Some(anchor) = scroll_anchor {
            let off = anchor
                .saturating_add(prefix_rows)
                .clamp(min_scroll, max_scroll);
            let new_follow = off >= max_scroll;
            (new_follow, off)
        } else {
            let off = scroll_offset.clamp(min_scroll, max_scroll);
            let new_follow = off >= max_scroll;
            (new_follow, off)
        };

        let version = cache.version;
        drop(cache);

        app.session_mgr.current_mut().messages.last_render_version = version;
        app.session_mgr.current_mut().ui.scroll_follow = new_follow;
        app.session_mgr.current_mut().ui.scroll_offset = off;
        app.session_mgr.current_mut().ui.scrollbar_min_offset = min_scroll;
        app.session_mgr.current_mut().ui.scrollbar_max_offset = max_scroll;

        (min_scroll, max_scroll, off)
    };

    // 欢迎前缀在本帧实际占用的高度（消息绘制与视口上报共用同一口径）
    let prefix_height = prefix_rows
        .saturating_sub(offset)
        .min(text_area.height as usize) as u16;

    // #629：视口变化单独上报渲染线程（滚动不再附带完整历史 VM）；
    // 坐标换算回消息空间——top 扣前缀、height 取消息正文区高度。
    {
        let top_msg = offset.saturating_sub(prefix_rows);
        let body_height = text_area.height.saturating_sub(prefix_height) as usize;
        let session = app.session_mgr.current_mut();
        let anchor = session
            .messages
            .render_cache
            .read()
            .viewport_anchor_at(top_msg);
        let request = (
            top_msg,
            body_height,
            session.ui.scroll_follow,
            text_area.width,
            anchor,
        );
        if session.messages.last_viewport != Some(request)
            && session
                .messages
                .render_tx
                .try_send(RenderEvent::Viewport {
                    top: top_msg,
                    height: body_height,
                    follow_tail: session.ui.scroll_follow,
                    anchor,
                })
                .is_ok()
        {
            session.messages.last_viewport = Some(request);
        }
        if empty_messages {
            session.messages.last_viewport = None;
        }
    }

    // 仅在当前 TUI 尾部仍可滚动时显示 sticky header
    if max_scroll > min_scroll {
        sticky_header::render_sticky_header(f, app, header_area);
    }

    // ── 视口裁剪 ──────────────────────────────────────────────────────────
    // 利用 wrap_map 定位可见的逻辑行范围，只传递视口内的行给 Paragraph，
    // 避免 ratatui Paragraph::render 内部 O(offset) 的 WordWrapper 遍历导致 CPU 暴涨。
    // （ratatui 即使设了 scroll(offset)，仍会对 offset 之前的所有行做 grapheme 分割 + wrap 计算）
    // 先用 Clear 清空整个 messages_area：Paragraph 不填满 area 时未覆盖 cell 会保留旧值，
    // 最终经 diff 输出到终端造成渲染残留。清空后 Paragraph 只渲染有效行、剩余为空格，
    // 确保每帧 viewport 区域 buffer 正确。
    f.render_widget(Clear, inner);
    // 欢迎卡是纯 UI 前缀，与消息一起滚动；切全屏时不会消失，也不进入会话历史。
    if prefix_height > 0 {
        welcome::render_welcome_slice(f, app, text_area, offset);
    }
    let body_area = Rect {
        y: text_area.y.saturating_add(prefix_height),
        height: text_area.height.saturating_sub(prefix_height),
        ..text_area
    };
    let clip = viewport_clip(
        app,
        offset.saturating_sub(prefix_rows),
        body_area.height,
        &tail,
        text_area.width,
    );
    app.session_mgr.current_mut().ui.presented_scroll_offset = clip.presented_offset;
    app.session_mgr
        .current_mut()
        .ui
        .presented_selection_snapshot = if empty_messages {
        None
    } else {
        clip.selection_snapshot
    };
    // 先取出链接行，clip.lines 随后会被 move 进 Paragraph。
    let link_rows = clip.link_rows;
    let paragraph = Paragraph::new(Text::from(clip.lines))
        .scroll((clip.local_offset, 0))
        .wrap(Wrap { trim: false });
    f.render_widget(paragraph, body_area);

    // ── OSC 8 超链接登记 ─────────────────────────────────────────────────
    // 这里只登记「哪个屏幕坐标是链接」，转义序列由输出层
    // （`terminal_backend::hyperlink`）在写字节时包裹。**绝不能改 buffer**：
    // ratatui 的 `Buffer::diff` 按 `symbol.width()` 决定跳过后续多少个 cell，
    // 序列载荷（URL）是可见字符，塞进 symbol 会让链接后面的整段文字不落笔。
    if !link_rows.is_empty() {
        for (row_in_view, row_links) in link_rows {
            let y = body_area.y.saturating_add(row_in_view);
            if y >= body_area.bottom() {
                continue;
            }
            let x0 = body_area.x;
            let x1 = body_area.right();
            let cells = (x0..x1).map(|x| (x - x0, (x, y)));
            for ((x, y), url, recolor, id) in
                crate::ui::osc8::link_cells(cells, &row_links, body_area)
            {
                crate::terminal_backend::hyperlink::register(x, y, &url, recolor, id);
            }
        }
    }

    // 只在最右侧热区或拖拽中显示；显隐不改变正文宽度。
    let popup_active = app.is_interaction_popup_active();
    let ui = &mut app.session_mgr.current_mut().ui;
    ui.message_scrollbar_area = needs_scrollbar
        .then(|| Rect::new(inner.right().saturating_sub(1), inner.y, 1, inner.height));
    let scrollbar_active = !popup_active
        && (ui.scrollbar_hover
            || ui.message_scrollbar_dragging
            || !crate::conpty::hover_available());
    let metrics = render_message_scrollbar(f, inner, max_scroll, offset, scrollbar_active);
    let ui = &mut app.session_mgr.current_mut().ui;
    ui.message_scrollbar_metrics = metrics;
    if metrics.is_none() {
        ui.message_scrollbar_dragging = false;
        ui.message_scrollbar_drag_origin = None;
    }
}

/// 新窗口未发布时继续显示旧窗口，滚动请求仍保留原目标，避免空白帧或取消待处理跳转。
fn presented_viewport_offset(
    cache: &RenderCache,
    requested: usize,
    previous: usize,
    visible_height: u16,
) -> usize {
    if cache.virtual_layout.is_none() || visible_height == 0 {
        return requested;
    }
    let (Some(first), Some(last)) = (cache.wrap_map.first(), cache.wrap_map.last()) else {
        return requested;
    };
    let content_start = requested.min(cache.total_lines);
    let content_end = requested
        .saturating_add(visible_height as usize)
        .min(cache.total_lines);
    if first.visual_row_start <= content_start && last.visual_row_end >= content_end {
        return requested;
    }
    let max_presented = last
        .visual_row_end
        .saturating_sub(visible_height as usize)
        .max(first.visual_row_start);
    previous.clamp(first.visual_row_start, max_presented)
}

fn render_message_scrollbar(
    f: &mut Frame,
    area: Rect,
    max_scroll: usize,
    offset: usize,
    active: bool,
) -> Option<MessageScrollbarMetrics> {
    if max_scroll == 0 || area.width == 0 || area.height == 0 || !active {
        return None;
    }

    let bar_area = Rect {
        x: area.right().saturating_sub(1),
        y: area.y,
        width: 1,
        height: area.height,
    };
    let offset = offset.min(max_scroll);
    let style = Style::default().fg(theme::cur().subtle);
    let mut scrollbar_state = ScrollbarState::new(max_scroll.saturating_add(1)).position(offset);
    f.render_stateful_widget(
        unified_vertical_scrollbar().style(style),
        area,
        &mut scrollbar_state,
    );

    // 从本帧真实绘制的滑块获取命中区，避免复制 ratatui 的私有舍入公式。
    let mut thumb_area = Rect::new(bar_area.x, bar_area.y, 1, 0);
    for row in bar_area.y..bar_area.bottom() {
        if f.buffer_mut()[(bar_area.x, row)].symbol() == "█" {
            if thumb_area.height == 0 {
                thumb_area.y = row;
            }
            thumb_area.height += 1;
        }
    }
    Some(MessageScrollbarMetrics {
        bar_area,
        thumb_area,
        max_offset: max_scroll,
        up_btn_area: None,
        down_btn_area: None,
    })
}

/// 基于视口裁剪提取可见行。
///
/// 策略：
/// 1. 从 RenderCache 的 wrap_map 用二分查找定位 [vis_start, vis_end) 视觉行范围
///    对应的逻辑行 [first_visible, last_visible]
/// 2. 只克隆这些逻辑行（O(visible) 而非 O(total)）
/// 3. 计算局部滚动偏移（local_offset = vis_start - first_visible.visual_row_start）
/// 4. 检查 spinner 行是否在视口范围内（spinner 在底部，visual 行号 > cache.total_lines）
/// 5. 如果有文本选区，在裁剪后的行上做高亮
fn viewport_clip(
    app: &App,
    offset: usize,
    visible_height: u16,
    tail: &[Line<'static>],
    width: u16,
) -> ViewportClip {
    // ── 阶段 1：从 cache 提取可见行（cache guard 在 block 结束时 drop） ──
    let (
        mut lines,
        mut local_offset,
        first_idx,
        total_lines,
        selection_snapshot,
        offset,
        link_rows,
    ) = {
        let cache = app.session_mgr.current().messages.render_cache.read();
        // #629：虚拟窗口未发布到请求位置时，先按上帧已呈现偏移绘制，避免空白帧。
        let offset = presented_viewport_offset(
            &cache,
            offset,
            app.session_mgr.current().ui.presented_scroll_offset,
            visible_height,
        );

        let vis_start = offset;
        // +1 给 wrap 行留余量
        let vis_end = (offset + visible_height as usize + 1).min(cache.total_lines);

        // wrap_map 按 visual_row_start 升序排列
        // 二分找第一个 visual_row_end > vis_start 的行（即首个与视口相交的行）
        let first_visible = cache
            .wrap_map
            .partition_point(|info| info.visual_row_end <= vis_start);
        // 二分找最后一个 visual_row_start < vis_end 的行
        let last_visible = cache
            .wrap_map
            .partition_point(|info| info.visual_row_start < vis_end)
            .saturating_sub(1);

        let total_lines = if app.session_mgr.current().current_view_messages().is_empty() {
            0
        } else {
            cache.total_lines
        };

        let (lines, local_offset, first_idx) = if total_lines > 0
            && first_visible < cache.wrap_map.len()
            && first_visible <= last_visible
        {
            let first_visual = cache.wrap_map[first_visible].visual_row_start;
            let local_offset = vis_start
                .saturating_sub(first_visual)
                .min(u16::MAX as usize) as u16;
            let lines = cache.lines[first_visible..=last_visible].to_vec();
            (lines, local_offset, first_visible)
        } else {
            // 空内容或视口超出范围
            (Vec::new(), 0u16, cache.lines.len())
        };

        let actual_width = app
            .session_mgr
            .current()
            .ui
            .messages_area
            .map_or(0, |area| area.width);
        let selection_snapshot = cache
            .virtual_layout
            .as_ref()
            .filter(|_| cache.width == actual_width)
            .map(|layout| {
                let end_idx = first_idx + lines.len();
                PresentedSelectionSnapshot {
                    layout: layout.clone(),
                    wrap_map: cache
                        .wrap_map
                        .get(first_idx..end_idx)
                        .unwrap_or_default()
                        .to_vec(),
                    links: cache
                        .links
                        .iter()
                        .filter(|link| link.line >= first_idx && link.line < end_idx)
                        .cloned()
                        .collect(),
                    width: cache.width,
                }
            });
        // 本帧可见的超链接命中区：把「全局行 + grapheme 区间」换算成
        // 「视口内行偏移 + 显示列区间」。wrap_map/presented offset 只在这里
        // 同时可用，故一并算好；plain_text/char_widths 同属该 wrap_map 条目。
        let mut link_rows: Vec<(u16, Vec<crate::ui::osc8::LinkColumns>)> = Vec::new();
        for link in cache
            .links
            .iter()
            .filter(|link| link.line >= first_idx && link.line < first_idx + lines.len())
        {
            let Some(info) = cache.wrap_map.get(link.line) else {
                continue;
            };
            // 视觉行号相对整个视口起点；超出本帧可见范围则丢弃。
            let Some(row_in_view) = info.visual_row_start.checked_sub(offset) else {
                continue;
            };
            if row_in_view >= visible_height as usize {
                continue;
            }
            let Some(columns) =
                crate::ui::osc8::graphemes_to_columns(&info.plain_text, &info.char_widths, link)
            else {
                continue;
            };
            match link_rows
                .iter_mut()
                .find(|(row, _)| *row as usize == row_in_view)
            {
                Some((_, links)) => links.push(columns),
                None => link_rows.push((row_in_view as u16, vec![columns])),
            }
        }
        for (_, links) in link_rows.iter_mut() {
            links.sort_by_key(|link| link.columns.start);
        }
        (
            lines,
            local_offset,
            first_idx,
            total_lines,
            selection_snapshot,
            offset,
            link_rows,
        )
    }; // cache guard dropped here

    // 附属区域也按实际换行裁剪；单独浏览尾部时不从第一行重复渲染。
    let tail_offset = offset.saturating_sub(total_lines);
    let mut tail_row = 0;
    for line in tail {
        let height = visual_line_count(std::slice::from_ref(line), width);
        if tail_row + height > tail_offset {
            if lines.is_empty() {
                local_offset = tail_offset.saturating_sub(tail_row).min(u16::MAX as usize) as u16;
            }
            lines.push(line.clone());
        }
        tail_row += height;
    }

    // ── 阶段 3：字符级选区高亮（需要再次读 cache 获取 wrap_map） ──
    // 只在裁剪后的可见行上做高亮（减少工作量）
    if app.session_mgr.current().ui.text_selection.is_active() {
        let ts = &app.session_mgr.current().ui.text_selection;
        let cache = app.session_mgr.current().messages.render_cache.read();
        if let Some(snapshot) = &selection_snapshot {
            let layout = &snapshot.layout;
            if let Some((start, end)) = ts.anchor_start.zip(ts.anchor_end) {
                if start.history_epoch == layout.history_epoch
                    && end.history_epoch == layout.history_epoch
                {
                    for (local_index, line) in lines.iter_mut().enumerate() {
                        let Some(&(message_index, logical_line)) =
                            layout.line_origins.get(first_idx + local_index)
                        else {
                            continue;
                        };
                        if let Some((from, to)) =
                            crate::app::text_selection::anchored_selection_columns(
                                start,
                                end,
                                message_index,
                                logical_line,
                            )
                        {
                            let spans = std::mem::take(&mut line.spans);
                            *line = Line::from(highlight_line_spans(spans, from, to));
                        }
                    }
                }
            }
        } else if let Some((start, end)) = ts
            .start
            .zip(ts.end)
            .filter(|_| cache.virtual_layout.is_none())
        {
            let wrap_map = &cache.wrap_map;
            let usable_width = app
                .session_mgr
                .current()
                .ui
                .messages_area
                .map(|a| a.width)
                .unwrap_or(0);

            let ((sr, sc), (er, ec)) = if start <= end {
                (start, end)
            } else {
                (end, start)
            };
            let logical_start =
                crate::app::text_selection::visual_to_logical(sr, sc, wrap_map, usable_width);
            let logical_end =
                crate::app::text_selection::visual_to_logical(er, ec, wrap_map, usable_width);

            if let (Some((start_line, start_char)), Some((end_line, end_char))) =
                (logical_start, logical_end)
            {
                // lines 中的第 i 行对应 cache.lines 中的 first_idx + i
                // 只处理 [start_line, end_line] ∩ [first_idx, first_idx + lines.len()) 的行
                let clip_start = start_line.max(first_idx);
                let clip_end = end_line.min(first_idx + lines.len().saturating_sub(1));

                for line_idx in clip_start..=clip_end {
                    let local_idx = line_idx - first_idx;
                    let (cs, ce) = if line_idx == start_line && line_idx == end_line {
                        (start_char, end_char)
                    } else if line_idx == start_line {
                        (start_char, usize::MAX)
                    } else if line_idx == end_line {
                        (0, end_char)
                    } else {
                        (0, usize::MAX)
                    };
                    let spans = std::mem::take(&mut lines[local_idx].spans);
                    lines[local_idx] = Line::from(highlight_line_spans(spans, cs, ce));
                }
            }
        }
    }

    ViewportClip {
        lines,
        local_offset,
        selection_snapshot,
        presented_offset: offset,
        link_rows,
    }
}

/// 测量与绘制共用同一组可见附属行，隐藏推理和折叠正文不占高度。
pub(super) fn visible_tail(app: &App) -> Vec<Line<'static>> {
    // 计算 loading spinner 行（CC 风格：✻ verb (Xm Xs · ↓ X.Xk tokens)）
    // compact 时紫色，其余橙色；loading 结束后显示总结行：✻ Brewed for Xm Xs
    let spinner_line: Option<Line<'static>> = if app.session_mgr.current().viewport_loading() {
        let session = app.session_mgr.current();
        let frame = session.viewport_spinner().spinner_frame();
        let verb = session.viewport_spinner().verb();
        let elapsed =
            cc_widgets::spinner::animation::format_elapsed(session.viewport_spinner().elapsed_ms());
        let tokens = session.viewport_spinner().displayed_tokens();

        // 手动 compact 期间用紫色；不再依赖 verb 文案判断（文案随语言变化）
        let is_compact = session.agent.compact_manual;
        // 第三字段：状态词（纯时间驱动）
        let status_word = thinking_status_word(
            session.viewport_spinner().is_thinking(),
            session.viewport_spinner().thinking_elapsed_ms(),
            session.viewport_spinner().last_thought_ms(),
        );
        // 动词色对（对齐 CC：底色 + 流光色成对，按状态切换；见 issue #510）
        // compact 沿用 thinking 紫作底色，流光保持默认 claude_shimmer（同 CC 的覆盖方式）
        let (base_color, shimmer_color) = if is_compact {
            (theme::cur().thinking, theme::cur().claude_shimmer)
        } else {
            (theme::cur().claude, theme::cur().claude_shimmer)
        };
        // 状态词配色：thinkingIntensity 向 warning 插值（状态词不闪烁）
        let status_thinking_ms = if session.viewport_spinner().is_thinking() {
            session.viewport_spinner().thinking_elapsed_ms()
        } else {
            session.viewport_spinner().last_thought_ms()
        };
        let status_style = thinking_status_style(&status_word, status_thinking_ms, is_compact);
        let gray = Style::default().fg(theme::cur().inactive);

        // 停滞转红（对齐 CC）：无新 token >10s 后流光熄火，字形与动词整体向 error 插值；
        // 否则动词走 ±1 列流光扫过。
        let stall = session.viewport_spinner().stalled_intensity();
        let (frame_color, verb_spans) = if stall > 0.0 {
            let color =
                cc_widgets::spinner::animation::blend_color(base_color, theme::cur().error, stall);
            (
                color,
                vec![Span::styled(verb.to_string(), Style::default().fg(color))],
            )
        } else {
            (
                base_color,
                cc_widgets::spinner::animation::shimmer_verb_spans(
                    verb,
                    session.viewport_spinner().elapsed_ms(),
                    base_color,
                    shimmer_color,
                ),
            )
        };

        let mut parts = vec![Span::styled(
            format!("{} ", frame),
            Style::default().fg(frame_color),
        )];
        parts.extend(verb_spans);
        parts.push(Span::styled(format!(" ({elapsed}"), gray));
        if tokens > 0 {
            let tokens_fmt = cc_widgets::spinner::animation::format_tokens(tokens);
            parts.push(Span::styled(format!(" · ↓ {tokens_fmt} tokens"), gray));
        }
        // 第三字段（状态词）紧随 tokens 之后
        if !status_word.is_empty() {
            parts.push(Span::styled(format!(" · {status_word}"), status_style));
        }
        parts.push(Span::styled(")", gray));
        Some(Line::from(parts))
    } else if app
        .session_mgr
        .current()
        .viewport_spinner()
        .last_summary_elapsed_ms()
        > 0
    {
        let session = app.session_mgr.current();
        let elapsed = cc_widgets::spinner::animation::format_elapsed(
            session.viewport_spinner().last_summary_elapsed_ms(),
        );
        let verb = {
            let v = session.viewport_spinner().last_summary_verb();
            if v.is_empty() { "Brewed" } else { v }.to_string()
        };
        let done_str = session
            .viewport_spinner()
            .last_summary_done_at()
            .map(|t| {
                let dt: DateTime<Local> = t.into();
                format!(" · done {}", dt.format("%H:%M"))
            })
            .unwrap_or_default();
        Some(Line::from(Span::styled(
            format!("✻ {verb} for {elapsed}{done_str}"),
            Style::default().fg(theme::cur().inactive),
        )))
    } else {
        None
    };

    supplementary_lines(app, &spinner_line)
}

/// 压缩进行中的「小人跑道」：固定 12 格，小人每 150ms 从右向左单向奔跑并穿墙回卷。
///
/// 占的是 tip 那一行（不增加布局高度），压缩结束即消失。进度是假的，
/// 这里只表达「还在跑」，所以不用进度条。
/// Unicode 中 `🏃` / `🚶` 默认面朝左侧，因此采用从右向左单向奔跑动线，
/// 使小人脸部与位移方向保持自然一致，避免双向折返时的太空步倒退违和。
fn compact_runner_line(started_at: std::time::Instant) -> Line<'static> {
    const TRACK: usize = 12;
    /// 跑：每 150ms 一格。
    const RUN_STEP_MS: u128 = 150;
    /// 走：步子明显放慢，避免长时间压缩时一直「狂奔」显得假。
    const WALK_STEP_MS: u128 = 420;
    /// 跑到这个时长就「累了」改走路。
    const TIRED_AFTER_SECS: u64 = 20;

    let elapsed = started_at.elapsed();
    let tired = elapsed.as_secs() >= TIRED_AFTER_SECS;
    let (glyph, step_ms, glyph_color) = if tired {
        ("🚶", WALK_STEP_MS, theme::cur().inactive)
    } else {
        ("🏃", RUN_STEP_MS, theme::cur().thinking)
    };
    let frame = (elapsed.as_millis() / step_ms) as usize;
    let step = frame % TRACK;
    // 从右端（TRACK - 1）向左端（0）单向奔跑
    let pos = TRACK - 1 - step;

    let mut spans = Vec::with_capacity(TRACK);
    for cell in 0..TRACK {
        if cell == pos {
            spans.push(Span::styled(glyph, Style::default().fg(glyph_color)));
        } else if cell > pos {
            // 向左跑时，已经经过的格子落在小人身后（右侧）留下足迹
            spans.push(Span::styled(
                "• ",
                Style::default().fg(theme::cur().inactive),
            ));
        } else {
            spans.push(Span::styled("· ", Style::default().fg(theme::cur().subtle)));
        }
    }
    Line::from(spans)
}

/// 压缩进行中的「螃蟹跑道」：固定 12 格，螃蟹每 160ms 横向爬行一格，到头折返。
///
/// 螃蟹天然侧身横向行走，双向折返跑没有任何脸部朝向违和感，同时呼应
/// Rust 吉祥物 Ferris。
fn compact_crab_line(started_at: std::time::Instant) -> Line<'static> {
    const TRACK: usize = 12;
    /// 爬：每 160ms 一格横行小碎步。
    const RUN_STEP_MS: u128 = 160;
    /// 慢爬：步子放慢。
    const WALK_STEP_MS: u128 = 420;
    /// 爬满这个时长就改慢爬。
    const TIRED_AFTER_SECS: u64 = 20;
    /// 折返的两段停留（帧数）。
    const HOLD_END: usize = 4;
    const HOLD_START: usize = 4;

    let elapsed = started_at.elapsed();
    let tired = elapsed.as_secs() >= TIRED_AFTER_SECS;
    let (step_ms, glyph_color) = if tired {
        (WALK_STEP_MS, theme::cur().inactive)
    } else {
        (RUN_STEP_MS, theme::cur().claude)
    };
    let glyph = "🦀";
    let frame = (elapsed.as_millis() / step_ms) as usize;
    let cycle = TRACK + HOLD_END + (TRACK - 1) + HOLD_START;
    let phase = frame % cycle;
    let (pos, forward) = if phase < TRACK {
        (phase, true)
    } else if phase < TRACK + HOLD_END {
        (TRACK - 1, true)
    } else if phase < TRACK + HOLD_END + (TRACK - 1) {
        (TRACK - 2 - (phase - TRACK - HOLD_END), false)
    } else {
        (0, false)
    };

    let mut spans = Vec::with_capacity(TRACK);
    for cell in 0..TRACK {
        if cell == pos {
            spans.push(Span::styled(glyph, Style::default().fg(glyph_color)));
        } else if (forward && cell < pos) || (!forward && cell > pos) {
            // 已经经过的格子留下足迹；折返时足迹落在身后那一侧。
            spans.push(Span::styled(
                "• ",
                Style::default().fg(theme::cur().inactive),
            ));
        } else {
            spans.push(Span::styled("· ", Style::default().fg(theme::cur().subtle)));
        }
    }
    Line::from(spans)
}

/// 压缩进行中的「飞碟跑道」。
///
/// 逐行移植 MiMo-Code `packages/cli/src/cli/cmd/tui/ui/spinner.ts` 的
/// `getScannerState` + `createFrames(style: "plane")`；参数取自
/// `component/prompt/index.tsx`：width 14 / holdStart 8 / holdEnd 8 / interval 40ms。
///
/// 帧序：前进 14 格 → 终点停留 8 帧 → 后退 13 格 → 起点停留 8 帧（共 43 帧 ≈ 1.72s）。
/// 尾迹覆盖 `dd ∈ 1..=5`（`createColors` 默认 trailSteps = 6），字符按
/// `["·","∙","˙"][(frame + dd) % 3]` 轮换。
fn compact_ufo_line(started_at: std::time::Instant) -> Line<'static> {
    const TRACK: usize = 14;
    const STEP_MS: u128 = 40;
    const HOLD_START: usize = 8;
    const HOLD_END: usize = 8;
    /// `trailSteps = 6`：尾迹占 dd ∈ 1..=5。
    const TRAIL_LEN: usize = 6;
    const TRAIL_CHARS: [&str; 3] = ["· ", "∙ ", "˙ "];

    let forward_frames = TRACK;
    let backward_frames = TRACK - 1;
    let total_frames = forward_frames + HOLD_END + backward_frames + HOLD_START;
    let frame = (started_at.elapsed().as_millis() / STEP_MS) as usize % total_frames;

    // getScannerState（direction = bidirectional）
    let (active, moving_forward) = if frame < forward_frames {
        (frame, true)
    } else if frame < forward_frames + HOLD_END {
        (TRACK - 1, true)
    } else if frame < forward_frames + HOLD_END + backward_frames {
        (TRACK - 2 - (frame - forward_frames - HOLD_END), false)
    } else {
        (0, false)
    };

    let mut spans = Vec::with_capacity(TRACK);
    for cell in 0..TRACK {
        let dd = if moving_forward {
            active as isize - cell as isize
        } else {
            cell as isize - active as isize
        };
        if dd == 0 {
            spans.push(Span::styled("🛸", Style::default().fg(theme::cur().cyan)));
            continue;
        }
        // 每格固定 2 列，与 🛸 的显示宽度对齐。
        if dd > 0 && (dd as usize) < TRAIL_LEN {
            // 参考实现按 alpha 渐隐；终端没有 alpha，用两级灰近似（近格更亮）。
            let color = if dd == 1 {
                theme::cur().inactive
            } else {
                theme::cur().subtle
            };
            spans.push(Span::styled(
                TRAIL_CHARS[(frame + dd as usize) % TRAIL_CHARS.len()],
                Style::default().fg(color),
            ));
        } else {
            spans.push(Span::styled("  ", Style::default().fg(theme::cur().subtle)));
        }
    }
    Line::from(spans)
}

fn supplementary_lines(app: &App, spinner_line: &Option<Line<'static>>) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    if let Some(line) = spinner_line {
        // 追加 spinner 分隔空行 + spinner line
        lines.push(Line::from(""));
        lines.push(line.clone());
        // Tip + TODO
        if app.session_mgr.current().viewport_loading() {
            // tip 由调度器在轮次边界挑选/发布（见 App::set_loading），渲染只读当前已发布的
            // 一条；本轮运行 >30min 时被 /clear 时间覆盖 tip 顶替（对齐 CC，见 issue #511）
            let turn_elapsed = app.session_mgr.current().viewport_spinner().elapsed_ms();
            // 压缩进行中：这一行让位给「压缩跑道」动画（不额外占行、不改布局高度）。
            if let Some(started_at) = app.session_mgr.current().agent.compact_started_at {
                lines.push(match app.session_mgr.current().agent.compact_motion {
                    crate::app::CompactMotion::Runner => compact_runner_line(started_at),
                    crate::app::CompactMotion::Crab => compact_crab_line(started_at),
                    crate::app::CompactMotion::Ufo => compact_ufo_line(started_at),
                });
            } else if let Some((label, text)) = app.spinner_tip_display(turn_elapsed) {
                lines.push(Line::from(vec![
                    Span::styled(
                        format!("{}{label}: ", BranchPrefix::PREFIX),
                        Style::default().fg(theme::cur().inactive),
                    ),
                    Span::styled(text, Style::default().fg(theme::cur().inactive)),
                ]));
            }
            if app.session_mgr.current().active_subagent().is_none()
                && !app.session_mgr.current().todo_items.is_empty()
            {
                lines.push(Line::from(""));
                let todos = &app.session_mgr.current().todo_items;
                let visible_count = todos.len().min(MAX_VISIBLE_TODOS);
                for item in &todos[..visible_count] {
                    let (icon, icon_style, text_style) = match item.status {
                        TodoStatus::InProgress => (
                            "  ◼  ",
                            Style::default()
                                .fg(theme::cur().claude)
                                .add_modifier(Modifier::BOLD),
                            Style::default().fg(theme::cur().text),
                        ),
                        TodoStatus::Completed => (
                            "  ✔  ",
                            Style::default().fg(theme::cur().success),
                            Style::default()
                                .fg(theme::cur().inactive)
                                .add_modifier(Modifier::CROSSED_OUT),
                        ),
                        TodoStatus::Pending => (
                            "  ◻  ",
                            Style::default().fg(theme::cur().inactive),
                            Style::default().fg(theme::cur().inactive),
                        ),
                    };
                    let hint = match item.status {
                        TodoStatus::Pending => Some("可开始"),
                        _ => None,
                    };
                    let mut spans = vec![
                        Span::styled(icon, icon_style),
                        Span::styled(item.content.clone(), text_style),
                    ];
                    if let Some(hint) = hint {
                        spans.push(Span::styled(
                            format!(" ({hint})"),
                            Style::default().fg(theme::cur().inactive),
                        ));
                    }
                    lines.push(Line::from(spans));
                }
                if todos.len() > MAX_VISIBLE_TODOS {
                    let summary = format_hidden_todos_summary(&todos[MAX_VISIBLE_TODOS..]);
                    lines.push(Line::from(Span::styled(
                        summary,
                        Style::default().fg(theme::cur().inactive),
                    )));
                }
            }
            lines.push(Line::from(""));
        } else {
            // 非 loading（Brewed/Cogitated 总结行）：
            // 空行分隔后展示 recap 行，避免与总结行贴死
            if let Some(ref recap_text) = app.session_mgr.current().latest_recap {
                lines.push(Line::from(""));
                lines.push(render_recap_line(recap_text, &app.services.lc));
            }
            // 1 个 trailing 空行作为呼吸空间
            lines.push(Line::from(""));
        }
    } else if let Some(ref recap_text) = app.session_mgr.current().latest_recap {
        lines.push(Line::from(""));
        lines.push(render_recap_line(recap_text, &app.services.lc));
        lines.push(Line::from(""));
    }

    lines
}

pub(super) fn visual_line_count(lines: &[Line<'static>], width: u16) -> usize {
    if lines.is_empty() || width == 0 {
        return 0;
    }
    Paragraph::new(Text::from(lines.to_vec()))
        .wrap(Wrap { trim: false })
        .line_count(width)
}

/// 计算 spinner 区域的额外逻辑行数
#[cfg(test)]
fn spinner_extra_count(app: &App) -> u16 {
    let has_recap = app.session_mgr.current().latest_recap.is_some();
    // recap 与总结行之间有一个分隔空行，故占 2 行
    let recap_extra: u16 = if has_recap { 2 } else { 0 };
    if app.session_mgr.current().viewport_loading() {
        // 空行(1) + spinner(1) + trailing(1) = 3，tip 行仅在有已发布 tip（或触发时间覆盖）时 +1；
        // 有 todo 时额外 + 分隔空行(1) + todo 渲染行数（最多 5 行 + 可选截断统计 1 行）
        let turn_elapsed = app.session_mgr.current().viewport_spinner().elapsed_ms();
        let tip_lines: u16 = if app.spinner_tip_visible(turn_elapsed) {
            1
        } else {
            0
        };
        let total_todos = if app.session_mgr.current().active_subagent().is_none() {
            app.session_mgr.current().todo_items.len()
        } else {
            0
        };
        if total_todos > 0 {
            4 + tip_lines + todo_render_line_count(total_todos)
        } else {
            3 + tip_lines
        }
    } else {
        // 空行(1) + spinner(1) + [分隔空行(1) + recap(1)] + trailing(1) = 3 + recap_extra
        3 + recap_extra
    }
}

pub(crate) fn render_recap_line(text: &str, lc: &crate::i18n::LcRegistry) -> Line<'static> {
    // 提示文本由 i18n 提供（不再从 LLM 摘要中 rfind，摘要本身不含该后缀）
    let hint = lc.tr("app-recap-hint");
    let mut spans = vec![
        Span::styled("※ ", Style::default().fg(theme::cur().inactive)),
        Span::styled(
            "recap:",
            Style::default()
                .fg(theme::cur().inactive)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        Span::styled(
            crate::app::tool_display::sanitize_display_text(text.trim()),
            Style::default()
                .fg(theme::cur().inactive)
                .add_modifier(Modifier::ITALIC),
        ),
    ];
    // key 缺失时 format_key 会回退为 key 本身，此时不展示
    if !hint.is_empty() && hint != "app-recap-hint" {
        spans.push(Span::styled(
            format!("  {hint}"),
            Style::default().fg(theme::cur().subtle),
        ));
    }
    Line::from(spans)
}

/// 对一行的 spans 做字符级选区高亮。
/// `char_start` / `char_end` 是该行 plain_text 的字符偏移（非 byte 索引）。
/// 将 spans 中对应范围的字符的 style 追加淡蓝色背景（深色主题选区色），范围外的 span 保持原样。
/// 使用 char_indices() 保证 unicode 安全切割。
pub(crate) fn highlight_line_spans<'a>(
    spans: Vec<Span<'a>>,
    char_start: usize,
    char_end: usize,
) -> Vec<Span<'a>> {
    // wrap 和复制均按字素定位，先统一转换为下面拆 span 使用的 Unicode 字符下标。
    // 在整行上转换，兼容组合字素恰好跨样式 span 的情况。
    let plain_text: String = spans.iter().map(|span| span.content.as_ref()).collect();
    let character_offset = |grapheme_offset: usize| {
        if grapheme_offset == usize::MAX {
            return usize::MAX;
        }
        let byte = plain_text
            .grapheme_indices(true)
            .nth(grapheme_offset)
            .map(|(byte, _)| byte)
            .unwrap_or(plain_text.len());
        plain_text[..byte].chars().count()
    };
    let char_start = character_offset(char_start);
    let char_end = character_offset(char_end);
    let mut result = Vec::new();
    let mut cursor: usize = 0; // 当前在 plain_text 中的字符位置
    for span in spans {
        let span_char_len = span.content.chars().count();
        let span_start = cursor;
        let span_end = cursor + span_char_len;

        if span_end <= char_start || span_start >= char_end {
            // 完全在选区外 → 保持原样
            result.push(span);
        } else if span_start >= char_start && span_end <= char_end {
            // 完全在选区内 → 淡蓝色背景（强制覆盖原有 bg）
            result.push(Span::styled(
                span.content,
                Style {
                    fg: span.style.fg,
                    bg: Some(theme::cur().selection_bg),
                    underline_color: span.style.underline_color,
                    add_modifier: span.style.add_modifier,
                    sub_modifier: span.style.sub_modifier,
                },
            ));
        } else {
            // 部分重叠 → 拆分为 2~3 个子 span
            // 左段（选区外）
            if span_start < char_start {
                let skip = char_start - span_start;
                let byte_cut = span
                    .content
                    .char_indices()
                    .nth(skip)
                    .map(|(i, _)| i)
                    .unwrap_or(span.content.len());
                result.push(Span::styled(
                    span.content[..byte_cut].to_string(),
                    span.style,
                ));
            }
            // 中段（选区内，淡蓝色背景）
            let hl_char_start = span_start.max(char_start) - span_start;
            let hl_char_end = span_end.min(char_end) - span_start;
            let byte_start = span
                .content
                .char_indices()
                .nth(hl_char_start)
                .map(|(i, _)| i)
                .unwrap_or(0);
            let byte_end = span
                .content
                .char_indices()
                .nth(hl_char_end)
                .map(|(i, _)| i)
                .unwrap_or(span.content.len());
            result.push(Span::styled(
                span.content[byte_start..byte_end].to_string(),
                Style {
                    fg: span.style.fg,
                    bg: Some(theme::cur().selection_bg),
                    underline_color: span.style.underline_color,
                    add_modifier: span.style.add_modifier,
                    sub_modifier: span.style.sub_modifier,
                },
            ));
            // 右段（选区外）
            if span_end > char_end {
                let skip = char_end - span_start;
                let byte_cut = span
                    .content
                    .char_indices()
                    .nth(skip)
                    .map(|(i, _)| i)
                    .unwrap_or(span.content.len());
                result.push(Span::styled(
                    span.content[byte_cut..].to_string(),
                    span.style,
                ));
            }
        }
        cursor = span_end;
    }
    result
}

#[cfg(test)]
#[path = "message_area_test.rs"]
mod message_area_test;
