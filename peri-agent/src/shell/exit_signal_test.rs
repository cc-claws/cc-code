use super::{ExitSignal, ShellOutcome};
use std::{sync::Arc, time::Duration};

#[tokio::test]
async fn test_exit_signal_wait_unblocks_after_completion() {
    let signal = Arc::new(ExitSignal::new());
    let waiter_signal = Arc::clone(&signal);
    let waiter = tokio::spawn(async move { waiter_signal.wait().await });
    signal.finish(ShellOutcome::Exited(0));
    tokio::time::timeout(Duration::from_secs(1), waiter)
        .await
        .expect("完成信号必须唤醒等待者")
        .expect("等待任务不能 panic");
    assert_eq!(signal.outcome(), Some(ShellOutcome::Exited(0)));
}

#[tokio::test]
async fn test_exit_signal_wait_returns_when_already_completed() {
    let signal = ExitSignal::new();
    signal.finish(ShellOutcome::Cancelled);
    tokio::time::timeout(Duration::from_secs(1), signal.wait())
        .await
        .expect("等待已完成信号不能挂起");
}
