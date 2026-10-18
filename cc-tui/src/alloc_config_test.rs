use crate::alloc_config::*;

/// 测试 init_alloc_conf 不覆盖已存在的 MALLOC_CONF 环境变量。
///
/// 仅非 Windows 生效：Windows 的 init_alloc_conf 走 mimalloc 运行时选项分支，不读
/// MALLOC_CONF，断言在此无意义；且它会额外调用一次 init() 参与同进程内 mimalloc
/// 全局选项的并行竞争（#545）。
#[cfg(not(target_os = "windows"))]
#[test]
fn test_init_alloc_conf_does_not_overwrite() {
    let sentinel = "dirty_decay_ms:9999";
    std::env::set_var("MALLOC_CONF", sentinel);

    init_alloc_conf();

    // 预设值不应被覆盖
    assert_eq!(
        std::env::var("MALLOC_CONF").unwrap(),
        sentinel,
        "不应覆盖用户设置的 MALLOC_CONF"
    );

    // Cleanup
    std::env::remove_var("MALLOC_CONF");
}

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
fn test_dump_stats() {
    // 分配一些内存让 stats 有意义
    let _vec: Vec<usize> = (0..256 * 1024).collect();
    eprintln!("=== jemalloc full stats ===");
    dump_stats();
    eprintln!("=== end ===");
}
