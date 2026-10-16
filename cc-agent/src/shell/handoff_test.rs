use super::*;

#[test]
fn test_handoff_cannot_settle_foreground_after_backgrounded() {
    let handoff = ShellHandoff::new(true, false);
    assert!(handoff.background());
    assert!(
        !handoff.settle_foreground(),
        "通知尚未被消费也不能误杀后台进程"
    );
    assert!(handoff.is_backgrounded());
}

#[test]
fn test_handoff_cannot_background_after_foreground_settled() {
    let handoff = ShellHandoff::new(true, false);
    assert!(handoff.settle_foreground());
    assert!(!handoff.background(), "完成或取消后不能承诺后台句柄");
}

#[test]
fn test_handoff_rejects_when_host_unsupported() {
    let handoff = ShellHandoff::new(false, false);
    assert!(!handoff.background());
    assert!(handoff.settle_foreground());
}

#[tokio::test]
async fn test_handoff_background_before_wait_resolves_immediately() {
    let handoff = ShellHandoff::new(true, false);
    assert!(handoff.background());
    tokio::time::timeout(
        std::time::Duration::from_secs(1),
        handoff.wait_for_background(),
    )
    .await
    .expect("共享状态必须使晚到的等待者立即返回");
}

#[test]
fn test_handoff_concurrent_cancel_and_background_mutually_exclusive() {
    for _ in 0..100 {
        let handoff = std::sync::Arc::new(ShellHandoff::new(true, false));
        let other = handoff.clone();
        let background = std::thread::spawn(move || other.background());
        let cancelled = handoff.settle_foreground();
        let backgrounded = background.join().expect("线程正常退出");
        assert_ne!(cancelled, backgrounded, "取消权和后台归属不能同时成立");
    }
}
