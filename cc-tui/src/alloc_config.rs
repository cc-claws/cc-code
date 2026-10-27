//! Allocator tuning for high-churn workloads.
//!
//! - macOS/Linux：jemalloc（aggressive decay），main.rs 静态配置，`_RJEM_MALLOC_CONF` 可覆盖
//! - Windows：mimalloc（运行时 purge 策略），jemalloc 无法在 MSVC 工具链编译
//!
//! Public API:
//! - `init_alloc_conf()` — configure Windows allocator during application startup
//! - `alloc_collect()` — reclaim allocator free memory (Windows: current thread heap)
//! - `query_stats()` — get allocator stats (RSS + allocator allocated)
//! - `query_breakdown()` — allocated/active/resident/metadata/mapped/retained
//! - `query_size_classes()` — per size class live object counts (fully decomposes `allocated`)
//! - `dump_stats()` — print detailed allocator stats to tracing
//! - `retained_metric_applicable()` — whether `retained` can be non-zero on this platform

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

/// 单个 size class 的存活占用（跨全部 arena 汇总）。
///
/// 逐 size class 求和**恒等于** `stats.allocated`（jemalloc 的 `allocated` 就是
/// 各 arena `Σ curregs×reg_size + Σ curlextents×size`），因此这张表能把
/// `/gc` 里「未识别」的那部分字节按**分配尺寸**直接定位，而不是只能给估算值。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SizeClassUsage {
    /// jemalloc size class 的字节大小（已含取整，不是请求尺寸）
    pub size: usize,
    /// 跨全部 arena 汇总的存活对象数（large class 为存活 extent 数）
    pub live_objects: usize,
}

impl SizeClassUsage {
    /// 该 size class 占用的存活字节（`size × live_objects`）。
    pub fn bytes(&self) -> usize {
        self.size * self.live_objects
    }
}

/// 当前平台的全局分配器名（用于 /gc 等诊断输出）。
pub fn allocator_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "mimalloc"
    } else {
        "jemalloc"
    }
}

/// 在 settings 环境变量注入后、应用工作线程创建前设置 mimalloc 运行时选项。
///
/// 默认空闲页 decommit、purge 延迟 1000ms；保留用户有效选项，不依赖首次分配时机。
#[cfg(target_os = "windows")]
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
                                              // stats 不可读时整体 fail-closed（调用方显示「RSS 不可用」），绝不用 RSS 冒充
                                              // allocated——否则 allocated-RSS 恒为 0，把「测量失败」伪装成「完全一致」。
    let current_allocated = tikv_jemalloc_ctl::stats::allocated::read().ok()?;
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

/// 当前平台的 `retained` 指标是否有意义（是否会非零）。
///
/// jemalloc 的 `stats.retained` 只在 `opt.retain=true` 时才可能非零，而 `opt.retain`
/// 的编译期默认值在 darwin 上为 0（jemalloc `configure.ac` 的 `default_retain="0"`
/// 分支不含 darwin），因此 macOS 上该指标恒为 0：直接渲染成「保留未归还 OS: 0 B」
/// 是恒零噪音。读取失败时按 `true` 处理——宁可显示读数，也不隐藏可能有效的指标。
#[cfg(not(target_os = "windows"))]
pub fn retained_metric_applicable() -> bool {
    // Safety: `opt.retain` 是只读 bool mallctl（jemalloc 文档标注访问权限为 `r-`），
    // 名字以 NUL 结尾，`bool` 宽度与 jemalloc 的 bool 一致。
    unsafe { tikv_jemalloc_ctl::raw::read::<bool>(b"opt.retain\0").unwrap_or(true) }
}

// ── size class 分布（jemalloc only）──────────────────────────────────────────

/// 栈上拼接 mallctl 键（含结尾 NUL），避免在统计路径里用 `format!`。
///
/// 非 Windows 上被统计的分配器**就是**本进程的全局分配器，读统计时产生的临时
/// `String` 会立刻进入 arena 记账，让「逐 class 求和 == allocated」的验算凭空多出
/// 几 KB 差额、把一次精确分解显示成有误差的估算。此处所有键都在固定缓冲里拼装，
/// 统计路径不再分配。
#[cfg(not(target_os = "windows"))]
struct KeyBuf {
    buf: [u8; 64],
    len: usize,
}

#[cfg(not(target_os = "windows"))]
impl KeyBuf {
    fn new() -> Self {
        KeyBuf {
            buf: [0; 64],
            len: 0,
        }
    }

    /// 复用同一个缓冲拼下一个键。
    fn reset(&mut self) {
        self.len = 0;
    }

    fn push_str(&mut self, s: &str) {
        assert!(self.len + s.len() < self.buf.len(), "mallctl key too long");
        self.buf[self.len..self.len + s.len()].copy_from_slice(s.as_bytes());
        self.len += s.len();
    }

    fn push_index(&mut self, mut idx: usize) {
        let mut digits = [0u8; 20];
        let mut n = 0;
        loop {
            digits[n] = b'0' + (idx % 10) as u8;
            n += 1;
            idx /= 10;
            if idx == 0 {
                break;
            }
        }
        assert!(self.len + n < self.buf.len(), "mallctl key too long");
        for i in (0..n).rev() {
            self.buf[self.len] = digits[i];
            self.len += 1;
        }
    }

    /// 带结尾 NUL 的键字节。
    fn key(&mut self) -> &[u8] {
        self.buf[self.len] = 0;
        &self.buf[..self.len + 1]
    }
}

/// 读一个只读 `size_t` mallctl 键（入参须以 NUL 结尾）。
#[cfg(not(target_os = "windows"))]
fn read_mallctl_size(key: &[u8]) -> Option<usize> {
    // Safety: 键以 NUL 结尾；本文件传入的均为只读 size_t 计数器。
    unsafe { tikv_jemalloc_ctl::raw::read::<usize>(key).ok() }
}

/// 读取 jemalloc 每个 size class 的存活对象数（跨全部 arena 汇总）。
///
/// 只返回存活对象数 > 0 的 class（36 个 small bin + 196 个 large lextent 中绝大多数
/// 恒为 0，全列只会淹没真正吃内存的几行），按字节数**降序**、同字节数按 size 升序排列。
///
/// 任一 size class 的尺寸读取失败时整体返回 `None`（fail-closed，与 `query_stats`
/// 一致）；单个 arena 未初始化（mallctl 报「unknown/invalid value」）时跳过该 arena，
/// 不视为错误。全部 arena 都读不到时返回 `None`。
#[cfg(not(target_os = "windows"))]
pub fn query_size_classes() -> Option<Vec<SizeClassUsage>> {
    // `arenas.nbins` / `arenas.nlextents` 是全局 size class 元数据，
    // `arenas.narenas` 是 arena 槽位总数（含尚未初始化的）。
    // 注意索引名：small 是 `bins`，large 是 `lextents`——`stats.arenas.<i>.large`
    // 只是聚合节点（allocated/nmalloc/…），按 class 的计数在 `lextents` 下。
    // Safety: 三者都是只读 `c_uint`。
    let (nbins, nlextents, narenas) = unsafe {
        (
            tikv_jemalloc_ctl::raw::read::<u32>(b"arenas.nbins\0").ok()? as usize,
            tikv_jemalloc_ctl::raw::read::<u32>(b"arenas.nlextents\0").ok()? as usize,
            tikv_jemalloc_ctl::raw::read::<u32>(b"arenas.narenas\0").ok()? as usize,
        )
    };
    if nbins == 0 || narenas == 0 {
        return None;
    }
    let total = nbins + nlextents;

    // 先把所有堆分配做完，**再**推进 epoch 取快照：否则本函数自己申请的这几个
    // Vec 会落在快照之后，使「逐 class 求和」比 stats.allocated 少掉它们。
    let mut sizes: Vec<usize> = Vec::with_capacity(total);
    let mut live: Vec<usize> = vec![0usize; total];
    let mut classes: Vec<SizeClassUsage> = Vec::with_capacity(total);
    advance_epoch();

    // 尺寸表与 arena 无关，只读一次；index j 的语义：j < nbins 为 small bin，
    // 否则为 large lextent 的第 j - nbins 个。
    let mut kb = KeyBuf::new();
    for j in 0..nbins {
        kb.reset();
        kb.push_str("arenas.bin.");
        kb.push_index(j);
        kb.push_str(".size");
        sizes.push(read_mallctl_size(kb.key())?);
    }
    for k in 0..nlextents {
        kb.reset();
        kb.push_str("arenas.lextent.");
        kb.push_index(k);
        kb.push_str(".size");
        sizes.push(read_mallctl_size(kb.key())?);
    }

    let mut kb = KeyBuf::new();
    let mut initialized_arenas = 0usize;
    for i in 0..narenas {
        let mut arena_read = false;
        for j in 0..nbins {
            kb.reset();
            kb.push_str("stats.arenas.");
            kb.push_index(i);
            kb.push_str(".bins.");
            kb.push_index(j);
            kb.push_str(".curregs");
            if let Some(n) = read_mallctl_size(kb.key()) {
                live[j] += n;
                arena_read = true;
            }
        }
        for k in 0..nlextents {
            kb.reset();
            kb.push_str("stats.arenas.");
            kb.push_index(i);
            kb.push_str(".lextents.");
            kb.push_index(k);
            kb.push_str(".curlextents");
            if let Some(n) = read_mallctl_size(kb.key()) {
                live[nbins + k] += n;
                arena_read = true;
            }
        }
        if arena_read {
            initialized_arenas += 1;
        }
    }
    if initialized_arenas == 0 {
        return None;
    }

    classes.extend(
        sizes
            .iter()
            .copied()
            .zip(live.iter().copied())
            .filter(|&(size, objects)| objects > 0 && size > 0)
            .map(|(size, live_objects)| SizeClassUsage { size, live_objects }),
    );
    classes.sort_by(|a, b| b.bytes().cmp(&a.bytes()).then(a.size.cmp(&b.size)));
    Some(classes)
}

/// Windows（mimalloc）没有 jemalloc 的 size class 计数接口，不提供该表。
#[cfg(target_os = "windows")]
pub fn query_size_classes() -> Option<Vec<SizeClassUsage>> {
    None
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

/// 从 mimalloc stats JSON 解析 allocated（malloc_normal + malloc_huge 的 current）。
/// 任一 current 为负说明该份 stats 已失真（长会话后 mimalloc 计数可为负，见 #758），
/// 返回 None 表示整份快照不可信：调用方不得用 0 冒充读数再派生碎片/未识别指标。
#[cfg(any(target_os = "windows", test))]
pub(crate) fn mimalloc_allocated_from_json(v: &serde_json::Value) -> Option<usize> {
    let normal = v["malloc_normal"]["current"].as_i64().unwrap_or(0);
    let huge = v["malloc_huge"]["current"].as_i64().unwrap_or(0);
    if normal < 0 || huge < 0 {
        return None;
    }
    Some(normal as usize + huge as usize)
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
    // stats 不可读或计数失真（负值，见 #758）时回退 0，不用 RSS 冒充 allocated
    // （否则差异诊断失真）；/gc 侧对 allocated == 0 不输出 allocated-RSS 行。
    let current_allocated = mimalloc_stats_json()
        .and_then(|json| serde_json::from_str::<serde_json::Value>(&json).ok())
        .and_then(|v| mimalloc_allocated_from_json(&v))
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
    // 计数失真时整份快照不可信（#758）：返回 None，/gc 跳过分配器明细段与
    // 已知/未识别段，不再用 0 派生「碎片 = active」「未识别为负」。
    let allocated = match mimalloc_allocated_from_json(&v) {
        Some(allocated) => allocated,
        None => return None,
    };
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
///
/// `mi_stats_get_json` 返回的 JSON 自带换行与缩进，直接输出会占满日志（仅首行带前缀，
/// 其余为裸行）。此处重序列化为紧凑单行，保证一条日志一条记录，便于检索与归档。
#[cfg(target_os = "windows")]
pub fn dump_stats() {
    if let Some(json) = mimalloc_stats_json() {
        let compact = serde_json::from_str::<serde_json::Value>(&json)
            .ok()
            .and_then(|v| serde_json::to_string(&v).ok())
            .unwrap_or(json);
        tracing::info!("mimalloc stats: {compact}");
    }
}

/// Windows 的 `retained` 是 `reserved - committed`（保留未提交），是真实读数，
/// 不依赖 jemalloc 的 `opt.retain`，恒有意义。
#[cfg(target_os = "windows")]
pub fn retained_metric_applicable() -> bool {
    true
}
