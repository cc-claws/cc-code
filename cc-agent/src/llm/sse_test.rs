use super::*;

#[test]
fn test_basic_single_line_data() {
    let mut parser = SseParser::new();
    let events = parser.push(b"data: {\"key\":\"value\"}\n\n");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].0, None);
    assert_eq!(events[0].1, "{\"key\":\"value\"}");
}

#[test]
fn test_crlf_line_endings() {
    let mut parser = SseParser::new();
    let events = parser.push(b"data: hello\r\n\r\n");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].0, None);
    assert_eq!(events[0].1, "hello");
}

#[test]
fn test_multi_line_data_joined() {
    let mut parser = SseParser::new();
    let events = parser.push(b"data: line1\ndata: line2\n\n");
    assert_eq!(events.len(), 1);
    // data 行被直接拼接（不添加换行符 —— 协议层自行处理）
    assert_eq!(events[0].1, "line1line2");
}

#[test]
fn test_cross_chunk_line_join() {
    let mut parser = SseParser::new();
    // 第一个 chunk 以不完整行结尾
    let events1 = parser.push(b"data: {\"partial");
    assert!(events1.is_empty());

    // 第二个 chunk 补全
    let events2 = parser.push(b"_key\":\"val\"}\n\n");
    assert_eq!(events2.len(), 1);
    assert_eq!(events2[0].1, "{\"partial_key\":\"val\"}");
}

#[test]
fn test_event_and_data_pair() {
    let mut parser = SseParser::new();
    let events = parser.push(b"event: content_block_delta\ndata: {\"delta\":\"text\"}\n\n");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].0.as_deref(), Some("content_block_delta"));
    assert_eq!(events[0].1, "{\"delta\":\"text\"}");
}

#[test]
fn test_data_done_termination() {
    let mut parser = SseParser::new();
    let events = parser.push(b"data: {\"last\":true}\n\ndata: [DONE]\n\n");
    assert!(!events.is_empty());
    assert!(parser.is_done());
}

#[test]
fn test_empty_data_line_skipped() {
    let mut parser = SseParser::new();
    let events = parser.push(b"data: \n\n");
    // 空 data: 行被跳过，不产出事件
    assert!(events.is_empty());
}

#[test]
fn test_empty_stream() {
    let mut parser = SseParser::new();
    let events = parser.push(b"");
    assert!(events.is_empty());
    assert!(!parser.is_done());
}

#[test]
fn test_multiple_events_in_one_chunk() {
    let mut parser = SseParser::new();
    let events = parser.push(b"data: first\n\ndata: second\n\ndata: third\n\n");
    assert_eq!(events.len(), 3);
    assert_eq!(events[0].1, "first");
    assert_eq!(events[1].1, "second");
    assert_eq!(events[2].1, "third");
}

#[test]
fn test_data_without_space_after_colon() {
    let mut parser = SseParser::new();
    let events = parser.push(b"data:hello\n\n");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].1, "hello");
}

#[test]
fn test_event_type_reset_after_commit() {
    let mut parser = SseParser::new();
    // 第一组: event + data
    let _ = parser.push(b"event: type_a\ndata: aaa\n\n");
    // 第二组: 仅 data（event_type 应已重置）
    let events = parser.push(b"data: bbb\n\n");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].0, None);
    assert_eq!(events[0].1, "bbb");
}

#[test]
fn test_only_event_no_data_no_commit() {
    let mut parser = SseParser::new();
    // 单独 event 行不触发 commit
    let events = parser.push(b"event: content_block_start\n");
    assert!(events.is_empty());
}

#[test]
fn test_done_without_space() {
    let mut parser = SseParser::new();
    let _ = parser.push(b"data:[DONE]\n\n");
    assert!(parser.is_done());
}

#[test]
fn test_cross_chunk_utf8_cjk() {
    // "描述" UTF-8: E6 8F 8F E8 BF B0
    // chunk 切在字符之间：第一个 chunk 含完整 "描"，第二个含完整 "述"
    let mut parser = SseParser::new();
    let events1 = parser.push(b"data: \xe6\x8f\x8f");
    assert!(events1.is_empty());
    let events2 = parser.push(b"\xe8\xbf\xb0\n\n");
    assert_eq!(events2.len(), 1);
    assert_eq!(events2[0].1, "描述");
}

#[test]
fn test_cross_chunk_utf8_mid_character() {
    // "描述" UTF-8: E6 8F 8F E8 BF B0
    // chunk 切在 "描" 的最后一个字节之前：E6 8F | 8F E8 BF B0
    // 旧代码会在此处产生 U+FFFD
    let mut parser = SseParser::new();
    let events1 = parser.push(b"data: \xe6\x8f");
    assert!(events1.is_empty());
    let events2 = parser.push(b"\x8f\xe8\xbf\xb0\n\n");
    assert_eq!(events2.len(), 1);
    assert_eq!(events2[0].1, "描述");
    // 确保没有乱码字符
    assert!(!events2[0].1.contains('\u{FFFD}'));
}

#[test]
fn test_cross_chunk_utf8_emoji() {
    // "🎉" UTF-8: F0 9F 8E 89 (4 字节)
    // chunk 切在 2+2：F0 9F | 8E 89
    let mut parser = SseParser::new();
    let events1 = parser.push(b"data: \xf0\x9f");
    assert!(events1.is_empty());
    let events2 = parser.push(b"\x8e\x89\n\n");
    assert_eq!(events2.len(), 1);
    assert_eq!(events2[0].1, "🎉");
}
