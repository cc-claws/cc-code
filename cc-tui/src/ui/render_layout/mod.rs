//! #629：消息成品按段物化；全局仅保留高度/版本，窗口之外按字节 LRU 淘汰。
use super::{
    message_view::MessageViewModel,
    render_thread::{RenderCache, RenderTask, WrappedLineInfo},
};
use cc_widgets::markdown::LinkHit;
use ratatui::text::Line;
use std::{collections::HashMap, sync::Arc};

pub(super) mod estimator;
mod interaction;

pub const DETAIL_CACHE_BUDGET: usize = 16 * 1024 * 1024;
pub const AUTO_DETAIL_ROWS: usize = 300_000;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ViewportAnchor {
    pub message_index: usize,
    pub logical_line: usize,
    pub row_in_line: usize,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SelectionAnchor {
    pub history_epoch: u64,
    pub message_index: usize,
    pub message_version: u64,
    pub logical_line: usize,
    pub grapheme_offset: usize,
}
#[derive(Clone, Debug, Default)]
pub struct VirtualRenderStats {
    pub resident_segments: usize,
    pub resident_bytes: usize,
    pub budget_bytes: usize,
    pub estimated_segments: usize,
    pub evictions: usize,
    pub largest_segment_bytes: usize,
}
#[derive(Clone, Debug, Default)]
pub struct VirtualLayoutSnapshot {
    pub message_visual_offsets: Arc<Vec<usize>>,
    pub line_origins: Vec<(usize, usize)>,
    pub message_versions: Arc<Vec<u64>>,
    pub history_epoch: u64,
    pub window_start: usize,
    pub requested_top: usize,
    pub requested_height: usize,
    pub diagnostics: VirtualRenderStats,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct LayoutMode {
    pub width: u16,
    pub detail: bool,
    pub diff: bool,
    pub theme: u64,
}
#[derive(Clone, Copy, Debug)]
pub(super) struct Viewport {
    pub top: usize,
    pub height: usize,
    pub follow_tail: bool,
    pub anchor: Option<ViewportAnchor>,
}
impl Default for Viewport {
    fn default() -> Self {
        Self {
            top: 0,
            height: 40,
            follow_tail: true,
            anchor: None,
        }
    }
}

struct MessageMeta {
    version: u64,
    summary: estimator::LayoutSummary,
    measured: Option<usize>,
    prepared_theme: u64,
}
#[derive(Hash, PartialEq, Eq, Clone, Copy)]
struct PayloadKey {
    index: usize,
    version: u64,
    detail: bool,
    diff: bool,
    tick: u64,
    tail: bool,
}
struct SegmentPayload {
    lines: Vec<Line<'static>>,
    origins: Vec<usize>,
    wrap: Vec<WrappedLineInfo>,
    links: Vec<LinkHit>,
    total: usize,
    bytes: usize,
}
struct Resident {
    payload: Arc<SegmentPayload>,
    used_at: u64,
}

/// 点更新/前缀和/视觉位置定位均为 O(log N)，避免滚动扫描历史渲染行。
#[derive(Default)]
struct HeightIndex {
    values: Vec<usize>,
    tree: Vec<usize>,
}
impl HeightIndex {
    fn reset(&mut self, values: Vec<usize>) {
        self.tree = vec![0; values.len() + 1];
        self.values = vec![0; values.len()];
        for (i, v) in values.into_iter().enumerate() {
            self.set(i, v);
        }
    }
    fn set(&mut self, index: usize, value: usize) {
        let old = self.values[index];
        self.values[index] = value;
        let mut i = index + 1;
        while i < self.tree.len() {
            self.tree[i] = self.tree[i].saturating_sub(old).saturating_add(value);
            i += i.isolate_lowest_one();
        }
    }
    fn prefix(&self, end: usize) -> usize {
        let mut i = end.min(self.values.len());
        let mut sum = 0usize;
        while i > 0 {
            sum = sum.saturating_add(self.tree[i]);
            i &= i - 1;
        }
        sum
    }
    fn total(&self) -> usize {
        self.prefix(self.values.len())
    }
    fn locate(&self, row: usize) -> usize {
        let n = self.values.len();
        if n == 0 {
            return 0;
        }
        let mut index = 0;
        let mut sum = 0usize;
        let mut bit = 1usize;
        while bit <= n / 2 {
            bit <<= 1;
        }
        while bit > 0 {
            let next = index + bit;
            if next <= n && sum.saturating_add(self.tree[next]) <= row {
                index = next;
                sum = sum.saturating_add(self.tree[next]);
            }
            bit >>= 1;
        }
        index.min(n - 1)
    }
}

pub(super) struct VirtualRenderer {
    metadata: Vec<MessageMeta>,
    heights: HeightIndex,
    residents: HashMap<PayloadKey, Resident>,
    bytes: usize,
    clock: u64,
    pub history_epoch: u64,
    mode: Option<LayoutMode>,
    tail: usize,
    evictions: usize,
    largest_segment: usize,
    offset_snapshot: Arc<Vec<usize>>,
    version_snapshot: Arc<Vec<u64>>,
    offsets_dirty: bool,
}
impl Default for VirtualRenderer {
    fn default() -> Self {
        Self {
            metadata: Vec::new(),
            heights: HeightIndex::default(),
            residents: HashMap::new(),
            bytes: 0,
            clock: 0,
            history_epoch: 1,
            mode: None,
            tail: 0,
            evictions: 0,
            largest_segment: 0,
            offset_snapshot: Arc::default(),
            version_snapshot: Arc::default(),
            offsets_dirty: true,
        }
    }
}

impl VirtualRenderer {
    pub fn clear(&mut self) {
        let next = self.history_epoch.wrapping_add(1);
        *self = Self::default();
        self.history_epoch = next;
    }
    pub fn release_products(&mut self) {
        self.residents.clear();
        self.bytes = 0;
    }
    pub fn estimated_detail_rows(&self) -> usize {
        self.metadata.iter().map(|m| m.summary.detail_rows).sum()
    }
    pub fn estimated_detail_bytes(&self) -> usize {
        self.metadata.iter().map(|m| m.summary.detail_bytes).sum()
    }
    pub fn sync_messages(&mut self, messages: &[MessageViewModel]) -> bool {
        let replaced = messages.len() < self.metadata.len()
            || self
                .metadata
                .first()
                .zip(messages.first())
                .is_some_and(|(a, b)| a.version != b.content_hash() && messages.len() > 1);
        if replaced {
            self.clear();
        }
        let old_tail = self.tail;
        self.metadata.truncate(messages.len());
        for (i, vm) in messages.iter().enumerate() {
            let version = vm.content_hash();
            if self.metadata.get(i).is_some_and(|m| m.version == version) {
                continue;
            }
            let meta = MessageMeta {
                version,
                summary: estimator::summarize(vm),
                measured: None,
                prepared_theme: 0,
            };
            if i < self.metadata.len() {
                self.metadata[i] = meta;
            } else {
                self.metadata.push(meta);
            }
        }
        self.tail = self
            .metadata
            .iter()
            .rposition(|m| m.summary.visible(self.mode.is_some_and(|m| m.detail)))
            .unwrap_or(0);
        if old_tail != self.tail {
            for index in [old_tail, self.tail] {
                if let Some(meta) = self.metadata.get_mut(index) {
                    meta.measured = None;
                }
            }
        }
        self.residents.retain(|key, _| {
            self.metadata
                .get(key.index)
                .is_some_and(|m| m.version == key.version)
                && key.tail == (key.index >= self.tail)
        });
        self.bytes = self.residents.values().map(|r| r.payload.bytes).sum();
        self.version_snapshot = Arc::new(self.metadata.iter().map(|m| m.version).collect());
        if let Some(mode) = self.mode {
            self.reset_heights(mode);
        }
        replaced
    }
    fn reset_heights(&mut self, mode: LayoutMode) {
        self.tail = self
            .metadata
            .iter()
            .rposition(|m| m.summary.visible(mode.detail))
            .unwrap_or(0);
        let values = self
            .metadata
            .iter()
            .enumerate()
            .map(|(i, m)| {
                m.measured
                    .unwrap_or_else(|| m.summary.height(mode.width, mode.detail, i == 0))
            })
            .collect();
        self.heights.reset(values);
        self.offsets_dirty = true;
    }
    fn set_mode(&mut self, mode: LayoutMode) {
        if self.mode == Some(mode) {
            return;
        }
        // 模式改变仅使布局代际失效；滚动不改变代际。
        self.history_epoch = self.history_epoch.wrapping_add(1);
        // detail/diff 的成品由 key 区分，共用同一个字节预算；热切换无需重渲染。
        // 只有宽度/主题变化会使所有成品失效。
        if self
            .mode
            .is_none_or(|old| old.width != mode.width || old.theme != mode.theme)
        {
            self.release_products();
        }
        for meta in &mut self.metadata {
            meta.measured = None;
        }
        self.mode = Some(mode);
        self.reset_heights(mode);
    }
    fn key(&self, index: usize, messages: &[Arc<MessageViewModel>], tick: u64) -> PayloadKey {
        let mode = self.mode.expect("布局模式必须先初始化");
        PayloadKey {
            index,
            version: self.metadata[index].version,
            detail: mode.detail,
            diff: mode.diff,
            tick: if estimator::is_live(&messages[index]) {
                tick
            } else {
                0
            },
            tail: index >= self.tail,
        }
    }
    fn payload(
        &mut self,
        index: usize,
        messages: &mut [Arc<MessageViewModel>],
        tick: u64,
    ) -> Arc<SegmentPayload> {
        self.clock = self.clock.wrapping_add(1);
        let key = self.key(index, messages, tick);
        if let Some(resident) = self.residents.get_mut(&key) {
            resident.used_at = self.clock;
            let payload = Arc::clone(&resident.payload);
            self.metadata[index].measured = Some(payload.total);
            if self.heights.values[index] != payload.total {
                self.offsets_dirty = true;
            }
            self.heights.set(index, payload.total);
            return payload;
        }
        let mode = self.mode.expect("布局模式必须先初始化");
        // VM 已由 worker 独占；只渲染目标消息，不克隆整段历史。
        let vm = Arc::make_mut(&mut messages[index]);
        if self.metadata[index].prepared_theme != mode.theme {
            // 内容 hash 不包含预解析样式。旧 Arc 可跨 Rebuild 复用，但主题换代后
            // 必须惰性使实际进入视口的 Text 快照失效，不能沿用旧调色板。
            if let MessageViewModel::AssistantBubble { blocks, .. } = vm {
                for block in blocks {
                    if let super::message_view::ContentBlockView::Text { rendered_width, .. } =
                        block
                    {
                        *rendered_width = 0;
                    }
                }
            }
            self.metadata[index].prepared_theme = mode.theme;
        }
        let payload = Arc::new(Self::render_payload(vm, index, mode, self.tail));
        self.largest_segment = self.largest_segment.max(payload.bytes);
        self.metadata[index].measured = Some(payload.total);
        if self.heights.values[index] != payload.total {
            self.offsets_dirty = true;
        }
        self.heights.set(index, payload.total);
        self.bytes = self.bytes.saturating_add(payload.bytes);
        self.residents.insert(
            key,
            Resident {
                payload: Arc::clone(&payload),
                used_at: self.clock,
            },
        );
        payload
    }
    fn render_payload(
        vm: &mut MessageViewModel,
        index: usize,
        mode: LayoutMode,
        tail: usize,
    ) -> SegmentPayload {
        let (raw, raw_links) =
            RenderTask::render_one(vm, index + 1, mode.width as usize, mode.diff, mode.detail);
        let mut lines = Vec::new();
        let mut origins = Vec::new();
        let mut previous_empty = index > 0;
        let mut line_map = vec![None; raw.len()];
        for (source, line) in raw.into_iter().enumerate() {
            let empty = super::render_thread::is_blank_line(&line);
            if empty && previous_empty {
                continue;
            }
            previous_empty = empty;
            line_map[source] = Some(lines.len());
            origins.push(source);
            lines.push(line);
        }
        if index >= tail {
            while lines
                .last()
                .is_some_and(super::render_thread::is_blank_line)
            {
                lines.pop();
                origins.pop();
            }
        }
        let links: Vec<_> = raw_links
            .into_iter()
            .filter_map(|mut hit| {
                let line = line_map.get(hit.line).copied().flatten()?;
                if line >= lines.len() {
                    return None;
                }
                hit.line = line;
                Some(hit)
            })
            .collect();
        let (total, wrap) = RenderTask::build_wrap_map(&lines, mode.width);
        let mut bytes = lines.capacity() * std::mem::size_of::<Line<'static>>()
            + wrap.capacity() * std::mem::size_of::<WrappedLineInfo>()
            + origins.capacity() * std::mem::size_of::<usize>()
            + links.capacity() * std::mem::size_of::<LinkHit>();
        for line in &lines {
            bytes += line.spans.capacity() * std::mem::size_of::<ratatui::text::Span<'static>>();
            for span in &line.spans {
                if let std::borrow::Cow::Owned(s) = &span.content {
                    bytes += s.capacity();
                }
            }
        }
        for entry in &wrap {
            bytes += entry.plain_text.capacity() + entry.char_widths.capacity();
        }
        bytes += links.iter().map(|hit| hit.url.capacity()).sum::<usize>();
        SegmentPayload {
            lines,
            origins,
            wrap,
            links,
            total,
            bytes,
        }
    }
    fn evict(&mut self, pinned: &[PayloadKey]) {
        while self.bytes > DETAIL_CACHE_BUDGET {
            let victim = self
                .residents
                .iter()
                .filter(|(k, _)| !pinned.contains(k))
                .min_by_key(|(_, v)| v.used_at)
                .map(|(k, _)| *k);
            let Some(victim) = victim else {
                break;
            };
            if let Some(value) = self.residents.remove(&victim) {
                self.bytes = self.bytes.saturating_sub(value.payload.bytes);
                self.evictions += 1;
            }
        }
    }
    pub async fn render(
        &mut self,
        messages: &mut [Arc<MessageViewModel>],
        mode: LayoutMode,
        viewport: Viewport,
        tick: u64,
    ) -> RenderCache {
        self.set_mode(mode);
        let mut output = RenderCache::new();
        output.width = mode.width;
        if messages.is_empty() {
            return output;
        }
        if viewport.follow_tail {
            // 只有读取尾窗才物化尾部；阅读头部不能顺带渲染离屏巨型尾消息。
            loop {
                let payload = self.payload(self.tail, messages, tick);
                if payload.total > 0 || self.tail == 0 {
                    break;
                }
                self.tail -= 1;
                self.metadata[self.tail].measured = None;
            }
        }
        let height = viewport.height.max(1);
        let mut anchor = viewport.anchor.filter(|a| a.message_index < messages.len());
        if !viewport.follow_tail && anchor.is_none() {
            let i = self.heights.locate(viewport.top);
            let local = viewport.top.saturating_sub(self.heights.prefix(i));
            let payload = self.payload(i, messages, tick);
            let line = payload
                .wrap
                .partition_point(|w| w.visual_row_end <= local)
                .min(payload.wrap.len().saturating_sub(1));
            if let Some(w) = payload.wrap.get(line) {
                anchor = Some(ViewportAnchor {
                    message_index: i,
                    logical_line: payload.origins[line],
                    row_in_line: local.saturating_sub(w.visual_row_start),
                });
            }
        }
        if let Some(a) = anchor {
            self.payload(a.message_index, messages, tick);
        }
        // 先固定实际可见内容；估算只参与定位，不能作为成品发布。
        let mut first = 0usize;
        let mut last = 0usize;
        let mut top = 0usize;
        for _ in 0..messages.len().saturating_add(2) {
            top = if viewport.follow_tail {
                self.heights.total().saturating_sub(height)
            } else if let Some(a) = anchor {
                let p = self.payload(a.message_index, messages, tick);
                let local = p
                    .origins
                    .partition_point(|line| *line < a.logical_line)
                    .min(p.wrap.len().saturating_sub(1));
                self.heights.prefix(a.message_index)
                    + p.wrap.get(local).map_or(0, |w| {
                        w.visual_row_start
                            + a.row_in_line
                                .min(w.visual_row_end.saturating_sub(w.visual_row_start + 1))
                    })
            } else {
                viewport
                    .top
                    .min(self.heights.total().saturating_sub(height))
            };
            first = self.heights.locate(top);
            last = first;
            let before = self.heights.total();
            loop {
                self.payload(last, messages, tick);
                if self.heights.prefix(last + 1) >= top.saturating_add(height)
                    || last + 1 == messages.len()
                {
                    break;
                }
                last += 1;
                tokio::task::yield_now().await;
            }
            // 非 follow 视口也可能覆盖末尾隐藏组。仅利用已实测的零高度回退尾条，
            // 让下一轮重新裁剪真实尾空行，不为确定全局高度而预渲染离屏历史。
            let old_tail = self.tail;
            while self.tail > 0 && self.metadata[self.tail].measured == Some(0) {
                self.tail -= 1;
            }
            let tail_changed = old_tail != self.tail;
            if tail_changed {
                self.metadata[self.tail].measured = None;
            }
            let new_first = if viewport.follow_tail {
                self.heights
                    .locate(self.heights.total().saturating_sub(height))
            } else {
                self.heights.locate(top)
            };
            if !tail_changed && before == self.heights.total() && new_first == first {
                break;
            }
            tokio::task::yield_now().await;
        }
        let mut pinned = Vec::new();
        let mut origins = Vec::new();
        for i in first..=last {
            let payload = self.payload(i, messages, tick);
            pinned.push(self.key(i, messages, tick));
            let line_base = output.lines.len();
            let visual_base = self.heights.prefix(i);
            output.lines.extend(payload.lines.iter().cloned());
            origins.extend(payload.origins.iter().map(|line| (i, *line)));
            output.wrap_map.extend(payload.wrap.iter().map(|w| {
                let mut w = w.clone();
                w.line_idx += line_base;
                w.visual_row_start += visual_base;
                w.visual_row_end += visual_base;
                w
            }));
            output.links.extend(payload.links.iter().map(|hit| {
                let mut hit = hit.clone();
                hit.line += line_base;
                hit
            }));
        }
        self.evict(&pinned);
        output.total_lines = self.heights.total();
        output.scroll_anchor = Some(top.min(output.total_lines.saturating_sub(height)));
        // 兼容字段仅含窗口局部下标，所有全局定位必须使用 snapshot 的视觉前缀。
        if self.offsets_dirty {
            let mut offset = 0usize;
            self.offset_snapshot = Arc::new(
                self.heights
                    .values
                    .iter()
                    .map(|height| {
                        let start = offset;
                        offset = offset.saturating_add(*height);
                        start
                    })
                    .collect(),
            );
            self.offsets_dirty = false;
        }
        output.virtual_layout = Some(Arc::new(VirtualLayoutSnapshot {
            requested_top: viewport.top,
            requested_height: viewport.height,
            message_visual_offsets: Arc::clone(&self.offset_snapshot),
            line_origins: origins,
            message_versions: Arc::clone(&self.version_snapshot),
            history_epoch: self.history_epoch,
            window_start: self.heights.prefix(first),
            diagnostics: VirtualRenderStats {
                resident_segments: self.residents.len(),
                resident_bytes: self.bytes,
                budget_bytes: DETAIL_CACHE_BUDGET,
                estimated_segments: self
                    .metadata
                    .iter()
                    .filter(|m| m.measured.is_none())
                    .count(),
                evictions: self.evictions,
                largest_segment_bytes: self.largest_segment,
            },
        }));
        output
    }
    pub fn export_selection(
        &self,
        messages: &[Arc<MessageViewModel>],
        mut start: SelectionAnchor,
        mut end: SelectionAnchor,
        reply: tokio::sync::oneshot::Sender<Option<String>>,
    ) {
        if reply.is_closed() {
            return;
        }
        if start.history_epoch != self.history_epoch || end.history_epoch != self.history_epoch {
            let _ = reply.send(None);
            return;
        }
        if (
            start.message_index,
            start.logical_line,
            start.grapheme_offset,
        ) > (end.message_index, end.logical_line, end.grapheme_offset)
        {
            std::mem::swap(&mut start, &mut end);
        }
        let valid = [start, end].iter().all(|a| {
            messages
                .get(a.message_index)
                .is_some_and(|m| m.content_hash() == a.message_version)
        });
        if !valid {
            let _ = reply.send(None);
            return;
        }
        let Some(mode) = self.mode else {
            let _ = reply.send(None);
            return;
        };
        let tail = self.tail;
        let frozen: Vec<_> = messages[start.message_index..=end.message_index].to_vec();
        // 用户导出独立快照；慢选区不占据渲染 worker，最终字符串不计为缓存。
        tokio::task::spawn_blocking(move || {
            use unicode_segmentation::UnicodeSegmentation;
            let mut text = String::new();
            let mut has_line = false;
            for (offset, mut vm) in frozen.into_iter().enumerate() {
                if reply.is_closed() {
                    return;
                }
                let i = start.message_index + offset;
                let payload = Self::render_payload(Arc::make_mut(&mut vm), i, mode, tail);
                for (local, source) in payload.origins.iter().enumerate() {
                    if i == start.message_index && *source < start.logical_line
                        || i == end.message_index && *source > end.logical_line
                    {
                        continue;
                    }
                    let raw = &payload.wrap[local].plain_text;
                    let from = if i == start.message_index && *source == start.logical_line {
                        start.grapheme_offset
                    } else {
                        0
                    };
                    let to = if i == end.message_index && *source == end.logical_line {
                        end.grapheme_offset
                    } else {
                        usize::MAX
                    };
                    if has_line {
                        text.push('\n');
                    }
                    has_line = true;
                    for g in raw.graphemes(true).skip(from).take(to.saturating_sub(from)) {
                        text.push_str(g);
                    }
                }
            }
            let _ = reply.send(Some(text));
        });
    }
}

#[cfg(test)]
#[path = "layout_test.rs"]
mod tests;
