//! mimalloc 的运行时选项与工作线程空闲回收。

use std::{
    cell::Cell,
    os::raw::c_long,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

// libmimalloc-sys 0.1.49 未导出这两个常量；对应捆绑的 v2/v3 mimalloc.h 枚举。
const OPTION_PURGE_DECOMMITS: libmimalloc_sys::mi_option_t = 5;
const OPTION_PURGE_DELAY: libmimalloc_sys::mi_option_t = 15;
const IDLE_COLLECT_INTERVAL: Duration = Duration::from_secs(1);
static COLLECT_EPOCH: AtomicU64 = AtomicU64::new(0);

thread_local! {
    static LAST_COLLECT: Cell<(u64, Option<Instant>)> = const { Cell::new((0, None)) };
}

pub(super) fn init() {
    set_option(
        OPTION_PURGE_DECOMMITS,
        &["MIMALLOC_PURGE_DECOMMITS", "MIMALLOC_RESET_DECOMMITS"],
        1,
    );
    // 保留 mimalloc v3 的默认复用窗口；空闲回调负责提前回收。
    set_option(
        OPTION_PURGE_DELAY,
        &["MIMALLOC_PURGE_DELAY", "MIMALLOC_RESET_DELAY"],
        1000,
    );
}

fn set_option(option: libmimalloc_sys::mi_option_t, names: &[&str], default: c_long) {
    let configured = names.iter().find_map(std::env::var_os);
    let value = match configured {
        None => Some(default),
        Some(raw) => raw.to_str().and_then(|raw| {
            let raw = raw.trim();
            if raw.is_empty()
                || ["true", "yes", "on"]
                    .iter()
                    .any(|v| raw.eq_ignore_ascii_case(v))
            {
                Some(1)
            } else if ["false", "no", "off"]
                .iter()
                .any(|v| raw.eq_ignore_ascii_case(v))
            {
                Some(0)
            } else {
                raw.parse().ok()
            }
        }),
    };
    if let Some(value) = value {
        // Safety: 选项来自捆绑头文件；仅在启动阶段、应用工作线程创建前修改。
        // 运行时 API 避免 main 前分配已初始化后，设置环境变量不再生效的问题。
        unsafe { libmimalloc_sys::mi_option_set(option, value) };
    }
}

pub(super) fn request_collect() {
    COLLECT_EPOCH.fetch_add(1, Ordering::Relaxed);
}

/// 在实际 Tokio 工作线程上执行回收；任务迁移不影响线程本地堆的覆盖。
/// 每线程自动回收最多每秒一次，手动回收请求在下次进入空闲时处理。
/// 不主动唤醒已经停泊的线程，不保证 /gc 返回前所有工作线程都已回收。
pub fn collect_on_thread_park() {
    // 用户显式禁用 purge 时，也不启用自动强制回收。
    if unsafe { libmimalloc_sys::mi_option_get(OPTION_PURGE_DELAY) } < 0 {
        return;
    }
    let epoch = COLLECT_EPOCH.load(Ordering::Relaxed);
    LAST_COLLECT.with(|state| {
        let (last_epoch, last_time) = state.get();
        if last_epoch == epoch
            && last_time.is_some_and(|time| time.elapsed() < IDLE_COLLECT_INTERVAL)
        {
            return;
        }
        // Safety: 回调在被收集堆所属线程运行，不访问其他线程的堆或应用锁。
        unsafe { libmimalloc_sys::mi_collect(true) };
        state.set((epoch, Some(Instant::now())));
    });
}
