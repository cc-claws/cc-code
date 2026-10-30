use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    sync::{Arc, OnceLock},
    time::{Duration, Instant},
};

use parking_lot::RwLock;
use ratatui::{
    style::Style,
    text::Line,
    widgets::{Paragraph, Wrap},
};
use tokio::sync::{mpsc, Notify};
use unicode_segmentation::UnicodeSegmentation;

/// 渲染事件通道容量。
/// 128 足以缓冲大量事件（resize 风暴 + 流式 Rebuild），同时防止内存无限膨胀。
/// 正常运行时队列深度通常 < 5。
const RENDER_CHANNEL_CAPACITY: usize = 128;

#[cfg(test)]
use super::message_render::CONTROL_B_BACKGROUND_HINT;
pub use super::render_layout::{
    SelectionAnchor, Viewport as RenderViewport, ViewportAnchor, VirtualLayoutSnapshot,
    VirtualRenderStats,
};
use super::theme;
use super::{
    markdown::{ensure_rendered_flush, ensure_rendered_incremental},
    message_render::{
        is_shell_running_status_line, render_view_model_with_flags, shell_output_status_text,
        shell_running_text,
    },
    message_view::{MessageViewModel, ToolPhase},
};
use cc_widgets::markdown::LinkHit;
mod worker;
use worker::RenderWorker;

const TOOL_INDICATOR_TICK_INTERVAL: Duration = Duration::from_millis(200);

#[cfg(test)]
#[path = "shell_runtime_render_test.rs"]
mod shell_runtime_tests;

fn current_tool_indicator_tick() -> u64 {
    static STARTUP: OnceLock<Instant> = OnceLock::new();
    STARTUP.get_or_init(Instant::now).elapsed().as_millis() as u64 / 200
}

/// 单个逻辑行的换行映射信息
#[derive(Debug, Clone)]
pub struct WrappedLineInfo {
    /// 该行在 cache.lines 中的索引
    pub line_idx: usize,
    /// 该逻辑行渲染后的起始视觉行号（基于 0）
    pub visual_row_start: usize,
    /// 该逻辑行渲染后的结束视觉行号（不含）
    pub visual_row_end: usize,
    /// 该逻辑行的纯文本内容（去样式，用于复制）
    pub plain_text: String,
    /// 每个字符的显示宽度序列（ASCII=1, CJK=2）
    pub char_widths: Vec<u8>,
    /// 复制时排除的装饰前缀；plain_text 与字符宽度仍保留完整显示坐标。
    pub copy_prefix_graphemes: usize,
}

/// 一次重排恢复的位置及捕获它的用户滚动版本。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ScrollAnchor {
    pub position: ViewportAnchor,
    pub visual_row: usize,
    /// 捕获位置时的用户滚动版本；重排不能覆盖后续的键鼠操作。
    pub scroll_revision: u64,
}

/// 渲染缓存，由渲染线程写入、UI 线程读取
pub struct RenderCache {
    /// 虚拟布局的全局元数据；lines/wrap_map/links 只持有当前物化窗口。
    pub virtual_layout: Option<Arc<VirtualLayoutSnapshot>>,
    /// 所有消息渲染后的行
    pub lines: Vec<Line<'static>>,
    /// 每条消息在 lines 中的起始行索引（用于定位）
    pub message_offsets: Vec<usize>,
    /// 经典缓存去重后的每行对应的（消息索引，渲染逻辑行）。
    pub line_origins: Vec<(usize, usize)>,
    /// 总行数（已考虑 wrap 换行后的真实视觉行数）
    pub total_lines: usize,
    /// 版本号，UI 线程比较是否有变化以决定是否重绘
    pub version: u64,
    pub wrap_map: Vec<WrappedLineInfo>,
    /// 超链接命中区（行号对应 cache.lines 下标，g 坐标对应 plain_text grapheme）
    pub links: Vec<LinkHit>,
    /// 当前渲染使用的文本区域宽度
    pub width: u16,
    /// RebuildAll 后的滚动锚点（视觉行号），UI 线程读取后清除
    pub scroll_anchor: Option<ScrollAnchor>,
}

impl Default for RenderCache {
    fn default() -> Self {
        Self::new()
    }
}

impl RenderCache {
    pub fn new() -> Self {
        Self {
            virtual_layout: None,
            lines: Vec::new(),
            message_offsets: Vec::new(),
            line_origins: Vec::new(),
            total_lines: 0,
            version: 0,
            wrap_map: Vec::new(),
            links: Vec::new(),
            width: 0,
            scroll_anchor: None,
        }
    }
}

/// 一次完整的宽度准备由 worker 顺序完成，UI 不等待其中的宽/窄中间态。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WidthPreparationRequest {
    pub width: u16,
    pub available_rows: usize,
    pub extra_rows: usize,
    pub has_messages: bool,
    pub fullscreen: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct PreparedWidth {
    pub width: u16,
    pub full_width_rows: usize,
    pub measured_rows: usize,
}

/// 当前视口独立保存的显示选项，随消息快照一起提交。
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct DisplayFlags {
    pub detail_mode: bool,
    pub diff_visible: bool,
    pub show_tool_messages: bool,
}

/// 渲染线程接收的事件
pub enum RenderEvent {
    /// 显示状态与目标消息不可分开入队，队列重试只保留最新完整快照。
    WithDisplay {
        display: DisplayFlags,
        event: Box<RenderEvent>,
    },
    /// 重排与捕获时的阅读意图原子入队，不能等待下一帧才同步位置。
    WithViewport {
        viewport: RenderViewport,
        event: Box<RenderEvent>,
    },
    /// 仅影响本地渲染，不改变 Agent 或系统提示词。
    SetDetailStrategy(String),
    Viewport {
        top: usize,
        height: usize,
        follow_tail: bool,
        anchor: Option<ViewportAnchor>,
        scroll_revision: u64,
    },
    CopySelection {
        start: SelectionAnchor,
        end: SelectionAnchor,
        reply: tokio::sync::oneshot::Sender<Option<String>>,
    },
    /// 全量重建消息列表（通过 hash diff 优化渲染）
    Rebuild(Vec<MessageViewModel>),
    /// 全量重建并设置滚动锚点（RebuildAll 后保持滚动位置）
    RebuildWithAnchor {
        messages: Vec<MessageViewModel>,
        anchor: ViewportAnchor,
        scroll_revision: u64,
    },
    /// 终端宽度变化，渲染线程自动用 last_messages 重建
    Resize(u16),
    /// 重排完成后通知 UI；增长视口必须先取得新宽度的实际行数才能判断触底。
    ResizeAndWait(u16, tokio::sync::oneshot::Sender<()>),
    PrepareWidth {
        request: WidthPreparationRequest,
        reply: tokio::sync::oneshot::Sender<PreparedWidth>,
    },
    /// 清空所有消息
    Clear,
    /// 切换工具调用消息的显示状态
    ToggleToolMessages(bool),
    /// 切换内联 diff 显示状态（强制全量重渲染）
    ToggleDiff(bool),
    /// 切换 hook 结果行的显示状态（数据保留，只影响渲染）
    ToggleHookRows(bool),
    /// 切换详细模式（shell 输出、粘贴内容、reasoning 展开/折叠）
    ToggleDetail(bool),
}

impl RenderEvent {
    #[cfg(test)]
    pub(crate) fn into_payload(self) -> Self {
        match self {
            Self::WithViewport { event, .. } | Self::WithDisplay { event, .. } => {
                event.into_payload()
            }
            event => event,
        }
    }

    pub(crate) fn payload(&self) -> &Self {
        match self {
            Self::WithViewport { event, .. } | Self::WithDisplay { event, .. } => event.payload(),
            event => event,
        }
    }

    pub(crate) fn clone_mode(&self) -> Option<Self> {
        Some(match self {
            Self::WithDisplay { display, event } => Self::WithDisplay {
                display: *display,
                event: Box::new(event.clone_mode()?),
            },
            Self::WithViewport { viewport, event } => Self::WithViewport {
                viewport: *viewport,
                event: Box::new(event.clone_mode()?),
            },
            Self::SetDetailStrategy(value) => Self::SetDetailStrategy(value.clone()),
            Self::ToggleDetail(value) => Self::ToggleDetail(*value),
            Self::ToggleDiff(value) => Self::ToggleDiff(*value),
            Self::ToggleHookRows(value) => Self::ToggleHookRows(*value),
            Self::ToggleToolMessages(value) => Self::ToggleToolMessages(*value),
            _ => return None,
        })
    }
}

/// #626：单条消息的 wrap 分段缓存。
///
/// 命中条件必须同时满足 `key`、`epoch`、`slice_len`：
/// - `key`：渲染键（内容 hash ⊕ 影响该消息渲染的 detail/diff 标志）；
/// - `epoch`：该消息**行内容**的版本（见 `RenderTask::line_epochs`）。指示器动画、
///   `Running… (Xs)` 秒数这类增量刷新只改行内容、不改渲染键，必须靠它失效；
/// - `slice_len`：dedup 后切片长度（尾空行裁剪会让尾消息切片伸缩）。
struct WrapSegment {
    key: u64,
    epoch: u64,
    /// 段内各逻辑行的 wrap 信息（`line_idx` 为段内局部下标）
    entries: Vec<WrappedLineInfo>,
    /// 段视觉行数
    total: usize,
    slice_len: usize,
}

/// 判断一个逻辑行是否为空行（全局 dedup / 尾空行裁剪的判定口径）。
pub(super) fn is_blank_line(line: &Line<'static>) -> bool {
    line.spans.is_empty() || (line.spans.len() == 1 && line.spans[0].content.is_empty())
}

/// 渲染线程，在后台执行渲染计算
///
/// 消息状态由 App 持有（view_messages），渲染线程通过 Rebuild 事件接收完整快照，
/// 通过 hash diff 只重新渲染发生变化的消息，避免不必要的 markdown 解析。
pub(super) struct RenderTask {
    /// 上一次 Rebuild 收到的消息（Resize 时用于全量重建）
    last_messages: Vec<MessageViewModel>,
    /// 每条消息的渲染行缓存
    message_lines: Vec<Vec<Line<'static>>>,
    /// 每条消息的链接命中区（行号基于该消息 message_lines 下标）
    message_links: Vec<Vec<LinkHit>>,
    /// 每条消息的渲染键（内容 hash ⊕ 敏感标志，见 render_key；用于复用判断）
    message_hashes: Vec<u64>,
    cache: Arc<RwLock<RenderCache>>,
    notify: Arc<Notify>,
    width: u16,
    show_tool_messages: bool,
    diff_visible: bool,
    /// hook 结果行是否可见（配置 `hooks_ui`，缺省可见）
    hook_rows_visible: bool,
    detail_mode: bool,
    /// 上次渲染时的主题指纹。主题切换后 markdown 的 `rendered` 快照与内容 hash
    /// 都不变，必须靠它强制放弃缓存行复用。
    theme_fingerprint: u64,
    /// #626：双变体行缓存——每条消息另一 (detail, diff) 组合下的渲染成品，
    /// 与 message_lines/message_links 同下标；alt_keys[i] 是该成品的渲染键，
    /// 键为 0 或成品为空表示无缓存。
    alt_lines: Vec<Vec<Line<'static>>>,
    alt_links: Vec<Vec<LinkHit>>,
    alt_keys: Vec<u64>,
    /// 与 alt_lines/alt_links 同下标的行内容版本号（交换时与 line_epochs 互换）。
    alt_epochs: Vec<u64>,
    /// 每条消息行内容的版本号：`message_lines[i]` 实际内容每变化一次就取一个新值。
    /// 渲染键只反映 VM 内容，而计时秒数 / 指示器动画是渲染期才知道的信息，
    /// 增量刷新改写行内容时靠它让对应的 wrap 分段失效。
    line_epochs: Vec<u64>,
    /// line_epochs 的分配器（单调递增，避免复用旧值造成假命中）。
    epoch_seq: u64,
    /// #626：wrap 按消息分段缓存 [detail_mode][msg_idx]，条目见 WrapSegment。
    wrap_segs: [Vec<Option<WrapSegment>>; 2],
}

/// 该消息的渲染结果是否随 detail_mode 变化（#626）。
/// 与 message_render 的 detail 分支一一对应：新增 detail 敏感分支时必须同步更新此处，
/// 否则切换 detail 时该消息会被错误复用。
fn detail_sensitive(vm: &MessageViewModel) -> bool {
    match vm {
        MessageViewModel::UserBubble {
            expanded_content, ..
        } => expanded_content.is_some(),
        MessageViewModel::AssistantBubble { blocks, .. } => blocks
            .iter()
            .any(|b| matches!(b, super::message_view::ContentBlockView::Reasoning { .. })),
        MessageViewModel::ToolBlock { .. } => true,
        MessageViewModel::ShellCommand { .. } => true,
        MessageViewModel::SystemNote {
            detail_content,
            detail_only,
            ..
        } => detail_content.is_some() || *detail_only,
        MessageViewModel::CacheWarning { .. } => false,
        MessageViewModel::ToolCallGroup { .. } => true,
        MessageViewModel::SubAgentGroup { .. } => true,
    }
}

/// 该消息的渲染结果是否随 diff_visible 变化（#626），规则同 detail_sensitive。
fn diff_sensitive(vm: &MessageViewModel) -> bool {
    match vm {
        MessageViewModel::ToolBlock { diff_input, .. } => diff_input.is_some(),
        MessageViewModel::ToolCallGroup { .. } => true,
        MessageViewModel::SubAgentGroup { .. } => true,
        _ => false,
    }
}

/// 该消息的渲染结果是否随 `hook_rows_visible` 变化。
///
/// 工具有 hook 行时敏感；组（工具组 / 子 Agent）内嵌渲染子 VM，可能带 hook 行，一并视为敏感。
fn hook_rows_sensitive(vm: &MessageViewModel) -> bool {
    match vm {
        MessageViewModel::ToolBlock { hook_rows, .. } => !hook_rows.is_empty(),
        // 会话级 hook 行本身就是 SystemNote（`branch_prefix = true`）。
        MessageViewModel::SystemNote { branch_prefix, .. } => *branch_prefix,
        MessageViewModel::ToolCallGroup { .. } => true,
        MessageViewModel::SubAgentGroup { .. } => true,
        _ => false,
    }
}

/// 渲染键（#626）= 内容 hash ⊕（仅当该消息对相应标志敏感时）当前 detail/diff 标志。
/// 键相等的消息在当前标志组合下渲染结果必然与缓存一致，可直接复用或跨变体交换；
/// 不敏感的消息切换标志时键不变，这是 Ctrl+O 反复切换不再全量重渲染的关键。
/// 运行中思考行的"走秒"秒数：参与渲染键，保证每秒重绘一次
/// （VM 内容不变时缓存会复用旧行，秒数会冻住）。
fn live_thinking_elapsed_ms(vm: &MessageViewModel) -> Option<u64> {
    // 一等公民：组自带思考行时，走秒也由组提供
    if let MessageViewModel::ToolCallGroup {
        thinking: Some(t), ..
    } = vm
    {
        if !t.active {
            return None;
        }
        let base = t.duration_ms.unwrap_or(0);
        let extra = t
            .active_since
            .map(|anchor| anchor.elapsed().as_millis() as u64)
            .unwrap_or(0);
        return Some(base + extra);
    }
    let MessageViewModel::AssistantBubble { blocks, .. } = vm else {
        return None;
    };
    blocks.iter().find_map(|b| match b {
        crate::ui::message_view::ContentBlockView::Reasoning {
            active: true,
            duration_ms,
            active_since,
            ..
        } => {
            let base = duration_ms.unwrap_or(0);
            let extra = active_since
                .map(|anchor| anchor.elapsed().as_millis() as u64)
                .unwrap_or(0);
            Some(base + extra)
        }
        _ => None,
    })
}

fn render_key(vm: &MessageViewModel, detail: bool, diff: bool, hook_rows: bool) -> u64 {
    let mut h = DefaultHasher::new();
    vm.content_hash().hash(&mut h);
    if let Some(ms) = live_thinking_elapsed_ms(vm) {
        // 秒数进键，保证走秒刷新；符号已改为常显，不再有相位维度
        (ms / 1000).hash(&mut h);
    }
    if detail_sensitive(vm) {
        detail.hash(&mut h);
    }
    if diff_sensitive(vm) {
        diff.hash(&mut h);
    }
    if hook_rows_sensitive(vm) {
        hook_rows.hash(&mut h);
    }
    h.finish()
}

impl RenderTask {
    fn apply_display(&mut self, display: DisplayFlags) {
        if self.detail_mode != display.detail_mode
            || self.diff_visible != display.diff_visible
            || self.show_tool_messages != display.show_tool_messages
        {
            // 与 Toggle 路径一致，禁止把旧 VM 的 cosmetic 判定用于新显示状态。
            // 保留行、双变体与换行缓存，仍按完整渲染键复用有效成品。
            self.last_messages.clear();
        }
        self.detail_mode = display.detail_mode;
        self.diff_visible = display.diff_visible;
        self.show_tool_messages = display.show_tool_messages;
    }

    /// 根据 cache.lines 和当前宽度计算 wrap_map。
    /// 对每个逻辑行使用 ratatui 的 Paragraph::line_count 精确计算视觉行数，
    /// 与实际渲染的 WordWrapper 算法完全一致。
    /// char_widths 使用 grapheme 级别（与 ratatui 一致）。
    pub(super) fn build_wrap_map(
        lines: &[Line<'static>],
        width: u16,
    ) -> (usize, Vec<WrappedLineInfo>) {
        if width == 0 || lines.is_empty() {
            return (0, Vec::new());
        }
        let mut wrap_map = Vec::with_capacity(lines.len());
        let mut visual_row: usize = 0;

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
                count.max(1)
            };

            wrap_map.push(WrappedLineInfo {
                line_idx: idx,
                visual_row_start: visual_row,
                visual_row_end: visual_row + visual_count,
                plain_text,
                char_widths,
                copy_prefix_graphemes: 0,
            });
            visual_row += visual_count;
        }
        (visual_row, wrap_map)
    }

    pub(super) fn build_message_wrap_map(
        lines: &[Line<'static>],
        width: u16,
        is_user: bool,
    ) -> (usize, Vec<WrappedLineInfo>) {
        let (total, mut entries) = Self::build_wrap_map(lines, width);
        if is_user {
            let mut in_attachment = false;
            for (line, entry) in lines.iter().zip(&mut entries) {
                in_attachment = line.spans.first().is_some_and(|span| {
                    span.content == super::branch_prefix::BranchPrefix::PREFIX
                        || in_attachment && span.content == super::branch_prefix::BranchPrefix::HANG
                });
                if in_attachment {
                    entry.copy_prefix_graphemes = super::branch_prefix::BranchPrefix::WIDTH;
                }
            }
        }
        (total, entries)
    }

    /// 渲染单条消息为 lines（含前后空行分隔）
    ///
    /// 注意：此函数修改的 rendered/dirty/rendered_prefix_len 等字段不参与 Hash 计算，
    /// 因此不会使 content_hash 失效。
    pub(super) fn render_one(
        vm: &mut MessageViewModel,
        index: usize,
        width: usize,
        diff_visible: bool,
        hook_rows_visible: bool,
        detail_mode: bool,
    ) -> (Vec<Line<'static>>, Vec<LinkHit>) {
        let tick = current_tool_indicator_tick();
        // 减去气泡前缀缩进（UserBubble "❯ " / "  "，AssistantBubble "● " / "  " 均占 2 字符），
        // 保证 Markdown 渲染宽度 + 前缀不超过视口宽度，避免 Paragraph.wrap() 二次硬折行
        let content_width = width.saturating_sub(2).max(20);

        // 处理 dirty blocks（使用增量解析）
        if let MessageViewModel::AssistantBubble {
            blocks,
            is_streaming,
            ..
        } = vm
        {
            for block in blocks.iter_mut() {
                if *is_streaming {
                    ensure_rendered_incremental(block, content_width);
                } else {
                    ensure_rendered_flush(block, content_width);
                }
            }
        }
        // 用实际终端内容宽度重新解析用户消息的 markdown（初始创建时用默认宽度 80）
        if let MessageViewModel::UserBubble {
            content,
            rendered,
            rendered_links,
            ..
        } = vm
        {
            let doc = super::markdown::parse_markdown_rich(content, content_width);
            *rendered = doc.text;
            *rendered_links = doc.links;
        }

        // detail_mode 控制 shell 输出、粘贴内容、reasoning 的展开/折叠
        // diff_visible 只控制普通页面是否内联显示 Write/Edit 的 diff（详细模式下恒显示）
        // hook_rows_visible 只控制 hook 结果行是否渲染（数据保留，切回来即恢复）
        // 三者语义不同，必须分开传：并成一个标志会让「内联 Diff」顺带打开整页展开
        let (mut lines, mut links) = render_view_model_with_flags(
            vm,
            Some(index),
            width,
            detail_mode,
            diff_visible,
            hook_rows_visible,
            tick,
        );
        // 裸 URL 只对工具结果与 `!` 命令输出登记命中区：Markdown 正文/代码块里的
        // `http(s)://` 一律不扫描（与 CC 的 `OutputLine` 口径一致），用户输入更不参与。
        if matches!(
            vm,
            MessageViewModel::ToolBlock { .. } | MessageViewModel::ShellCommand { .. }
        ) {
            links.extend(Self::bare_url_links(&lines));
        }
        // 每条消息后追加空行分隔符（包括空内容消息，确保间距一致）
        lines.push(Line::from(""));
        (lines, links)
    }

    /// 从已渲染行里补出裸 URL 命中区（`line` 是 `lines` 内下标）。
    ///
    /// 工具结果与 `!` 输出没有 Markdown 层，终端与鼠标点击都无从得知哪段是链接，
    /// 只能按行扫描补上。扫描发生在消息渲染期（结果随 `message_links` 缓存），
    /// 不在每帧的视口裁剪里重复做。
    fn bare_url_links(lines: &[Line<'static>]) -> Vec<LinkHit> {
        let mut links = Vec::new();
        for (idx, line) in lines.iter().enumerate() {
            let plain: String = line
                .spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect();
            links.extend(super::osc8::bare_url_hits(&plain, idx));
        }
        links
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
    /// cosmetic change 的消息可以安全复用旧的渲染缓存，避免不必要的重渲染。
    fn is_cosmetic_change(old: &MessageViewModel, new: &MessageViewModel) -> bool {
        // 活跃思考行的渲染结果依赖墙钟（走秒 + 500ms 相位闪烁），而 blocks/字段内容
        // 在一秒内往往完全相同——一旦判成 cosmetic 就会把"新键 + 旧成品"登记下来，
        // 秒数与闪烁随之停住。含活跃思考的 VM 一律走真重渲染。
        if Self::has_active_thinking(old) || Self::has_active_thinking(new) {
            return false;
        }
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

    /// 该 VM 是否包含「活跃」思考行：走秒与相位闪烁随时钟变化，不能复用旧成品。
    fn has_active_thinking(vm: &MessageViewModel) -> bool {
        match vm {
            MessageViewModel::AssistantBubble { blocks, .. } => blocks.iter().any(|b| {
                matches!(
                    b,
                    crate::ui::message_view::ContentBlockView::Reasoning { active, .. } if *active
                )
            }),
            MessageViewModel::ToolCallGroup {
                thinking: Some(t), ..
            } => t.active,
            _ => false,
        }
    }

    /// 全量重建：接收完整消息列表，按渲染键逐条复用缓存行与 wrap 分段
    ///
    /// 优化策略：
    /// 1. 计算前缀稳定长度（连续 hash 未变的消息数量）
    /// 2. 前缀消息直接复用 message_lines 缓存，跳过渲染
    /// 3. 逐条按渲染键复用当前变体成品、或与 alt 中另一变体成品交换，
    ///    仅真变化的消息重渲染（重渲染的旧成品存入 alt 备切换时复用）
    /// 4. 对 hash 不同但属于 cosmetic change 的消息也复用缓存
    fn rebuild(&mut self, messages: Vec<MessageViewModel>) {
        self.rebuild_with_scroll_anchor(messages, None);
    }

    fn rebuild_with_scroll_anchor(
        &mut self,
        messages: Vec<MessageViewModel>,
        scroll_anchor: Option<(ViewportAnchor, u64)>,
    ) {
        let width = self.width as usize;
        let new_len = messages.len();

        // 保留一份用于 Resize（Resize 时没有新的 Rebuild 事件）
        let old_messages = std::mem::replace(&mut self.last_messages, messages.clone());

        // #626：比较键 = 渲染键（内容 hash ⊕ 影响该消息渲染的 detail/diff 标志）。
        // 不敏感的消息切换标志时键不变，可直接复用缓存行。
        let new_keys: Vec<u64> = messages
            .iter()
            .map(|vm| {
                render_key(
                    vm,
                    self.detail_mode,
                    self.diff_visible,
                    self.hook_rows_visible,
                )
            })
            .collect();

        // 主题切换：markdown 的 `rendered` 快照与内容 hash 都不变，但颜色已过期。
        // 丢弃 hash 缓存（prefix_stable_len 归零）强制整屏重渲染。
        let theme_fp = crate::ui::theme::fingerprint();
        if theme_fp != self.theme_fingerprint {
            self.theme_fingerprint = theme_fp;
            self.message_hashes.clear();
        }

        // 计算 prefix_stable_len：前缀中连续 hash 未变的消息数量
        let old_len = self.message_hashes.len();

        // 保存旧的 message_lines（用于 cosmetic change 复用）
        let mut old_message_lines = std::mem::take(&mut self.message_lines);
        let mut old_message_links = std::mem::take(&mut self.message_links);
        let old_epochs = std::mem::take(&mut self.line_epochs);
        let old_alt_epochs = std::mem::take(&mut self.alt_epochs);
        let mut epoch_seq = self.epoch_seq;

        // 调整 message_lines 容量
        self.message_lines.resize(new_len, Vec::new());
        self.message_links.resize(new_len, Vec::new());
        self.line_epochs.resize(new_len, 0);
        self.alt_epochs.resize(new_len, 0);

        self.alt_lines.resize(new_len, Vec::new());
        self.alt_links.resize(new_len, Vec::new());
        self.alt_keys.resize(new_len, 0);
        // #626：逐条按键复用，不依赖前缀连续性——中段单条变化不牵连其后消息。
        // 注意：Toggle/Resize 路径上 old_messages 已被 mem::take 取空，
        // 涉及它的分支（cosmetic）必须先判下标在界内。
        for (i, mut vm) in messages.into_iter().enumerate() {
            // 默认继承旧版本号；交换 / 重渲染分支会在下面覆写。
            self.line_epochs[i] = old_epochs.get(i).copied().unwrap_or(0);
            self.alt_epochs[i] = old_alt_epochs.get(i).copied().unwrap_or(0);
            // 渲染键未变 → 当前变体成品直接复用
            if i < old_len
                && i < old_message_lines.len()
                && self.message_hashes[i] == new_keys[i]
                && !old_message_lines[i].is_empty()
            {
                self.message_lines[i] = std::mem::take(&mut old_message_lines[i]);
                self.message_links[i] = std::mem::take(&mut old_message_links[i]);
                continue;
            }
            // 另一 (detail, diff) 变体的成品还在 alt 中且键命中 → 两变体交换，零渲染
            if self.alt_keys[i] == new_keys[i] && !self.alt_lines[i].is_empty() {
                let cur_key = if i < old_len {
                    self.message_hashes[i]
                } else {
                    0
                };
                let cur_lines = if i < old_message_lines.len() {
                    std::mem::take(&mut old_message_lines[i])
                } else {
                    Vec::new()
                };
                let cur_links = if i < old_message_links.len() {
                    std::mem::take(&mut old_message_links[i])
                } else {
                    Vec::new()
                };
                self.message_lines[i] = std::mem::take(&mut self.alt_lines[i]);
                self.message_links[i] = std::mem::take(&mut self.alt_links[i]);
                self.alt_lines[i] = cur_lines;
                self.alt_links[i] = cur_links;
                self.alt_keys[i] = cur_key;
                // 版本号随成品一起交换：两个变体各自的 wrap 分段仍然有效
                std::mem::swap(&mut self.line_epochs[i], &mut self.alt_epochs[i]);
                continue;
            }
            // hash 不同但属于 cosmetic change（仅流式/运行标志变化）→ 复用旧行；
            // alt 中的另一变体成品与新内容不再对应，一并作废
            if i < old_message_lines.len()
                && i < old_messages.len()
                && !old_message_lines[i].is_empty()
                && Self::is_cosmetic_change(&old_messages[i], &vm)
            {
                self.message_lines[i] = std::mem::take(&mut old_message_lines[i]);
                self.message_links[i] = std::mem::take(&mut old_message_links[i]);
                self.alt_lines[i].clear();
                self.alt_keys[i] = 0;
                self.alt_epochs[i] = 0;
                continue;
            }
            // 真重渲染：旧成品存入 alt 备另一变体复用；无旧成品时提前释放占位
            if i < old_message_lines.len() && !old_message_lines[i].is_empty() {
                self.alt_lines[i] = std::mem::take(&mut old_message_lines[i]);
                self.alt_links[i] = std::mem::take(&mut old_message_links[i]);
                self.alt_keys[i] = if i < old_len {
                    self.message_hashes[i]
                } else {
                    0
                };
                self.alt_epochs[i] = old_epochs.get(i).copied().unwrap_or(0);
            } else if i < old_message_lines.len() {
                old_message_lines[i].clear();
                old_message_links[i].clear();
                self.alt_epochs[i] = 0;
            }
            let (lines, links) = Self::render_one(
                &mut vm,
                i + 1,
                width,
                self.diff_visible,
                self.hook_rows_visible,
                self.detail_mode,
            );
            self.message_lines[i] = lines;
            self.message_links[i] = links;
            // 新成品 = 新版本号：旧的 wrap 分段必然失效
            epoch_seq += 1;
            self.line_epochs[i] = epoch_seq;
        }

        self.message_hashes = new_keys;
        self.epoch_seq = epoch_seq;

        // 拼接所有消息行，同时做全局 dedup（消除连续空行），
        // 并在 deduped 索引空间构建 message_offsets 与全局超链接映射。
        // 修复：旧代码先构建 offsets（基于 all_lines），再做 dedup 生成 deduped，
        // 导致 offsets 和 wrap_map 处于不同索引空间。
        let mut deduped: Vec<Line<'static>> = Vec::new();
        let mut line_origins = Vec::new();
        let mut offsets: Vec<usize> = Vec::new();
        let mut global_links: Vec<LinkHit> = Vec::new();
        let mut prev_empty = false;
        for (mi, lines) in self.message_lines.iter().enumerate() {
            offsets.push(deduped.len());
            let mut line_map: Vec<Option<usize>> = Vec::with_capacity(lines.len());
            for (logical_line, line) in lines.iter().enumerate() {
                let is_empty = line.spans.is_empty()
                    || (line.spans.len() == 1 && line.spans[0].content.is_empty());
                if is_empty && prev_empty {
                    line_map.push(None);
                    continue;
                }
                prev_empty = is_empty;
                line_map.push(Some(deduped.len()));
                deduped.push(line.clone());
                line_origins.push((mi, logical_line));
            }
            if let Some(msg_links) = self.message_links.get(mi) {
                for hit in msg_links {
                    if let Some(Some(new_line)) = line_map.get(hit.line) {
                        global_links.push(LinkHit {
                            line: *new_line,
                            ..hit.clone()
                        });
                    }
                }
            }
        }
        // 移除末尾多余空行
        while deduped.last().is_some_and(|l| {
            l.spans.is_empty() || (l.spans.len() == 1 && l.spans[0].content.is_empty())
        }) {
            let dropped = deduped.len() - 1;
            deduped.pop();
            line_origins.pop();
            global_links.retain(|hit| hit.line != dropped);
        }

        // 尾空行裁剪会让 offsets 越过 deduped.len()（尾部消息渲染为空时，
        // 其 offset 等于裁剪前的长度）。offsets 是消息分段与 wrap 切片的边界，
        // 必须先夹紧到 deduped.len() 并保持单调，否则 deduped[start..end] 越界 panic
        // （表现为重建失败、画面停留在旧状态）。
        let mut clamped = 0usize;
        for offset in offsets.iter_mut() {
            clamped = (*offset).max(clamped).min(deduped.len());
            *offset = clamped;
        }

        // #626：wrap 按消息分段缓存——段键 = (渲染键, 行版本号, dedup 后切片长度)，
        // 命中段平移复用，未命中只算该消息的切片。切片长度必须进键：
        // dedup 的尾空行裁剪会让尾消息切片随消息追加而伸缩；
        // 行版本号必须进键：计时秒数 / 指示器动画只改行内容、不改渲染键，
        // 否则会继续命中旧 wrap（复制文本、视觉行数、选区映射全部过期）。
        let (total_lines, wrap_map) = {
            let slot = self.detail_mode as usize;
            if self.wrap_segs[slot].len() != new_len {
                self.wrap_segs[slot].resize_with(new_len, || None);
            }
            let mut wrap_map: Vec<WrappedLineInfo> = Vec::with_capacity(deduped.len());
            let mut visual_base = 0usize;
            for i in 0..new_len {
                let start = offsets[i];
                let end = if i + 1 < new_len {
                    offsets[i + 1]
                } else {
                    deduped.len()
                };
                debug_assert!(start <= end && end <= deduped.len());
                let slice_len = end - start;
                let key = self.message_hashes[i];
                let epoch = self.line_epochs[i];
                let seg_hit = self.wrap_segs[slot][i]
                    .as_ref()
                    .is_some_and(|s| s.key == key && s.epoch == epoch && s.slice_len == slice_len);
                let seg = if seg_hit {
                    self.wrap_segs[slot][i].as_ref().unwrap()
                } else {
                    let (total, entries) = Self::build_message_wrap_map(
                        &deduped[start..end],
                        self.width,
                        matches!(self.last_messages[i], MessageViewModel::UserBubble { .. }),
                    );
                    self.wrap_segs[slot][i] = Some(WrapSegment {
                        key,
                        epoch,
                        entries,
                        total,
                        slice_len,
                    });
                    self.wrap_segs[slot][i].as_ref().unwrap()
                };
                for info in &seg.entries {
                    let mut info = info.clone();
                    info.line_idx += start;
                    info.visual_row_start += visual_base;
                    info.visual_row_end += visual_base;
                    wrap_map.push(info);
                }
                visual_base += seg.total;
            }
            (visual_base, wrap_map)
        };
        let mut next = RenderCache {
            lines: deduped,
            message_offsets: offsets,
            line_origins,
            total_lines,
            wrap_map,
            links: global_links,
            width: self.width,
            ..RenderCache::new()
        };
        let old = {
            let mut cache = self.cache.write();
            next.version = cache.version + 1;
            // 未消费的锚点仍按语义位置重算，不能把旧宽度的视觉行复制到新布局。
            let pending = scroll_anchor.or_else(|| {
                cache
                    .scroll_anchor
                    .map(|anchor| (anchor.position, anchor.scroll_revision))
            });
            next.scroll_anchor = pending.and_then(|(position, scroll_revision)| {
                next.visual_row_for_anchor(position)
                    .map(|visual_row| ScrollAnchor {
                        position,
                        visual_row,
                        scroll_revision,
                    })
            });
            std::mem::replace(&mut *cache, next)
        };
        // 全量旧成品的析构放在锁外，避免 UI 读缓存时等待历史行释放。
        drop(old);
    }

    /// 与 SubAgentGroup 渲染规则一致；折叠/批次摘要/被 final_result 替代的条目不计时刷新。
    fn visible_subagent_messages(vm: &MessageViewModel) -> &[MessageViewModel] {
        match vm {
            MessageViewModel::SubAgentGroup {
                recent_messages,
                collapsed: false,
                batch_agents,
                final_result,
                ..
            } if batch_agents.is_empty() => {
                let skip_last = final_result.as_ref().is_some_and(|r| !r.is_empty())
                    && recent_messages.len() > 1;
                &recent_messages[..recent_messages.len() - usize::from(skip_last)]
            }
            _ => &[],
        }
    }

    fn has_running_tool_blocks(&self) -> bool {
        fn running(vm: &MessageViewModel) -> bool {
            match vm {
                MessageViewModel::ToolBlock {
                    tool_name,
                    phase,
                    is_error,
                    ..
                } => tool_name != "AskUserQuestion" && *phase != ToolPhase::Finished && !*is_error,
                // `!` 命令同样要按 tick 刷新 `Running… (Xs)` 秒数，否则静默命令
                // （如 `! sleep 5`）的状态行会一直停在首帧文本。
                MessageViewModel::ShellCommand { exit_code, .. } => exit_code.is_none(),
                _ => RenderTask::visible_subagent_messages(vm)
                    .iter()
                    .any(running),
            }
        }
        self.last_messages.iter().any(running)
    }

    /// `!` 命令运行中应显示的状态文本。
    ///
    /// 与 `render_shell_command` 的产出逐字对齐：无输出时是 `Running…`/`Running… (Xs)`
    /// 状态行；已有输出时是单独的 `(Xs)` 耗时行（前 2 秒没有这一行）。
    fn running_shell_command_status(vm: &MessageViewModel) -> Option<String> {
        let MessageViewModel::ShellCommand {
            exit_code: None,
            started_at: Some(start),
            stdout,
            stderr,
            moved_to_background: false,
            ..
        } = vm
        else {
            return None;
        };
        let past_threshold = start.elapsed() >= Duration::from_secs(2);
        let has_output = !stdout.trim().is_empty() || !stderr.trim().is_empty();
        if !has_output {
            Some(if past_threshold {
                super::message_render::shell_running_text(*start, None)
            } else {
                super::message_render::SHELL_RUNNING_TEXT_PREFIX.to_string()
            })
        } else if past_threshold {
            Some(super::message_render::shell_elapsed_text(*start))
        } else {
            None
        }
    }

    /// 判断 running Bash 是否已超过 2 秒（需要显示 Running…/ctrl+b hint）
    fn is_running_bash_past_threshold(vm: &MessageViewModel) -> bool {
        let MessageViewModel::ToolBlock {
            tool_name,
            phase,
            is_error,
            started_at,
            shell_backgrounded,
            ..
        } = vm
        else {
            return false;
        };
        tool_name == "Bash"
            && *phase == ToolPhase::Running
            && !shell_backgrounded
            && !*is_error
            && started_at.is_some_and(|t| t.elapsed() >= Duration::from_secs(2))
    }

    #[cfg(test)]
    fn refresh_running_tool_indicators(&mut self, tick: u64) -> bool {
        self.refresh_running_tool_indicators_with_scroll_anchor(tick, None)
    }

    fn refresh_running_tool_indicators_with_scroll_anchor(
        &mut self,
        tick: u64,
        reading_anchor: Option<(ViewportAnchor, u64)>,
    ) -> bool {
        if !self.has_running_tool_blocks() {
            return false;
        }
        // tick 也会改变上方消息高度；旧锚点已被 UI 消费后，仍需恢复当前阅读位置。
        // 未消费的锚点与视口快照取最新滚动版本，同版本优先使用当前视口。
        let scroll_anchor = self
            .cache
            .read()
            .scroll_anchor
            .map(|anchor| (anchor.position, anchor.scroll_revision))
            .into_iter()
            .chain(reading_anchor)
            .max_by_key(|(_, revision)| *revision);

        // 场景 A：有 Bash 首次跨越 2 秒阈值，但缓存仍是 1 行（未渲染 hint 行）。
        // 行数变化无法增量更新，需要重建——但只失效该条消息的 hash，
        // 其余消息的渲染键不受影响，避免全量重绘。
        let mut needs_rebuild = false;
        for (idx, vm) in self.last_messages.iter().enumerate() {
            // 场景 B：`!` 命令的状态行/耗时行随秒数变化。缓存里找不到期望文本时
            // 只失效该条消息的 hash，由重建写入新秒数。
            if let Some(expected) = Self::running_shell_command_status(vm) {
                let has_expected = self.message_lines.get(idx).is_some_and(|lines| {
                    lines
                        .iter()
                        .flat_map(|line| &line.spans)
                        .any(|span| span.content == expected)
                });
                if !has_expected {
                    needs_rebuild = true;
                    if let Some(h) = self.message_hashes.get_mut(idx) {
                        *h = h.wrapping_add(1);
                    }
                    if let Some(lines) = self.message_lines.get_mut(idx) {
                        lines.clear();
                    }
                }
            }
            // 子 Agent 工具不是顶层行，不能用 lines[1] 更新；只在秒数变化时
            // 失效该组的缓存，同时覆盖首次越过 2 秒且没有新输出的情况。
            let children = Self::visible_subagent_messages(vm);
            if !children.is_empty() {
                fn needs_refresh(vm: &MessageViewModel, lines: &[Line<'_>]) -> bool {
                    match vm {
                        MessageViewModel::ToolBlock {
                            tool_name,
                            phase,
                            is_error,
                            started_at: Some(start),
                            timeout_ms,
                            live_output,
                            shell_backgrounded,
                            ..
                        } if tool_name == "Bash"
                            && *phase == ToolPhase::Running
                            && !shell_backgrounded
                            && !is_error
                            && start.elapsed() >= Duration::from_secs(2) =>
                        {
                            let expected = live_output
                                .as_ref()
                                .filter(|output| !output.tail.is_empty())
                                .map(|output| shell_output_status_text(*start, *timeout_ms, output))
                                .unwrap_or_else(|| shell_running_text(*start, *timeout_ms));
                            !lines
                                .iter()
                                .flat_map(|line| &line.spans)
                                .any(|span| span.content == expected)
                        }
                        _ => RenderTask::visible_subagent_messages(vm)
                            .iter()
                            .any(|child| needs_refresh(child, lines)),
                    }
                }
                let lines = self
                    .message_lines
                    .get(idx)
                    .map(Vec::as_slice)
                    .unwrap_or_default();
                if children.iter().any(|child| needs_refresh(child, lines)) {
                    needs_rebuild = true;
                    if let Some(hash) = self.message_hashes.get_mut(idx) {
                        *hash = hash.wrapping_add(1);
                    }
                    // 时间变化不改变 VM 内容，必须同时禁用 cosmetic-change 缓存复用。
                    if let Some(lines) = self.message_lines.get_mut(idx) {
                        lines.clear();
                    }
                }
            }
            if Self::is_running_bash_past_threshold(vm) {
                // 跨越 2 秒阈值后必须已有**带耗时**的 Running… 状态行（以及 ctrl+b 提示行）。
                // 不能只判断「是否已是 Running 行」：状态行在执行一开始就以裸 `Running…`
                // 出现了，那个判定会永远为真，导致本分支再不重建、秒数与提示永不出现。
                // 也不能用固定行数（旧假设 header 恒 1 行 → 共 3 行）：详细模式下超长
                // 命令会让 header 折成多行，固定判定会漏掉，状态行永远不出现。
                // 因此按「期望文本是否已存在于缓存行」定位。
                let expected = {
                    let MessageViewModel::ToolBlock {
                        started_at: Some(start),
                        timeout_ms,
                        live_output,
                        ..
                    } = vm
                    else {
                        continue;
                    };
                    live_output
                        .as_ref()
                        .filter(|output| !output.tail.is_empty())
                        .map(|output| shell_output_status_text(*start, *timeout_ms, output))
                        .unwrap_or_else(|| shell_running_text(*start, *timeout_ms))
                };
                let has_full_status = self.message_lines.get(idx).is_some_and(|lines| {
                    lines
                        .iter()
                        .flat_map(|line| &line.spans)
                        .any(|span| span.content == expected)
                });
                if !has_full_status {
                    needs_rebuild = true;
                    // 只失效该条消息的 hash，触发 rebuild 从该处增量重建
                    if let Some(h) = self.message_hashes.get_mut(idx) {
                        *h = h.wrapping_add(1);
                    }
                }
            }
        }
        if needs_rebuild {
            let messages = self.last_messages.clone();
            self.rebuild_safe_with_anchor(messages, scroll_anchor);
            return true;
        }

        // 场景 B：行数不变的增量更新（indicator 动画 + Bash 秒数）
        let (indicator, indicator_color) = cc_widgets::tool_call::display::format_indicator(
            cc_widgets::ToolCallStatus::Running,
            tick,
            theme::cur().inactive,
            theme::cur().success,
            theme::cur().error,
        );
        let mut changed = false;
        // 单独拿出 Arc：这样 cache 守卫不借用 self，后面才能同时 &mut self 刷新
        // wrap 分段（见 sync_wrap_after_inline_patch）。
        let cache_arc = Arc::clone(&self.cache);
        let mut cache = cache_arc.write();
        let mut patched: Vec<usize> = Vec::new();

        for (idx, vm) in self.last_messages.iter().enumerate() {
            let MessageViewModel::ToolBlock {
                tool_name,
                phase,
                is_error,
                started_at,
                timeout_ms,
                live_output,
                shell_backgrounded,
                ..
            } = vm
            else {
                continue;
            };
            // 所有灰色/未完成的工具块（Queued/AwaitingApproval/Running）都参与统一频率的闪烁刷新
            if *tool_name == "AskUserQuestion" || *phase == ToolPhase::Finished || *is_error {
                continue;
            }

            let Some(lines) = self.message_lines.get_mut(idx) else {
                continue;
            };
            let mut msg_changed = false;

            // 更新 header 的 indicator 动画帧
            if let Some(header) = lines.first_mut() {
                if let Some(indicator_span) = header.spans.first_mut() {
                    if indicator_span.content.as_ref() != indicator {
                        indicator_span.content = indicator.to_string().into();
                        indicator_span.style = Style::default().fg(indicator_color);
                        msg_changed = true;
                    }
                }
            }

            // Bash 运行中且已超 2 秒：增量更新 Running… (Xs) 秒数行（等待/审批阶段不计执行耗时）。
            // 状态行下标**不固定**（详细模式下超长命令会让 header 折成多行），
            // 必须按内容定位；旧代码写死 `lines[1]` 会覆盖命令续行，
            // 同时真正的状态行因不在下标 1 而被冻结（时间不刷新）。
            if *tool_name == "Bash"
                && *phase == ToolPhase::Running
                && !shell_backgrounded
                && started_at.is_some_and(|t| t.elapsed() >= Duration::from_secs(2))
            {
                let new_running_text = live_output
                    .as_ref()
                    .filter(|output| !output.tail.is_empty())
                    .map(|output| {
                        shell_output_status_text(started_at.unwrap(), *timeout_ms, output)
                    })
                    .unwrap_or_else(|| shell_running_text(started_at.unwrap(), *timeout_ms));

                if let Some(running_line) =
                    lines.iter_mut().rfind(|l| is_shell_running_status_line(l))
                {
                    if running_line.spans[1].content.as_ref() != new_running_text {
                        running_line.spans[1].content = new_running_text.into();
                        msg_changed = true;
                    }
                }
            }

            if msg_changed {
                changed = true;
                // 同步更新缓存中该消息的所有行（按 dedup 规则映射，不能按下标直接平移：
                // 与上一条消息相邻的重复空行在 cache.lines 中已被裁掉）
                if let Some(cache_idx) = cache.message_offsets.get(idx).copied() {
                    let mut prev_empty =
                        cache_idx > 0 && cache.lines.get(cache_idx - 1).is_some_and(is_blank_line);
                    let mut write = cache_idx;
                    for line in lines.iter() {
                        let blank = is_blank_line(line);
                        if blank && prev_empty {
                            continue;
                        }
                        prev_empty = blank;
                        if let Some(cache_line) = cache.lines.get_mut(write) {
                            *cache_line = line.clone();
                        }
                        write += 1;
                    }
                }
                patched.push(idx);
            }
        }

        // 行内容变了而渲染键没变（计时秒数 / 指示器动画）：wrap 分段必须同步重建，
        // 否则 plain_text / char_widths / 视觉行数停留在旧内容（#631 P2）。
        for idx in patched {
            changed |= self.sync_wrap_after_inline_patch(&mut cache, idx);
        }

        if changed {
            // inline 秒数更新也可能跨 wrap 边界；旧视觉行不能直接沿用。
            cache.scroll_anchor = scroll_anchor.and_then(|(position, scroll_revision)| {
                cache
                    .visual_row_for_anchor(position)
                    .map(|visual_row| ScrollAnchor {
                        position,
                        visual_row,
                        scroll_revision,
                    })
            });
            cache.version += 1;
        }
        changed
    }

    /// 场景 B 就地改写行内容后，重建该消息的 wrap 分段并刷新 wrap_map 中的视觉行号。
    ///
    /// 秒数变化会改变行宽（`(9s)` → `(10s)`），进而可能改变视觉行数，因此不能只
    /// 原地改写 `plain_text`——必须重算该段并把其后所有分段的视觉行号整体平移。
    /// 返回该消息的 wrap 是否真的发生变化（完全未变时为 false）。
    fn sync_wrap_after_inline_patch(&mut self, cache: &mut RenderCache, idx: usize) -> bool {
        let Some(&start) = cache.message_offsets.get(idx) else {
            return false;
        };
        // 该消息在 deduped 空间中的逻辑行数：到下一条消息 offset 为止（末条到末尾）。
        // 就地改写不增删行，因此 offsets 与 cache.lines 仍保持一致。
        let end = cache
            .message_offsets
            .get(idx + 1)
            .copied()
            .unwrap_or(cache.lines.len())
            .min(cache.lines.len())
            .max(start);
        let slice_len = end - start;

        // wrap_map 与 cache.lines 一一对应；两者不一致说明缓存已损坏，放弃刷新
        // （留给下一次 rebuild 全量修正，不要在这里越界）。
        let Some(old_slice) = cache.wrap_map.get(start..end) else {
            return false;
        };
        let old_entries: Vec<WrappedLineInfo> = old_slice.to_vec();
        let base_visual = old_slice
            .first()
            .map(|info| info.visual_row_start)
            .unwrap_or(0);
        let old_total = old_slice
            .last()
            .map(|last| last.visual_row_end.saturating_sub(base_visual))
            .unwrap_or(0);

        let (total, fresh) = Self::build_message_wrap_map(
            &cache.lines[start..end],
            self.width,
            matches!(self.last_messages[idx], MessageViewModel::UserBubble { .. }),
        );
        if total == old_total && Self::wrap_entries_equal(&old_entries, &fresh) {
            return false;
        }

        // 更新该消息的 wrap 分段缓存，并换一个新版本号：
        // 行内容已变而渲染键未变，必须靠版本号让后续 rebuild 不再命中旧段。
        self.epoch_seq += 1;
        let epoch = self.epoch_seq;
        if idx < self.line_epochs.len() {
            self.line_epochs[idx] = epoch;
        }
        let slot = self.detail_mode as usize;
        if self.wrap_segs[slot].len() <= idx {
            self.wrap_segs[slot].resize_with(idx + 1, || None);
        }
        self.wrap_segs[slot][idx] = Some(WrapSegment {
            key: self.message_hashes.get(idx).copied().unwrap_or(0),
            epoch,
            entries: fresh.clone(),
            total,
            slice_len,
        });

        // 重写 cache.wrap_map 中的这一段（含绝对行号与绝对视觉行号），
        // 再把其后所有分段的视觉行号平移 delta。
        let patched: Vec<WrappedLineInfo> = fresh
            .into_iter()
            .map(|mut info| {
                info.line_idx += start;
                info.visual_row_start += base_visual;
                info.visual_row_end += base_visual;
                info
            })
            .collect();
        cache.wrap_map.splice(start..end, patched);

        let delta = total as isize - old_total as isize;
        if delta != 0 {
            for info in cache.wrap_map.iter_mut().skip(end) {
                info.visual_row_start = info.visual_row_start.saturating_add_signed(delta);
                info.visual_row_end = info.visual_row_end.saturating_add_signed(delta);
            }
            cache.total_lines = cache.total_lines.saturating_add_signed(delta);
        }
        true
    }

    /// 两个 wrap 分段是否等价。
    ///
    /// `old` 来自 cache.wrap_map（line_idx / 视觉行号为绝对坐标），`new` 来自
    /// build_wrap_map（相对坐标），因此只比较段内相对量。
    fn wrap_entries_equal(old: &[WrappedLineInfo], new: &[WrappedLineInfo]) -> bool {
        if old.len() != new.len() {
            return false;
        }
        let Some(new_start) = new.first().map(|info| info.visual_row_start) else {
            return true;
        };
        let old_start = old.first().map(|info| info.visual_row_start).unwrap_or(0);
        old.iter().zip(new.iter()).all(|(o, n)| {
            o.plain_text == n.plain_text
                && o.char_widths == n.char_widths
                && o.copy_prefix_graphemes == n.copy_prefix_graphemes
                && (o.visual_row_end - o.visual_row_start)
                    == (n.visual_row_end - n.visual_row_start)
                && (o.visual_row_start - old_start) == (n.visual_row_start - new_start)
        })
    }

    fn resize(&mut self, new_width: u16) {
        self.resize_with_scroll_anchor(new_width, None);
    }

    fn resize_with_scroll_anchor(
        &mut self,
        new_width: u16,
        scroll_anchor: Option<(ViewportAnchor, u64)>,
    ) {
        if self.width == new_width && self.cache.read().width == new_width {
            return;
        }
        self.width = new_width;
        // 清空 hash 缓存，强制全量重渲染
        self.message_hashes.clear();
        // #626：宽度变化使双变体与 wrap 分段缓存全部失效
        self.alt_lines.clear();
        self.alt_links.clear();
        self.alt_keys.clear();
        self.alt_epochs.clear();
        self.wrap_segs = [Vec::new(), Vec::new()];
        // 用 last_messages 重新渲染（宽度变化需要重新计算所有行的 wrap）
        if !self.last_messages.is_empty() {
            let messages = std::mem::take(&mut self.last_messages);
            self.rebuild_safe_with_anchor(messages, scroll_anchor);
        }
        if self.last_messages.is_empty() {
            self.cache.write().width = new_width;
        }
    }

    fn handle_event(&mut self, event: RenderEvent, rx: &mut mpsc::Receiver<RenderEvent>) {
        self.handle_event_with_scroll_anchor(event, rx, None);
    }

    fn handle_event_with_scroll_anchor(
        &mut self,
        event: RenderEvent,
        rx: &mut mpsc::Receiver<RenderEvent>,
        scroll_anchor: Option<(ViewportAnchor, u64)>,
    ) {
        let _ = rx;
        match event {
            RenderEvent::WithDisplay { display, event } => {
                self.apply_display(display);
                self.handle_event_with_scroll_anchor(*event, rx, scroll_anchor);
            }
            RenderEvent::WithViewport { viewport, event } => {
                let anchor = (!viewport.follow_tail)
                    .then_some(viewport.anchor)
                    .flatten()
                    .map(|anchor| (anchor, viewport.scroll_revision));
                self.handle_event_with_scroll_anchor(*event, rx, anchor);
            }
            RenderEvent::SetDetailStrategy(_) | RenderEvent::Viewport { .. } => {}
            RenderEvent::CopySelection { reply, .. } => {
                let _ = reply.send(None);
            }
            RenderEvent::Rebuild(messages) => {
                self.rebuild_safe(messages);
            }
            RenderEvent::RebuildWithAnchor {
                messages,
                anchor,
                scroll_revision,
            } => {
                self.rebuild_safe_with_anchor(messages, Some((anchor, scroll_revision)));
            }
            RenderEvent::Resize(new_width) => {
                // 合并连续 Resize 由 RenderWorker 层统一处理（保序且不吞控制
                // 事件）；这里只做宽度失效 + 全量重渲染（#650 抽出的 resize()）。
                self.resize(new_width);
            }
            RenderEvent::ResizeAndWait(new_width, done) => {
                self.resize(new_width);
                let _ = done.send(());
            }
            RenderEvent::PrepareWidth { request, reply } => {
                self.resize(request.width);
                let full_width_rows = self.cache.read().total_lines;
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
                self.resize(width);
                let _ = reply.send(PreparedWidth {
                    width,
                    full_width_rows,
                    measured_rows: self.cache.read().total_lines,
                });
            }
            RenderEvent::Clear => {
                self.message_lines.clear();
                self.message_lines.shrink_to_fit();
                self.message_links = Vec::new();
                self.alt_lines.clear();
                self.alt_links.clear();
                self.alt_keys.clear();
                self.alt_epochs.clear();
                self.line_epochs.clear();
                self.wrap_segs = [Vec::new(), Vec::new()];
                self.message_hashes.clear();
                self.message_hashes.shrink_to_fit();
                self.last_messages.clear();
                self.last_messages.shrink_to_fit();
                let mut next = RenderCache::new();
                let old = {
                    let mut cache = self.cache.write();
                    next.version = cache.version + 1;
                    next.width = cache.width;
                    std::mem::replace(&mut *cache, next)
                };
                // 清空同样只在锁内交换，旧行、换行表与链接在锁外释放。
                drop(old);
            }
            RenderEvent::ToggleToolMessages(show) => {
                self.show_tool_messages = show;
                // collapsed 状态是 hash 的一部分，ToggleToolMessages 会改变消息的 hash
                // 但如果 App 端没有修改 view_messages 中的 collapsed 状态，
                // 需要 App 发送新的 Rebuild 事件来反映变化
                // 这里只更新标志位，实际渲染由后续 Rebuild 驱动
            }
            RenderEvent::ToggleDiff(show) => {
                self.diff_visible = show;
                // #626：渲染键已包含 diff 标志，切换后按键复用/交换变体即可，
                // 不再清空 message_hashes 强制全量重渲染。
                if !self.last_messages.is_empty() {
                    let messages = std::mem::take(&mut self.last_messages);
                    self.rebuild_safe_with_anchor(messages, scroll_anchor);
                }
            }
            RenderEvent::ToggleDetail(show) => {
                self.detail_mode = show;
                // #626：渲染键已包含 detail 标志，切换后按键复用/交换变体即可，
                // 不再清空 message_hashes 强制全量重渲染。
                if !self.last_messages.is_empty() {
                    let messages = std::mem::take(&mut self.last_messages);
                    self.rebuild_safe_with_anchor(messages, scroll_anchor);
                }
            }
            RenderEvent::ToggleHookRows(show) => {
                self.hook_rows_visible = show;
                // 渲染键包含 hook 标志（仅对带 hook 行的消息敏感），切换后按
                // 键复用/交换变体即可；历史行不入 VM 数据，只影响渲染。
                if !self.last_messages.is_empty() {
                    let messages = std::mem::take(&mut self.last_messages);
                    self.rebuild_safe_with_anchor(messages, scroll_anchor);
                }
            }
        }
    }

    /// rebuild 的安全包装：捕获 panic 防止渲染线程死亡。
    /// panic 信息通过全局 panic hook（main.rs）记录到日志并通知 TUI。
    fn rebuild_safe(&mut self, messages: Vec<MessageViewModel>) {
        self.rebuild_safe_with_anchor(messages, None);
    }

    fn rebuild_safe_with_anchor(
        &mut self,
        messages: Vec<MessageViewModel>,
        scroll_anchor: Option<(ViewportAnchor, u64)>,
    ) {
        use std::panic::AssertUnwindSafe;
        let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
            if scroll_anchor.is_some() {
                self.rebuild_with_scroll_anchor(messages, scroll_anchor);
            } else {
                self.rebuild(messages);
            }
        }));
        if let Err(panic_payload) = result {
            // panic hook 已处理日志和通知，这里只需确保渲染线程存活
            tracing::error!(
                "render task rebuild panicked: {:?}",
                panic_payload
                    .downcast_ref::<String>()
                    .cloned()
                    .unwrap_or_else(|| "unknown panic".to_string())
            );
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
        message_links: Vec::new(),
        message_hashes: Vec::new(),
        cache: Arc::clone(&cache),
        notify: Arc::clone(&notify),
        width,
        show_tool_messages: false,
        diff_visible: false,
        hook_rows_visible: true,
        detail_mode: false,
        theme_fingerprint: crate::ui::theme::fingerprint(),
        alt_lines: Vec::new(),
        alt_links: Vec::new(),
        alt_keys: Vec::new(),
        alt_epochs: Vec::new(),
        line_epochs: Vec::new(),
        epoch_seq: 0,
        wrap_segs: [Vec::new(), Vec::new()],
    };

    tokio::spawn(RenderWorker::new(task).run(rx));

    (tx, cache, notify)
}

#[cfg(test)]
#[path = "render_thread_test.rs"]
mod tests;

#[cfg(test)]
#[path = "cache_publish_test.rs"]
mod cache_publish_tests;

#[cfg(test)]
#[path = "tick_scroll_anchor_test.rs"]
mod tick_scroll_anchor_tests;

#[cfg(test)]
#[path = "render_display_test.rs"]
mod display_tests;
