use peri_widgets::ScrollbarMetrics;
use tui_textarea::TextArea;

use super::at_mention::AtMentionState;
use crate::app::text_selection::{PanelTextSelection, TextSelection};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MessageScrollbarMetrics {
    pub bar_area: ratatui::layout::Rect,
    pub max_offset: usize,
    pub up_btn_area: Option<ratatui::layout::Rect>,
    pub down_btn_area: Option<ratatui::layout::Rect>,
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
    pub scroll_follow: bool,
    pub show_tool_messages: bool,
    pub hint_cursor: Option<usize>,
    pub input_history: Vec<String>,
    pub history_index: Option<usize>,
    pub draft_input: Option<String>,
    pub text_selection: TextSelection,
    pub messages_area: Option<ratatui::layout::Rect>,
    pub message_scrollbar_metrics: Option<MessageScrollbarMetrics>,
    pub message_scrollbar_dragging: bool,
    pub textarea_area: Option<ratatui::layout::Rect>,
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
    /// 后台 Agent Bar 渲染区域（用于鼠标点击检测）
    pub bg_bar_area: Option<ratatui::layout::Rect>,
    /// Write/Edit 工具结果内联 diff 是否可见
    pub diff_visible: bool,
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
}

impl UiState {
    pub fn new(
        textarea: TextArea<'static>,
        cwd: &str,
        detail_enabled: bool,
        diff_enabled: bool,
    ) -> Self {
        let _ = cwd; // 历史路径已迁移至 ~/.peri/，cwd 保留用于未来扩展
        let input_history = super::history_persistence::load_input_history();
        Self {
            textarea,
            loading: false,
            scroll_offset: usize::MAX,
            scroll_follow: true,
            show_tool_messages: false,
            hint_cursor: None,
            input_history,
            history_index: None,
            draft_input: None,
            text_selection: TextSelection::new(),
            messages_area: None,
            message_scrollbar_metrics: None,
            message_scrollbar_dragging: false,
            textarea_area: None,
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
            bg_bar_area: None,
            diff_visible: diff_enabled,
            detail_mode: detail_enabled,
            pasted_text_blocks: Vec::new(),
            next_pasted_text_id: 1,
            cursor_visible: true,
            cursor_tick_count: 0,
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
}
