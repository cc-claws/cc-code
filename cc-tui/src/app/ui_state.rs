use cc_widgets::ScrollbarMetrics;
use tui_textarea::TextArea;

use super::at_mention::AtMentionState;
use crate::app::text_selection::{
    PanelTextSelection, PresentedSelectionSnapshot, ScreenSelection, ScreenSnapshot, TextSelection,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MessageScrollbarMetrics {
    pub bar_area: ratatui::layout::Rect,
    pub thumb_area: ratatui::layout::Rect,
    pub max_offset: usize,
    pub up_btn_area: Option<ratatui::layout::Rect>,
    pub down_btn_area: Option<ratatui::layout::Rect>,
}

/// 「立即补充」后给前台命令的宽限期登记。
///
/// 点「立即补充」时若有一条 Bash 正在前台跑，不立刻把它扔后台：1s 级短命令会自己
/// 结束、本轮马上收口，白扔后台只会让输出莫名其妙挪走。宽限期内仍在前台跑才移交。
#[derive(Clone, Debug)]
pub struct SteerForegroundGrace {
    /// 前台 agent shell 的稳定 `task_id`。
    pub task_id: String,
    /// 登记时刻。
    pub started_at: std::time::Instant,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PastedTextBlock {
    pub placeholder: String,
    pub content: String,
}

/// UI 交互状态：会话级的输入、滚动、选区、历史等。
pub struct UiState {
    pub textarea: TextArea<'static>,
    pub loading: bool,
    pub scroll_offset: usize,
    /// 虚拟窗口尚未就绪时保留实际显示位置，不能拿请求位置解释鼠标选区。
    pub presented_scroll_offset: usize,
    pub scroll_follow: bool,
    /// 用户滚动/切换视口的版本，异步布局只能恢复同一版本的位置。
    pub scroll_revision: u64,
    pub show_tool_messages: bool,
    pub hint_cursor: Option<usize>,
    /// Esc 关掉候选浮层时记下当时的输入文本，文本不变就不再弹（对齐 CC
    /// `useTypeahead.tsx` 的 `dismissedForInputRef`）。清空或继续编辑即恢复。
    pub hint_dismissed_for: Option<String>,
    pub input_history: Vec<String>,
    pub history_index: Option<usize>,
    pub draft_input: Option<String>,
    pub text_selection: TextSelection,
    /// 跨淘汰消息复制由渲染任务分段处理，主循环只轮询结果。
    pub pending_text_copy: Option<(u64, tokio::sync::oneshot::Receiver<Option<String>>)>,
    pub presented_selection_snapshot: Option<PresentedSelectionSnapshot>,
    /// 最近一次鼠标左键单击的 (时间, 行, 列)，用于双击选行检测
    pub last_left_click: Option<(std::time::Instant, u16, u16)>,
    /// 全局屏幕选区（面板/状态栏/sticky header/bg agent bar/空白区域，绝对屏幕坐标）
    pub screen_selection: ScreenSelection,
    /// terminal.draw() 后的 Buffer 文本快照，供 MouseUp 提取 screen 选区文本
    pub screen_snapshot: Option<ScreenSnapshot>,
    /// 请求下一帧先清理物理终端，再交给 ratatui 全量重绘。
    pub force_terminal_clear_redraw: bool,
    /// textarea MouseDown 暂存的屏幕坐标：判断"textarea 内拖选 vs 拖出切换 screen 选区"
    pub pending_screen_start: Option<(u16, u16)>,
    pub messages_area: Option<ratatui::layout::Rect>,
    /// 欢迎卡的完整视觉行数；鼠标命中缓存消息时从统一滚动坐标扣除。
    pub message_prefix_rows: usize,
    /// 全宽正文测量（列宽、内容布局版本、视觉行数），视口物化不使测量失效。
    pub full_width_message_rows: Option<(u16, u64, usize)>,
    pub message_scrollbar_metrics: Option<MessageScrollbarMetrics>,
    /// 隐藏时仍保留最右侧轨道的位置，用于悬停唤出。
    pub message_scrollbar_area: Option<ratatui::layout::Rect>,
    pub message_scrollbar_dragging: bool,
    /// 按下时的鼠标行、内容偏移和滑块可移动行数，避免量化误差累积。
    pub message_scrollbar_drag_origin: Option<(u16, usize, u16)>,
    /// 虚拟布局实测高度可能变化；一次拖拽内固定按下时的滚动比例。
    pub message_scrollbar_drag_max_offset: Option<usize>,
    pub textarea_area: Option<ratatui::layout::Rect>,
    /// 输入框上方待发消息的分页起点和鼠标命中区域。
    pub queued_messages_offset: usize,
    pub queued_messages_area: Option<ratatui::layout::Rect>,
    pub queued_message_actions: Vec<(ratatui::layout::Rect, super::QueuedMessageAction)>,
    pub copy_message_until: Option<std::time::Instant>,
    pub copy_char_count: usize,
    pub panel_selection: PanelTextSelection,
    pub panel_area: Option<ratatui::layout::Rect>,
    pub panel_plain_lines: Vec<String>,
    pub panel_scroll_offset: u16,
    /// 消息区域最小偏移量；小于该值的历史已交给终端原生 scrollback。
    pub scrollbar_min_offset: usize,
    /// 消息区域滚动条的最大偏移量（内容高度 - 可见高度）
    pub scrollbar_max_offset: usize,
    /// Panel scrollbar geometry for mouse interaction
    pub panel_scrollbar_metrics: Option<ScrollbarMetrics>,
    /// Whether user is currently dragging the panel scrollbar
    pub panel_scrollbar_dragging: bool,
    /// @ 文件提及状态
    pub at_mention: AtMentionState,
    /// 后台 Agent Bar 光标位置
    pub bg_bar_cursor: Option<usize>,
    /// 后台任务 status bar 入口是否聚焦（Down 进入，Enter 打开面板）
    pub background_tasks_bar_focused: bool,
    /// 「立即补充」等待前台命令自行结束的宽限期（见 [`SteerForegroundGrace`]）
    pub steer_foreground_grace: Option<SteerForegroundGrace>,
    /// 后台 Agent Bar 渲染区域（用于鼠标点击检测）
    pub bg_bar_area: Option<ratatui::layout::Rect>,
    /// Write/Edit 工具结果内联 diff 是否可见
    pub diff_visible: bool,
    /// hook 执行结果行是否可见（配置 `hooks_ui`；数据始终保留，只控制渲染）
    pub hook_rows_visible: bool,
    /// Shell 命令输出详细模式（Ctrl+O 切换）
    pub detail_mode: bool,
    /// 输入框中被占位符折叠展示的外部多行粘贴内容
    pub pasted_text_blocks: Vec<PastedTextBlock>,
    /// 当前 draft 内下一个粘贴占位符编号
    pub next_pasted_text_id: usize,
    /// 光标闪烁状态：true=可见，false=隐藏
    pub cursor_visible: bool,
    /// 光标闪烁计数器（每 tick 递增，每 15 tick 切换一次，约 500ms）
    pub cursor_tick_count: u8,
    /// 鼠标是否悬停在滚动条区域
    pub scrollbar_hover: bool,
    /// "回到最新" 浮层按钮的命中区域：仅当离开底部且内容溢出时由渲染层登记
    pub scroll_to_bottom_area: Option<ratatui::layout::Rect>,
    /// 鼠标是否悬停在"回到最新"按钮上（用于高亮）
    pub scroll_to_bottom_hover: bool,
    /// 离开底部时的内容行数基线（跟随底部时持续刷新，用于判断离开后是否新增内容）
    pub scroll_baseline_lines: usize,
    /// 离开底部时的已提交消息数基线（与 `scroll_baseline_lines` 组合判断新活动）
    pub scroll_baseline_len: usize,
    /// 离开底部后内容有增长（新消息/新输出），按钮文案切换为"有新活动"
    pub scroll_to_bottom_unseen: bool,
}

impl UiState {
    pub(crate) fn set_scroll_position(&mut self, offset: usize, follow: bool) {
        self.scroll_offset = offset;
        self.scroll_follow = follow;
        self.scroll_revision = self.scroll_revision.wrapping_add(1);
    }

    pub fn new(
        textarea: TextArea<'static>,
        cwd: &str,
        detail_enabled: bool,
        diff_enabled: bool,
        hook_rows_visible: bool,
    ) -> Self {
        let _ = cwd; // 历史路径已迁移至 ~/.cc-code/，cwd 保留用于未来扩展
        let input_history = super::history_persistence::load_input_history();
        Self {
            textarea,
            loading: false,
            scroll_offset: usize::MAX,
            presented_scroll_offset: 0,
            scroll_follow: true,
            scroll_revision: 0,
            show_tool_messages: false,
            hint_cursor: None,
            hint_dismissed_for: None,
            input_history,
            history_index: None,
            draft_input: None,
            text_selection: TextSelection::new(),
            pending_text_copy: None,
            presented_selection_snapshot: None,
            last_left_click: None,
            screen_selection: ScreenSelection::default(),
            screen_snapshot: None,
            force_terminal_clear_redraw: false,
            pending_screen_start: None,
            messages_area: None,
            message_prefix_rows: 0,
            full_width_message_rows: None,
            message_scrollbar_metrics: None,
            message_scrollbar_area: None,
            message_scrollbar_dragging: false,
            message_scrollbar_drag_origin: None,
            message_scrollbar_drag_max_offset: None,
            textarea_area: None,
            queued_messages_offset: 0,
            queued_messages_area: None,
            queued_message_actions: Vec::new(),
            copy_message_until: None,
            copy_char_count: 0,
            panel_selection: PanelTextSelection::new(),
            panel_area: None,
            panel_plain_lines: Vec::new(),
            panel_scroll_offset: 0,
            scrollbar_min_offset: 0,
            scrollbar_max_offset: 0,
            panel_scrollbar_metrics: None,
            panel_scrollbar_dragging: false,
            at_mention: AtMentionState::new(),
            bg_bar_cursor: None,
            background_tasks_bar_focused: false,
            steer_foreground_grace: None,
            bg_bar_area: None,
            diff_visible: diff_enabled,
            hook_rows_visible,
            detail_mode: detail_enabled,
            pasted_text_blocks: Vec::new(),
            next_pasted_text_id: 1,
            cursor_visible: true,
            cursor_tick_count: 0,
            scrollbar_hover: false,
            scroll_to_bottom_area: None,
            scroll_to_bottom_hover: false,
            scroll_baseline_lines: 0,
            scroll_baseline_len: 0,
            scroll_to_bottom_unseen: false,
        }
    }

    /// 推进光标闪烁状态（每 10 tick 切换一次，约 500ms @ 50ms/tick）
    /// 返回 true 表示可见性发生了切换，调用方应触发重绘
    pub fn advance_cursor_tick(&mut self) -> bool {
        self.cursor_tick_count = self.cursor_tick_count.wrapping_add(1);
        if self.cursor_tick_count >= 10 {
            self.cursor_tick_count = 0;
            self.cursor_visible = !self.cursor_visible;
            true
        } else {
            false
        }
    }

    /// 重置光标为可见状态（用户输入时调用）
    pub fn reset_cursor_blink(&mut self) {
        self.cursor_visible = true;
        self.cursor_tick_count = 0;
    }

    pub fn request_terminal_clear_redraw(&mut self) {
        self.force_terminal_clear_redraw = true;
    }

    pub fn take_terminal_clear_redraw(&mut self) -> bool {
        std::mem::take(&mut self.force_terminal_clear_redraw)
    }
}
