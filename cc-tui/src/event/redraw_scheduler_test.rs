use super::*;

const INTERVAL: Duration = Duration::from_millis(33);
const IDLE_TIMEOUT: Duration = Duration::from_millis(50);

#[test]
fn test_initial_frame_is_due_without_waiting() {
    let now = Instant::now();
    let scheduler = RedrawScheduler::new(INTERVAL, now);
    assert!(scheduler.is_due(now), "初始帧应立即绘制");
    assert_eq!(scheduler.wait_timeout(now, IDLE_TIMEOUT), Duration::ZERO);
}

#[test]
fn test_repeated_requests_keep_original_deadline() {
    let now = Instant::now();
    let mut scheduler = RedrawScheduler::new(INTERVAL, now);
    scheduler.finish_draw(now, true);
    for offset in [1, 10, 20, 32] {
        scheduler.request_redraw();
        let current = now + Duration::from_millis(offset);
        assert!(!scheduler.is_due(current), "连续输入不能突破帧间隔");
        assert_eq!(
            scheduler.wait_timeout(current, IDLE_TIMEOUT),
            Duration::from_millis(33 - offset),
            "连续请求不能延后已排定的重画期限"
        );
    }
    assert!(scheduler.is_due(now + INTERVAL), "停下输入后尾帧仍须到期");
}

#[test]
fn test_pending_frame_retries_with_bounded_wait() {
    let now = Instant::now();
    let mut scheduler = RedrawScheduler::new(INTERVAL, now);
    scheduler.finish_draw(now, false);
    assert!(!scheduler.is_due(now), "布局未就绪时不能立即忙等重试");
    assert_eq!(scheduler.wait_timeout(now, IDLE_TIMEOUT), INTERVAL);
    assert!(scheduler.is_due(now + INTERVAL), "未绘制的请求不能丢失");
    scheduler.finish_draw(now + INTERVAL, false);
    assert_eq!(
        scheduler.wait_timeout(now + INTERVAL, IDLE_TIMEOUT),
        INTERVAL,
        "多次未就绪仍须按帧间隔重试"
    );
}

#[test]
fn test_successful_frame_clears_pending_request() {
    let now = Instant::now();
    let mut scheduler = RedrawScheduler::new(INTERVAL, now);
    scheduler.finish_draw(now, true);
    assert!(!scheduler.is_due(now + INTERVAL), "成功绘制后应清除 dirty");
    assert_eq!(
        scheduler.wait_timeout(now + INTERVAL, IDLE_TIMEOUT),
        IDLE_TIMEOUT,
        "没有更新时恢复普通输入等待"
    );
}

#[test]
fn test_frame_interval_starts_after_draw_completion() {
    let now = Instant::now();
    let completed_at = now + Duration::from_millis(80);
    let mut scheduler = RedrawScheduler::new(INTERVAL, now);
    scheduler.finish_draw(completed_at, true);
    scheduler.request_redraw();
    assert!(!scheduler.is_due(completed_at), "慢帧结束后仍须保留帧间隔");
    assert_eq!(scheduler.wait_timeout(completed_at, IDLE_TIMEOUT), INTERVAL);
    assert!(scheduler.is_due(completed_at + INTERVAL));
}

#[test]
fn test_input_wait_never_exceeds_dirty_deadline() {
    let now = Instant::now();
    let mut scheduler = RedrawScheduler::new(INTERVAL, now);
    scheduler.finish_draw(now, true);
    scheduler.request_redraw();
    assert_eq!(
        scheduler.wait_timeout(now + Duration::from_millis(30), IDLE_TIMEOUT),
        Duration::from_millis(3)
    );
    assert_eq!(
        scheduler.wait_timeout(now + Duration::from_millis(40), IDLE_TIMEOUT),
        Duration::ZERO,
        "已过期的尾帧不能再阻塞等待输入"
    );
    assert_eq!(
        scheduler.wait_timeout(now, Duration::from_millis(5)),
        Duration::from_millis(5),
        "帧期限不能延长调用方允许的等待"
    );
}

#[test]
fn test_redraw_after_idle_is_immediately_due() {
    let now = Instant::now();
    let mut scheduler = RedrawScheduler::new(INTERVAL, now);
    scheduler.finish_draw(now, true);
    scheduler.request_redraw();
    assert!(
        scheduler.is_due(now + Duration::from_secs(1)),
        "空闲后的新请求无需额外等待一个帧间隔"
    );
}
