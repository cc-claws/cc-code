//! 文件权限收紧：密钥/历史类文件仅所有者可读写。
//!
//! Unix 上是经典的 `0o600`（文件）/`0o700`（目录）；
//! Windows 上没有 chmod 等价物，这里用 `SetNamedSecurityInfoW` 写一条
//! owner-only 的受保护 DACL，达到同样的"仅所有者"语义。
//!
//! 全部实现都是 best-effort：失败时返回 `Err`，调用方自行决定是否忽略
//! （现有调用点沿用 `let _ =`，保持历史行为：权限收紧失败不中断主流程）。

use std::io;
use std::path::Path;

/// 把 `path` 收紧为仅所有者可读写（文件 `0o600` / 目录 `0o700` 语义）。
pub fn restrict_to_owner(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        restrict_to_owner_unix(path)
    }
    #[cfg(windows)]
    {
        restrict_to_owner_windows(path)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = path;
        Ok(())
    }
}

#[cfg(unix)]
fn restrict_to_owner_unix(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let target_mode = if path.is_dir() { 0o700 } else { 0o600 };
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(target_mode))
}

#[cfg(windows)]
fn restrict_to_owner_windows(path: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::{LocalFree, ERROR_SUCCESS, GENERIC_ALL};
    use windows_sys::Win32::Security::Authorization::{
        GetNamedSecurityInfoW, SetNamedSecurityInfoW, SE_FILE_OBJECT,
    };
    use windows_sys::Win32::Security::{
        AddAccessAllowedAce, InitializeAcl, ACL, ACCESS_ALLOWED_ACE, ACL_REVISION,
        DACL_SECURITY_INFORMATION, GetLengthSid, OWNER_SECURITY_INFORMATION,
        PROTECTED_DACL_SECURITY_INFORMATION,
    };

    let path_w: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    // 取文件当前 owner 的 SID（后续 ACL 只给这个 SID 授权）。
    let mut owner_sid: *mut std::ffi::c_void = std::ptr::null_mut();
    let mut sd: *mut std::ffi::c_void = std::ptr::null_mut();
    let rc = unsafe {
        GetNamedSecurityInfoW(
            path_w.as_ptr(),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION,
            &mut owner_sid,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut sd,
        )
    };
    if rc != ERROR_SUCCESS {
        if !sd.is_null() {
            unsafe { LocalFree(sd) };
        }
        return Err(io::Error::from_raw_os_error(rc as i32));
    }
    // owner_sid 借用自 sd，在 sd 释放前保持有效。
    let result = (|| -> io::Result<()> {
        let sid_len = unsafe { GetLengthSid(owner_sid) } as usize;
        // ACL 头 + 一条 ACCESS_ALLOWED_ACE（SidStart 是变长数组，减去占位的 u32）。
        let ace_size =
            std::mem::size_of::<ACCESS_ALLOWED_ACE>() - std::mem::size_of::<u32>() + sid_len;
        let acl_size = std::mem::size_of::<ACL>() + ace_size;
        let mut acl_buf = vec![0u8; acl_size];
        let acl = acl_buf.as_mut_ptr() as *mut ACL;
        if unsafe { InitializeAcl(acl, acl_size as u32, ACL_REVISION) } == 0 {
            return Err(io::Error::last_os_error());
        }
        if unsafe { AddAccessAllowedAce(acl, ACL_REVISION, GENERIC_ALL, owner_sid) } == 0 {
            return Err(io::Error::last_os_error());
        }
        // 受保护的 DACL：阻断继承，达到 0600"仅所有者"的语义。
        let rc = unsafe {
            SetNamedSecurityInfoW(
                path_w.as_ptr(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                acl,
                std::ptr::null_mut(),
            )
        };
        if rc != ERROR_SUCCESS {
            return Err(io::Error::from_raw_os_error(rc as i32));
        }
        Ok(())
    })();
    unsafe { LocalFree(sd) };
    result
}
