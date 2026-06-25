//! macOS 下 arboard 初始化 NSPasteboard 会触发 os_log / NSLog 污染 TUI 屏幕，
//! 通过 RAII 临时把 fd 2 重定向到 /dev/null 抑制。其他平台为空操作。

#[cfg(target_os = "macos")]
pub(crate) struct SuppressStderr {
    saved_fd: Option<libc::c_int>,
}

#[cfg(target_os = "macos")]
impl SuppressStderr {
    pub(crate) fn new() -> Self {
        unsafe {
            // 备份当前 stderr fd
            let saved = libc::dup(2);
            if saved < 0 {
                return Self { saved_fd: None };
            }
            // 打开 /dev/null 并把 fd 2 指向它
            let devnull = libc::open(c"/dev/null".as_ptr(), libc::O_WRONLY);
            if devnull < 0 {
                libc::close(saved);
                return Self { saved_fd: None };
            }
            if libc::dup2(devnull, 2) < 0 {
                libc::close(saved);
                libc::close(devnull);
                return Self { saved_fd: None };
            }
            libc::close(devnull);
            Self {
                saved_fd: Some(saved),
            }
        }
    }
}

#[cfg(target_os = "macos")]
impl Drop for SuppressStderr {
    fn drop(&mut self) {
        if let Some(saved) = self.saved_fd {
            unsafe {
                libc::dup2(saved, 2);
                libc::close(saved);
            }
        }
    }
}

#[cfg(not(target_os = "macos"))]
pub(crate) struct SuppressStderr;

#[cfg(not(target_os = "macos"))]
impl SuppressStderr {
    pub(crate) fn new() -> Self {
        Self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 抑制_stderr_可正常创建并销毁() {
        let _guard = SuppressStderr::new();
        // 创建后立即销毁，验证不 panic
    }
}
