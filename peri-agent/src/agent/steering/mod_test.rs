use super::*;

#[tokio::test]
async fn test_steering_close_rejects_unconsumed_messages() {
    let queue = SteeringQueue::default();
    let receipt = queue
        .enqueue(MessageContent::text("补充"))
        .expect("队列应开放");
    queue.close();
    assert!(receipt.await.is_err(), "未消费消息必须拒绝，不能误报成功");
    assert!(queue.enqueue(MessageContent::text("迟到")).is_none());
}

#[tokio::test]
async fn test_steering_final_boundary_closes_only_when_empty() {
    let queue = SteeringQueue::default();
    let mut receipt = queue
        .enqueue(MessageContent::text("补充"))
        .expect("队列应开放");
    let pending = queue.drain_or_close();
    assert_eq!(pending.len(), 1);
    assert!(matches!(
        receipt.try_recv(),
        Err(oneshot::error::TryRecvError::Empty)
    ));
    assert!(queue.enqueue(MessageContent::text("第二条")).is_some());
    assert_eq!(queue.drain_or_close().len(), 1);
    assert!(queue.drain_or_close().is_empty());
    assert!(queue.enqueue(MessageContent::text("迟到")).is_none());
}

#[tokio::test]
async fn test_steering_guard_rejects_on_early_exit() {
    let queue = SteeringQueue::default();
    let receipt = queue
        .enqueue(MessageContent::text("补充"))
        .expect("队列应开放");
    let guard = queue.close_on_drop();
    drop(guard);
    assert!(receipt.await.is_err());
}
