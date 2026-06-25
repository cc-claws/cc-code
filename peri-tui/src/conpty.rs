//! ConPTY mouse tracking workaround.
//!
//! crossterm `EnableMouseCapture` on Windows uses the WinAPI `SetConsoleMode`
//! path (input console) instead of emitting ANSI `?1000h`. Under ConPTY,
//! Windows Terminal only learns mouse tracking is on when ConPTY itself calls
//! `WriteSGR1006` — and ConPTY only does that when an input-mode
//! `SetConsoleMode` call changes the `ENABLE_MOUSE_INPUT` bit.
//!
//! The trap: crossterm's `enable_raw_mode()` already leaves `ENABLE_MOUSE_INPUT`
//! ON (raw mode only clears LINE/ECHO/PROCESSED). So crossterm's subsequent
//! `EnableMouseCapture` `SetConsoleMode(0x98)` call produces *no* MOUSE-bit diff
//! → ConPTY skips `WriteSGR1006` → the frontend never enables mouse tracking →
//! alternate-scroll mode (`?1007`, default ON) converts the wheel into
//! Up/Down arrow keys → the textarea scrolls instead of the message area.
//!
//! Primary fix: `force_conpty_mouse_notify()` toggles the MOUSE bit off then
//! back on *after* `EnableMouseCapture`, forcing ConPTY's diff check to fire
//! `WriteSGR1006(?1003;1006h)` through ConPTY's own output pipe — the only
//! channel ConPTY uses to notify the terminal frontend.
//!
//! Writing `?1000h` to our own output does NOT work reliably: ConPTY's *output*
//! VT state machine does not translate an app-emitted mouse-mode sequence into
//! an input-mode change, so it never notifies the frontend. The ANSI sequence
//! is therefore kept only as defense-in-depth for ConPTY versions that happen
//! to passthrough-emit it.
//!
//! Enable alternate-scroll (`?1007h`) so the wheel is converted to Up/Down arrow
//! keys by ConPTY. The TUI binds plain Up/Down to message-area scrolling and
//! Ctrl+Up/Ctrl+Down to textarea cursor movement, so the converted wheel events
//! naturally scroll the message area without conflicting with text editing.

use anyhow::Result;
#[cfg(windows)]
use std::io;

/// Enable mouse tracking under ConPTY. Must be called *after*
/// `EnterAlternateScreen` and `EnableMouseCapture` on Windows. No-op elsewhere.
pub fn enable_mouse_tracking() -> Result<()> {
    #[cfg(windows)]
    {
        // 主修复：toggle MOUSE bit，强制 ConPTY 经 WriteSGR1006 通知前端。
        force_conpty_mouse_notify();
        // ANSI mouse tracking 序列（?1000/?1002/?1003/?1006）：开启点击/拖拽/悬停报告。
        enable_vt_processing()?;
        write_console_sequence(ENABLE_MOUSE_TRACKING_SEQUENCE)?;
        // 启用 alternate scroll (?1007h)：实测 ConPTY 下它让滚轮作为
        // Mouse(ScrollUp/ScrollDown) 事件报告（而非被吞），滚轮经 mouse arm 正常
        // 路由到消息区/面板滚动。?1007l 则滚轮 0 事件。
        write_console_sequence(ENABLE_ALTERNATE_SCROLL_SEQUENCE)?;
    }
    Ok(())
}

/// Disable mouse tracking under ConPTY. Must be called *before*
/// `DisableMouseCapture` on Windows. No-op elsewhere.
pub fn disable_mouse_tracking() -> Result<()> {
    #[cfg(windows)]
    {
        enable_vt_processing()?;
        // 恢复 alternate scroll mode（Windows Terminal 默认 ON）。
        write_console_sequence(ENABLE_ALTERNATE_SCROLL_SEQUENCE)?;
        write_console_sequence(DISABLE_MOUSE_TRACKING_SEQUENCE)?;
    }
    Ok(())
}

pub const ENABLE_MOUSE_TRACKING_SEQUENCE: &str = concat!(
    // Normal tracking: button press/release.
    "\x1b[?1000h",
    // Button-event tracking: drag events.
    "\x1b[?1002h",
    // Any-event tracking: hover/drag parity with crossterm's ANSI path.
    "\x1b[?1003h",
    // RXVT coordinate mode.
    "\x1b[?1015h",
    // SGR coordinate mode.
    "\x1b[?1006h",
);

pub const DISABLE_MOUSE_TRACKING_SEQUENCE: &str = concat!(
    "\x1b[?1006l",
    "\x1b[?1015l",
    "\x1b[?1003l",
    "\x1b[?1002l",
    "\x1b[?1000l",
);

/// Disable alternate-scroll mode so the wheel is never turned into arrow keys.
pub const DISABLE_ALTERNATE_SCROLL_SEQUENCE: &str = "\x1b[?1007l";

/// Restore alternate-scroll mode (Windows Terminal default ON).
pub const ENABLE_ALTERNATE_SCROLL_SEQUENCE: &str = "\x1b[?1007h";

/// Force ConPTY to notify Windows Terminal that mouse tracking is on.
///
/// ConPTY's `SetConsoleInputModeImpl` only calls `WriteSGR1006` (which tells the
/// frontend to start mouse tracking) when the new input mode *differs* from the
/// old one in the `ENABLE_MOUSE_INPUT` bit. Because `enable_raw_mode()` already
/// left that bit ON, crossterm's own `EnableMouseCapture` `SetConsoleMode` call
/// produces no diff and the notify is skipped.
///
/// Toggling the bit off then back on (restoring crossterm's intended mode)
/// guarantees a diff on both transitions, so ConPTY emits `WriteSGR1006`. This
/// is best-effort: failures here must not abort startup — crossterm has already
/// configured the input console.
#[cfg(windows)]
fn force_conpty_mouse_notify() {
    use windows_sys::Win32::System::Console::{
        GetConsoleMode, GetStdHandle, SetConsoleMode, STD_INPUT_HANDLE,
    };

    const ENABLE_MOUSE_INPUT: u32 = 0x0010;

    unsafe {
        let handle = GetStdHandle(STD_INPUT_HANDLE);
        let mut mode: u32 = 0;
        if GetConsoleMode(handle, &mut mode) == 0 {
            return;
        }
        // 清除 MOUSE 位 → ConPTY 检测到 diff，WriteSGR1006(false)。
        if SetConsoleMode(handle, mode & !ENABLE_MOUSE_INPUT) == 0 {
            return;
        }
        // 恢复 crossterm EnableMouseCapture 设置的原值（含 MOUSE 位）→
        // ConPTY 再次检测到 diff，WriteSGR1006(true) 通知前端开启 tracking。
        let _ = SetConsoleMode(handle, mode);
    }
}

#[cfg(windows)]
fn enable_vt_processing() -> io::Result<()> {
    use windows_sys::Win32::System::Console::{
        GetConsoleMode, GetStdHandle, SetConsoleMode, ENABLE_VIRTUAL_TERMINAL_PROCESSING,
        STD_OUTPUT_HANDLE,
    };

    unsafe {
        let handle = GetStdHandle(STD_OUTPUT_HANDLE);
        let mut mode = 0;
        if GetConsoleMode(handle, &mut mode) == 0 {
            return Err(io::Error::last_os_error());
        }
        if mode & ENABLE_VIRTUAL_TERMINAL_PROCESSING == 0
            && SetConsoleMode(handle, mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING) == 0
        {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}

#[cfg(windows)]
fn write_console_sequence(sequence: &str) -> io::Result<()> {
    use std::io::Write;
    use std::ptr;
    use windows_sys::Win32::System::Console::{GetStdHandle, WriteConsoleW, STD_OUTPUT_HANDLE};

    let wide: Vec<u16> = sequence.encode_utf16().collect();
    unsafe {
        let handle = GetStdHandle(STD_OUTPUT_HANDLE);
        let mut written = 0;
        if WriteConsoleW(
            handle,
            wide.as_ptr(),
            wide.len() as u32,
            &mut written,
            ptr::null(),
        ) != 0
        {
            if written == wide.len() as u32 {
                return Ok(());
            }
            return Err(io::Error::new(
                io::ErrorKind::WriteZero,
                "partial WriteConsoleW for mouse tracking sequence",
            ));
        }
    }

    let mut stdout = io::stdout();
    stdout.write_all(sequence.as_bytes())?;
    stdout.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enable_sequence_enables_mouse_modes() {
        assert_eq!(
            ENABLE_MOUSE_TRACKING_SEQUENCE,
            "\x1b[?1000h\x1b[?1002h\x1b[?1003h\x1b[?1015h\x1b[?1006h"
        );
    }

    #[test]
    fn disable_sequence_reverses_mouse_modes() {
        assert_eq!(
            DISABLE_MOUSE_TRACKING_SEQUENCE,
            "\x1b[?1006l\x1b[?1015l\x1b[?1003l\x1b[?1002l\x1b[?1000l"
        );
    }

    #[test]
    fn alternate_scroll_sequences_are_inverse() {
        assert_eq!(DISABLE_ALTERNATE_SCROLL_SEQUENCE, "\x1b[?1007l");
        assert_eq!(ENABLE_ALTERNATE_SCROLL_SEQUENCE, "\x1b[?1007h");
    }
}
