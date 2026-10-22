//! 本次终端运行的视口状态；不参与会话历史或持久化。

use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use ratatui::crossterm::{cursor, event, execute, terminal};
use ratatui::layout::{Rect, Size};

static ALTERNATE_SCREEN: AtomicBool = AtomicBool::new(false);
// 高 16 位保存正常屏起点，低 16 位保存末行的下一行；0 表示没有待清理区域。
static OWNED_AREA: AtomicU32 = AtomicU32::new(0);
static TITLE_SAVED: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Debug)]
pub struct ViewportState {
    pub terminal_size: Size,
    pub origin_row: u16,
    pub available_rows: u16,
    pub allocated_rows: u16,
    pub fullscreen_latched: bool,
    pub startup_welcome: bool,
    welcome_session: Option<uuid::Uuid>,
}

impl Default for ViewportState {
    fn default() -> Self {
        Self {
            terminal_size: Size::default(),
            origin_row: 0,
            available_rows: 0,
            allocated_rows: 0,
            fullscreen_latched: true,
            startup_welcome: false,
            welcome_session: None,
        }
    }
}

#[derive(Debug)]
pub struct ViewportPlan {
    pub area: Rect,
    pub scroll_rows: u16,
    pub enter_fullscreen: bool,
}

impl ViewportState {
    pub fn growing(origin_row: u16) -> Self {
        Self {
            origin_row,
            fullscreen_latched: false,
            ..Self::default()
        }
    }

    /// 会话 ID 在新建/恢复时改变，首条消息提交不会改变它。
    pub fn sync_welcome(&mut self, session: uuid::Uuid, has_history: bool) {
        if self.welcome_session != Some(session) {
            self.welcome_session = Some(session);
            self.startup_welcome = !has_history;
        }
    }

    pub fn prepare(&mut self, required_rows: u16, size: Size) -> ViewportPlan {
        self.terminal_size = size;
        let mut scroll_rows = 0;
        let was_fullscreen = self.fullscreen_latched;
        let next_rows = self.allocated_rows.max(required_rows).max(1);
        if !self.fullscreen_latched {
            self.origin_row = self.origin_row.min(size.height.saturating_sub(1));
            if next_rows < size.height {
                // 每次增长都只滚动缺少的行，把应用起点上移；应用本身长满
                // 宿主高度后才切全屏，不因启动位置靠近屏底提前锁定。
                // 先减再比较，避免 origin + next_rows 在极大尺寸下溢出。
                let last_origin = size.height - next_rows;
                scroll_rows = self.origin_row.saturating_sub(last_origin);
                self.origin_row = self.origin_row.saturating_sub(scroll_rows);
                self.available_rows = size.height.saturating_sub(self.origin_row);
            } else {
                self.fullscreen_latched = true;
            }
        }
        let area = if self.fullscreen_latched {
            self.available_rows = size.height;
            self.allocated_rows = size.height;
            Rect::new(0, 0, size.width, size.height)
        } else {
            self.allocated_rows = next_rows;
            Rect::new(0, self.origin_row, size.width, next_rows)
        };
        ViewportPlan {
            area,
            scroll_rows,
            enter_fullscreen: !was_fullscreen && self.fullscreen_latched,
        }
    }

    /// 向后兼容接口：临时浮层不再计入 `required_rows`，直接委托给 `prepare`。
    pub fn prepare_persistent(
        &mut self,
        required_rows: u16,
        _persistent_rows: u16,
        size: Size,
    ) -> ViewportPlan {
        self.prepare(required_rows, size)
    }
}

pub fn enter_alternate_screen(writer: &mut impl Write) -> io::Result<()> {
    execute!(writer, terminal::EnterAlternateScreen)?;
    ALTERNATE_SCREEN.store(true, Ordering::SeqCst);
    Ok(())
}

pub fn leave_alternate_screen(writer: &mut impl Write) -> io::Result<()> {
    if ALTERNATE_SCREEN.load(Ordering::SeqCst) {
        execute!(writer, terminal::LeaveAlternateScreen)?;
        ALTERNATE_SCREEN.store(false, Ordering::SeqCst);
    }
    Ok(())
}

pub fn record_owned_area(area: Rect) {
    if !ALTERNATE_SCREEN.load(Ordering::SeqCst) && !area.is_empty() {
        let owned = (u32::from(area.top()) << 16) | u32::from(area.bottom());
        OWNED_AREA.store(owned, Ordering::Relaxed);
    }
}

pub fn save_title(writer: &mut impl Write) -> io::Result<()> {
    execute!(writer, ratatui::crossterm::style::Print("\x1b[22;0t"))?;
    TITLE_SAVED.store(true, Ordering::Relaxed);
    Ok(())
}

pub fn restore_title(writer: &mut impl Write) -> io::Result<()> {
    if TITLE_SAVED.swap(false, Ordering::Relaxed) {
        execute!(writer, ratatui::crossterm::style::Print("\x1b[23;0t"))?;
        crate::terminal_title::clear_terminal_title();
    }
    Ok(())
}

/// 只清理应用拥有的正常屏行，回到区域起点等待 Session ID 与 Resume 输出。
/// 直接写入 ANSI 序列，测试可使用内存 writer，不访问真实终端或全局状态。
fn clear_owned_area(writer: &mut impl Write, area: Rect) -> io::Result<()> {
    if area.is_empty() {
        return Ok(());
    }
    for row in area.top()..area.bottom() {
        write!(writer, "\x1b[{};1H\x1b[2K", u32::from(row) + 1)?;
    }
    write!(writer, "\x1b[{};1H", u32::from(area.top()) + 1)?;
    writer.flush()
}

/// 先恢复正常屏，再清掉此前应用区域；成功后消费记录，避免 Drop 重复擦掉退出提示。
pub fn finish_screen(writer: &mut impl Write) -> io::Result<()> {
    if !ALTERNATE_SCREEN.load(Ordering::SeqCst) && OWNED_AREA.load(Ordering::Relaxed) == 0 {
        return Ok(());
    }
    execute!(writer, terminal::BeginSynchronizedUpdate)?;
    let restore_result: io::Result<()> = (|| {
        leave_alternate_screen(writer)?;
        let owned = OWNED_AREA.load(Ordering::Relaxed);
        if owned != 0 {
            let top = (owned >> 16) as u16;
            let bottom = owned as u16;
            clear_owned_area(writer, Rect::new(0, top, 1, bottom.saturating_sub(top)))?;
            let _ = OWNED_AREA.compare_exchange(owned, 0, Ordering::Relaxed, Ordering::Relaxed);
        }
        Ok(())
    })();
    let end_result = execute!(writer, terminal::EndSynchronizedUpdate);
    restore_result?;
    end_result
}

/// 初始化中途报错和主线程 panic 也必须恢复 raw mode 与鼠标。
pub struct TerminalRestoreGuard;

impl Drop for TerminalRestoreGuard {
    fn drop(&mut self) {
        restore_process_terminal();
    }
}

pub fn restore_process_terminal() {
    let mut stdout = io::stdout();
    crate::keyboard_modes::restore_keyboard_enhancement();
    let _ = crate::conpty::disable_mouse_tracking();
    let _ = execute!(
        stdout,
        event::DisableMouseCapture,
        event::DisableBracketedPaste,
        event::DisableFocusChange,
        cursor::Show
    );
    let _ = finish_screen(&mut stdout);
    let _ = restore_title(&mut stdout);
    let _ = terminal::disable_raw_mode();
}

#[cfg(test)]
#[path = "viewport_test.rs"]
mod tests;
