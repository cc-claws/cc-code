use std::{collections::VecDeque, io::Cursor, time::Duration};

use ratatui::backend::{Backend, TestBackend};
use ratatui::buffer::Cell;
use unicode_width::UnicodeWidthStr;

use super::{parse_position, Byte, ByteSource, CursorWidthProbe, Position, PROBE_GLYPHS};
use crate::terminal_backend::compatible::{fallback_symbol, WidthSafeBackend};

struct ScriptedSource {
    bytes: VecDeque<u8>,
}

impl ScriptedSource {
    fn new(bytes: &[u8]) -> Self {
        Self {
            bytes: bytes.iter().copied().collect(),
        }
    }

    fn repeating(reply: &[u8], times: usize) -> Self {
        let mut bytes = Vec::with_capacity(reply.len() * times);
        for _ in 0..times {
            bytes.extend_from_slice(reply);
        }
        Self::new(&bytes)
    }
}

impl ByteSource for ScriptedSource {
    fn next_byte(&mut self, _timeout: Duration) -> Byte {
        match self.bytes.pop_front() {
            Some(byte) => Byte::Value(byte),
            None => Byte::Timeout,
        }
    }
}

fn count(haystack: &[u8], needle: &[u8]) -> usize {
    haystack
        .windows(needle.len())
        .filter(|window| *window == needle)
        .count()
}

fn probed_glyph_count() -> usize {
    PROBE_GLYPHS
        .iter()
        .filter(|glyph| glyph.width() == 1)
        .count()
}

#[test]
fn test_parse_position_reads_cursor_report_column() {
    let mut source = ScriptedSource::new(b"\x1b[12;40R");
    assert!(matches!(parse_position(&mut source), Position::Column(40)));
}

#[test]
fn test_parse_position_reports_missing_reply() {
    let mut source = ScriptedSource::new(b"");
    assert!(matches!(parse_position(&mut source), Position::NoReply));
}

#[test]
fn test_parse_position_rejects_arrow_key_bytes() {
    // 方向键也是转义序列：必须判为错位并停止探测，不能当成回包。
    let mut source = ScriptedSource::new(b"\x1b[A");
    assert!(matches!(parse_position(&mut source), Position::Malformed));
}

#[test]
fn test_parse_position_rejects_plain_key_bytes() {
    let mut source = ScriptedSource::new(b"q");
    assert!(matches!(parse_position(&mut source), Position::Malformed));
}

#[test]
fn test_parse_position_rejects_reply_without_column() {
    let mut source = ScriptedSource::new(b"\x1b[12R");
    assert!(matches!(parse_position(&mut source), Position::Malformed));
}

#[test]
fn test_parse_position_rejects_device_attribute_reply() {
    // 某些终端会在启动阶段回 DA（`ESC[?1;2c`），不能误判成列号。
    let mut source = ScriptedSource::new(b"\x1b[?1;2c");
    assert!(matches!(parse_position(&mut source), Position::Malformed));
}

#[test]
fn test_probe_glyphs_are_single_column_and_degradable() {
    // 探测清单只能包含「本该 1 列」的图标字形：反向分歧会退化成 `?`，
    // 绝不能落到 CJK 正文或 emoji 上。
    for glyph in PROBE_GLYPHS {
        assert_eq!(
            glyph.chars().count(),
            1,
            "{glyph} 必须是单字符（组合序列的列宽不可靠）"
        );
        assert_eq!(glyph.width(), 1, "{glyph} 必须按 1 列预期列宽参与探测");
        assert_ne!(
            fallback_symbol(glyph, 1),
            "?",
            "{glyph} 没有专用降级字形，探测到它变宽也无法修复"
        );
    }
}

#[test]
fn test_measure_marks_glyphs_the_terminal_draws_wide() {
    let widened = probed_glyph_count();
    // 回包列号 3，减去擦行后的 1 列，等于实测 2 列。
    let mut source = ScriptedSource::repeating(b"\x1b[1;3R", widened);
    let mut out = Cursor::new(Vec::new());
    let widths = super::measure_with_source(&mut out, &mut source);
    assert_eq!(widths.len(), widened, "所有被测字形都应记为双列");
    assert!(widths.values().all(|&width| width == 2));
    let written = out.into_inner();
    assert_eq!(count(&written, b"\x1b[6n"), widened, "每个字形一次往返");
    assert!(written.starts_with(b"\x1b7"), "探测前保存光标");
    assert!(written.ends_with(b"\r\x1b[2K\x1b8"), "探测后擦行并还原光标");
}

#[test]
fn test_measure_keeps_map_empty_when_columns_match_unicode_width() {
    let mut source = ScriptedSource::repeating(b"\x1b[1;2R", probed_glyph_count());
    let mut out = Cursor::new(Vec::new());
    let widths = super::measure_with_source(&mut out, &mut source);
    assert!(widths.is_empty(), "实测 1 列时不得产生任何降级");
}

#[test]
fn test_measure_stops_after_two_unanswered_probes() {
    let mut source = ScriptedSource::new(b"");
    let mut out = Cursor::new(Vec::new());
    let widths = super::measure_with_source(&mut out, &mut source);
    assert!(widths.is_empty());
    let written = out.into_inner();
    assert_eq!(
        count(&written, b"\x1b[6n"),
        2,
        "终端不回应 CPR 时应在两次尝试后放弃，不拖住启动"
    );
    assert!(written.ends_with(b"\r\x1b[2K\x1b8"), "放弃时也要还原光标");
}

#[test]
fn test_measure_stops_on_malformed_reply() {
    let mut source = ScriptedSource::new(b"q");
    let mut out = Cursor::new(Vec::new());
    let widths = super::measure_with_source(&mut out, &mut source);
    assert!(widths.is_empty());
    assert_eq!(
        count(&out.into_inner(), b"\x1b[6n"),
        1,
        "错位后不得继续探测"
    );
}

#[test]
fn test_cursor_probe_only_degrades_measured_glyphs() {
    // 探针只报告实测有分歧的字形；其余字形保持原件，输出与裸后端一致。
    let probe = CursorWidthProbe::with_widths([("·".to_owned(), 2)].into_iter().collect());
    let mut backend = WidthSafeBackend::with_probe(TestBackend::new(10, 1), probe);
    let middot = Cell::new("·");
    let check = Cell::new("✓");
    assert!(backend
        .draw([(0, 0, &middot), (2, 0, &check)].into_iter())
        .is_ok());
    assert_eq!(backend.inner.buffer()[(0, 0)].symbol(), ".");
    assert_eq!(backend.inner.buffer()[(2, 0)].symbol(), "✓");
}
