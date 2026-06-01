//! mimalloc allocator tuning for high-churn workloads.
//!
//! Two functions:
//! 1. `init_mimalloc_conf()` — sets MI_OPTION env vars BEFORE mimalloc init.
//!    Must be called at the very first line of `main()`.
//! 2. `alloc_collect()` — triggers aggressive memory reclamation via double
//!    `mi_collect` + immediate purge. Called after `/clear` and session switches.

/// mimalloc option constants not exposed as named constants in libmimalloc-sys.
/// Values from mimalloc v2/src/options.c enumeration order.
#[cfg(not(target_os = "windows"))]
mod mi_opts {
    use libmimalloc_sys::mi_option_t;
    /// purge_decommits (legacy: reset_decommits) — option index 5.
    /// When enabled, purging uses decommit (madvise MADV_DONTNEED on Linux/macOS)
    /// instead of page reset.
    pub const PURGE_DECOMMITS: mi_option_t = 5;
    /// purge_delay (legacy: reset_delay) — option index 15.
    /// Delay in milliseconds before freed pages are purged. Default is 10.
    /// Setting to 0 causes immediate purge on free.
    pub const PURGE_DELAY: mi_option_t = 15;
}

/// Set mimalloc environment variables before the allocator initializes.
///
/// mimalloc reads these env vars during its one-time init (triggered by
/// the first allocation through `#[global_allocator]`). Must be called
/// before any significant allocation — ideally line 1 of `main()`.
///
/// Options configured:
/// - `MIMALLOC_PAGE_RESET=1` — reset freed pages immediately (more aggressive than default)
/// - `MIMALLOC_DECOMMIT=1` — decommit (return to OS) freed virtual address space
/// - `MIMALLOC_BACKGROUND_THREAD=1` — enable background thread for memory reclamation
/// - `MIMALLOC_PURGE_DELAY=0` — immediate purge of freed pages (no 10ms delay)
#[cfg(not(target_os = "windows"))]
#[allow(dead_code)]
pub fn init_mimalloc_conf() {
    // Only set if not already configured by the user externally.
    if std::env::var("MIMALLOC_PAGE_RESET").is_err() {
        std::env::set_var("MIMALLOC_PAGE_RESET", "1");
    }
    if std::env::var("MIMALLOC_DECOMMIT").is_err() {
        std::env::set_var("MIMALLOC_DECOMMIT", "1");
    }
    if std::env::var("MIMALLOC_BACKGROUND_THREAD").is_err() {
        std::env::set_var("MIMALLOC_BACKGROUND_THREAD", "1");
    }
    // Purge delay 0: immediately return freed pages to the OS.
    // Default is 10ms which holds pages briefly for potential reuse.
    // For a TUI app, the latency impact is negligible vs memory savings.
    if std::env::var("MIMALLOC_PURGE_DELAY").is_err() {
        std::env::set_var("MIMALLOC_PURGE_DELAY", "0");
    }
}

#[cfg(target_os = "windows")]
#[allow(dead_code)]
pub fn init_mimalloc_conf() {
    // No-op on Windows (system allocator used instead)
}

/// Force mimalloc to aggressively reclaim freed memory and return it to the OS.
///
/// Strategy:
/// 1. Temporarily set purge_delay=0 for immediate page reclamation
/// 2. First `mi_collect(true)` — frees all unreachable objects
/// 3. Yield to allow mimalloc's internal purge machinery to process
/// 4. Second `mi_collect(true)` — reclaims pages that became empty after pass 1
/// 5. Restore original purge_delay
///
/// Call after `/clear` or session switches where large amounts of memory
/// have been freed. The double-collect handles the common case where a page
/// still had live objects during the first pass but became fully free afterward.
#[cfg(not(target_os = "windows"))]
pub fn alloc_collect() {
    use libmimalloc_sys::{mi_collect, mi_option_get, mi_option_set};

    unsafe {
        // Ensure purge_decommits is enabled (uses madvise MADV_DONTNEED to return
        // pages to the OS rather than just resetting page content).
        mi_option_set_enabled_purge_decommits(true);

        // Save and override purge_delay for immediate reclamation.
        let orig_delay = mi_option_get(mi_opts::PURGE_DELAY);
        mi_option_set(mi_opts::PURGE_DELAY, 0);

        // Pass 1: free all reachable objects, trigger purge of now-empty pages.
        mi_collect(true);

        // Give mimalloc a moment to process pending purges and segment operations.
        // yield_now() is a lightweight hint — no actual wall-clock delay in practice.
        std::thread::yield_now();

        // Pass 2: pages that had objects freed in pass 1 are now empty;
        // this second collect can purge them.
        mi_collect(true);

        // Restore original purge_delay (may be 0 if we set MIMALLOC_PURGE_DELAY=0
        // at init, in which case this is a no-op).
        mi_option_set(mi_opts::PURGE_DELAY, orig_delay);
    }
}

/// Enable or disable the purge_decommits option (index 5).
///
/// Separate helper because the constant isn't in the libmimalloc-sys bindings.
#[cfg(not(target_os = "windows"))]
unsafe fn mi_option_set_enabled_purge_decommits(enable: bool) {
    use libmimalloc_sys::mi_option_set_enabled;
    mi_option_set_enabled(mi_opts::PURGE_DECOMMITS, enable);
}

#[cfg(target_os = "windows")]
pub fn alloc_collect() {
    // No-op on Windows (system allocator used instead)
}

/// Query mimalloc for current and peak RSS (resident set size) in bytes.
///
/// Returns `None` on Windows or if `mi_process_info` fails.
#[cfg(not(target_os = "windows"))]
pub fn query_rss() -> Option<(usize, usize)> {
    use libmimalloc_sys::mi_process_info;
    unsafe {
        let mut current_rss: usize = 0;
        let mut peak_rss: usize = 0;
        mi_process_info(
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut current_rss,
            &mut peak_rss,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        );
        if current_rss > 0 {
            Some((current_rss, peak_rss))
        } else {
            None
        }
    }
}

#[cfg(target_os = "windows")]
pub fn query_rss() -> Option<(usize, usize)> {
    None
}
