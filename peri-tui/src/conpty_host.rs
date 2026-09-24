//! 老版 ConPTY 不识别 SGR 的无按键移动（button=35），会回退成键盘输入。
//! 仅为可识别的兼容宿主启用 hover；未知宿主保守降级，不按 WT_SESSION 猜测。
use std::{ffi::c_void, path::Path, ptr};
use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE},
    Storage::FileSystem::{
        GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW, VS_FIXEDFILEINFO,
    },
    System::{
        Console::GetConsoleWindow,
        Threading::{
            GetCurrentProcess, OpenProcess, QueryFullProcessImageNameW,
            PROCESS_QUERY_LIMITED_INFORMATION,
        },
    },
    UI::WindowsAndMessaging::{GetWindowThreadProcessId, IsWindowVisible},
};

pub(super) fn supports_hover() -> bool {
    let detected = detect();
    tracing::debug!(?detected, "ConPTY hover detection result");
    detected.unwrap_or(false)
}

fn detect() -> Option<bool> {
    // SAFETY: 只查询当前进程关联的控制台窗口、宿主映像和版本资源；不注入输入。
    unsafe {
        let window = GetConsoleWindow();
        tracing::debug!(?window, "ConPTY console window");
        if window.is_null() {
            return None;
        }
        if IsWindowVisible(window) != 0 {
            // 传统可见 conhost 直接产生 MOUSE_EVENT_RECORD，不经过 SGR 解码。
            return Some(true);
        }
        // 新版 OpenConsole 的 GetConsoleWindow 返回客户端自己的消息代理窗，
        // 不能把该窗口的 owner 当宿主。查询当前进程实际绑定的 console host。
        let pid = console_host_pid().or_else(|| {
            let mut pid = 0;
            (GetWindowThreadProcessId(window, &mut pid) != 0).then_some(pid)
        })?;
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if process.is_null() {
            return None;
        }
        let mut path = vec![0u16; 32768];
        let mut length = path.len() as u32;
        let ok = QueryFullProcessImageNameW(process, 0, path.as_mut_ptr(), &mut length);
        CloseHandle(process);
        if ok == 0 || length as usize >= path.len() {
            return None;
        }
        let name = String::from_utf16_lossy(&path[..length as usize]);
        tracing::debug!(host = %name, pid, "ConPTY console host");
        path[length as usize] = 0;
        let size = GetFileVersionInfoSizeW(path.as_ptr(), ptr::null_mut());
        if size == 0 {
            return None;
        }
        let mut data = vec![0u8; size as usize];
        if GetFileVersionInfoW(path.as_ptr(), 0, size, data.as_mut_ptr().cast()) == 0 {
            return None;
        }
        let mut info: *mut c_void = ptr::null_mut();
        let mut info_size = 0;
        if VerQueryValueW(
            data.as_ptr().cast(),
            [b'\\' as u16, 0].as_ptr(),
            &mut info,
            &mut info_size,
        ) == 0
            || info_size < std::mem::size_of::<VS_FIXEDFILEINFO>() as u32
            || info.is_null()
        {
            return None;
        }
        let info = ptr::read_unaligned(info.cast::<VS_FIXEDFILEINFO>());
        let file_name = Path::new(&name).file_name()?.to_str()?;
        let major = info.dwFileVersionMS >> 16;
        let minor = info.dwFileVersionMS & 0xffff;
        let build = info.dwFileVersionLS >> 16;
        let supported = compatible_host(file_name, major, minor, build);
        tracing::debug!(host = %name, major, minor, build, supported, "ConPTY hover capability");
        Some(supported)
    }
}

fn console_host_pid() -> Option<u32> {
    // Native 查询类并非稳定的 Win32 契约；失败就回退窗口查询，再保守关闭 hover。
    // 不枚举无关进程，不修改控制台；返回值的低两位是控制台标志而非 PID。
    #[link(name = "ntdll")]
    unsafe extern "system" {
        fn NtQueryInformationProcess(
            process: HANDLE,
            information_class: u32,
            information: *mut c_void,
            length: u32,
            return_length: *mut u32,
        ) -> i32;
    }
    let mut host: usize = 0;
    // SAFETY: 查询当前进程的 ProcessConsoleHostProcess (49)，输出缓冲区大小匹配 ULONG_PTR。
    let status = unsafe {
        NtQueryInformationProcess(
            GetCurrentProcess(),
            49,
            (&mut host as *mut usize).cast(),
            std::mem::size_of::<usize>() as u32,
            ptr::null_mut(),
        )
    };
    let pid = u32::try_from(host & !3).ok()?;
    (status >= 0 && pid != 0).then_some(pid)
}

fn compatible_host(name: &str, major: u32, minor: u32, _build: u32) -> bool {
    // 新版 OpenConsole 使用 1.x 版本；系统 conhost 使用 Windows 版本。
    // OpenConsole 1.24 已做真实压力验证；旧版本保守保留点击/拖拽。
    name.eq_ignore_ascii_case("OpenConsole.exe") && (major, minor) >= (1, 24) && major < 10
}

#[cfg(test)]
#[path = "conpty_host_test.rs"]
mod tests;
