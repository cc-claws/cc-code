//! Keep the physical terminal's output aligned with Ratatui's logical cells.
//!
//! 两层校正都在 [`compatible::WidthSafeBackend`]：Windows 用独立控制台缓冲区测量
//! 字形列宽（`windows::ConsoleWidthProbe`），其余平台用启动期 CPR 往返测量
//! （`cursor_probe::CursorWidthProbe`）。两侧探针都可能在量不到时返回空结果，
//! 此时输出与裸 `CrosstermBackend` 完全一致。
//!
//! 颜色校正同样在这一层：`TuiBackend::new` 按 [`crate::terminal_color`] 探测到的
//! 色深，把 `Color::Rgb` 降级成 256 色 / 16 色 / 无色。渲染侧始终按主题调色板
//! 取真彩色，因此配色表、主题指纹与渲染缓存都不受影响。

mod compatible;
#[cfg(not(windows))]
mod cursor_probe;
pub mod hyperlink;
pub mod viewport;
#[cfg(windows)]
mod windows;

#[cfg(windows)]
pub type TuiBackend<W> =
    compatible::WidthSafeBackend<ratatui::backend::CrosstermBackend<W>, windows::ConsoleWidthProbe>;
#[cfg(not(windows))]
pub type TuiBackend<W> = compatible::WidthSafeBackend<
    ratatui::backend::CrosstermBackend<W>,
    cursor_probe::CursorWidthProbe,
>;

#[cfg(windows)]
impl<W: std::io::Write> TuiBackend<W> {
    pub fn new(writer: W) -> Self {
        Self::with_probe_and_depth(
            ratatui::backend::CrosstermBackend::new(writer),
            windows::ConsoleWidthProbe::new(),
            crate::terminal_color::depth(),
        )
    }
}

#[cfg(not(windows))]
impl<W: std::io::Write> TuiBackend<W> {
    /// 构造前会先测量宿主终端的字形列宽：`CursorWidthProbe` 要向 stdin 收 CPR 回包，
    /// 所以必须在输入线程启动之前调用（见 `main.rs` 的启动顺序注释）。
    pub fn new(mut writer: W) -> Self {
        let probe = cursor_probe::CursorWidthProbe::measure(&mut writer);
        Self::with_probe_and_depth(
            ratatui::backend::CrosstermBackend::new(writer),
            probe,
            crate::terminal_color::depth(),
        )
    }
}
