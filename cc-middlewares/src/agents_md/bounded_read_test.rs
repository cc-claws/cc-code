use super::*;
use std::io::Cursor;

struct MockCountingReader<R> {
    inner: R,
    bytes_read: usize,
    seek_count: usize,
}

impl<R: Read> Read for MockCountingReader<R> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let count = self.inner.read(buffer)?;
        self.bytes_read += count;
        Ok(count)
    }
}

impl<R: Seek> Seek for MockCountingReader<R> {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        self.seek_count += 1;
        self.inner.seek(position)
    }
}

fn make_counting_reader(bytes: Vec<u8>) -> MockCountingReader<Cursor<Vec<u8>>> {
    MockCountingReader {
        inner: Cursor::new(bytes),
        bytes_read: 0,
        seek_count: 0,
    }
}

#[test]
fn test_read_bounded_reader_large_file_reads_only_head_and_tail() {
    let mut bytes = vec![b'x'; 16 * 1024 * 1024];
    bytes[..4].copy_from_slice(b"HEAD");
    let file_length = bytes.len() as u64;
    let tail_start = bytes.len() - 4;
    bytes[tail_start..].copy_from_slice(b"TAIL");
    let mut reader = make_counting_reader(bytes);
    let output = read_bounded_reader(&mut reader, file_length, 256, "AGENTS.md")
        .expect("大文件有界读取应成功");
    assert!(output.starts_with("HEAD"), "应保留文件头部");
    assert!(output.ends_with("TAIL"), "应保留文件尾部");
    assert!(
        output.contains("truncated AGENTS.md"),
        "应包含来源和截断标记"
    );
    assert!(output.len() <= 256, "输出不能超过字节限额");
    assert!(reader.bytes_read <= 262, "必须限制实际读取量而非仅限制输出");
    assert_eq!(reader.seek_count, 1, "应直接跳转尾部而非读取中间区域");
}

#[test]
fn test_read_bounded_file_preserves_small_file_contents() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("AGENTS.md");
    let content = "\u{feff}规则\r\n保留原文🙂\n";
    std::fs::write(&path, content).unwrap();
    let output = read_bounded_file(&path, 128).expect("小文件读取应成功");
    assert_eq!(output, content, "有界读取不应归一或改写小文件原文");
}

#[test]
fn test_read_bounded_reader_preserves_multibyte_head_and_tail() {
    let content = format!("头部{}尾部", "规则🙂".repeat(4096));
    for budget in 128..145 {
        let mut reader = make_counting_reader(content.as_bytes().to_vec());
        let output = read_bounded_reader(&mut reader, content.len() as u64, budget, "规则.md")
            .expect("切开中文和表情时应安全裁剪");
        assert!(output.starts_with("头部"), "应保留完整头部中文");
        assert!(output.ends_with("尾部"), "应保留完整尾部中文");
        assert!(output.len() <= budget, "多字节内容不能超过字节预算");
        assert!(
            reader.bytes_read <= budget + 6,
            "UTF-8 边界校验最多额外读取 6 字节"
        );
    }
}

#[test]
fn test_read_bounded_reader_handles_zero_and_tiny_budgets() {
    let content = "中文🙂规则".repeat(64);
    for budget in 0..40 {
        let mut reader = make_counting_reader(content.as_bytes().to_vec());
        let output = read_bounded_reader(&mut reader, content.len() as u64, budget, "AGENTS.md")
            .expect("极小预算应安全返回有效 UTF-8");
        assert!(output.len() <= budget, "小预算也不能越界");
        assert!(
            reader.bytes_read <= budget + 6,
            "小预算也必须限制实际读取量"
        );
        if budget == 0 {
            assert_eq!(reader.bytes_read, 0, "零预算不读取正文");
            assert!(output.is_empty(), "零预算返回空字符串");
        }
    }
}

#[test]
fn test_read_bounded_reader_rejects_invalid_small_file_utf8() {
    let bytes = vec![b'A', 0xff, b'B'];
    let mut reader = make_counting_reader(bytes);
    let error = read_bounded_reader(&mut reader, 3, 16, "AGENTS.md")
        .expect_err("小文件非法 UTF-8 必须报错");
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
}

#[test]
fn test_read_bounded_reader_rejects_invalid_selected_head_and_tail() {
    for invalid_index in [1, 1023] {
        let mut bytes = vec![b'x'; 1024];
        bytes[invalid_index] = 0xff;
        let mut reader = make_counting_reader(bytes);
        let error = read_bounded_reader(&mut reader, 1024, 128, "AGENTS.md")
            .expect_err("选中头尾的非法 UTF-8 不应被静默替换");
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    }
}

#[test]
fn test_read_bounded_reader_rejects_invalid_character_across_head_boundary() {
    let mut bytes = vec![b'x'; 1024];
    let (_, head_bytes, _) = truncation_layout(128, "AGENTS.md", 1024).unwrap();
    bytes[head_bytes - 1] = 0xe4;
    // 起始字节落在选中区域，后面的 ASCII 不能当成合法中文切边吞掉。
    let mut reader = make_counting_reader(bytes);
    let error = read_bounded_reader(&mut reader, 1024, 128, "AGENTS.md")
        .expect_err("跨头部边界的非法字符应报错");
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
}

#[test]
fn test_read_bounded_reader_rejects_invalid_continuation_at_tail_boundary() {
    let mut bytes = vec![b'x'; 1024];
    let (_, _, tail_bytes) = truncation_layout(128, "AGENTS.md", 1024).unwrap();
    bytes[1024 - tail_bytes] = 0x80;
    // 尾部首字节虽然像被切开的字符，但前一个 ASCII 证明它是非法续字节。
    let mut reader = make_counting_reader(bytes);
    let error = read_bounded_reader(&mut reader, 1024, 128, "AGENTS.md")
        .expect_err("非法续字节不能被当成切边吞掉");
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
}

#[test]
fn test_read_bounded_reader_skips_unselected_middle_without_utf8_decoding() {
    let mut bytes = vec![b'x'; 1024];
    bytes[512] = 0xff;
    let mut reader = make_counting_reader(bytes);
    let output = read_bounded_reader(&mut reader, 1024, 128, "AGENTS.md")
        .expect("未读取的中部内容不需要 UTF-8 校验");
    assert!(output.len() <= 128);
    assert!(reader.bytes_read <= 134, "不能为校验中部内容加载全文");
}

#[test]
fn test_truncation_layout_uses_shared_ratios_and_handles_large_limits() {
    let (marker, head_bytes, tail_bytes) =
        truncation_layout(usize::MAX, "AGENTS.md", u64::MAX).unwrap();
    assert_eq!(marker.len() + head_bytes + tail_bytes, usize::MAX);
    assert!(head_bytes > tail_bytes, "正文预算应优先分配给头部");
    assert!(truncation_layout(1, "AGENTS.md", 1024).is_none());
    let (short_marker, _, _) = truncation_layout(32, &"长".repeat(4096), 1024).unwrap();
    assert_eq!(short_marker, SHORT_MARKER, "长路径应回退短标记");
}
