use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use ratatui::backend::{Backend, TestBackend};
use ratatui::buffer::Cell;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::Paragraph;
use ratatui::Terminal;
use unicode_width::UnicodeWidthStr;

use super::{WidthProbe, WidthSafeBackend};

#[derive(Default)]
struct MockProbeState {
    widths: HashMap<String, usize>,
    calls: Vec<String>,
    changed: bool,
}

struct MockProbe {
    state: Rc<RefCell<MockProbeState>>,
}

impl WidthProbe for MockProbe {
    fn width(&mut self, symbol: &str) -> Option<usize> {
        let mut state = self.state.borrow_mut();
        state.calls.push(symbol.to_owned());
        Some(
            state
                .widths
                .get(symbol)
                .copied()
                .unwrap_or_else(|| symbol.width()),
        )
    }

    fn refresh(&mut self) -> bool {
        std::mem::take(&mut self.state.borrow_mut().changed)
    }
}

fn make_probe(widths: &[(&str, usize)]) -> (MockProbe, Rc<RefCell<MockProbeState>>) {
    let state = Rc::new(RefCell::new(MockProbeState {
        widths: widths
            .iter()
            .map(|(symbol, width)| ((*symbol).to_owned(), *width))
            .collect(),
        ..MockProbeState::default()
    }));
    (
        MockProbe {
            state: Rc::clone(&state),
        },
        state,
    )
}

#[test]
fn test_width_safe_backend_narrow_console_keeps_symbols() {
    // 实测与逻辑列宽相同的终端保持完整 Unicode 输出。
    let (probe, _) = make_probe(&[]);
    let mut backend = WidthSafeBackend::with_probe(TestBackend::new(20, 1), probe);
    let cells = [
        Cell::new("●"),
        Cell::new("∴"),
        Cell::new("“"),
        Cell::new("”"),
        Cell::new("—"),
        Cell::new("中"),
    ];
    assert!(backend
        .draw(
            cells
                .iter()
                .enumerate()
                .map(|(x, cell)| (x as u16, 0, cell))
        )
        .is_ok());
    for (x, cell) in cells.iter().enumerate() {
        assert_eq!(
            backend.inner.buffer()[(x as u16, 0)].symbol(),
            cell.symbol(),
            "正常终端不应替换符号"
        );
    }
}

#[test]
fn test_width_safe_backend_wide_symbols_preserve_source_and_style() {
    // 兼容替换仅发生在输出副本，正文原文与 RGB、修饰符保持不变。
    let (probe, _) = make_probe(&[("●", 2), ("∴", 2), ("“", 2), ("”", 2), ("—", 2)]);
    let mut backend = WidthSafeBackend::with_probe(TestBackend::new(20, 1), probe);
    let originals = ["●", "∴", "“", "”", "—"];
    let style = Style::default()
        .fg(Color::Rgb(12, 34, 56))
        .bg(Color::Rgb(78, 90, 123))
        .add_modifier(Modifier::BOLD | Modifier::ITALIC | Modifier::UNDERLINED);
    let cells: Vec<Cell> = originals
        .iter()
        .map(|symbol| {
            let mut cell = Cell::default();
            cell.set_symbol(symbol).set_style(style);
            cell
        })
        .collect();
    assert!(backend
        .draw(
            cells
                .iter()
                .enumerate()
                .map(|(x, cell)| (x as u16, 0, cell))
        )
        .is_ok());
    for (x, expected) in ["*", ".", "\"", "\"", "-"].iter().enumerate() {
        let output = &backend.inner.buffer()[(x as u16, 0)];
        assert_eq!(output.symbol(), *expected, "歧义符号应使用单列替代文本");
        assert_eq!(output.symbol().width(), cells[x].symbol().width());
        assert_eq!(output.fg, Color::Rgb(12, 34, 56));
        assert_eq!(output.bg, Color::Rgb(78, 90, 123));
        assert_eq!(output.modifier, cells[x].modifier);
        assert_eq!(cells[x].symbol(), originals[x], "不得修改原始 Cell");
    }
}

#[test]
fn test_width_safe_backend_unknown_symbols_keep_logical_width() {
    // 未列出的符号也要保护，原本双列的符号需要补空格。
    let (probe, _) = make_probe(&[("Ω", 2), ("😀", 1)]);
    let mut backend = WidthSafeBackend::with_probe(TestBackend::new(10, 1), probe);
    let single = Cell::new("Ω");
    let double = Cell::new("😀");
    assert!(backend
        .draw([(0, 0, &single), (2, 0, &double)].into_iter())
        .is_ok());
    assert_eq!(backend.inner.buffer()[(0, 0)].symbol(), "?");
    assert_eq!(backend.inner.buffer()[(2, 0)].symbol(), "? ");
    assert_eq!(
        backend.inner.buffer()[(2, 0)].symbol().width(),
        double.symbol().width()
    );
    assert_eq!(double.symbol(), "😀", "双列回退不得改变消息原文");
}

#[test]
fn test_width_safe_backend_shorter_frame_clears_previous_text() {
    // 通过真实 Terminal 差分路径验证长行缩短，而非只调用静态替换函数。
    let (probe, _) = make_probe(&[("∴", 2), ("●", 2), ("“", 2), ("”", 2), ("—", 2)]);
    let backend = WidthSafeBackend::with_probe(TestBackend::new(64, 2), probe);
    let mut terminal = Terminal::new(backend).expect("构造测试终端");
    assert!(terminal
        .draw(|frame| {
            frame.render_widget(
                Paragraph::new("∴ Thought for 57 chars (ctrl+o to expand)\n● “回答”—正文"),
                frame.area(),
            );
        })
        .is_ok());
    assert_eq!(terminal.backend().inner.buffer()[(0, 0)].symbol(), ".");
    assert!(terminal
        .draw(|frame| frame.render_widget(Paragraph::new("● 2"), frame.area()))
        .is_ok());
    let buffer = terminal.backend().inner.buffer();
    assert_eq!(buffer[(0, 0)].symbol(), "*");
    assert_eq!(buffer[(2, 0)].symbol(), "2");
    for y in 0..2 {
        for x in if y == 0 { 3 } else { 0 }..64 {
            assert_eq!(buffer[(x, y)].symbol(), " ", "缩短后的旧正文应完整擦除");
        }
    }
}

#[test]
fn test_width_safe_backend_caches_complete_symbol_and_skips_ascii() {
    // 缓存键使用完整组合符号，重复绘制无需反复调用控制台探针。
    let (probe, state) = make_probe(&[]);
    let mut backend = WidthSafeBackend::with_probe(TestBackend::new(10, 1), probe);
    let plain = Cell::new("é");
    let combined = Cell::new("é\u{301}");
    let ascii = Cell::new("A");
    for _ in 0..2 {
        assert!(backend
            .draw([(0, 0, &plain), (1, 0, &combined), (2, 0, &ascii)].into_iter())
            .is_ok());
    }
    assert_eq!(state.borrow().calls, ["é", "é\u{301}"]);
}

#[test]
fn test_width_safe_backend_refresh_invalidates_changed_widths() {
    // 字体能力变化后重测；未变化的 refresh 应保留缓存。
    let (probe, state) = make_probe(&[("●", 2)]);
    let mut backend = WidthSafeBackend::with_probe(TestBackend::new(10, 1), probe);
    let cell = Cell::new("●");
    assert!(backend.draw([(0, 0, &cell)].into_iter()).is_ok());
    assert_eq!(backend.inner.buffer()[(0, 0)].symbol(), "*");
    assert!(!backend.refresh_widths(), "能力未变化时无需强制重绘");
    assert!(backend.draw([(0, 0, &cell)].into_iter()).is_ok());
    assert_eq!(state.borrow().calls.len(), 1);
    {
        let mut state = state.borrow_mut();
        state.widths.insert("●".to_owned(), 1);
        state.changed = true;
    }
    assert!(backend.refresh_widths(), "能力变化应通知调用方强制重绘");
    assert!(backend.draw([(0, 0, &cell)].into_iter()).is_ok());
    assert_eq!(backend.inner.buffer()[(0, 0)].symbol(), "●");
    assert_eq!(state.borrow().calls.len(), 2, "能力变化必须清除旧宽度缓存");
}

#[test]
fn test_width_safe_backend_box_drawing_and_status_glyphs_fallback() {
    // 制表符、勾叉、进度条在异常列宽终端中降级为 ASCII，不应回退为问号。
    let (probe, _) = make_probe(&[
        ("┌", 2),
        ("┼", 2),
        ("┘", 2),
        ("✓", 2),
        ("█", 2),
        ("░", 2),
        ("⏱", 2),
    ]);
    let mut backend = WidthSafeBackend::with_probe(TestBackend::new(10, 1), probe);
    let symbols = ["┌", "┼", "┘", "✓", "█", "░", "⏱"];
    let cells: Vec<Cell> = symbols.iter().map(|s| Cell::new(s)).collect();
    assert!(backend
        .draw(
            cells
                .iter()
                .enumerate()
                .map(|(x, cell)| (x as u16, 0, cell))
        )
        .is_ok());
    let expected = ["+", "+", "+", "v", "#", "-", "t"];
    for (x, exp) in expected.iter().enumerate() {
        assert_eq!(
            backend.inner.buffer()[(x as u16, 0)].symbol(),
            *exp,
            "符号 {} 应降级为 {}",
            symbols[x],
            exp
        );
    }
}

