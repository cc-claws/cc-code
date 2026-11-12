use std::ops::{Deref, DerefMut};

use cc_widgets::ScrollbarMetrics;
use tui_textarea::TextArea;

use super::at_mention::AtMentionState;
use crate::app::text_selection::{
    PanelTextSelection, PresentedSelectionSnapshot, ScreenSelection, ScreenSnapshot, TextSelection,
};
use crate::ui::render_thread::ViewportAnchor;

pub use cc_tui_core::ui_state::{PastedTextBlock, SteerForegroundGrace, UiStateCore};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MessageScrollbarMetrics {
    pub bar_area: ratatui::layout::Rect,
    pub thumb_area: ratatui::layout::Rect,
    pub max_offset: usize,
    pub up_btn_area: Option<ratatui::layout::Rect>,
    pub down_btn_area: Option<ratatui::layout::Rect>,
}

/// UI 交互状态：纯状态核心（`cc-tui-core::UiStateCore`）+ 表现层控件与几何。
///
/// 纯状态字段通过 `Deref`/`DerefMut` 委托给 `core`，全仓 `ui.scroll_offset` 等
/// 访问零改动；表现层专属字段（TextArea、Rect 命中几何、Buffer 快照）留在本结构。
pub struct UiState {
    pub core: UiStateCore,
    /// 未被渲染结果确认的原始滚轮步骤；不能在估算坐标顶部提前丢弃。
    pub pending_relative_scroll: Option<(ViewportAnchor, usize, Vec<(u64, isize)>)>,
    /// 消息之后的 spinner 等尾部占位，供相对滚动计算真实视口底边。
    pub message_suffix_rows: usize,
    pub textarea: TextArea<'static>,
    pub text_selection: TextSelection,
    pub presented_selection_snapshot: Option<PresentedSelectionSnapshot>,
    /// 全局屏幕选区（面板/状态栏/sticky header/bg agent bar/空白区域，绝对屏幕坐标）
    pub screen_selection: ScreenSelection,
    /// terminal.draw() 后的 Buffer 文本快照，供 MouseUp 提取 screen 选区文本
    pub screen_snapshot: Option<ScreenSnapshot>,
    pub messages_area: Option<ratatui::layout::Rect>,
    pub message_scrollbar_metrics: Option<MessageScrollbarMetrics>,
    /// 隐藏时仍保留最右侧轨道的位置，用于悬停唤出。
    pub message_scrollbar_area: Option<ratatui::layout::Rect>,
    pub textarea_area: Option<ratatui::layout::Rect>,
    /// 输入框上方待发消息的鼠标命中区域。
    pub queued_messages_area: Option<ratatui::layout::Rect>,
    pub queued_message_actions: Vec<(ratatui::layout::Rect, super::QueuedMessageAction)>,
    pub panel_selection: PanelTextSelection,
    pub panel_area: Option<ratatui::layout::Rect>,
    pub panel_scrollbar_metrics: Option<ScrollbarMetrics>,
    /// @ 文件提及状态
    pub at_mention: AtMentionState,
    /// 后台 Agent Bar 渲染区域（用于鼠标点击检测）
    pub bg_bar_area: Option<ratatui::layout::Rect>,
    /// "回到最新" 浮层按钮的命中区域
    pub scroll_to_bottom_area: Option<ratatui::layout::Rect>,
}

impl Deref for UiState {
    type Target = UiStateCore;

    fn deref(&self) -> &Self::Target {
        &self.core
    }
}

impl DerefMut for UiState {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.core
    }
}

impl UiState {
    pub fn set_scroll_position(&mut self, offset: usize, follow: bool) {
        self.pending_relative_scroll = None;
        self.core.set_scroll_position(offset, follow);
    }

    pub fn new(
        textarea: TextArea<'static>,
        cwd: &str,
        detail_enabled: bool,
        diff_enabled: bool,
        hook_rows_visible: bool,
    ) -> Self {
        let _ = cwd; // 历史路径已迁移至 ~/.cc-code/，cwd 保留用于未来扩展
        Self {
            core: UiStateCore::new(
                super::history_persistence::load_input_history(),
                detail_enabled,
                diff_enabled,
                hook_rows_visible,
            ),
            textarea,
            pending_relative_scroll: None,
            message_suffix_rows: 0,
            text_selection: TextSelection::new(),
            presented_selection_snapshot: None,
            screen_selection: ScreenSelection::default(),
            screen_snapshot: None,
            messages_area: None,
            message_scrollbar_metrics: None,
            message_scrollbar_area: None,
            textarea_area: None,
            queued_messages_area: None,
            queued_message_actions: Vec::new(),
            panel_selection: PanelTextSelection::new(),
            panel_area: None,
            panel_scrollbar_metrics: None,
            at_mention: AtMentionState::new(),
            bg_bar_area: None,
            scroll_to_bottom_area: None,
        }
    }
}

#[cfg(test)]
#[path = "ui_state_test.rs"]
mod tests;
