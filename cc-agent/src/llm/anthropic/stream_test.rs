use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio_util::sync::CancellationToken;

use crate::{
    agent::events::{AgentEvent, FnEventHandler},
    llm::{BaseModel, ChatAnthropic, LlmRequest, LlmResponse, StopReason, StreamingContext},
    messages::{BaseMessage, ContentBlock, MessageId},
};

#[derive(Default)]
struct StreamedText {
    text: String,
    thinking: String,
}

fn make_text_block(text: &str) -> (Value, Vec<Value>) {
    (
        json!({"type": "text", "text": ""}),
        vec![json!({"type": "text_delta", "text": text})],
    )
}

async fn make_stream_response(
    blocks: Vec<(Value, Vec<Value>)>,
    stop_reason: &str,
) -> (LlmResponse, StreamedText) {
    let mut events = vec![json!({
        "type": "message_start",
        "message": {"id": "msg-block-regression", "usage": {"input_tokens": 3}}
    })];
    for (index, (block, deltas)) in blocks.into_iter().enumerate() {
        events.push(json!({
            "type": "content_block_start", "index": index, "content_block": block
        }));
        for delta in deltas {
            events.push(json!({"type": "content_block_delta", "index": index, "delta": delta}));
        }
        events.push(json!({"type": "content_block_stop", "index": index}));
    }
    events.push(json!({
        "type": "message_delta", "delta": {"stop_reason": stop_reason},
        "usage": {"output_tokens": 9}
    }));
    events.push(json!({"type": "message_stop"}));
    let body: String = events
        .iter()
        .map(|event| {
            format!(
                "event: {}\ndata: {}\n\n",
                event["type"].as_str().unwrap(),
                event
            )
        })
        .collect();
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("本地 SSE 服务应能绑定临时端口");
    let address = listener.local_addr().expect("应取得本地监听地址");
    let server = tokio::spawn(async move {
        let (socket, _) = listener.accept().await.expect("应收到流式请求");
        let mut reader = BufReader::new(socket);
        let mut content_length = 0;
        loop {
            let mut line = String::new();
            assert!(reader.read_line(&mut line).await.expect("应能读取请求头") > 0);
            if line == "\r\n" {
                break;
            }
            if let Some((name, value)) = line.split_once(':') {
                if name.eq_ignore_ascii_case("content-length") {
                    content_length = value.trim().parse().expect("请求长度应有效");
                }
            }
        }
        let mut request_body = vec![0; content_length];
        reader
            .read_exact(&mut request_body)
            .await
            .expect("应能读取完整请求体");
        let mut socket = reader.into_inner();
        let headers = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        socket
            .write_all(headers.as_bytes())
            .await
            .expect("应能发送响应头");
        socket
            .write_all(body.as_bytes())
            .await
            .expect("应能发送 SSE 事件");
        socket.shutdown().await.expect("应能结束响应");
    });
    let emitted = Arc::new(Mutex::new(StreamedText::default()));
    let captured = Arc::clone(&emitted);
    let context = StreamingContext {
        event_handler: Arc::new(FnEventHandler(move |event| match event {
            AgentEvent::TextChunk { chunk, .. } => captured.lock().unwrap().text.push_str(&chunk),
            AgentEvent::AiReasoning(chunk) => captured.lock().unwrap().thinking.push_str(&chunk),
            _ => {}
        })),
        message_id: MessageId::new(),
        cancel: CancellationToken::new(),
    };
    let mut adapter = ChatAnthropic::new("local-test-key", "stream-block-test")
        .with_base_url(format!("http://{address}"));
    adapter.client = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(10))
        .build()
        .expect("应能构造本地测试客户端");
    let response = adapter
        .invoke_streaming(
            LlmRequest::new(vec![BaseMessage::human("验证分块")]),
            context,
        )
        .await
        .expect("本地 SSE 流应能解析");
    server.await.expect("本地 SSE 服务应正常退出");
    let streamed = std::mem::take(&mut *emitted.lock().unwrap());
    (response, streamed)
}

#[tokio::test]
async fn test_anthropic_stream_text_blocks_do_not_repeat_acp_notice() {
    let notice = "\n📦 [ACP] Compressed m00001–m00122 → 1 block(s), ~54917 tokens saved.\n";
    let answer = "下一步检查渲染结果。";
    let (response, streamed) = make_stream_response(
        vec![make_text_block(notice), make_text_block(answer)],
        "end_turn",
    )
    .await;
    let blocks = response.message.content_blocks();
    let texts: Vec<_> = blocks.iter().filter_map(ContentBlock::as_text).collect();
    assert_eq!(texts, vec![notice, answer], "每个文本块只能包含自己的增量");
    assert_eq!(
        response.message.content(),
        streamed.text,
        "快照与实时文本必须一致"
    );
    assert_eq!(
        response
            .message
            .content()
            .matches("[ACP] Compressed")
            .count(),
        1
    );
}

#[tokio::test]
async fn test_anthropic_stream_text_tool_text_keeps_block_order() {
    let (response, streamed) = make_stream_response(
        vec![
            make_text_block("工具前文本。"),
            (
                json!({"type": "tool_use", "id": "call-read", "name": "Read", "input": {}}),
                vec![json!({"type": "input_json_delta", "partial_json": "{\"file_path\":\"a.txt\"}"})],
            ),
            make_text_block("工具后文本。"),
        ],
        "tool_use",
    )
    .await;
    let blocks = response.message.content_blocks();
    assert_eq!(blocks.len(), 3);
    assert!(matches!(&blocks[0], ContentBlock::Text { text } if text.as_ref() == "工具前文本。"));
    assert!(
        matches!(&blocks[1], ContentBlock::ToolUse { id, name, input }
        if id == "call-read" && name == "Read" && input["file_path"] == "a.txt")
    );
    assert!(matches!(&blocks[2], ContentBlock::Text { text } if text.as_ref() == "工具后文本。"));
    assert_eq!(response.stop_reason, StopReason::ToolUse);
    assert_eq!(response.message.tool_calls().len(), 1);
    assert_eq!(response.message.content(), streamed.text);
}

#[tokio::test]
async fn test_anthropic_stream_thinking_blocks_do_not_share_text_or_signature() {
    let (response, streamed) = make_stream_response(
        vec![
            (
                json!({"type": "thinking", "thinking": "", "signature": "signature-one"}),
                vec![json!({"type": "thinking_delta", "thinking": "第一段思考。"})],
            ),
            (
                json!({"type": "thinking", "thinking": ""}),
                vec![json!({"type": "thinking_delta", "thinking": "第二段思考。"})],
            ),
            (
                json!({"type": "thinking", "thinking": "", "signature": "signature-three"}),
                vec![json!({"type": "thinking_delta", "thinking": "第三段思考。"})],
            ),
        ],
        "end_turn",
    )
    .await;
    let blocks = response.message.content_blocks();
    let reasoning: Vec<_> = blocks
        .iter()
        .map(|block| match block {
            ContentBlock::Reasoning {
                text, signature, ..
            } => (text.as_str(), signature.as_deref()),
            _ => panic!("响应只能包含指定的思考块"),
        })
        .collect();
    assert_eq!(
        reasoning,
        vec![
            ("第一段思考。", Some("signature-one")),
            ("第二段思考。", None),
            ("第三段思考。", Some("signature-three")),
        ],
        "思考内容和签名必须以块为边界"
    );
    assert_eq!(
        reasoning.iter().map(|(text, _)| *text).collect::<String>(),
        streamed.thinking
    );
}
