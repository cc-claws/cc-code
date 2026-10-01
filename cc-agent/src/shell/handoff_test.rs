use super::*;

#[test]
fn test_handoff_后台移交后不能领取取消权() {
    let handoff = ShellHandoff::new(true, false);
    assert!(handoff.background());
    assert!(
        !handoff.settle_foreground(),
        "通知尚未被消费也不能误杀后台进程"
    );
    assert!(handoff.is_backgrounded());
}

#[test]
fn test_handoff_前台已收口不能再后台化() {
    let handoff = ShellHandoff::new(true, false);
    assert!(handoff.settle_foreground());
    assert!(!handoff.background(), "完成或取消后不能承诺后台句柄");
}

#[test]
fn test_handoff_无后台宿主拒绝移交() {
    let handoff = ShellHandoff::new(false, false);
    assert!(!handoff.background());
    assert!(handoff.settle_foreground());
}

#[tokio::test]
async fn test_handoff_移交早于等待也不会丢通知() {
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
fn test_handoff_并发取消与后台化只能一个成功() {
    for _ in 0..100 {
        let handoff = std::sync::Arc::new(ShellHandoff::new(true, false));
        let other = handoff.clone();
        let background = std::thread::spawn(move || other.background());
        let cancelled = handoff.settle_foreground();
        let backgrounded = background.join().expect("线程正常退出");
        assert_ne!(cancelled, backgrounded, "取消权和后台归属不能同时成立");
    }
}
