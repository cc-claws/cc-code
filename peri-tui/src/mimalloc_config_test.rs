use crate::mimalloc_config::*;

/// 测试 init_mimalloc_conf 不覆盖已存在的环境变量。
/// 使用唯一的哨兵值 "42" 来区分与其他并行测试的干扰。
#[test]
fn test_init_mimalloc_conf_does_not_overwrite() {
    let sentinel = "42";
    std::env::set_var("MIMALLOC_PAGE_RESET", sentinel);
    std::env::set_var("MIMALLOC_DECOMMIT", sentinel);
    std::env::set_var("MIMALLOC_BACKGROUND_THREAD", sentinel);

    init_mimalloc_conf();

    // 预设值不应被覆盖
    assert_eq!(
        std::env::var("MIMALLOC_PAGE_RESET").unwrap(),
        sentinel,
        "不应覆盖用户设置的 MIMALLOC_PAGE_RESET"
    );
    assert_eq!(
        std::env::var("MIMALLOC_DECOMMIT").unwrap(),
        sentinel,
        "不应覆盖用户设置的 MIMALLOC_DECOMMIT"
    );
    assert_eq!(
        std::env::var("MIMALLOC_BACKGROUND_THREAD").unwrap(),
        sentinel,
        "不应覆盖用户设置的 MIMALLOC_BACKGROUND_THREAD"
    );

    // Cleanup
    std::env::remove_var("MIMALLOC_PAGE_RESET");
    std::env::remove_var("MIMALLOC_DECOMMIT");
    std::env::remove_var("MIMALLOC_BACKGROUND_THREAD");
}

#[test]
fn test_alloc_collect_does_not_panic() {
    // alloc_collect 应可安全多次调用
    alloc_collect();
    alloc_collect();
    alloc_collect();
}

#[test]
fn test_query_stats_returns_valid_data() {
    let stats = query_stats().expect("query_stats 应返回数据");
    assert!(stats.current_rss > 0, "RSS 应大于 0");
    assert!(stats.current_commit > 0, "jemalloc allocated 应大于 0");
    // RSS >= allocated（RSS 含碎片和元数据）
    assert!(
        stats.current_rss >= stats.current_commit,
        "RSS({}) 应 >= allocated({})",
        stats.current_rss,
        stats.current_commit,
    );
    eprintln!(
        "stats: rss={} allocated={} gap={}",
        stats.current_rss,
        stats.current_commit,
        stats.current_rss - stats.current_commit,
    );
}

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

#[test]
fn test_dump_stats() {
    // 分配一些内存让 stats 有意义
    let _vec: Vec<usize> = (0..256 * 1024).collect();
    eprintln!("=== jemalloc full stats ===");
    dump_stats();
    eprintln!("=== end ===");
}
