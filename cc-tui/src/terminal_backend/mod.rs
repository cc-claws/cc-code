//! Keep the physical terminal's output aligned with Ratatui's logical cells.
//!
//! 两层校正都在 [`compatible::WidthSafeBackend`]：Windows 用独立控制台缓冲区测量
//! 字形列宽（`windows::ConsoleWidthProbe`），其余平台用启动期 CPR 往返测量
//! （`cursor_probe::CursorWidthProbe`）。两侧探针都可能在量不到时返回空结果，
//! 此时输出与裸 `CrosstermBackend` 完全一致。

mod compatible;
#[cfg(not(windows))]
mod cursor_probe;
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
        Self::with_probe(
            ratatui::backend::CrosstermBackend::new(writer),
            windows::ConsoleWidthProbe::new(),
        )
    }
}

#[cfg(not(windows))]
impl<W: std::io::Write> TuiBackend<W> {
    /// 构造前会先测量宿主终端的字形列宽：`CursorWidthProbe` 要向 stdin 收 CPR 回包，
    /// 所以必须在输入线程启动之前调用（见 `main.rs` 的启动顺序注释）。
    pub fn new(mut writer: W) -> Self {
        let probe = cursor_probe::CursorWidthProbe::measure(&mut writer);
        Self::with_probe(ratatui::backend::CrosstermBackend::new(writer), probe)
    }
}
