//! jemalloc allocator tuning for high-churn workloads.
//!
//! Two-phase configuration:
//! 1. `malloc_conf` global symbol — compile-time embedded config string, read
//!    by jemalloc during its **first** initialization (before `main()` runs).
//! 2. `configure_jemalloc()` — runtime mallctl writes as fallback/diagnostics.
//!
//! Configuration applied:
//! - `dirty_decay_ms:200` — purge freed arena pages after 200ms (default: 10000ms)
//! - `lg_tcache_max:16` — limit thread cache to objects ≤64KB (default: unlimited)
//!
//! NOTE: `background_thread:true` is NOT set here because it requires pthread and
//! does not work on macOS — jemalloc prints a warning and ignores the option.
//! The runtime `raw::write` fallback also fails silently on macOS.

// ─── Compile-time malloc_conf ──────────────────────────────────────────────
//
// The `_rjem_malloc_conf` export symbol is read by tikv-jemallocator during
// its one-time init. This happens BEFORE main() — the Rust runtime (lang_start)
// allocates memory (Box, Vec<OsString>, ...) which triggers jemalloc init.
//
// Setting `MALLOC_CONF` env var from Rust code is too late — jemalloc has
// already initialized and read the (empty) env var. The global symbol is the
// only way to guarantee the config takes effect.
//
// Pattern from tikv-jemallocator test suite:
// https://github.com/tikv/jemallocator/blob/main/tests/background_thread_enabled.rs
#[cfg(not(target_os = "windows"))]
#[allow(non_upper_case_globals)]
#[export_name = "_rjem_malloc_conf"]
pub static JEMALLOC_CONF: Option<&'static std::ffi::c_char> = Some(unsafe {
    union U {
        x: &'static u8,
        y: &'static std::ffi::c_char,
    }
    U {
        x: &b"dirty_decay_ms:200,lg_tcache_max:16\0"[0],
    }
    .y
});

/// Configure jemalloc for aggressive memory reclamation via runtime mallctl.
///
/// This is a best-effort fallback that applies settings at runtime.
/// `background_thread` is not set because it requires pthread and does not
/// work on macOS.
// Called from main.rs (bin target) via peri_tui::jemalloc_config::configure_jemalloc().
// Clippy's dead_code lint fires on lib targets even when used by the bin target.
#[allow(dead_code)]
#[cfg(not(target_os = "windows"))]
pub fn configure_jemalloc() {
    use tracing::{debug, warn};

    // Advance epoch to ensure stats are fresh
    let _ = tikv_jemalloc_ctl::epoch::advance();

    // 1. dirty_decay_ms — time before freed dirty pages are purged
    //    Default is 10000ms on many builds; we set 200ms for aggressive reclamation.
    //    Lower values increase CPU overhead from madvise syscalls but prevent
    //    the observed ~27MB dirty extent accumulation per turn.
    match unsafe { tikv_jemalloc_ctl::raw::write(b"arenas.dirty_decay_ms\0", 200i64) } {
        Ok(()) => debug!("jemalloc: arenas.dirty_decay_ms = 200"),
        Err(e) => warn!("jemalloc: failed to set dirty_decay_ms: {}", e),
    }

    // 2. lg_tcache_max — log2 of max cached allocation size in thread caches.
    //    Default is ~23 (8MB), which means large allocations linger in tcache.
    //    Setting to 16 (64KB) limits tcache to small objects, reducing the
    //    5-7MB tcache_bytes overhead observed in heapdumps.
    match unsafe { tikv_jemalloc_ctl::raw::write(b"arenas.lg_tcache_max\0", 16usize) } {
        Ok(()) => debug!("jemalloc: arenas.lg_tcache_max = 16 (64KB)"),
        Err(e) => warn!("jemalloc: failed to set lg_tcache_max: {}", e),
    }
}

#[cfg(target_os = "windows")]
pub fn configure_jemalloc() {
    // jemalloc not used on Windows (system allocator instead)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_configure_jemalloc_does_not_panic() {
        configure_jemalloc();
        configure_jemalloc();
    }

    #[test]
    #[cfg(not(target_os = "windows"))]
    fn test_dirty_decay_ms_is_set() {
        configure_jemalloc();
        let _ = tikv_jemalloc_ctl::epoch::advance();
        let val: i64 = unsafe { tikv_jemalloc_ctl::raw::read(b"arenas.dirty_decay_ms\0") }
            .expect("should read dirty_decay_ms");
        assert_eq!(val, 200, "dirty_decay_ms should be 200ms after configure");
    }
}
