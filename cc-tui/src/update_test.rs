//! `update.rs` 的行切分与容错解码测试（不触发真实下载）。

use super::take_complete_lines;

#[test]
fn test_take_complete_lines_splits_on_newline() {
    let mut buf = b"a\nbc\n".to_vec();
    assert_eq!(
        take_complete_lines(&mut buf),
        vec!["a".to_string(), "bc".to_string()]
    );
    assert!(buf.is_empty());
}

#[test]
fn test_take_complete_lines_keeps_partial_tail() {
    let mut buf = b"a\npar".to_vec();
    assert_eq!(take_complete_lines(&mut buf), vec!["a".to_string()]);
    assert_eq!(buf, b"par");
}

#[test]
fn test_take_complete_lines_strips_crlf() {
    let mut buf = b"a\r\nb\r\n".to_vec();
    assert_eq!(
        take_complete_lines(&mut buf),
        vec!["a".to_string(), "b".to_string()]
    );
}

#[test]
fn test_take_complete_lines_decodes_non_utf8_lossily() {
    // "文件" 的 GBK 字节（0xCE 0xC4 0xBC 0xFE）不是合法 UTF-8，不能 panic。
    let mut buf = vec![0xCE, 0xC4, 0xBC, 0xFE, b'\n'];
    let lines = take_complete_lines(&mut buf);
    assert_eq!(lines.len(), 1);
    assert!(!lines[0].is_empty());
    assert!(buf.is_empty());
}

#[test]
fn test_take_complete_lines_empty_buffer_yields_nothing() {
    let mut buf: Vec<u8> = Vec::new();
    assert!(take_complete_lines(&mut buf).is_empty());
}
