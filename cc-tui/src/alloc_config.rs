//! Allocator tuning for high-churn workloads.
//!
//! - macOS/Linux：jemalloc（aggressive decay），配置 `MALLOC_CONF`
//! - Windows：mimalloc（运行时 purge 策略），jemalloc 无法在 MSVC 工具链编译
//!
//! Public API:
//! - `init_alloc_conf()` — configure allocator during application startup
//! - `alloc_collect()` — reclaim allocator free memory (Windows: current thread heap)
//! - `query_stats()` — get allocator stats (RSS + allocator allocated)
//! - `query_breakdown()` — allocated/active/resident/metadata/mapped/retained
//! - `dump_stats()` — print detailed allocator stats to tracing
//! - `os_rss_mb()` — OS-level RSS via sysinfo (MB)

#[cfg(target_os = "windows")]
mod windows_tuning;
#[cfg(target_os = "windows")]
pub use windows_tuning::collect_on_thread_park;

/// Allocator stats (RSS from sysinfo + allocator allocated).
#[derive(Debug, Clone, Copy)]
pub struct AllocStats {
    /// 全进程物理驻留内存（sysinfo 报告，字节；不是全部虚拟内存）
    pub current_rss: usize,
    /// 分配器跟踪的在用分配，不含碎片/元数据，也不保证覆盖原生库分配；Windows 由 mimalloc 提供
    pub current_allocated: usize,
}

/// 分配器详细统计（需要 advance epoch / stats 快照才准确）。
///
/// 底层分别来自 jemalloc mallctl（非 Windows）与 mimalloc stats JSON（Windows）。
/// 字段口径不同，Windows 的 active/resident/metadata 不可按 jemalloc 语义解释。
#[derive(Debug, Clone, Copy)]
pub struct AllocBreakdown {
    /// 分配器跟踪的在用分配字节，不保证覆盖原生库分配
    pub allocated: usize,
    /// jemalloc：活跃页字节；Windows：page_committed 历史触及量，非当前活跃页
    pub active: usize,
    /// jemalloc：分配器物理驻留估算；Windows：全进程 WorkingSet（换出页不计入）
    pub resident: usize,
    /// jemalloc：分配器元数据；Windows：committed - page_committed，不能据此估算元数据
    pub metadata: usize,
    /// 映射/保留的字节
    pub mapped: usize,
    /// 保留未归还 OS 的字节
    pub retained: usize,
}

/// 当前平台的全局分配器名（用于 /gc 等诊断输出）。
pub fn allocator_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "mimalloc"
    } else {
        "jemalloc"
    }
}

/// Set allocator environment variables before initialization.
#[cfg(not(target_os = "windows"))]
#[allow(dead_code)]
pub fn init_alloc_conf() {
    if std::env::var("MALLOC_CONF").is_err() {
        std::env::set_var(
            "MALLOC_CONF",
            "dirty_decay_ms:0,muzzy_decay_ms:0,background_thread:true",
        );
    }
}

/// 在 settings 环境变量注入后、应用工作线程创建前设置 mimalloc 运行时选项。
///
/// 默认空闲页 decommit、purge 延迟 1000ms；保留用户有效选项，不依赖首次分配时机。
#[cfg(target_os = "windows")]
#[allow(dead_code)]
pub fn init_alloc_conf() {
    windows_tuning::init();
}

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

/// 当前线程立即回收，并通知 TUI 工作线程在下次空闲时回收；不释放存活对象。
#[cfg(target_os = "windows")]
pub fn alloc_collect() {
    windows_tuning::request_collect();
    // Safety: mi_collect 是线程安全的 C API，参数仅 force 标志
    unsafe { libmimalloc_sys::mi_collect(true) };
}

/// Advance jemalloc epoch to refresh cached stats.
#[cfg(not(target_os = "windows"))]
fn advance_epoch() {
    let _ = tikv_jemalloc_ctl::epoch::advance();
}

/// Query RSS + jemalloc allocated bytes.
#[cfg(not(target_os = "windows"))]
pub fn query_stats() -> Option<AllocStats> {
    advance_epoch();
    use sysinfo::{ProcessesToUpdate, System};
    let mut sys = System::new();
    let pid = sysinfo::get_current_pid().ok()?;
    sys.refresh_processes(ProcessesToUpdate::Some(&[pid]), true);
    let proc = sys.process(pid)?;
    let current_rss = proc.memory() as usize; // sysinfo returns bytes
    let current_allocated = tikv_jemalloc_ctl::stats::allocated::read().unwrap_or(current_rss);
    Some(AllocStats {
        current_rss,
        current_allocated,
    })
}

/// Query allocator detailed breakdown.
#[cfg(not(target_os = "windows"))]
pub fn query_breakdown() -> Option<AllocBreakdown> {
    advance_epoch();
    Some(AllocBreakdown {
        allocated: tikv_jemalloc_ctl::stats::allocated::read().ok()?,
        active: tikv_jemalloc_ctl::stats::active::read().ok()?,
        resident: tikv_jemalloc_ctl::stats::resident::read().ok()?,
        metadata: tikv_jemalloc_ctl::stats::metadata::read().ok()?,
        mapped: tikv_jemalloc_ctl::stats::mapped::read().ok()?,
        retained: tikv_jemalloc_ctl::stats::retained::read().ok()?,
    })
}

/// Print jemalloc full stats to stderr via tracing.
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

/// 通过 sysinfo 获取 OS 级 RSS（MB）。
/// 公共函数，供 gc.rs 和 thread_ops.rs 复用。
#[cfg(not(target_os = "windows"))]
pub fn os_rss_mb() -> Option<u64> {
    use sysinfo::{ProcessesToUpdate, System};
    let mut sys = System::new();
    let pid = sysinfo::get_current_pid().ok()?;
    sys.refresh_processes(ProcessesToUpdate::Some(&[pid]), true);
    sys.process(pid).map(|p| p.memory() / (1024 * 1024)) // bytes → MB
}

// ── Windows (mimalloc) ─────────────────────────────────────────────────────

/// 读取 mimalloc stats JSON（buf=NULL 时由 mi_malloc 分配，须 mi_free 释放）。
#[cfg(target_os = "windows")]
fn mimalloc_stats_json() -> Option<String> {
    // Safety: 传 NULL buf 让 mimalloc 自分配；返回指针用 mi_free 归还
    unsafe {
        let ptr = libmimalloc_sys::mi_stats_get_json(0, std::ptr::null_mut());
        if ptr.is_null() {
            return None;
        }
        let s = std::ffi::CStr::from_ptr(ptr).to_string_lossy().into_owned();
        libmimalloc_sys::mi_free(ptr as *mut std::ffi::c_void);
        Some(s)
    }
}

/// 从 stats JSON 的 `mi_stat_count` 对象读 current 字段。
#[cfg(target_os = "windows")]
fn json_current(v: &serde_json::Value, name: &str) -> usize {
    v[name]["current"].as_i64().unwrap_or(0).max(0) as usize
}

/// Query RSS + allocator allocated bytes.
#[cfg(target_os = "windows")]
pub fn query_stats() -> Option<AllocStats> {
    use sysinfo::{ProcessesToUpdate, System};
    let mut sys = System::new();
    let pid = sysinfo::get_current_pid().ok()?;
    sys.refresh_processes(ProcessesToUpdate::Some(&[pid]), true);
    let proc = sys.process(pid)?;
    // sysinfo 的字节值是全进程 WorkingSet。
    let current_rss = proc.memory() as usize;
    // stats 不可读时回退 0，不用 RSS 冒充 allocated（否则差异诊断失真）
    let current_allocated = mimalloc_stats_json()
        .and_then(|json| serde_json::from_str::<serde_json::Value>(&json).ok())
        .map(|v| json_current(&v, "malloc_normal") + json_current(&v, "malloc_huge"))
        .unwrap_or(0);
    Some(AllocStats {
        current_rss,
        current_allocated,
    })
}

/// Query allocator detailed breakdown via mimalloc stats JSON.
///
/// 字段映射：
/// - allocated = malloc_normal + malloc_huge（mimalloc 跟踪的在用分配字节）
/// - active = page_committed（历史触及量，非当前活跃页）
/// - resident = process.rss_current（全进程 WorkingSet，非分配器独占）
/// - metadata = committed - page_committed（不同统计口径的饱和差，不可用于元数据估算）
/// - mapped = reserved（累计向 OS 保留的虚拟地址空间）
/// - retained = reserved - committed（保留未提交）
#[cfg(target_os = "windows")]
pub fn query_breakdown() -> Option<AllocBreakdown> {
    let json = mimalloc_stats_json()?;
    let v: serde_json::Value = serde_json::from_str(&json).ok()?;
    let allocated = json_current(&v, "malloc_normal") + json_current(&v, "malloc_huge");
    let active = json_current(&v, "page_committed");
    let committed = json_current(&v, "committed");
    let reserved = json_current(&v, "reserved");
    let resident = v["process"]["rss_current"].as_i64().unwrap_or(0).max(0) as usize;
    Some(AllocBreakdown {
        allocated,
        active,
        resident,
        metadata: committed.saturating_sub(active),
        mapped: reserved,
        retained: reserved.saturating_sub(committed),
    })
}

/// 打印 mimalloc 完整 stats（JSON）到 tracing。
#[cfg(target_os = "windows")]
pub fn dump_stats() {
    if let Some(json) = mimalloc_stats_json() {
        tracing::info!("mimalloc stats: {json}");
    }
}

#[cfg(target_os = "windows")]
pub fn os_rss_mb() -> Option<u64> {
    use sysinfo::{ProcessesToUpdate, System};
    let mut sys = System::new();
    let pid = sysinfo::get_current_pid().ok()?;
    sys.refresh_processes(ProcessesToUpdate::Some(&[pid]), true);
    sys.process(pid).map(|p| p.memory() / (1024 * 1024)) // bytes → MB
}
