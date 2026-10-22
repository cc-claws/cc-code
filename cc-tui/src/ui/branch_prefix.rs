//! cc-tui/src/ui/branch_prefix.rs
//! 系统级分支折角前缀公共组件：统一全系统 `⎿` 的字符几何、悬挂缩进与配色标准。

use ratatui::style::Style;
use ratatui::text::{Line, Span};

/// 分支折角前缀组件
pub struct BranchPrefix;

impl BranchPrefix {
    /// 核心符号
    pub const SYMBOL: &'static str = "⎿";

    /// 一级分支折角（前 2 空格 + ⎿ + 后 2 空格 = 5 列）
    pub const PREFIX: &'static str = "  ⎿  ";
    /// 一级悬挂缩进（5 空格）
    pub const HANG: &'static str = "     ";
    /// 一级占用显示宽度
    pub const WIDTH: usize = 5;

    /// 二级嵌套折角（前 5 空格 + ⎿ + 后 2 空格 = 8 列）
    pub const NESTED_PREFIX: &'static str = "     ⎿  ";
    /// 二级嵌套悬挂缩进（8 空格）
    pub const NESTED_HANG: &'static str = "        ";
    /// 二级嵌套占用显示宽度
    pub const NESTED_WIDTH: usize = 8;

    /// 构造标准符号 Span（根据成功/失败返回 inactive 或 error 颜色）
    #[inline]
    pub fn span(is_error: bool) -> Span<'static> {
        let fg = if is_error {
            crate::ui::theme::cur().error
        } else {
            crate::ui::theme::cur().inactive
        };
        Span::styled(Self::PREFIX, Style::default().fg(fg))
    }

    /// 构造嵌套符号 Span
    #[inline]
    pub fn nested_span(is_error: bool) -> Span<'static> {
        let fg = if is_error {
            crate::ui::theme::cur().error
        } else {
            crate::ui::theme::cur().inactive
        };
        Span::styled(Self::NESTED_PREFIX, Style::default().fg(fg))
    }

    /// 计算在总宽度 `width` 下，扣除一级前缀后的可用正文宽度
    #[inline]
    pub fn content_width(width: usize) -> usize {
        width.saturating_sub(Self::WIDTH).max(20)
    }

    /// 计算扣除二级嵌套前缀后的可用正文宽度
    #[inline]
    pub fn nested_content_width(width: usize) -> usize {
        width.saturating_sub(Self::NESTED_WIDTH).max(20)
    }

    /// 判定 Line 首个 Span 是否为标准分支前缀
    #[inline]
    pub fn is_branch_line(line: &Line<'_>) -> bool {
        line.spans
            .first()
            .is_some_and(|s| s.content == Self::PREFIX)
    }

    /// 判定文本行是否包含或以分支前缀开头（兼容剥离前缀匹配）
    #[inline]
    pub fn strip_prefix(text: &str) -> Option<&str> {
        text.strip_prefix(Self::PREFIX).or_else(|| {
            text.trim_start()
                .strip_prefix(Self::SYMBOL)
                .map(|s| s.trim_start())
        })
    }
}

#[cfg(test)]
#[path = "branch_prefix_test.rs"]
mod branch_prefix_test;
