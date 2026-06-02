use std::collections::VecDeque;

use peri_widgets::ScrollbarMetrics;
use tui_textarea::TextArea;

use super::at_mention::AtMentionState;
use crate::app::text_selection::{PanelTextSelection, TextSelection};

/// UI 交互状态：会话级的输入、滚动、选区、历史等。
pub struct UiState {
    pub textarea: TextArea<'static>,
    pub loading: bool,
    pub scroll_offset: u16,
    pub scroll_follow: bool,
    pub show_tool_messages: bool,
    pub hint_cursor: Option<usize>,
    pub input_history: Vec<String>,
    pub history_index: Option<usize>,
    pub draft_input: Option<String>,
    pub text_selection: TextSelection,
    pub messages_area: Option<ratatui::layout::Rect>,
    pub textarea_area: Option<ratatui::layout::Rect>,
    pub copy_message_until: Option<std::time::Instant>,
    pub copy_char_count: usize,
    pub panel_selection: PanelTextSelection,
    pub panel_area: Option<ratatui::layout::Rect>,
    pub panel_plain_lines: Vec<String>,
    pub panel_scroll_offset: u16,
    /// 用户是否正在拖拽消息区域右侧滚动条
    pub scrollbar_dragging: bool,
    /// 消息区域滚动条的最大偏移量（内容高度 - 可见高度）
    pub scrollbar_max_offset: u16,
    /// 滚动条拖拽起始时的鼠标 Y 坐标
    pub scrollbar_drag_start_y: u16,
    /// 滚动条拖拽起始时的 scroll offset
    pub scrollbar_drag_start_offset: u16,
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
    /// 详细模式：强制展开所有工具调用，显示完整内容（Ctrl+O 切换）
    pub detail_mode: bool,
    /// 大段粘贴的占位符和实际内容（占位符, 实际内容）
    pub pending_pastes: Vec<(String, String)>,
}

impl UiState {
    pub fn new(textarea: TextArea<'static>, cwd: &str, detail_enabled: bool) -> Self {
        let _ = cwd; // 历史路径已迁移至 ~/.peri/，cwd 保留用于未来扩展
        let input_history = super::history_persistence::load_input_history();
        Self {
            textarea,
            loading: false,
            scroll_offset: u16::MAX,
            scroll_follow: true,
            show_tool_messages: false,
            hint_cursor: None,
            input_history,
            history_index: None,
            draft_input: None,
            text_selection: TextSelection::new(),
            messages_area: None,
            textarea_area: None,
            copy_message_until: None,
            copy_char_count: 0,
            panel_selection: PanelTextSelection::new(),
            panel_area: None,
            panel_plain_lines: Vec::new(),
            panel_scroll_offset: 0,
            scrollbar_dragging: false,
            scrollbar_max_offset: 0,
            scrollbar_drag_start_y: 0,
            scrollbar_drag_start_offset: 0,
            panel_scrollbar_metrics: None,
            panel_scrollbar_dragging: false,
            at_mention: AtMentionState::new(),
            bg_bar_cursor: None,
            bg_bar_area: None,
            detail_mode: detail_enabled,
            pending_pastes: Vec::new(),
        }
    }

    /// 生成大段粘贴的占位符文本
    pub fn next_large_paste_placeholder(&self, char_count: usize) -> String {
        let base = format!("[Pasted Content {} chars]", char_count);
        let prefix = format!("{} #", base);
        let mut max_suffix = 0usize;

        for (placeholder, _) in &self.pending_pastes {
            if placeholder == &base {
                max_suffix = max_suffix.max(1);
                continue;
            }
            if let Some(suffix) = placeholder.strip_prefix(&prefix) {
                if let Ok(value) = suffix.parse::<usize>() {
                    max_suffix = max_suffix.max(value);
                }
            }
        }

        if max_suffix == 0 {
            base
        } else {
            format!("{} #{}", base, max_suffix + 1)
        }
    }

    /// 展开 pending_pastes 中的占位符，返回完整文本
    pub fn expand_pending_pastes(&self, text: &str) -> String {
        if self.pending_pastes.is_empty() {
            return text.to_string();
        }

        let mut result = text.to_string();
        let mut pending_queue: VecDeque<&str> = VecDeque::new();
        for (placeholder, actual) in &self.pending_pastes {
            if result.contains(placeholder.as_str()) {
                pending_queue.push_back(actual.as_str());
            }
        }

        for (placeholder, _) in &self.pending_pastes {
            if let Some(actual) = pending_queue.pop_front() {
                result = result.replacen(placeholder, actual, 1);
            }
        }

        result
    }
}
