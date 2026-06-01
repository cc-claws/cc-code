//! mimalloc allocator tuning for high-churn workloads.
//!
//! Two functions:
//! 1. `init_mimalloc_conf()` — sets MI_OPTION env vars BEFORE mimalloc init.
//!    Must be called at the very first line of `main()`.
//! 2. `alloc_collect()` — triggers `mi_collect(true)` to force memory reclamation.
//!    Called after `/clear` and session switches.

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
}

#[cfg(target_os = "windows")]
#[allow(dead_code)]
pub fn init_mimalloc_conf() {
    // No-op on Windows (system allocator used instead)
}

/// Force mimalloc to reclaim freed memory and return it to the OS.
///
/// Call after `/clear` or session switches where large
/// amounts of memory have been freed.
#[cfg(not(target_os = "windows"))]
pub fn alloc_collect() {
    unsafe {
        libmimalloc_sys::mi_collect(true);
    }
}

#[cfg(target_os = "windows")]
pub fn alloc_collect() {
    // No-op on Windows (system allocator used instead)
}
