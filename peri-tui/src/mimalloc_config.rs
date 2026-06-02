//! Allocator tuning for high-churn workloads.
//!
//! Using jemalloc with aggressive decay for better fragmentation handling on macOS.
//!
//! Public API:
//! - `init_mimalloc_conf()` — set env vars before allocator init
//! - `alloc_collect()` — force aggressive memory reclamation
//! - `query_rss()` — get current and peak RSS
//! - `query_stats()` — get full allocator stats (RSS + jemalloc breakdown)
//! - `query_breakdown()` — jemalloc allocated/active/resident/metadata/mapped/retained
//! - `dump_stats()` — print detailed allocator stats to stderr

/// Allocator stats breakdown.
#[derive(Debug, Clone, Copy)]
pub struct MimallocStats {
    /// OS 级 RSS（sysinfo 报告，含所有内存）
    pub current_rss: usize,
    pub peak_rss: usize,
    /// jemalloc stats.allocated（应用实际分配字节数，不含碎片/元数据）
    pub current_commit: usize,
    pub peak_commit: usize,
}

/// jemalloc 详细统计（需要 advance epoch 才准确）。
#[derive(Debug, Clone, Copy)]
pub struct JemallocBreakdown {
    /// 应用实际分配的字节
    pub allocated: usize,
    /// 活跃页中的字节（页对齐，>= allocated）
    pub active: usize,
    /// 物理驻留字节（含脏页、元数据，>= active）
    pub resident: usize,
    /// jemalloc 元数据开销
    pub metadata: usize,
    /// 映射的字节
    pub mapped: usize,
    /// 保留未归还 OS 的字节
    pub retained: usize,
}

/// Set allocator environment variables before initialization.
#[cfg(not(target_os = "windows"))]
#[allow(dead_code)]
pub fn init_mimalloc_conf() {
    if std::env::var("MALLOC_CONF").is_err() {
        std::env::set_var(
            "MALLOC_CONF",
            "dirty_decay_ms:0,muzzy_decay_ms:0,background_thread:true",
        );
    }
}

#[cfg(target_os = "windows")]
#[allow(dead_code)]
pub fn init_mimalloc_conf() {}

/// Force jemalloc to aggressively reclaim freed memory.
#[cfg(not(target_os = "windows"))]
pub fn alloc_collect() {
    let _ = tikv_jemalloc_ctl::epoch::advance();
    // Purge each arena
    if let Ok(n) = tikv_jemalloc_ctl::arenas::narenas::read() {
        for i in 0..n {
            let key = format!("arena.{}.purge\0", i);
            // Safety: key is null-terminated, jemalloc handles arena.purge
            unsafe {
                tikv_jemalloc_sys::mallctl(
                    key.as_ptr() as *const _,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    0usize,
                );
            }
        }
    }
    std::thread::yield_now();
    let _ = tikv_jemalloc_ctl::epoch::advance();
}

#[cfg(target_os = "windows")]
pub fn alloc_collect() {}

/// Advance jemalloc epoch to refresh cached stats.
#[cfg(not(target_os = "windows"))]
fn advance_epoch() {
    let _ = tikv_jemalloc_ctl::epoch::advance();
}

/// Query OS-level RSS via sysinfo.
#[cfg(not(target_os = "windows"))]
pub fn query_rss() -> Option<(usize, usize)> {
    query_stats().map(|s| (s.current_rss, s.peak_rss))
}

/// Query RSS + jemalloc allocated bytes.
#[cfg(not(target_os = "windows"))]
pub fn query_stats() -> Option<MimallocStats> {
    advance_epoch();
    use sysinfo::{ProcessesToUpdate, System};
    let mut sys = System::new();
    let pid = sysinfo::get_current_pid().ok()?;
    sys.refresh_processes(ProcessesToUpdate::Some(&[pid]), true);
    let proc = sys.process(pid)?;
    let current_rss = (proc.memory() * 1024) as usize; // sysinfo returns KB
    let current_commit = tikv_jemalloc_ctl::stats::allocated::read().unwrap_or(current_rss);
    Some(MimallocStats {
        current_rss,
        peak_rss: current_rss,
        current_commit,
        peak_commit: current_commit,
    })
}

/// Query jemalloc detailed breakdown.
#[cfg(not(target_os = "windows"))]
pub fn query_breakdown() -> Option<JemallocBreakdown> {
    advance_epoch();
    Some(JemallocBreakdown {
        allocated: tikv_jemalloc_ctl::stats::allocated::read().ok()?,
        active: tikv_jemalloc_ctl::stats::active::read().ok()?,
        resident: tikv_jemalloc_ctl::stats::resident::read().ok()?,
        metadata: tikv_jemalloc_ctl::stats::metadata::read().ok()?,
        mapped: tikv_jemalloc_ctl::stats::mapped::read().ok()?,
        retained: tikv_jemalloc_ctl::stats::retained::read().ok()?,
    })
}

/// Print jemalloc full stats to stderr.
#[cfg(not(target_os = "windows"))]
pub fn dump_stats() {
    let mut buf = Vec::new();
    let _ = tikv_jemalloc_ctl::stats_print::stats_print(&mut buf, Default::default());
    if let Ok(s) = String::from_utf8(buf) {
        for line in s.lines() {
            tracing::info!("{line}");
        }
    }
}

#[cfg(target_os = "windows")]
pub fn query_rss() -> Option<(usize, usize)> {
    None
}
#[cfg(target_os = "windows")]
pub fn query_stats() -> Option<MimallocStats> {
    None
}
#[cfg(target_os = "windows")]
pub fn query_breakdown() -> Option<JemallocBreakdown> {
    None
}
#[cfg(target_os = "windows")]
pub fn dump_stats() {}
