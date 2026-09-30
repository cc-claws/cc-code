//! Windows mimalloc 全局分配器冒烟测试。
//!
//! 验证 peri-tui 在 Windows 上启用 mimalloc 后，`/gc` 依赖的诊断接口能读到
//! 真实数据（而非此前的 stub 硬编码 0）：
//!
//! - `query_breakdown()` 返回 `Some` 且 allocated > 1MB
//! - `query_stats().current_allocated > 0`
//! - `alloc_collect()` 可安全调用、不 panic
//! - `allocator_name() == "mimalloc"`
//! - 层级关系：allocated <= mapped（reserved 应 >= 应用分配）
//!
//! 非 Windows 平台 `allocator_name()` 返回 "jemalloc"，故整个文件按平台跳过。

#![cfg(target_os = "windows")]

use peri_tui::alloc_config::{alloc_collect, allocator_name, query_breakdown, query_stats};

// 集成测试是独立 crate，不会继承 main.rs 的 `#[global_allocator]`。
// 若不在测试二进制内显式声明，本进程仍走系统分配器，mimalloc stats 恒为 0。
// 与生产 main.rs 使用同一 allocator 类型，才能真实响应 query_breakdown()。
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

/// 分配并 touch 一批内存（> 8 MiB），确保分配器 stats 有可观测的量。
fn touch_batch() -> Vec<Vec<u8>> {
    let mut keep = Vec::new();
    for i in 0..16u8 {
        let mut v = vec![0u8; 1 << 20]; // 每块 1 MiB
        // 逐页写入，强制提交物理页
        for b in v.iter_mut().step_by(4096) {
            *b = i.wrapping_add(1);
        }
        keep.push(v);
    }
    keep
}

#[test]
fn test_allocator_name_is_mimalloc_on_windows() {
    assert_eq!(allocator_name(), "mimalloc");
}

#[test]
fn test_query_breakdown_returns_real_data() {
    let _keep = touch_batch(); // 保持 > 8 MiB 在用

    let bd = query_breakdown().expect("query_breakdown 应返回 Some");
    eprintln!(
        "breakdown: allocated={} active={} resident={} metadata={} mapped={} retained={}",
        bd.allocated, bd.active, bd.resident, bd.metadata, bd.mapped, bd.retained
    );

    assert!(
        bd.allocated > 1024 * 1024,
        "allocated 应 > 1MB，实际 {}",
        bd.allocated
    );
    // 层级：应用分配不应超过累计向 OS 保留的虚拟地址空间
    assert!(
        bd.allocated <= bd.mapped,
        "allocated({}) 应 <= mapped/reserved({})",
        bd.allocated,
        bd.mapped
    );

    if bd.resident == 0 {
        eprintln!(
            "警告: resident == 0，可能 mimalloc JSON 字段 process.rss_current 缺失或映射错误"
        );
    }
    // 注意：不断言 active <= resident——resident 为 WorkingSet，换出页不计入，允许小于 active。
}

#[test]
fn test_query_stats_reports_allocated() {
    let _keep = touch_batch();

    let stats = query_stats().expect("query_stats 应返回 Some");
    eprintln!(
        "stats: rss={} allocated={}",
        stats.current_rss, stats.current_allocated
    );

    assert!(
        stats.current_allocated > 0,
        "current_allocated 应 > 0，实际 {}",
        stats.current_allocated
    );
}

#[test]
fn test_alloc_collect_does_not_panic() {
    let _keep = touch_batch();

    alloc_collect();
    alloc_collect();
}
