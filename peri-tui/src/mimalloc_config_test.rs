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
