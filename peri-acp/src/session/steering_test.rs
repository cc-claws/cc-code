use super::*;
use crate::transport::{mpsc::mpsc_transport_pair, types::IncomingMessage, AcpTransport};
use peri_agent::{
    agent::{AgentInput, AgentState, ReActAgent, State},
    llm::MockLLM,
    messages::ContentBlock,
};

#[tokio::test]
async fn test_steering_transport_preserves_image_and_confirms_consumption() {
    let (client, server) = mpsc_transport_pair();
    let queue = SteeringQueue::default();
    let content = MessageContent::blocks(vec![
        ContentBlock::text("补充截图"),
        ContentBlock::image_base64("image/png", "cG5n"),
    ]);
    let expected = content.clone();
    let request = tokio::spawn(async move {
        client
            .send_request(
                "peri/session/steer",
                json!({
                    "sessionId": "test-session", "message": { "content": content },
                }),
            )
            .await
    });
    let Some(IncomingMessage::Request { id, params, .. }) = server.recv().await else {
        panic!("应收到补充请求");
    };
    let receipt = enqueue(Some(&queue), &params);
    assert!(!request.is_finished(), "仅入队不能成功确认");
    let agent = ReActAgent::new(MockLLM::always_answer("完成")).with_steering(queue);
    let mut state = AgentState::new(".");
    assert!(agent
        .execute(AgentInput::text("原任务"), &mut state, None)
        .await
        .is_ok());
    assert!(server
        .send_response(id, confirm(receipt).await)
        .await
        .is_ok());
    let response = request
        .await
        .expect("请求任务正常结束")
        .expect("消费后确认成功");
    assert_eq!(response["consumed"], true);
    assert!(state
        .messages()
        .iter()
        .any(|message| message.message_content() == &expected));
}

#[tokio::test]
async fn test_steering_closed_execution_keeps_request_rejected() {
    let queue = SteeringQueue::default();
    let params = json!({ "message": { "content": "补充" } });
    let receipt = enqueue(Some(&queue), &params);
    queue.close();
    assert!(
        confirm(receipt).await.is_err(),
        "结束时尚未消费必须返回失败"
    );
    assert!(
        enqueue(Some(&queue), &params).is_err(),
        "已结束后必须拒绝入队"
    );
    assert!(enqueue(None, &params).is_err(), "无运行中执行必须拒绝");
}

#[test]
fn test_steering_rejects_invalid_or_empty_content() {
    let queue = SteeringQueue::default();
    assert!(enqueue(Some(&queue), &json!({})).is_err());
    assert!(enqueue(Some(&queue), &json!({ "message": { "content": 42 } })).is_err());
    assert!(enqueue(Some(&queue), &json!({ "message": { "content": "" } })).is_err());
}
