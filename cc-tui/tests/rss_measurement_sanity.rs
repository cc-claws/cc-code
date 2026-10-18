//! RSS 采样链路自检（issue #589 的回归守门）
//!
//! 其余内存测试只靠打印 RSS 数值观察，一旦采样链路再次失效（比如又退回读
//! `/proc/self/status`），读数恒为 0、所有「增量」都变成 +0 KB，测试仍然全绿。
//! 这里用一次可预期的分配，把「读数确实跟随真实驻留内存」钉成断言。
//!
//! 单独开一个测试二进制，避免 64 MiB 的扰动飘到并行测量的其它测试里。

mod common;

use common::current_rss_kb;

#[test]
fn test_rss_measurement_tracks_growth() {
    let baseline = current_rss_kb();
    assert!(baseline > 0, "RSS 基线读数必须 > 0，实得 {baseline} KB");
    // 64 MiB 逐页触碰，强制提交物理页；阈值取一半，留出平台差异余量
    let mut ballast = vec![0u8; 64 * 1024 * 1024];
    for page in ballast.chunks_mut(4096) {
        page[0] = 1;
    }
    let after = current_rss_kb();
    drop(ballast);
    assert!(
        after >= baseline + 32 * 1024,
        "触碰 64 MiB 后 RSS 至少应涨 32 MiB：baseline={baseline} KB after={after} KB"
    );
}
