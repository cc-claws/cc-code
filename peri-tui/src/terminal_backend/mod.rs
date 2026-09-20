//! Keep the physical terminal's output aligned with Ratatui's logical cells.

#[cfg(any(windows, test))]
mod compatible;
#[cfg(windows)]
mod windows;

#[cfg(not(windows))]
pub type TuiBackend<W> = ratatui::backend::CrosstermBackend<W>;
#[cfg(windows)]
pub type TuiBackend<W> =
    compatible::WidthSafeBackend<ratatui::backend::CrosstermBackend<W>, windows::ConsoleWidthProbe>;

#[cfg(windows)]
impl<W: std::io::Write> TuiBackend<W> {
    pub fn new(writer: W) -> Self {
        Self::with_probe(
            ratatui::backend::CrosstermBackend::new(writer),
            windows::ConsoleWidthProbe::new(),
        )
    }
}
