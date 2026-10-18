//! RSS/VSZ 采集共享工具：所有内存回归测试（tests/*.rs）共用的取样入口。
//!
//! 历史问题（issue #589）：每个测试文件各拷一份 `current_rss_kb()`，实现是读
//! `/proc/self/status` 的 `VmRSS`，读不到就 `return 0`。macOS 属于 `unix` 却
//! 没有 `/proc`，于是所有读数恒为 `0 KB`、每一步增量恒为 `+0 KB`，测试照样
//! 通过——内存回归测试在 macOS 上等于空跑。
//!
//! 现在统一走 `sysinfo`，口径与 TUI 的 /gc 一致（见 `cc-tui/src/alloc_config.rs`）：
//! macOS 走 `proc_pidinfo`、Windows 走 `WorkingSetSize`、Linux 走 `/proc`。并且
//! **采集失败直接 panic，绝不返回 0**：0 会被调用方的 `saturating_sub` 伪装成
//! 「没有增长」，把「测量失败」洗成「测试通过」。

#![allow(dead_code)]

/// 当前进程 RSS，单位 KB。采集失败直接 panic（见模块文档）。
pub fn current_rss_kb() -> usize {
    current_rss_bytes() / 1024
}

/// 当前进程 RSS，单位字节。采集失败直接 panic。
pub fn current_rss_bytes() -> usize {
    current_process_memory()
        .map(|(rss, _)| rss)
        .unwrap_or_else(|| panic!("RSS 采集失败：读不到当前进程的驻留内存"))
}

/// 当前进程虚拟内存大小，单位 KB。采集失败直接 panic。
pub fn current_vsize_kb() -> usize {
    let vsz = current_process_memory()
        .map(|(_, vsz)| vsz)
        .unwrap_or_else(|| panic!("VSZ 采集失败：读不到当前进程的虚拟内存"));
    vsz / 1024
}

/// 取当前进程的 (RSS 字节, 虚拟内存字节) 快照。
fn current_process_memory() -> Option<(usize, usize)> {
    let mut sys = sysinfo::System::new();
    let pid = sysinfo::get_current_pid().ok()?;
    sys.refresh_processes(sysinfo::ProcessesToUpdate::Some(&[pid]), true);
    let proc = sys.process(pid)?;
    Some((proc.memory() as usize, proc.virtual_memory() as usize))
}

/// `kb` 转 MB，保留两位小数的打印口径。
pub fn mb(kb: usize) -> f64 {
    kb as f64 / 1024.0
}
