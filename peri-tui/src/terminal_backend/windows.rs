use std::{
    io,
    mem::{size_of, zeroed},
    os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle},
    ptr,
};

use windows_sys::Win32::{
    Foundation::{GENERIC_READ, GENERIC_WRITE, INVALID_HANDLE_VALUE},
    System::Console::{
        CreateConsoleScreenBuffer, GetConsoleOutputCP, GetConsoleScreenBufferInfo,
        GetCurrentConsoleFontEx, GetStdHandle, SetConsoleCursorPosition,
        SetConsoleScreenBufferSize, WriteConsoleW, CONSOLE_FONT_INFOEX, CONSOLE_SCREEN_BUFFER_INFO,
        CONSOLE_TEXTMODE_BUFFER, COORD, STD_OUTPUT_HANDLE,
    },
};

use super::compatible::WidthProbe;

#[derive(PartialEq, Eq)]
struct ConsoleFingerprint {
    face: [u16; 32],
    size: (i16, i16),
    family: u32,
    weight: u32,
    code_page: u32,
}

impl ConsoleFingerprint {
    fn read() -> Option<Self> {
        // SAFETY: the structure is initialized with its documented size and a valid out pointer.
        unsafe {
            let mut font: CONSOLE_FONT_INFOEX = zeroed();
            font.cbSize = size_of::<CONSOLE_FONT_INFOEX>() as u32;
            if GetCurrentConsoleFontEx(GetStdHandle(STD_OUTPUT_HANDLE), 0, &mut font) == 0 {
                return None;
            }
            Some(Self {
                face: font.FaceName,
                size: (font.dwFontSize.X, font.dwFontSize.Y),
                family: font.FontFamily,
                weight: font.FontWeight,
                code_page: GetConsoleOutputCP(),
            })
        }
    }
}

/// Measurements are made in an inactive screen buffer, never in the user's displayed frame.
pub struct ConsoleWidthProbe {
    buffer: Option<OwnedHandle>,
    fingerprint: Option<ConsoleFingerprint>,
}

impl ConsoleWidthProbe {
    pub fn new() -> Self {
        let fingerprint = ConsoleFingerprint::read();
        let buffer = Self::create_buffer(&fingerprint);
        Self {
            buffer,
            fingerprint,
        }
    }

    fn create_buffer(fingerprint: &Option<ConsoleFingerprint>) -> Option<OwnedHandle> {
        fingerprint.as_ref()?;
        // SAFETY: no security attributes or reserved data; ownership transfers only on success.
        // The new buffer inherits the active buffer's font. It is never made active.
        let handle = unsafe {
            CreateConsoleScreenBuffer(
                GENERIC_READ | GENERIC_WRITE,
                3, // FILE_SHARE_READ | FILE_SHARE_WRITE
                ptr::null(),
                CONSOLE_TEXTMODE_BUFFER,
                ptr::null(),
            )
        };
        if handle == INVALID_HANDLE_VALUE || handle.is_null() {
            tracing::warn!(error = %io::Error::last_os_error(), "无法创建终端列宽探针，保留默认输出");
            None
        } else {
            // SAFETY: CreateConsoleScreenBuffer returned a new owned handle.
            let buffer = unsafe { OwnedHandle::from_raw_handle(handle) };
            // A tiny terminal must not make a long grapheme scroll the measurement buffer.
            // SAFETY: valid owned handle and initialized out parameter; only the private buffer grows.
            unsafe {
                let mut info: CONSOLE_SCREEN_BUFFER_INFO = zeroed();
                if GetConsoleScreenBufferInfo(handle, &mut info) == 0
                    || SetConsoleScreenBufferSize(
                        handle,
                        COORD {
                            X: info.dwSize.X.max(512),
                            Y: info.dwSize.Y.max(2),
                        },
                    ) == 0
                {
                    tracing::warn!(error = %io::Error::last_os_error(), "无法设置终端列宽探针大小，保留默认输出");
                    return None;
                }
            }
            Some(buffer)
        }
    }
}

#[cfg(test)]
#[path = "windows_test.rs"]
mod tests;

impl WidthProbe for ConsoleWidthProbe {
    fn width(&mut self, symbol: &str) -> Option<usize> {
        let buffer = self.buffer.as_ref()?;
        let wide: Vec<u16> = symbol.encode_utf16().collect();
        // Cell symbols are graphemes, but put a bound on malformed/unusually large input.
        if wide.len() > 256 || symbol.chars().any(char::is_control) {
            return None;
        }
        // SAFETY: the handle is owned and inactive; all pointers have the stated capacity.
        unsafe {
            let handle = buffer.as_raw_handle();
            if SetConsoleCursorPosition(handle, COORD { X: 0, Y: 0 }) == 0 {
                return None;
            }
            let mut written = 0;
            if WriteConsoleW(
                handle,
                wide.as_ptr().cast(),
                wide.len() as u32,
                &mut written,
                ptr::null(),
            ) == 0
                || written as usize != wide.len()
            {
                return None;
            }
            let mut info: CONSOLE_SCREEN_BUFFER_INFO = zeroed();
            if GetConsoleScreenBufferInfo(handle, &mut info) == 0 {
                return None;
            }
            Some(
                info.dwCursorPosition.Y as usize * info.dwSize.X as usize
                    + info.dwCursorPosition.X as usize,
            )
        }
    }

    fn refresh(&mut self) -> bool {
        let fingerprint = ConsoleFingerprint::read();
        if fingerprint == self.fingerprint {
            return false;
        }
        self.buffer = Self::create_buffer(&fingerprint);
        self.fingerprint = fingerprint;
        true
    }
}
