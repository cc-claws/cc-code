use peri_widgets::ScrollbarMetrics;
use tui_textarea::TextArea;

use super::at_mention::AtMentionState;
use crate::app::text_selection::{
    PanelTextSelection, ScreenSelection, ScreenSnapshot, TextSelection,
};

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
    /// 最近一次鼠标左键单击的 (时间, 行, 列)，用于双击选行检测
    pub last_left_click: Option<(std::time::Instant, u16, u16)>,
            screen_selection: ScreenSelection::default(),
            screen_snapshot: None,
            pending_screen_start: None,
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
