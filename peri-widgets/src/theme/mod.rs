mod presets;

pub use presets::DarkTheme;

use ratatui::style::Color;

/// 纯 UI 颜色主题 trait——不含业务语义方法
///
/// 组件通过此 trait 查询颜色，不硬编码色值。
/// 业务特有颜色（工具分级色、模型信息色等）由调用方在 TUI 层自行管理。
pub trait Theme: Send + Sync + 'static {
    // ── 强调色 ──────────────────────────────────────────────
    /// 主交互色（激活边框、光标、关键操作）
    fn accent(&self) -> Color;

    // ── 功能色 ──────────────────────────────────────────────
    /// 成功/完成色
    fn success(&self) -> Color;
    /// 次要强调/警告色
    fn warning(&self) -> Color;
    /// 错误/拒绝色
    fn error(&self) -> Color;
    /// 推理/思考色
    fn thinking(&self) -> Color;

    // ── 文字层级 ────────────────────────────────────────────
    /// 主文字（需要立即看到的内容）
    fn text(&self) -> Color;
    /// 次要文字（标签、路径、辅助信息）
    fn muted(&self) -> Color;
    /// 极弱文字（占位、已完成项、分隔符）
    fn dim(&self) -> Color;

    // ── 边框 ────────────────────────────────────────────────
    /// 空闲边框色
    fn border(&self) -> Color;
    /// 激活边框色（输入框/当前 panel focus）
    fn border_active(&self) -> Color;

    // ── 弹窗专用 ────────────────────────────────────────────
    /// 弹窗底色（Clear 后的背景）
    fn popup_bg(&self) -> Color;
    /// 光标行背景（列表选中行）
    fn cursor_bg(&self) -> Color;

    // ── 状态 ────────────────────────────────────────────────
    /// Loading 色（高辨识度状态指示）
    fn loading(&self) -> Color;

    // ── 业务语义色 ──────────────────────────────────────────
    /// 用户消息背景色
    fn user_bg(&self) -> Color {
        Color::Rgb(55, 55, 55)
    }

    /// Bash 工具边框色
    fn bash_border(&self) -> Color {
        Color::Rgb(253, 93, 177)
    }

    // ── Diff 高亮色 ─────────────────────────────────────────
    /// Diff 新增行前景色
    fn diff_add(&self) -> Color {
        Color::Rgb(63, 185, 80)
    } // DIFF_ADD #3FB950
    /// Diff 新增行背景色
    fn diff_add_bg(&self) -> Color {
        Color::Rgb(18, 52, 26)
    } // DIFF_ADD_BG #12341A
    /// Diff 新增单词高亮背景色
    fn diff_add_word_bg(&self) -> Color {
        Color::Rgb(26, 78, 36)
    } // DIFF_ADD_WORD_BG #1A4E24
    /// Diff 删除行前景色
    fn diff_remove(&self) -> Color {
        Color::Rgb(248, 81, 73)
    } // DIFF_REMOVE #F85149
    /// Diff 删除行背景色
    fn diff_remove_bg(&self) -> Color {
        Color::Rgb(55, 20, 18)
    } // DIFF_REMOVE_BG #371412
    /// Diff 删除单词高亮背景色
    fn diff_remove_word_bg(&self) -> Color {
        Color::Rgb(78, 28, 22)
    } // DIFF_REMOVE_WORD_BG #4E1C16
    /// Diff hunk 头部颜色
    fn diff_hunk(&self) -> Color {
        Color::Rgb(87, 143, 169)
    } // DIFF_HUNK #578FA9
}
