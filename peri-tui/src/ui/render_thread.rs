//! 渲染线程：后台执行消息渲染计算，避免阻塞 UI 线程。
//!
//! 通过 hash diff 优化：只重新渲染发生变化的消息行。

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use parking_lot::RwLock;
use ratatui::text::Line;
use tokio::sync::{mpsc, Notify};
use unicode_segmentation::UnicodeSegmentation;

use super::message_view::MessageViewModel;
use super::message_render::render_view_model;
use super::markdown::ensure_rendered_incremental;
use super::markdown::ensure_rendered_flush;

use ratatui::widgets::{Paragraph, Wrap};

/// 渲染缓存：供 UI 线程读取的渲染结果
pub struct RenderCache {
    /// 所有消息渲染后的行（拼接后）
    pub lines: Vec<Line<'static>>,
    /// 每条消息在 lines 中的起始偏移
    pub message_offsets: Vec<usize>,
    /// 总行数（逻辑行）
    pub total_lines: usize,
    /// wrap 信息：每个逻辑行对应的视觉行范围
    pub wrap_map: Vec<WrappedLineInfo>,
    /// 缓存版本号，每次 Rebuild 递增
    pub version: u64,
    /// 当前终端宽度
    pub width: u16,
    /// RebuildWithAnchor 设置的滚动锚点视觉行号
    pub scroll_anchor: Option<u16>,
}

/// 单行的 wrap 信息
#[derive(Debug, Clone)]
pub struct WrappedLineInfo {
    pub line_idx: usize,
    pub visual_row_start: u16,
    pub visual_row_end: u16,
    pub char_widths: Vec<u8>,
    /// 行的纯文本内容（用于文本选择）
    pub plain_text: String,
}

impl RenderCache {
    fn new() -> Self {
        Self {
            lines: Vec::new(),
            message_offsets: Vec::new(),
            total_lines: 0,
            wrap_map: Vec::new(),
            version: 0,
            width: 0,
            scroll_anchor: None,
        }
    }
}

/// 渲染线程通道容量
const RENDER_CHANNEL_CAPACITY: usize = 128;

/// 渲染线程接收的事件
pub enum RenderEvent {
    /// 全量重建消息列表（通过 hash diff 优化渲染）
    Rebuild(Vec<MessageViewModel>),
    /// 全量重建并设置滚动锚点（RebuildAll 后保持滚动位置）
    RebuildWithAnchor {
        messages: Vec<MessageViewModel>,
        /// 锚点对应的消息在旧 view_messages 中的索引
        anchor_message_idx: usize,
    },
    /// 终端宽度变化，渲染线程自动用 last_messages 重建
    Resize(u16),
    /// 清空所有消息
    Clear,
    /// 切换工具调用消息的显示状态
    ToggleToolMessages(bool),
    /// 切换详细模式（强制全量重渲染）
    ToggleDetail(bool),
}

/// 渲染线程，在后台执行渲染计算
///
/// 消息状态由 App 持有（view_messages），渲染线程通过 Rebuild 事件接收完整快照，
/// 通过 hash diff 只重新渲染发生变化的消息，避免不必要的 markdown 解析。
struct RenderTask {
    /// 上一次 Rebuild 收到的消息（Resize 时用于全量重建）
    last_messages: Vec<MessageViewModel>,
    /// 每条消息的渲染行缓存
    message_lines: Vec<Vec<Line<'static>>>,
    /// 每条消息的语义 hash（用于 diff 判断）
    message_hashes: Vec<u64>,
    cache: Arc<RwLock<RenderCache>>,
    notify: Arc<Notify>,
    width: u16,
    show_tool_messages: bool,
    detail_mode: bool,
}

impl RenderTask {
    /// 根据 cache.lines 和当前宽度计算 wrap_map。
    /// 对每个逻辑行使用 ratatui 的 Paragraph::line_count 精确计算视觉行数，
    /// 与实际渲染的 WordWrapper 算法完全一致。
    /// char_widths 使用 grapheme 级别（与 ratatui 一致）。
    fn build_wrap_map(lines: &[Line<'static>], width: u16) -> (usize, Vec<WrappedLineInfo>) {
        if width == 0 || lines.is_empty() {
            return (0, Vec::new());
        }
        let mut wrap_map = Vec::with_capacity(lines.len());
        let mut visual_row: u16 = 0;

        for (idx, line) in lines.iter().enumerate() {
            let plain_text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
            // 使用 grapheme 级别（与 ratatui WordWrapper 一致）
            let char_widths: Vec<u8> = plain_text
                .graphemes(true)
                .map(|g| unicode_width::UnicodeWidthStr::width(g) as u8)
                .collect();

            // 使用 ratatui 的 Paragraph::line_count 精确计算该行的视觉行数
            let visual_count = if plain_text.is_empty() {
                1
            } else {
                let text = ratatui::text::Text::from(line.clone());
                let count = Paragraph::new(text)
                    .wrap(Wrap { trim: false })
                    .line_count(width);
                count.max(1) as u16
            };

            wrap_map.push(WrappedLineInfo {
                line_idx: idx,
                visual_row_start: visual_row,
                visual_row_end: visual_row + visual_count,
                plain_text: plain_text.clone(),
                char_widths,
            });
            visual_row += visual_count;
        }

        (visual_row as usize, wrap_map)
    }

    /// 渲染单条消息（处理 dirty block + markdown 解析 + render_view_model）
    ///
    /// 注意：会修改 vm 的 rendered 字段（确保 rendered 与当前 width 一致），
    /// 因此不会使 content_hash 失效。
    fn render_one(
        vm: &mut MessageViewModel,
        index: usize,
        width: usize,
        detail_mode: bool,
    ) -> Vec<Line<'static>> {
        // 处理 dirty blocks（使用增量解析）
        if let MessageViewModel::AssistantBubble {
            blocks,
            is_streaming,
            ..
        } = vm
        {
            for block in blocks.iter_mut() {
                if *is_streaming {
                    ensure_rendered_incremental(block, width);
                } else {
                    ensure_rendered_flush(block, width);
                }
            }
        }
        // 用实际终端宽度重新解析用户消息的 markdown（初始创建时用默认宽度 80）
        if let MessageViewModel::UserBubble {
            content, rendered, ..
        } = vm
        {
            *rendered = super::markdown::parse_markdown(content, width);
        }

        let mut lines = render_view_model(vm, Some(index), width, detail_mode);
        // 每条消息后追加空行分隔符（包括空内容消息，确保间距一致）
        lines.push(Line::from(""));
        lines
    }

    /// 计算单个 MessageViewModel 的语义 hash（legacy，应使用 vm.content_hash()）
    #[allow(dead_code)]
    fn compute_hash(vm: &MessageViewModel) -> u64 {
        let mut hasher = DefaultHasher::new();
        vm.hash(&mut hasher);
        hasher.finish()
    }

    /// 判断两个消息是否仅存在"外观"差异（不影响渲染输出）。
    ///
    /// 用于跳过无需重新渲染的消息（如 streaming 状态变化但内容未变）。
    fn is_cosmetic_change(old: &MessageViewModel, new: &MessageViewModel) -> bool {
        match (old, new) {
            (
                MessageViewModel::AssistantBubble {
                    content_hash: h1, ..
                },
                MessageViewModel::AssistantBubble {
                    content_hash: h2, ..
                },
            ) => h1 == h2,
            _ => false,
        }
    }

    /// 全量重建：对比 hash 只重渲染变化的消息，拼接所有行并更新缓存
    fn rebuild(&mut self, messages: Vec<MessageViewModel>) {
        let width = self.width as usize;
        let old_last_messages = std::mem::replace(&mut self.last_messages, messages.clone());
        let mut new_hashes = Vec::with_capacity(messages.len());

        // 保存旧的渲染结果用于 hash 对比
        let mut old_message_lines = std::mem::take(&mut self.message_lines);
        let old_hashes = std::mem::take(&mut self.message_hashes);

        // 重新分配 message_lines，长度匹配新消息数
        self.message_lines.resize_with(messages.len(), Vec::new);

        for (i, mut vm) in messages.into_iter().enumerate() {
            let new_hash = vm.content_hash();
            new_hashes.push(new_hash);

            // hash 未变化且无外观差异 → 复用旧渲染
            if i < old_hashes.len()
                && new_hash == old_hashes[i]
                && i < old_message_lines.len()
                && !old_message_lines[i].is_empty()
                && Self::is_cosmetic_change(old_last_messages.get(i).unwrap_or(&vm), &vm)
            {
                self.message_lines[i] = std::mem::take(&mut old_message_lines[i]);
                continue;
            }
            self.message_lines[i] = Self::render_one(&mut vm, i + 1, width, self.detail_mode);
        }

        self.message_hashes = new_hashes;

        // 拼接所有消息行，同时做全局 dedup（消除连续空行），
        // 并在 deduped 索引空间构建 message_offsets。
        // 修复：旧代码先构建 offsets（基于 all_lines），再做 dedup 生成 deduped，
        // 导致 offsets 和 wrap_map 处于不同索引空间。
        let mut deduped: Vec<Line<'static>> = Vec::new();
        let mut offsets: Vec<usize> = Vec::with_capacity(self.message_lines.len());

        for msg_lines in &self.message_lines {
            offsets.push(deduped.len());
            for line in msg_lines {
                // 全局 dedup：跳过紧跟在空行后的空行
                if line.spans.is_empty() && deduped.last().is_some_and(|l: &Line| l.spans.is_empty())
                {
                    continue;
                }
                deduped.push(line.clone());
            }
        }
        // 清除尾部空行
        while deduped.last().is_some_and(|l| {
            l.spans.is_empty() || (l.spans.len() == 1 && l.spans[0].content.is_empty())
        }) {
            deduped.pop();
        }

        let (total_visual_rows, wrap_map) = Self::build_wrap_map(&deduped, self.width);

        {
            let mut cache = self.cache.write();
            cache.lines = deduped;
            cache.message_offsets = offsets;
            cache.total_lines = total_visual_rows;
            cache.wrap_map = wrap_map;
            cache.width = self.width;
            cache.version += 1;
        }
        // 更新 last_messages（用于 Resize 时全量重建）
        // 注意：messages 已被消费，需要从 message_lines 推断
        // 实际上 messages 在循环中被消费了，这里需要重新设计
        // 简单方案：在 rebuild 开始前保存 last_messages
    }

    /// 事件循环主入口
    async fn run(mut self, mut rx: mpsc::Receiver<RenderEvent>) {
        while let Some(event) = rx.recv().await {
            match event {
                RenderEvent::Rebuild(messages) => {
                    self.rebuild(messages);
                }
                RenderEvent::RebuildWithAnchor {
                    messages,
                    anchor_message_idx,
                } => {
                    // 计算锚点行在旧缓存中的视觉行位置
                    let anchor_visual_row = {
                        let cache = self.cache.read();
                        cache
                            .message_offsets
                            .get(anchor_message_idx)
                            .and_then(|&offset| {
                                cache.wrap_map.get(offset).map(|info| info.visual_row_start)
                            })
                            .unwrap_or(0)
                    };
                    self.rebuild(messages);
                    // 写入锚点信息（供 UI 线程恢复滚动位置）
                    {
                        let mut cache = self.cache.write();
                        cache.scroll_anchor = Some(anchor_visual_row);
                    }
                }
                RenderEvent::Resize(width) => {
                    self.width = width;
                    if !self.last_messages.is_empty() {
                        let messages = self.last_messages.clone();
                        self.rebuild(messages);
                    }
                }
                RenderEvent::Clear => {
                    self.last_messages.clear();
                    self.message_lines.clear();
                    self.message_hashes.clear();
                    let mut cache = self.cache.write();
                    cache.lines.clear();
                    cache.message_offsets.clear();
                    cache.total_lines = 0;
                    cache.wrap_map.clear();
                    cache.message_offsets.shrink_to_fit();
                    cache.total_lines = 0;
                    cache.wrap_map = Vec::new();
                    cache.scroll_anchor = None;
                    cache.scroll_anchor = None;
                    cache.version += 1;
                }
                RenderEvent::ToggleToolMessages(show) => {
                    self.show_tool_messages = show;
                    // collapsed 状态是 hash 的一部分，ToggleToolMessages 会改变消息的 hash
                    // 但如果 App 端没有修改 view_messages 中的 collapsed 状态，
                    // 需要 App 发送新的 Rebuild 事件来反映变化
                    // 这里只更新标志位，实际渲染由后续 Rebuild 驱动
                }
                RenderEvent::ToggleDetail(show) => {
                    self.detail_mode = show;
                    // detail_mode 不影响消息的语义 hash，必须清空 hash 缓存
                    // 强制后续 Rebuild 全量重渲染
                    self.message_hashes.clear();
                    if !self.last_messages.is_empty() {
                        let messages = std::mem::take(&mut self.last_messages);
                        self.rebuild(messages);
                    }
                }
            }

            self.notify.notify_one();
        }
    }
}

/// 启动渲染线程，返回事件发送端、共享缓存和通知
///
/// 使用有界 channel（容量 128）：正常使用远达不到上限，极端场景下通过背压限速防止内存膨胀。
/// - 所有事件使用 `try_send()`，通道满时静默丢弃（128 容量在实践中不会达到上限）
/// - Resize 丢弃更安全（下一帧 resize 会补偿，渲染线程有 drain 合并逻辑）
/// - Rebuild 丢弃可接受（下一个 Rebuild 携带完整快照，丢失的是中间状态）
pub fn spawn_render_thread(
    width: u16,
) -> (
    mpsc::Sender<RenderEvent>,
    Arc<RwLock<RenderCache>>,
    Arc<Notify>,
) {
    let (tx, rx) = mpsc::channel(RENDER_CHANNEL_CAPACITY);
    let cache = Arc::new(RwLock::new(RenderCache::new()));
    let notify = Arc::new(Notify::new());

    let task = RenderTask {
        last_messages: Vec::new(),
        message_lines: Vec::new(),
        message_hashes: Vec::new(),
        cache: Arc::clone(&cache),
        notify: Arc::clone(&notify),
        width,
        show_tool_messages: false,
        detail_mode: false,
    };

    tokio::spawn(task.run(rx));

    (tx, cache, notify)
}

#[cfg(test)]
#[path = "render_thread_test.rs"]
mod tests;
