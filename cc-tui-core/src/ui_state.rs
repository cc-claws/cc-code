//! 会话交互的纯状态核心（零 UI 依赖）。
//!
//! cc-tui 的 `UiState` 组合本结构 + 表现层侧车（TextArea、Rect 几何、
//! 屏幕选区快照）。本结构只承载"用户意图与会话状态"，不持有任何
//! ratatui / tui-textarea / cc-widgets 类型。

use crate::display_flags::DisplayFlags;
use crate::i18n::LcRegistry;

#[derive(Clone, Debug)]
pub struct SteerForegroundGrace {
    pub task_id: String,
    pub started_at: std::time::Instant,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PastedTextBlock {
    pub placeholder: String,
    pub content: String,
}

/// UI 交互状态：会话级的输入、滚动、选区、历史等（不含表现层控件与几何）。
pub struct UiStateCore {
    pub loading: bool,
    pub scroll_offset: usize,
    /// 虚拟窗口尚未就绪时保留实际显示位置，不能拿请求位置解释鼠标选区。
    pub presented_scroll_offset: usize,
    pub scroll_follow: bool,
    /// 用户滚动/切换视口的版本，异步布局只能恢复同一版本的位置。
    pub scroll_revision: u64,
    /// 本次定位来自相对滚动；Home、滚动条和视口恢复使用绝对坐标。
    pub scroll_relative: bool,
    pub show_tool_messages: bool,
    pub hint_cursor: Option<usize>,
    /// Esc 关掉候选浮层时记下当时的输入文本，文本不变就不再弹。
    pub hint_dismissed_for: Option<String>,
    pub input_history: Vec<String>,
    pub history_index: Option<usize>,
    pub draft_input: Option<String>,
    /// 跨淘汰消息复制由渲染任务分段处理，主循环只轮询结果。
    pub pending_text_copy: Option<(u64, tokio::sync::oneshot::Receiver<Option<String>>)>,
    /// 最近一次鼠标左键单击的 (时间, 行, 列)，用于双击选行检测。
    pub last_left_click: Option<(std::time::Instant, u16, u16)>,
    /// 请求下一帧先清理物理终端，再交给 ratatui 全量重绘。
    pub force_terminal_clear_redraw: bool,
    /// textarea MouseDown 暂存的屏幕坐标。
    pub pending_screen_start: Option<(u16, u16)>,
    /// 欢迎卡的完整视觉行数；鼠标命中缓存消息时从统一滚动坐标扣除。
    pub message_prefix_rows: usize,
    /// 全宽正文测量（列宽、内容布局版本、视觉行数）。
    pub full_width_message_rows: Option<(u16, u64, usize)>,
    pub message_scrollbar_dragging: bool,
    /// 按下时的鼠标行、内容偏移和滑块可移动行数，避免量化误差累积。
    pub message_scrollbar_drag_origin: Option<(u16, usize, u16)>,
    /// 虚拟布局实测高度可能变化；一次拖拽内固定按下时的滚动比例。
    pub message_scrollbar_drag_max_offset: Option<usize>,
    /// 输入框上方待发消息的分页起点。
    pub queued_messages_offset: usize,
    pub copy_message_until: Option<std::time::Instant>,
    pub copy_char_count: usize,
    /// 浮动提示（如模型切换、复制成功等）。
    pub toast: Option<(String, std::time::Instant)>,
    pub panel_plain_lines: Vec<String>,
    pub panel_scroll_offset: u16,
    /// 消息区域最小/最大偏移量。
    pub scrollbar_min_offset: usize,
    pub scrollbar_max_offset: usize,
    pub panel_scrollbar_dragging: bool,
    /// 后台 Agent Bar 光标位置。
    pub bg_bar_cursor: Option<usize>,
    pub background_tasks_bar_focused: bool,
    pub steer_foreground_grace: Option<SteerForegroundGrace>,
    /// Write/Edit 工具结果内联 diff 是否可见。
    pub diff_visible: bool,
    /// hook 执行结果行是否可见（数据始终保留，只控制渲染）。
    pub hook_rows_visible: bool,
    /// Shell 命令输出详细模式（Ctrl+O 切换）。
    pub detail_mode: bool,
    /// 输入框中被占位符折叠展示的外部多行粘贴内容。
    pub pasted_text_blocks: Vec<PastedTextBlock>,
    pub next_pasted_text_id: usize,
    pub cursor_visible: bool,
    pub cursor_tick_count: u8,
    pub scrollbar_hover: bool,
    pub scroll_to_bottom_hover: bool,
    pub scroll_baseline_lines: usize,
    pub scroll_baseline_len: usize,
    pub scroll_to_bottom_unseen: bool,
}

impl UiStateCore {
    pub fn new(
        input_history: Vec<String>,
        detail_enabled: bool,
        diff_enabled: bool,
        hook_rows_visible: bool,
    ) -> Self {
        Self {
            loading: false,
            scroll_offset: usize::MAX,
            presented_scroll_offset: 0,
            scroll_follow: true,
            scroll_revision: 0,
            scroll_relative: false,
            show_tool_messages: false,
            hint_cursor: None,
            hint_dismissed_for: None,
            input_history,
            history_index: None,
            draft_input: None,
            pending_text_copy: None,
            last_left_click: None,
            force_terminal_clear_redraw: false,
            pending_screen_start: None,
            message_prefix_rows: 0,
            full_width_message_rows: None,
            message_scrollbar_dragging: false,
            message_scrollbar_drag_origin: None,
            message_scrollbar_drag_max_offset: None,
            queued_messages_offset: 0,
            copy_message_until: None,
            copy_char_count: 0,
            toast: None,
            panel_plain_lines: Vec::new(),
            panel_scroll_offset: 0,
            scrollbar_min_offset: 0,
            scrollbar_max_offset: 0,
            panel_scrollbar_dragging: false,
            bg_bar_cursor: None,
            background_tasks_bar_focused: false,
            steer_foreground_grace: None,
            diff_visible: diff_enabled,
            hook_rows_visible,
            detail_mode: detail_enabled,
            pasted_text_blocks: Vec::new(),
            next_pasted_text_id: 1,
            cursor_visible: true,
            cursor_tick_count: 0,
            scrollbar_hover: false,
            scroll_to_bottom_hover: false,
            scroll_baseline_lines: 0,
            scroll_baseline_len: 0,
            scroll_to_bottom_unseen: false,
        }
    }

    pub fn display(&self) -> DisplayFlags {
        DisplayFlags {
            detail_mode: self.detail_mode,
            diff_visible: self.diff_visible,
            show_tool_messages: self.show_tool_messages,
        }
    }

    pub fn apply_display(&mut self, display: DisplayFlags) {
        self.detail_mode = display.detail_mode;
        self.diff_visible = display.diff_visible;
        self.show_tool_messages = display.show_tool_messages;
    }

    pub fn set_scroll_position(&mut self, offset: usize, follow: bool) {
        self.scroll_offset = offset;
        self.scroll_follow = follow;
        self.scroll_relative = false;
        self.scroll_revision = self.scroll_revision.wrapping_add(1);
    }

    pub fn set_relative_scroll_position(&mut self, offset: usize, follow: bool) {
        self.set_scroll_position(offset, follow);
        self.scroll_relative = true;
    }

    /// 推进光标闪烁状态；返回 true 表示可见性发生切换，调用方应触发重绘。
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

    pub fn set_toast(&mut self, text: impl Into<String>, duration: std::time::Duration) {
        self.toast = Some((text.into(), std::time::Instant::now() + duration));
    }

    /// 获取当前未过期的浮动 Toast 提示文本。
    pub fn active_toast(&self, lc: &LcRegistry) -> Option<String> {
        let now = std::time::Instant::now();
        if let Some((ref text, until)) = self.toast {
            if now < until {
                return Some(text.clone());
            }
        }
        if let Some(until) = self.copy_message_until {
            if now < until {
                return Some(lc.tr_args(
                    "statusbar-copied",
                    &[("count".into(), (self.copy_char_count as i64).into())],
                ));
            }
        }
        None
    }
}
