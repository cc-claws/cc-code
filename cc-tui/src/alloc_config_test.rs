use crate::alloc_config::*;

#[test]
fn test_alloc_collect_does_not_panic() {
    // alloc_collect 应可安全多次调用
    alloc_collect();
    alloc_collect();
    alloc_collect();
}

/// jemalloc stats 查询仅在非 Windows 平台有效（Windows stub 返回 None）
#[cfg(not(target_os = "windows"))]
#[test]
fn test_query_stats_returns_valid_data() {
    let stats = query_stats().expect("query_stats 应返回数据");
    assert!(stats.current_rss > 0, "RSS 应大于 0");
    assert!(stats.current_allocated > 0, "jemalloc allocated 应大于 0");
    // RSS >= allocated（RSS 含碎片和元数据）
    assert!(
        stats.current_rss >= stats.current_allocated,
        "RSS({}) 应 >= allocated({})",
        stats.current_rss,
        stats.current_allocated,
    );
    eprintln!(
        "stats: rss={} allocated={} gap={}",
        stats.current_rss,
        stats.current_allocated,
        stats.current_rss - stats.current_allocated,
    );
}

/// jemalloc breakdown 查询仅在非 Windows 平台有效（Windows stub 返回 None）
#[cfg(not(target_os = "windows"))]
#[test]
fn test_breakdown_shows_fragmentation() {
    let bd = query_breakdown().expect("query_breakdown 应返回数据");
    eprintln!("jemalloc breakdown:");
    eprintln!("  allocated: {} bytes", bd.allocated);
    eprintln!(
        "  active:    {} bytes (frag: {})",
        bd.active,
        bd.active.saturating_sub(bd.allocated)
    );
    eprintln!(
        "  resident:  {} bytes (waste: {})",
        bd.resident,
        bd.resident.saturating_sub(bd.active)
    );
    eprintln!("  metadata:  {} bytes", bd.metadata);
    eprintln!("  mapped:    {} bytes", bd.mapped);
    eprintln!("  retained:  {} bytes", bd.retained);
    // 层级关系：allocated <= active <= resident
    assert!(
        bd.allocated <= bd.active,
        "allocated({}) 应 <= active({})",
        bd.allocated,
        bd.active
    );
    assert!(
        bd.active <= bd.resident,
        "active({}) 应 <= resident({})",
        bd.active,
        bd.resident
    );
}

/// `retained` 指标可用性与 jemalloc `opt.retain` 一致。
///
/// 不可用（`opt.retain=false`，macOS 即如此）时 `stats.retained` 必须恒为 0，
/// 因此 /gc 展示「不适用」而不是一个恒零读数。
#[cfg(not(target_os = "windows"))]
#[test]
fn test_retained_metric_applicability() {
    let applicable = retained_metric_applicable();
    let bd = query_breakdown().expect("query_breakdown 应返回数据");
    eprintln!(
        "opt.retain => applicable={applicable}, stats.retained={} bytes",
        bd.retained
    );
    if !applicable {
        assert_eq!(
            bd.retained, 0,
            "opt.retain=false 时 stats.retained 应为 0，否则读数口径有问题"
        );
    }
}

#[cfg(not(target_os = "windows"))]
#[test]
fn test_size_classes_decompose_allocated() {
    // 说明：`cc-tui` 的 lib 测试二进制里 jemalloc **不是**全局分配器
    //（`#[global_allocator]` 只注册在 main.rs），所以下面这张表里只有 jemalloc 自身的
    // 内部分配（几十 KB）。代码路径与真实 TUI 进程完全一致，只是规模小得多。
    let classes = query_size_classes().expect("query_size_classes 应返回数据");
    assert!(!classes.is_empty(), "size class 表不应为空");
    // 只保留存活对象数 > 0 的 class（232 个 class 里绝大多数恒为 0，全列是大片噪音）
    assert!(
        classes.iter().all(|c| c.live_objects > 0 && c.size > 0),
        "不应返回空 class: {:?}",
        classes.iter().find(|c| c.live_objects == 0 || c.size == 0)
    );
    // 排序契约：/gc 按此顺序直接打印，必须稳定（bytes 降序，同 bytes 时 size 升序）
    for w in classes.windows(2) {
        let (a, b) = (w[0], w[1]);
        assert!(
            a.bytes() > b.bytes() || (a.bytes() == b.bytes() && a.size < b.size),
            "排序应为 bytes 降序 / size 升序，实际 {a:?} 之后是 {b:?}"
        );
    }
    // 每个 size class 尺寸唯一（不存在两个 class 拥有同一尺寸）
    let mut sizes: Vec<usize> = classes.iter().map(|c| c.size).collect();
    sizes.sort_unstable();
    let unique = {
        let mut s = sizes.clone();
        s.dedup();
        s.len()
    };
    assert_eq!(sizes.len(), unique, "size class 尺寸不应重复");

    // 逐 class 求和恒等于 stats.allocated：jemalloc 的 allocated 本身就是
    // `Σ curregs×reg_size + Σ curlextents×size`，两者是同一份记账。
    //
    // 这一条是「表能自解释 allocated」的核心保证，但它依赖两次读数之间分配器状态不变。
    // 其他用例会并发触发 jemalloc 的**临时**分配（例：`test_dump_stats` 里 stats_print
    // 的 ~64 KiB 输出缓冲），而本测试的基准只有几十 KB，瞬时漂移就足以把差额放大到
    // 100%。故此处多次采样，只要出现过一次精确相等即通过——恒等式本身不容许容差。
    let mut observed = Vec::new();
    let mut matched = false;
    for _ in 0..8 {
        let classes = query_size_classes().expect("query_size_classes 应返回数据");
        let sum: usize = classes.iter().map(|c| c.bytes()).sum();
        let allocated = query_breakdown().expect("query_breakdown 应返回数据").allocated;
        observed.push((classes.len(), sum, allocated));
        if allocated > 0 && sum == allocated {
            matched = true;
            break;
        }
    }
    assert!(
        matched,
        "size class 逐项求和应精确等于 stats.allocated（8 次采样均不相等）：{observed:?}"
    );
}

/// mimalloc 没有 jemalloc 的 size class 计数接口，Windows 上必须返回 None，
/// 由 /gc 跳过整段而不是打印空表。
#[cfg(target_os = "windows")]
#[test]
fn test_size_classes_absent_on_windows() {
    assert!(query_size_classes().is_none());
}

#[cfg(not(target_os = "windows"))]
#[test]
fn test_dump_stats() {
    // 分配一些内存让 stats 有意义
    let _vec: Vec<usize> = (0..256 * 1024).collect();
    eprintln!("=== jemalloc full stats ===");
    dump_stats();
    eprintln!("=== end ===");
}
