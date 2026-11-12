//! 保序控制事件与视口惰性渲染调度。
use super::*;
use crate::ui::render_layout::{
    LayoutMode, Viewport, VirtualRenderer, AUTO_DETAIL_BYTES, AUTO_DETAIL_MESSAGES,
    AUTO_DETAIL_ROWS,
};
use futures_util::FutureExt;
use std::panic::AssertUnwindSafe;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Strategy {
    Auto,
    Cache,
    Virtual,
}
pub(super) struct RenderWorker {
    task: RenderTask,
    layout: VirtualRenderer,
    strategy: Strategy,
    viewport: Viewport,
    published_viewport: Option<Viewport>,
    latched: bool,
    virtual_active: bool,
    sources: Vec<Arc<MessageViewModel>>,
    resolved_scroll: Option<ScrollAnchor>,
    consumed_scroll_revision: u64,
}
impl RenderWorker {
    pub fn new(task: RenderTask) -> Self {
        Self {
            task,
            layout: VirtualRenderer::default(),
            strategy: Strategy::Auto,
            viewport: Viewport::default(),
            published_viewport: None,
            latched: false,
            virtual_active: false,
            sources: Vec::new(),
            resolved_scroll: None,
            consumed_scroll_revision: 0,
        }
    }
    fn should_virtualize(&self) -> bool {
        self.strategy == Strategy::Virtual || self.strategy == Strategy::Auto && self.latched
    }
    fn clear_legacy_products(&mut self) {
        self.task.message_lines = Vec::new();
        self.task.message_links = Vec::new();
        self.task.message_hashes = Vec::new();
        self.task.alt_lines = Vec::new();
        self.task.alt_links = Vec::new();
        self.task.alt_keys = Vec::new();
        self.task.alt_epochs = Vec::new();
        self.task.line_epochs = Vec::new();
        self.task.wrap_segs = [Vec::new(), Vec::new()];
    }
    async fn publish_virtual(&mut self) {
        let mode = LayoutMode {
            width: self.task.width,
            detail: self.task.detail_mode,
            diff: self.task.diff_visible,
            hook_rows: self.task.hook_rows_visible,
            theme: theme::fingerprint(),
        };
        let result = AssertUnwindSafe(self.layout.render(
            &mut self.sources,
            mode,
            self.viewport,
            current_tool_indicator_tick(),
        ))
        .catch_unwind()
        .await;
        let Ok(mut next) = result else {
            tracing::error!("virtual renderer panicked; retaining the last published frame");
            return;
        };
        if let Some(anchor) = &mut next.scroll_anchor {
            if let Some(resolved) = self
                .resolved_scroll
                .filter(|resolved| resolved.scroll_revision == self.viewport.scroll_revision)
            {
                anchor.prefix_visible = resolved.prefix_visible;
                anchor.follow_tail = resolved.follow_tail;
                self.resolved_scroll = Some(*anchor);
                self.viewport.anchor = Some(anchor.position);
            }
        }
        let old = {
            let mut cache = self.task.cache.write();
            next.version = cache.version.wrapping_add(1);
            std::mem::replace(&mut *cache, next)
        };
        // 大对象析构不占据 UI 读锁。
        drop(old);
        self.published_viewport = Some(self.viewport);
        self.task.notify.notify_one();
    }
    async fn publish_viewport_if_needed(&mut self) {
        if self.virtual_active && self.published_viewport != Some(self.viewport) {
            self.publish_virtual().await;
        }
    }
    async fn rebuild(&mut self, messages: Vec<MessageViewModel>) {
        self.rebuild_with_scroll_anchor(messages, None).await;
    }

    fn reading_anchor(&self) -> Option<(ViewportAnchor, u64)> {
        (!self.viewport.follow_tail)
            .then_some(self.viewport.anchor)
            .flatten()
            .map(|anchor| (anchor, self.viewport.scroll_revision))
    }

    fn resolve_viewport_scroll(&mut self, mut viewport: Viewport) -> Viewport {
        if let Some(resolved) = self
            .resolved_scroll
            .filter(|resolved| resolved.scroll_revision == viewport.scroll_revision)
        {
            viewport.anchor = Some(resolved.position);
            viewport.top = resolved.visual_row;
            viewport.follow_tail = resolved.follow_tail;
            return viewport;
        }
        if let Some(anchor) = viewport.anchor {
            viewport.anchor = Some(self.resolve_scroll_anchor(anchor, viewport.scroll_revision));
        }
        if viewport.scroll_revision > self.consumed_scroll_revision {
            // 没有步骤包装的新版本来自绝对定位，切断此前尚未确认的相对步骤。
            self.consumed_scroll_revision = viewport.scroll_revision;
            if self
                .resolved_scroll
                .is_none_or(|resolved| resolved.scroll_revision != viewport.scroll_revision)
            {
                self.resolved_scroll = None;
            }
        }
        viewport
    }

    fn resolve_scroll_anchor(&mut self, anchor: ViewportAnchor, revision: u64) -> ViewportAnchor {
        if let Some(resolved) = self
            .resolved_scroll
            .filter(|resolved| resolved.scroll_revision == revision)
        {
            return resolved.position;
        }
        if anchor.relative_rows == 0 || !self.virtual_active || self.sources.is_empty() {
            return anchor;
        }
        let position = self.layout.resolve_relative_anchor(
            &mut self.sources,
            anchor,
            current_tool_indicator_tick(),
        );
        self.resolved_scroll = Some(ScrollAnchor {
            position,
            scroll_revision: revision,
            ..Default::default()
        });
        position
    }

    #[allow(clippy::too_many_arguments)]
    async fn apply_relative_scroll(
        &mut self,
        viewport: Viewport,
        base_anchor: ViewportAnchor,
        prefix_rows: usize,
        base_prefix_visible: usize,
        full_height: usize,
        steps: &[(u64, isize)],
    ) -> bool {
        if viewport.scroll_revision < self.viewport.scroll_revision {
            return false;
        }
        let trailing_rows = viewport.trailing_rows;
        let mut runs: Vec<(u64, isize)> = Vec::new();
        for &(revision, delta) in steps
            .iter()
            .filter(|(revision, _)| *revision > self.consumed_scroll_revision)
        {
            if let Some((last_revision, last_delta)) = runs
                .last_mut()
                .filter(|(_, last)| last.signum() == delta.signum())
            {
                *last_revision = revision;
                *last_delta = last_delta.saturating_add(delta);
            } else {
                runs.push((revision, delta));
            }
        }
        if runs.is_empty() {
            let mut viewport = viewport;
            if let Some(resolved) = self
                .resolved_scroll
                .filter(|resolved| resolved.scroll_revision == viewport.scroll_revision)
            {
                viewport.height = full_height.saturating_sub(resolved.prefix_visible).max(1);
            }
            self.viewport = self.resolve_viewport_scroll(viewport);
            return self
                .resolved_scroll
                .is_some_and(|resolved| resolved.scroll_revision == viewport.scroll_revision);
        }
        let continuing = steps.first().is_some_and(|(revision, _)| {
            self.consumed_scroll_revision >= revision.saturating_sub(1)
        });
        let previous = self.resolved_scroll.filter(|_| continuing);
        let mut position = previous.map_or(base_anchor, |resolved| resolved.position);
        let mut prefix_visible = previous
            .map_or(base_prefix_visible, |resolved| resolved.prefix_visible)
            .min(prefix_rows);
        if !self.virtual_active {
            let mut cache = self.task.cache.write();
            let Some(body_top) = cache.visual_row_for_anchor(position) else {
                return false;
            };
            let max_scroll = prefix_rows
                .saturating_add(cache.total_lines)
                .saturating_add(trailing_rows)
                .saturating_sub(full_height);
            let mut offset = if prefix_visible > 0 {
                prefix_rows - prefix_visible
            } else {
                prefix_rows.saturating_add(body_top)
            }
            .min(max_scroll);
            for (revision, delta) in runs {
                offset = offset.saturating_add_signed(delta).min(max_scroll);
                prefix_visible = prefix_rows.saturating_sub(offset);
                let body_top = offset.saturating_sub(prefix_rows);
                position = cache
                    .viewport_anchor_at(body_top)
                    .or_else(|| {
                        let last = cache.total_lines.checked_sub(1)?;
                        let mut anchor = cache.viewport_anchor_at(last)?;
                        anchor.after_body_rows = body_top.saturating_sub(last);
                        Some(anchor)
                    })
                    .unwrap_or(position);
                let resolved = ScrollAnchor {
                    position,
                    visual_row: body_top,
                    scroll_revision: revision,
                    follow_tail: delta > 0 && offset >= max_scroll,
                    prefix_visible,
                };
                self.viewport = Viewport {
                    top: body_top,
                    height: full_height.saturating_sub(prefix_visible).max(1),
                    trailing_rows,
                    follow_tail: resolved.follow_tail,
                    anchor: Some(position),
                    scroll_revision: revision,
                };
                self.resolved_scroll = Some(resolved);
                self.consumed_scroll_revision = revision;
                cache.scroll_anchor = Some(resolved);
            }
            cache.version = cache.version.wrapping_add(1);
            self.task.notify.notify_one();
            return true;
        }
        if self.sources.is_empty() {
            return false;
        }
        let mode = self
            .layout
            .current_mode()
            .expect("已有虚拟窗口必须有布局模式");
        let tick = current_tool_indicator_tick();
        let full_height = full_height.max(1);
        for (revision, delta) in runs {
            let mut distance = delta.unsigned_abs();
            if delta < 0 && prefix_visible > 0 {
                prefix_visible = prefix_visible.saturating_add(distance).min(prefix_rows);
            } else {
                if delta > 0 {
                    let used = distance.min(prefix_visible);
                    prefix_visible -= used;
                    distance -= used;
                }
                if distance > 0 {
                    let signed = isize::try_from(distance).unwrap_or(isize::MAX) * delta.signum();
                    let (next, overflow) = self.layout.resolve_relative_anchor_with_overflow(
                        &mut self.sources,
                        ViewportAnchor {
                            relative_rows: signed,
                            ..position
                        },
                        tick,
                    );
                    position = next;
                    if delta < 0 {
                        prefix_visible = overflow.min(prefix_rows);
                    }
                }
            }
            let mut target = Viewport {
                top: 0,
                height: full_height.saturating_sub(prefix_visible).max(1),
                trailing_rows,
                follow_tail: false,
                anchor: Some(position),
                scroll_revision: revision,
            };
            let mut measured = self
                .layout
                .render(&mut self.sources, mode, target, tick)
                .await;
            // 内容不足一屏时，向下滚也不能把欢迎卡从实际 max_scroll 之外藏掉。
            let visible_floor = full_height
                .saturating_sub(measured.total_lines.saturating_add(trailing_rows))
                .min(prefix_rows);
            if prefix_visible < visible_floor {
                prefix_visible = visible_floor;
                target.height = full_height.saturating_sub(prefix_visible).max(1);
                measured = self
                    .layout
                    .render(&mut self.sources, mode, target, tick)
                    .await;
            }
            if let Some(resolved) = measured.scroll_anchor {
                position = resolved.position;
                target.top = resolved.visual_row;
            }
            let max_scroll = prefix_rows
                .saturating_add(measured.total_lines)
                .saturating_add(trailing_rows)
                .saturating_sub(full_height);
            let offset = if prefix_visible > 0 {
                prefix_rows - prefix_visible
            } else {
                prefix_rows.saturating_add(target.top)
            };
            target.anchor = Some(position);
            target.follow_tail = delta > 0 && offset >= max_scroll;
            self.viewport = target;
            self.consumed_scroll_revision = revision;
            self.resolved_scroll = Some(ScrollAnchor {
                position,
                visual_row: target.top,
                scroll_revision: revision,
                follow_tail: target.follow_tail,
                prefix_visible,
            });
        }
        true
    }

    async fn rebuild_with_scroll_anchor(
        &mut self,
        messages: Vec<MessageViewModel>,
        scroll_anchor: Option<(ViewportAnchor, u64)>,
    ) {
        let scroll_anchor = scroll_anchor
            .filter(|(_, revision)| *revision >= self.viewport.scroll_revision)
            .map(|(anchor, revision)| (self.resolve_scroll_anchor(anchor, revision), revision));
        let replaced = self.layout.sync_messages(&messages);
        if replaced {
            self.latched = false;
            self.viewport.anchor = None;
            self.resolved_scroll = None;
        }
        if let Some((anchor, scroll_revision)) = scroll_anchor {
            if scroll_revision >= self.viewport.scroll_revision {
                self.viewport.anchor = Some(anchor);
                self.viewport.scroll_revision = scroll_revision;
                self.viewport.follow_tail = self.resolved_scroll.is_some_and(|resolved| {
                    resolved.scroll_revision == scroll_revision && resolved.follow_tail
                });
            }
        }
        if self.strategy == Strategy::Auto
            && (self.layout.estimated_detail_rows() >= AUTO_DETAIL_ROWS
                || messages.len() >= AUTO_DETAIL_MESSAGES
                || self.layout.estimated_detail_bytes() > AUTO_DETAIL_BYTES)
        {
            self.latched = true;
        }
        if self.should_virtualize() {
            if !self.virtual_active {
                self.clear_legacy_products();
            }
            let old = std::mem::take(&mut self.sources);
            self.sources = messages
                .into_iter()
                .enumerate()
                .map(|(i, vm)| {
                    old.get(i)
                        .filter(|old| old.content_hash() == vm.content_hash())
                        .map_or_else(|| Arc::new(vm), Arc::clone)
                })
                .collect();
            self.task.last_messages = Vec::new();
            self.virtual_active = true;
            self.publish_virtual().await;
        } else {
            if self.virtual_active {
                self.layout.release_products();
            }
            self.sources = Vec::new();
            self.virtual_active = false;
            self.task.rebuild_safe_with_anchor(
                messages,
                scroll_anchor.or_else(|| self.reading_anchor()),
            );
            self.task.notify.notify_one();
        }
    }
    async fn handle(&mut self, event: RenderEvent, rx: &mut mpsc::Receiver<RenderEvent>) {
        let mut event = event;
        let mut relative_revision = None;
        let event = loop {
            match event {
                RenderEvent::WithDisplay {
                    display,
                    event: inner,
                } => {
                    self.task.apply_display(display);
                    event = *inner;
                }
                RenderEvent::WithViewport {
                    viewport,
                    event: inner,
                } => {
                    if relative_revision != Some(viewport.scroll_revision)
                        && viewport.scroll_revision >= self.viewport.scroll_revision
                    {
                        self.viewport = self.resolve_viewport_scroll(viewport);
                    }
                    event = *inner;
                }
                RenderEvent::WithRelativeScroll {
                    viewport,
                    base_anchor,
                    prefix_rows,
                    base_prefix_visible,
                    full_height,
                    steps,
                    event: inner,
                } => {
                    let applied = self
                        .apply_relative_scroll(
                            viewport,
                            base_anchor,
                            prefix_rows,
                            base_prefix_visible,
                            full_height,
                            &steps,
                        )
                        .await;
                    if applied {
                        relative_revision = Some(viewport.scroll_revision);
                    }
                    event = *inner;
                }
                event => break event,
            }
        };
        if !matches!(&event, RenderEvent::Viewport { scroll_revision, .. } if relative_revision == Some(*scroll_revision))
        {
            self.handle_payload(event, rx).await;
        }
        if !self.virtual_active {
            if let Some(mut resolved) = self
                .resolved_scroll
                .filter(|resolved| resolved.scroll_revision == self.viewport.scroll_revision)
            {
                let mut cache = self.task.cache.write();
                if let Some(row) = cache.visual_row_for_anchor(resolved.position) {
                    resolved.visual_row = row;
                    self.resolved_scroll = Some(resolved);
                    cache.scroll_anchor = Some(resolved);
                }
            }
        }
        // 上下文已接收不代表窗口已发布；同值模式事件也必须兑现新的视口。
        self.publish_viewport_if_needed().await;
    }
    async fn handle_payload(&mut self, event: RenderEvent, rx: &mut mpsc::Receiver<RenderEvent>) {
        match event {
            RenderEvent::PrepareWidth { request, reply } => {
                let restore_viewport = self.virtual_active
                    && self.task.width < request.width
                    && self
                        .task
                        .cache
                        .read()
                        .total_lines
                        .saturating_add(request.extra_rows)
                        <= request.available_rows;
                let viewport = self.viewport;
                if restore_viewport {
                    // 窄宽已能容纳时，全宽重测完整窗口，避免切宽清空实测高度后
                    // 又因旧的局部视口退回过大的估计值。
                    self.viewport = Viewport {
                        top: 0,
                        height: request.available_rows,
                        follow_tail: false,
                        anchor: None,
                        scroll_revision: self.viewport.scroll_revision,
                        trailing_rows: viewport.trailing_rows,
                    };
                }
                self.resize(request.width).await;
                let full_width_rows = self.task.cache.read().total_lines;
                let message_rows = if request.has_messages {
                    full_width_rows
                } else {
                    0
                };
                let width = if request.fullscreen
                    && request.width > 1
                    && message_rows.saturating_add(request.extra_rows) > request.available_rows
                {
                    request.width - 1
                } else {
                    request.width
                };
                self.viewport = viewport;
                self.resize(width).await;
                let _ = reply.send(PreparedWidth {
                    width,
                    full_width_rows,
                    measured_rows: self.task.cache.read().total_lines,
                });
                self.task.notify.notify_one();
            }
            RenderEvent::ResizeAndWait(width, done) => {
                self.resize(width).await;
                let _ = done.send(());
            }
            RenderEvent::SetDetailStrategy(value) => {
                let strategy = match value.as_str() {
                    "cache" => Strategy::Cache,
                    "virtual" => Strategy::Virtual,
                    _ => Strategy::Auto,
                };
                if strategy == self.strategy {
                    return;
                }
                self.strategy = strategy;
                let messages = if self.virtual_active {
                    std::mem::take(&mut self.sources)
                        .into_iter()
                        .map(Arc::unwrap_or_clone)
                        .collect()
                } else {
                    std::mem::take(&mut self.task.last_messages)
                };
                if !messages.is_empty() {
                    self.rebuild(messages).await;
                }
            }
            RenderEvent::Viewport {
                top,
                height,
                trailing_rows,
                follow_tail,
                anchor,
                scroll_revision,
            } => {
                if scroll_revision < self.viewport.scroll_revision {
                    return;
                }
                self.viewport = self.resolve_viewport_scroll(Viewport {
                    top,
                    height,
                    trailing_rows,
                    follow_tail,
                    anchor,
                    scroll_revision,
                });
            }
            RenderEvent::CopySelection { start, end, reply } => {
                if self.virtual_active {
                    self.layout
                        .export_selection(&self.sources, start, end, reply);
                } else {
                    let _ = reply.send(None);
                }
            }
            RenderEvent::Rebuild(messages) => self.rebuild(messages).await,
            RenderEvent::RebuildWithAnchor {
                messages,
                anchor,
                scroll_revision,
            } => {
                self.rebuild_with_scroll_anchor(messages, Some((anchor, scroll_revision)))
                    .await;
            }
            RenderEvent::Clear => {
                self.layout.clear();
                self.resolved_scroll = None;
                self.sources = Vec::new();
                self.latched = false;
                self.virtual_active = false;
                self.published_viewport = None;
                self.consumed_scroll_revision = self.viewport.scroll_revision;
                self.viewport = Viewport {
                    scroll_revision: self.viewport.scroll_revision,
                    ..Viewport::default()
                };
                self.task.handle_event(RenderEvent::Clear, rx);
                self.task.notify.notify_one();
            }
            RenderEvent::Resize(width) => {
                self.resize(width).await;
                self.task.notify.notify_one();
            }
            RenderEvent::ToggleDetail(show) if self.virtual_active => {
                self.task.detail_mode = show;
                self.publish_virtual().await;
            }
            RenderEvent::ToggleDiff(show) if self.virtual_active => {
                self.task.diff_visible = show;
                self.publish_virtual().await;
            }
            RenderEvent::ToggleHookRows(show) if self.virtual_active => {
                self.task.hook_rows_visible = show;
                self.publish_virtual().await;
            }
            event => {
                self.task
                    .handle_event_with_scroll_anchor(event, rx, self.reading_anchor());
                self.task.notify.notify_one();
            }
        }
    }
    async fn resize(&mut self, width: u16) {
        if self.task.width == width && self.task.cache.read().width == width {
            // 宽度准备/同步的完成信号发出前，仍须发布上下文带来的新窗口。
            self.publish_viewport_if_needed().await;
            return;
        }
        if self.virtual_active {
            self.task.width = width;
            self.publish_virtual().await;
        } else {
            self.task
                .resize_with_scroll_anchor(width, self.reading_anchor());
        }
    }
    pub async fn run(mut self, mut rx: mpsc::Receiver<RenderEvent>) {
        let mut pending = None;
        // tick 截止时间必须跨事件保持单调推进。若每轮循环重建 `sleep(200ms)`，
        // 只要事件间隔短于 200ms（流式输出、每帧视口上报），sleep 就会在到期前
        // 被 `rx.recv()` 抢先并丢弃，导致 `Running… (Xs)` 秒数与指示器动画永久
        // 冻结（只靠 Ctrl+O 这类改 detail 标志的事件强制重渲染才会跳一次）。
        let mut tick_deadline: Option<tokio::time::Instant> = None;
        loop {
            let live = if self.virtual_active {
                self.sources
                    .iter()
                    .any(|vm| crate::ui::render_layout::estimator::is_live(vm))
            } else {
                self.task.has_running_tool_blocks()
            };
            if !live {
                tick_deadline = None;
            }
            let event = if let Some(event) = pending.take() {
                Some(event)
            } else if live {
                let deadline = *tick_deadline.get_or_insert_with(|| {
                    tokio::time::Instant::now() + TOOL_INDICATOR_TICK_INTERVAL
                });
                tokio::select! {
                    // 优先处理已到期的 tick：deadline 固定，事件再密也不会被无限推迟。
                    biased;
                    _ = tokio::time::sleep_until(deadline) => {
                        tick_deadline = Some(tokio::time::Instant::now() + TOOL_INDICATOR_TICK_INTERVAL);
                        if self.virtual_active {
                            self.publish_virtual().await;
                        } else {
                            let reading_anchor = self.reading_anchor();
                            if self.task.refresh_running_tool_indicators_with_scroll_anchor(
                                current_tool_indicator_tick(),
                                reading_anchor,
                            ) {
                                self.task.notify.notify_one();
                            }
                        }
                        continue;
                    }
                    event=rx.recv()=>event,
                }
            } else {
                rx.recv().await
            };
            let Some(mut event) = event else {
                break;
            };
            // 只合并连续同类事件。取出的第一条控制事件必须保留，而不是模式匹配失败后丢弃。
            loop {
                if !matches!(event, RenderEvent::Resize(_) | RenderEvent::Viewport { .. }) {
                    break;
                }
                match rx.try_recv() {
                    Ok(next)
                        if matches!(
                            (&event, &next),
                            (RenderEvent::Resize(_), RenderEvent::Resize(_))
                                | (RenderEvent::Viewport { .. }, RenderEvent::Viewport { .. })
                        ) =>
                    {
                        event = next
                    }
                    Ok(next) => {
                        pending = Some(next);
                        break;
                    }
                    Err(_) => break,
                }
            }
            self.handle(event, &mut rx).await;
        }
    }
}

#[cfg(test)]
#[path = "worker_test.rs"]
mod tests;

#[cfg(test)]
#[path = "relative_scroll_test.rs"]
mod relative_scroll_tests;
