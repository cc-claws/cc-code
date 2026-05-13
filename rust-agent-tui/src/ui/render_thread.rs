use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use parking_lot::RwLock;
use ratatui::text::Line;
use ratatui::widgets::{Paragraph, Wrap};
use tokio::sync::{mpsc, Notify};
use unicode_segmentation::UnicodeSegmentation;

use super::markdown::ensure_rendered_incremental;
use super::message_render::render_view_model;
use super::message_view::MessageViewModel;

/// 单个逻辑行的换行映射信息
#[derive(Debug, Clone)]
pub struct WrappedLineInfo {
    /// 该行在 cache.lines 中的索引
    pub line_idx: usize,
    /// 该逻辑行渲染后的起始视觉行号（基于 0）
    pub visual_row_start: u16,
    /// 该逻辑行渲染后的结束视觉行号（不含）
    pub visual_row_end: u16,
    /// 该逻辑行的纯文本内容（去样式，用于复制）
    pub plain_text: String,
    /// 每个字符的显示宽度序列（ASCII=1, CJK=2）
    pub char_widths: Vec<u8>,
}

/// 渲染缓存，由渲染线程写入、UI 线程读取
pub struct RenderCache {
    /// 所有消息渲染后的行
    pub lines: Vec<Line<'static>>,
    /// 每条消息在 lines 中的起始行索引（用于定位）
    pub message_offsets: Vec<usize>,
    /// 总行数（已考虑 wrap 换行后的真实视觉行数）
    pub total_lines: usize,
    /// 版本号，UI 线程比较是否有变化以决定是否重绘
    pub version: u64,
    pub wrap_map: Vec<WrappedLineInfo>,
    /// 当前渲染使用的宽度（= text_area.width，已减去滚动条 1 列）
    pub width: u16,
    /// RebuildAll 后的滚动锚点（视觉行号），UI 线程读取后清除
    pub scroll_anchor: Option<usize>,
}

impl Default for RenderCache {
    fn default() -> Self {
        Self::new()
    }
}

impl RenderCache {
    pub fn new() -> Self {
        Self {
            lines: Vec::new(),
            message_offsets: Vec::new(),
            total_lines: 0,
            version: 0,
            wrap_map: Vec::new(),
            width: 0,
            scroll_anchor: None,
        }
    }

    /// 计算给定 lines 在指定宽度下 wrap 后的真实视觉行数。
    /// 使用 ratatui 的 Paragraph::line_count 与 Wrap{trim:false} 确保与实际渲染一致。
    fn compute_wrapped_height(lines: &[Line<'static>], width: u16) -> usize {
        if width == 0 || lines.is_empty() {
            return 0;
        }
        let text = ratatui::text::Text::from(lines.to_vec());
        Paragraph::new(text)
            .wrap(Wrap { trim: false })
            .line_count(width)
    }
}

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
}

impl RenderTask {
    /// 根据 cache.lines 和当前宽度计算 wrap_map。
    /// 对每个逻辑行使用 ratatui 的 Paragraph::line_count 精确计算视觉行数，
    /// 与实际渲染的 WordWrapper 算法完全一致。
    /// char_widths 使用 grapheme 级别（与 ratatui 一致）。
    fn build_wrap_map(lines: &[Line<'static>], width: u16) -> Vec<WrappedLineInfo> {
        if width == 0 || lines.is_empty() {
            return Vec::new();
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
                plain_text,
                char_widths,
            });
            visual_row += visual_count;
        }
        wrap_map
    }

    /// 渲染单条消息为 lines（含前后空行分隔）
    fn render_one(vm: &mut MessageViewModel, index: usize, width: usize) -> Vec<Line<'static>> {
        // 处理 dirty blocks（使用增量解析）
        if let MessageViewModel::AssistantBubble { blocks, .. } = vm {
            for block in blocks.iter_mut() {
                ensure_rendered_incremental(block, width);
            }
        }
        // 用实际终端宽度重新解析用户消息的 markdown（初始创建时用默认宽度 80）
        if let MessageViewModel::UserBubble {
            content, rendered, ..
        } = vm
        {
            *rendered = super::markdown::parse_markdown(content, width);
        }

        let mut lines = render_view_model(vm, Some(index), width);
        // 每条消息后追加空行分隔符（包括空内容消息，确保间距一致）
        lines.push(Line::from(""));
        lines
    }

    /// 计算单个 MessageViewModel 的语义 hash
    fn compute_hash(vm: &MessageViewModel) -> u64 {
        let mut hasher = DefaultHasher::new();
        vm.hash(&mut hasher);
        hasher.finish()
    }

    /// 判断两个消息是否仅存在"外观"差异（不影响渲染输出）。
    ///
    /// cosmetic change 的消息可以安全复用旧的渲染缓存，避免不必要的重渲染。
    fn is_cosmetic_change(old: &MessageViewModel, new: &MessageViewModel) -> bool {
        match (old, new) {
            // AssistantBubble: blocks 内容相同，仅 is_streaming 变化
            (
                MessageViewModel::AssistantBubble {
                    blocks: old_blocks,
                    collapsed: old_collapsed,
                    ..
                },
                MessageViewModel::AssistantBubble {
                    blocks: new_blocks,
                    collapsed: new_collapsed,
                    ..
                },
            ) => old_blocks == new_blocks && old_collapsed == new_collapsed,

            // SubAgentGroup: recent_messages + final_result 相同，仅 is_running 变化
            (
                MessageViewModel::SubAgentGroup {
                    agent_id: old_id,
                    task_preview: old_preview,
                    total_steps: old_steps,
                    recent_messages: old_msgs,
                    final_result: old_result,
                    is_error: old_err,
                    is_background: old_bg,
                    bg_hash: old_hash,
                    collapsed: old_collapsed,
                    batch_agents: old_batch,
                    ..
                },
                MessageViewModel::SubAgentGroup {
                    agent_id: new_id,
                    task_preview: new_preview,
                    total_steps: new_steps,
                    recent_messages: new_msgs,
                    final_result: new_result,
                    is_error: new_err,
                    is_background: new_bg,
                    bg_hash: new_hash,
                    collapsed: new_collapsed,
                    batch_agents: new_batch,
                    ..
                },
            ) => {
                old_id == new_id
                    && old_preview == new_preview
                    && old_steps == new_steps
                    && old_msgs == new_msgs
                    && old_result == new_result
                    && old_err == new_err
                    && old_bg == new_bg
                    && old_hash == new_hash
                    && old_collapsed == new_collapsed
                    && old_batch == new_batch
            }

            // 其他类型变化都不是 cosmetic
            _ => false,
        }
    }

    /// 全量重建：接收完整消息列表，通过 prefix_stable_len 优化渲染
    ///
    /// 优化策略：
    /// 1. 计算前缀稳定长度（连续 hash 未变的消息数量）
    /// 2. 前缀消息直接复用 message_lines 缓存，跳过渲染
    /// 3. 只从变化点开始重新渲染后续消息
    /// 4. 对 hash 不同但属于 cosmetic change 的消息也复用缓存
    fn rebuild(&mut self, messages: Vec<MessageViewModel>) {
        let width = self.width as usize;
        let new_len = messages.len();

        // 保留一份用于 Resize（Resize 时没有新的 Rebuild 事件）
        let old_messages = std::mem::replace(&mut self.last_messages, messages.clone());

        // 在渲染前计算 hash（render_one 会修改 dirty 等字段）
        let new_hashes: Vec<u64> = messages.iter().map(Self::compute_hash).collect();

        // 计算 prefix_stable_len：前缀中连续 hash 未变的消息数量
        let old_len = self.message_hashes.len();
        let prefix_stable_len = new_hashes
            .iter()
            .zip(self.message_hashes.iter())
            .position(|(new_h, old_h)| new_h != old_h)
            .unwrap_or_else(|| old_len.min(new_len));

        // 保存旧的 message_lines（用于 cosmetic change 复用）
        let mut old_message_lines = std::mem::take(&mut self.message_lines);

        // 调整 message_lines 容量
        self.message_lines.resize(new_len, Vec::new());

        // 复用前缀的缓存行
        for i in 0..prefix_stable_len {
            if i < old_message_lines.len() {
                self.message_lines[i] = std::mem::take(&mut old_message_lines[i]);
            }
        }
        // 复用 prefix_stable_len 之前、未被复用的旧行（这些 hash 未变但之前渲染过）
        // 这些已经通过上面的循环处理了

        // 从 prefix_stable_len 开始渲染变化的消息
        for (i, mut vm) in messages.into_iter().enumerate() {
            if i < prefix_stable_len {
                // 前缀稳定区，已复用缓存
                continue;
            }
            // 对 hash 不同但属于 cosmetic change 的消息复用旧缓存
            if i < old_message_lines.len()
                && i < old_len
                && Self::is_cosmetic_change(&old_messages[i], &vm)
            {
                self.message_lines[i] = std::mem::take(&mut old_message_lines[i]);
                continue;
            }
            self.message_lines[i] = Self::render_one(&mut vm, i + 1, width);
        }

        self.message_hashes = new_hashes;

        // 拼接所有消息行
        let mut all_lines: Vec<Line<'static>> = Vec::new();
        let mut offsets: Vec<usize> = Vec::new();
        for lines in &self.message_lines {
            offsets.push(all_lines.len());
            all_lines.extend(lines.iter().cloned());
        }

        // 过滤连续空行，保留单个空行作为消息分隔符
        let mut deduped: Vec<Line<'static>> = Vec::with_capacity(all_lines.len());
        let mut prev_empty = false;
        for line in all_lines {
            let is_empty = line.spans.is_empty()
                || (line.spans.len() == 1 && line.spans[0].content.is_empty());
            if is_empty && prev_empty {
                continue;
            }
            prev_empty = is_empty;
            deduped.push(line);
        }
        // 移除末尾多余空行
        while deduped.last().is_some_and(|l| {
            l.spans.is_empty() || (l.spans.len() == 1 && l.spans[0].content.is_empty())
        }) {
            deduped.pop();
        }

        let render_width = self.width;
        let mut cache = self.cache.write();
        cache.lines = deduped;
        cache.message_offsets = offsets;
        cache.total_lines = RenderCache::compute_wrapped_height(&cache.lines, render_width);
        cache.wrap_map = Self::build_wrap_map(&cache.lines, self.width);
        cache.width = self.width;
        cache.version += 1;
    }

    /// 主事件循环
    async fn run(mut self, mut rx: mpsc::UnboundedReceiver<RenderEvent>) {
        while let Some(event) = rx.recv().await {
            match event {
                RenderEvent::Rebuild(messages) => {
                    self.rebuild(messages);
                }
                RenderEvent::RebuildWithAnchor {
                    messages,
                    anchor_message_idx,
                } => {
                    self.rebuild(messages);
                    // 计算锚点消息在新布局中的视觉行起始位置
                    let anchor_visual_row =
                        if anchor_message_idx < self.cache.read().message_offsets.len() {
                            let cache = self.cache.read();
                            let line_idx = cache.message_offsets[anchor_message_idx];
                            if line_idx < cache.wrap_map.len() {
                                Some(cache.wrap_map[line_idx].visual_row_start as usize)
                            } else {
                                None
                            }
                        } else {
                            None
                        };
                    if let Some(row) = anchor_visual_row {
                        self.cache.write().scroll_anchor = Some(row);
                    }
                }
                RenderEvent::Resize(new_width) => {
                    self.width = new_width;
                    // 清空 hash 缓存，强制全量重渲染
                    self.message_hashes.clear();
                    // 用 last_messages 重新渲染（宽度变化需要重新计算所有行的 wrap）
                    if !self.last_messages.is_empty() {
                        let messages = std::mem::take(&mut self.last_messages);
                        self.rebuild(messages);
                    }
                }
                RenderEvent::Clear => {
                    self.message_lines.clear();
                    self.message_hashes.clear();
                    let mut cache = self.cache.write();
                    cache.lines.clear();
                    cache.message_offsets.clear();
                    cache.total_lines = 0;
                    cache.wrap_map = Vec::new();
                    cache.version += 1;
                }
                RenderEvent::ToggleToolMessages(show) => {
                    self.show_tool_messages = show;
                    // collapsed 状态是 hash 的一部分，ToggleToolMessages 会改变消息的 hash
                    // 但如果 App 端没有修改 view_messages 中的 collapsed 状态，
                    // 需要 App 发送新的 Rebuild 事件来反映变化
                    // 这里只更新标志位，实际渲染由后续 Rebuild 驱动
                }
            }

            self.notify.notify_one();
        }
    }
}

/// 启动渲染线程，返回事件发送端、共享缓存和通知
///
/// 使用无界 channel：渲染事件处理耗时微秒级，不会积压；
/// 有界 channel 的 try_send 静默丢弃会导致渲染线程与 App 状态分叉。
pub fn spawn_render_thread(
    width: u16,
) -> (
    mpsc::UnboundedSender<RenderEvent>,
    Arc<RwLock<RenderCache>>,
    Arc<Notify>,
) {
    let (tx, rx) = mpsc::unbounded_channel();
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
    };

    tokio::spawn(task.run(rx));

    (tx, cache, notify)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_rebuild_increments_version() {
        let (tx, cache, _notify) = spawn_render_thread(80);

        assert_eq!(cache.read().version, 0);

        tx.send(RenderEvent::Rebuild(vec![MessageViewModel::user(
            "Hello".to_string(),
        )]))
        .unwrap();

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let c = cache.read();
        assert!(c.version > 0, "version should increment after Rebuild");
        assert!(
            !c.lines.is_empty(),
            "lines should not be empty after Rebuild"
        );
    }

    #[tokio::test]
    async fn test_rebuild_hash_diff_skips_unchanged() {
        let (tx, cache, _notify) = spawn_render_thread(80);

        // 第一次 Rebuild：渲染两条消息
        let user1 = MessageViewModel::user("First".to_string());
        let user2 = MessageViewModel::user("Second".to_string());
        tx.send(RenderEvent::Rebuild(vec![user1.clone(), user2.clone()]))
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let v1 = cache.read().version;
        let lines_v1 = cache.read().lines.len();

        // 第二次 Rebuild：相同内容，hash diff 应跳过渲染
        tx.send(RenderEvent::Rebuild(vec![user1, user2])).unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let c = cache.read();
        // version 仍应递增（即使内容不变）
        assert!(c.version > v1, "version should still increment");
        // 行数不变
        assert_eq!(c.lines.len(), lines_v1, "lines count should be the same");
    }

    #[tokio::test]
    async fn test_rebuild_no_trailing_blank() {
        let (tx, cache, _notify) = spawn_render_thread(80);

        tx.send(RenderEvent::Rebuild(vec![MessageViewModel::user(
            "Hello".to_string(),
        )]))
        .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let c = cache.read();
        let last_is_empty = c.lines.last().is_some_and(|l| {
            l.spans.is_empty() || (l.spans.len() == 1 && l.spans[0].content.is_empty())
        });
        assert!(!last_is_empty, "should not have trailing blank line");
    }

    #[tokio::test]
    async fn test_rebuild_multiple_messages_have_gaps() {
        let (tx, cache, _notify) = spawn_render_thread(80);

        tx.send(RenderEvent::Rebuild(vec![
            MessageViewModel::user("First message".to_string()),
            MessageViewModel::user("Second message".to_string()),
        ]))
        .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let c = cache.read();
        // 找 "Second message" 的行，检查前一行是否为空行
        let mut second_msg_idx = None;
        for (i, line) in c.lines.iter().enumerate() {
            for span in &line.spans {
                if span.content.contains("Second message") {
                    second_msg_idx = Some(i);
                    break;
                }
            }
            if second_msg_idx.is_some() {
                break;
            }
        }
        let idx = second_msg_idx.expect("should find second user message");
        assert!(idx > 0, "second message should not be the first line");
        let prev_is_empty = c.lines[idx - 1].spans.is_empty()
            || (c.lines[idx - 1].spans.len() == 1 && c.lines[idx - 1].spans[0].content.is_empty());
        assert!(
            prev_is_empty,
            "should have blank line before second user message, but line {} is: {:?}",
            idx - 1,
            c.lines[idx - 1]
        );
    }

    #[tokio::test]
    async fn test_rebuild_with_anchor_sets_scroll_anchor() {
        let (tx, cache, _notify) = spawn_render_thread(80);

        tx.send(RenderEvent::RebuildWithAnchor {
            messages: vec![
                MessageViewModel::user("First".to_string()),
                MessageViewModel::user("Second".to_string()),
            ],
            anchor_message_idx: 1,
        })
        .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let c = cache.read();
        assert!(c.scroll_anchor.is_some(), "scroll_anchor should be set");
    }

    #[tokio::test]
    async fn test_clear_resets_cache() {
        let (tx, cache, _notify) = spawn_render_thread(80);

        tx.send(RenderEvent::Rebuild(vec![MessageViewModel::user(
            "Hello".to_string(),
        )]))
        .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        tx.send(RenderEvent::Clear).unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let c = cache.read();
        assert!(c.lines.is_empty(), "lines should be empty after Clear");
        assert_eq!(c.total_lines, 0);
    }

    #[tokio::test]
    async fn test_resize_rebuilds_with_new_width() {
        let (tx, cache, _notify) = spawn_render_thread(80);

        let user = MessageViewModel::user("Hello world".to_string());
        tx.send(RenderEvent::Rebuild(vec![user.clone()])).unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let v1 = cache.read().version;
        let total_v1 = cache.read().total_lines;

        // Resize
        tx.send(RenderEvent::Resize(40)).unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let c = cache.read();
        assert!(c.version > v1, "version should increment after Resize");
        // 窄宽度可能导致更多 wrap 行
        assert!(c.total_lines >= total_v1);
    }

    #[test]
    fn test_build_wrap_map_empty() {
        let result = RenderTask::build_wrap_map(&[], 80);
        assert!(result.is_empty());
    }

    #[test]
    fn test_build_wrap_map_single_short_line() {
        let lines = vec![Line::from("Hello")];
        let result = RenderTask::build_wrap_map(&lines, 80);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].visual_row_start, 0);
        assert_eq!(result[0].visual_row_end, 1);
        assert_eq!(result[0].plain_text, "Hello");
    }

    #[test]
    fn test_build_wrap_map_single_long_line_wraps() {
        let long_text: String = "A".repeat(200);
        let lines: Vec<Line<'static>> = vec![Line::from(long_text)];
        let result = RenderTask::build_wrap_map(&lines, 40);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].visual_row_start, 0);
        assert_eq!(result[0].visual_row_end, 5);
    }

    #[test]
    fn test_build_wrap_map_cjk_char_width() {
        let lines = vec![Line::from("你好世界")];
        let result = RenderTask::build_wrap_map(&lines, 80);
        assert_eq!(result[0].char_widths, vec![2, 2, 2, 2]);
        assert_eq!(result[0].visual_row_end - result[0].visual_row_start, 1);
    }

    #[test]
    fn test_build_wrap_map_multi_line_visual_rows() {
        let first_line: String = "A".repeat(80);
        let second_line = Line::from("short");
        let lines: Vec<Line<'static>> = vec![Line::from(first_line), second_line];
        let result = RenderTask::build_wrap_map(&lines, 40);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].visual_row_start, 0);
        assert_eq!(result[0].visual_row_end, 2);
        assert_eq!(result[1].visual_row_start, 2);
        assert_eq!(result[1].visual_row_end, 3);
    }

    #[test]
    fn test_build_wrap_map_empty_line() {
        let lines = vec![Line::from("")];
        let result = RenderTask::build_wrap_map(&lines, 80);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].visual_row_end - result[0].visual_row_start, 1);
    }
}
