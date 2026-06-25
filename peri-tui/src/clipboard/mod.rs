//! 剪贴板抽象层
//!
//! 参照 openai/codex 的 `clipboard_copy.rs` 实现多层 fallback：
//! 1. SSH 远程会话：优先 tmux clipboard，回落 OSC 52 转义
//! 2. 本地：优先 arboard（native clipboard）
//! 3. WSL 本地：arboard 失败时回落到 powershell.exe Set-Clipboard
//! 4. 都失败：tmux / OSC 52 兜底
//!
//! Linux X11/Wayland 下 `ClipboardLease` 持有 arboard handle 直到 TUI 退出，
//! 否则剪贴板内容会随写入进程释放而消失。macOS 下 `SuppressStderr` 抑制
//! NSPasteboard 初始化 stderr 污染。

pub mod copy;
pub mod image_placeholder;
pub mod paste;
pub mod path_normalize;

mod stderr_suppress;
pub(crate) use stderr_suppress::SuppressStderr;

/// 平台剪贴板所有权持有。
///
/// Linux X11 和部分 Wayland compositor 要求剪贴板内容由写入进程持有，
/// 进程退出或释放 handle 后内容会消失。把 `arboard::Clipboard` 包在 lease
/// 里，挂到 `GlobalUiState` 让它活到 TUI 退出。其他平台 lease 为 None。
pub struct ClipboardLease {
    #[cfg(target_os = "linux")]
    _clipboard: Option<arboard::Clipboard>,
}

impl ClipboardLease {
    #[cfg(target_os = "linux")]
    pub(crate) fn native_linux(clipboard: arboard::Clipboard) -> Self {
        Self {
            _clipboard: Some(clipboard),
        }
    }

    #[allow(dead_code)]
    pub(crate) fn empty() -> Self {
        Self {
            #[cfg(target_os = "linux")]
            _clipboard: None,
        }
    }
}
