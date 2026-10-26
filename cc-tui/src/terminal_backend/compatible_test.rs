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

#[test]
fn test_fixed_viewport_clear_preserves_rows_outside_owned_area() {
    use ratatui::{layout::Rect, TerminalOptions, Viewport};
    let (probe, _) = make_probe(&[]);
    let mut inner = TestBackend::new(20, 12);
    let cells: Vec<_> = (0..12).map(|row| (0, row, Cell::new("X"))).collect();
    inner
        .draw(cells.iter().map(|(x, y, cell)| (*x, *y, cell)))
        .expect("写入宿主屏幕");
    let mut backend = WidthSafeBackend::with_probe(inner, probe);
    backend.clear_owned_lines_only();
    let mut terminal = Terminal::with_options(
        backend,
        TerminalOptions {
            viewport: Viewport::Fixed(Rect::new(0, 3, 20, 2)),
        },
    )
    .expect("创建固定视口");
    terminal
        .resize(Rect::new(0, 3, 20, 4))
        .expect("扩大应用区域");
    for row in [0, 1, 2, 7, 8, 9, 10, 11] {
        assert_eq!(
            terminal.backend().inner.buffer()[(0, row)].symbol(),
            "X",
            "清理固定视口不能覆盖应用外的宿主内容"
        );
    }
    for row in 3..7 {
        assert_eq!(terminal.backend().inner.buffer()[(0, row)].symbol(), " ");
    }
}

#[derive(Default)]
struct MockProbeState {
    widths: HashMap<String, usize>,
    calls: Vec<String>,
    changed: bool,
    ascii_fallback: bool,
}

struct MockProbe {
    state: Rc<RefCell<MockProbeState>>,
}

impl WidthProbe for MockProbe {
    fn prefer_ascii_fallback(&self) -> bool {
        self.state.borrow().ascii_fallback
    }

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

#[test]
fn test_width_safe_backend_native_console_spinner_keeps_animating() {
    // 缺字仍占一列：必须按宿主策略降级，不能只依靠列宽差异。
    let (probe, state) = make_probe(&[]);
    state.borrow_mut().ascii_fallback = true;
    let backend = WidthSafeBackend::with_probe(TestBackend::new(64, 1), probe);
    let mut terminal = Terminal::new(backend).expect("构造测试终端");
    let frames = [
        "✵", "✶", "✷", "✸", "✹", "✺", "✻", "✼", "❃", "❊", "✼", "✻", "✺", "✸", "✹", "✷",
    ];
    let expected = [
        "|", "/", "-", "\\", "|", "/", "-", "\\", "|", "/", "\\", "-", "/", "\\", "|", "-",
    ];
    for (symbol, expected) in frames.into_iter().zip(expected) {
        assert!(terminal
            .draw(|frame| {
                frame.render_widget(
                    Paragraph::new(format!(
                        "{symbol} Cooking… (43s · ↓ 15k tokens · still thinking)"
                    )),
                    frame.area(),
                );
            })
            .map(|frame| assert_eq!(
                frame.buffer[(0, 0)].symbol(),
                symbol,
                "输出降级不得改变逻辑帧"
            ))
            .is_ok());
        assert_eq!(
            terminal.backend().inner.buffer()[(0, 0)].symbol(),
            expected,
            "降级后动画帧必须不同"
        );
        assert_eq!(
            terminal.backend().inner.buffer()[(2, 0)].symbol(),
            "C",
            "动词不能随动画错位"
        );
    }
    assert!(terminal
        .draw(|frame| frame
            .render_widget(Paragraph::new("✻ Ran for 1m 0s · done 18:50"), frame.area()))
        .is_ok());
    assert_eq!(
        terminal.backend().inner.buffer()[(0, 0)].symbol(),
        "-",
        "完成总结也不能输出缺失字形"
    );
}

#[test]
fn test_width_safe_backend_native_console_degrades_cursor_prefix() {
    // `❯`(U+276F) 缺字形时传统 conhost 仍按 1 列推进，宽度探针量出的就是 1 列，
    // 与 unicode-width 一致，宽度分歧分支永远走不到，只能由缺字表定向降级。
    let (probe, state) = make_probe(&[]);
    state.borrow_mut().ascii_fallback = true;
    let mut backend = WidthSafeBackend::with_probe(TestBackend::new(10, 1), probe);
    let cell = Cell::new("❯");
    assert!(backend.draw([(0, 0, &cell)].into_iter()).is_ok());
    assert_eq!(
        backend.inner.buffer()[(0, 0)].symbol(),
        ">",
        "原生控制台必须把提示符降级为 >"
    );
    assert_eq!(cell.symbol(), "❯", "输出降级不得改变逻辑帧");
}

#[test]
fn test_width_safe_backend_modern_console_preserves_cursor_prefix() {
    // 现代宿主（Windows Terminal 走 DirectWrite 字体回退）能正常显示 ❯，不得降级。
    let (probe, _) = make_probe(&[]);
    let mut backend = WidthSafeBackend::with_probe(TestBackend::new(10, 1), probe);
    let cell = Cell::new("❯");
    assert!(backend.draw([(0, 0, &cell)].into_iter()).is_ok());
    assert_eq!(backend.inner.buffer()[(0, 0)].symbol(), "❯");
}

#[test]
fn test_width_safe_backend_native_console_degrades_permission_pill_symbol() {
    // 状态栏权限 pill 前缀 `⏵⏵`（U+23F5 ×2）同属控制台字体覆盖盲区，
    // 缺字时画成两个方框，同样量不出列宽差异。
    let (probe, state) = make_probe(&[]);
    state.borrow_mut().ascii_fallback = true;
    let mut backend = WidthSafeBackend::with_probe(TestBackend::new(10, 1), probe);
    let cells: Vec<Cell> = ["⏵", "⏵"].iter().map(|s| Cell::new(s)).collect();
    assert!(backend
        .draw(cells.iter().enumerate().map(|(x, cell)| (x as u16, 0, cell)))
        .is_ok());
    assert_eq!(backend.inner.buffer()[(0, 0)].symbol(), ">");
    assert_eq!(
        backend.inner.buffer()[(1, 0)].symbol(),
        ">",
        "两个 ⏵ 都要降级为 >"
    );
}

#[test]
fn test_width_safe_backend_wide_cursor_prefix_falls_back_to_gt() {
    // 列宽分歧路径同样要落到 `>`，不能命中 `_ => '?'` 显示成无语义问号。
    // `fallback_symbol` 收的是逻辑列宽（`unicode-width` 判定的 1 列），不补空格。
    let (probe, _) = make_probe(&[("❯", 2)]);
    let mut backend = WidthSafeBackend::with_probe(TestBackend::new(10, 1), probe);
    let cell = Cell::new("❯");
    assert!(backend.draw([(0, 0, &cell)].into_iter()).is_ok());
    assert_eq!(
        backend.inner.buffer()[(0, 0)].symbol(),
        ">",
        "宽度分歧时也降级为 >"
    );
}

#[test]
fn test_width_safe_backend_modern_console_preserves_spinner_glyphs() {
    let (probe, _) = make_probe(&[]);
    let mut backend = WidthSafeBackend::with_probe(TestBackend::new(20, 1), probe);
    for symbol in ["✵", "✶", "✷", "✸", "✹", "✺", "✻", "✼", "❃", "❊"] {
        let cell = Cell::new(symbol);
        assert!(backend.draw([(0, 0, &cell)].into_iter()).is_ok());
        assert_eq!(
            backend.inner.buffer()[(0, 0)].symbol(),
            symbol,
            "正常宿主保留星形字形"
        );
    }
}

#[test]
fn test_width_safe_backend_wide_spinner_frames_remain_distinct() {
    let (probe, _) = make_probe(&[("✵", 2), ("✶", 2), ("✷", 2), ("✸", 2)]);
    let mut backend = WidthSafeBackend::with_probe(TestBackend::new(20, 1), probe);
    for (symbol, expected) in [("✵", "|"), ("✶", "/"), ("✷", "-"), ("✸", "\\")] {
        let cell = Cell::new(symbol);
        assert!(backend.draw([(0, 0, &cell)].into_iter()).is_ok());
        assert_eq!(
            backend.inner.buffer()[(0, 0)].symbol(),
            expected,
            "列宽降级也应保留动画"
        );
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

/// OSC 8 注入的 symbol 必须原样穿过宽度适配器：
/// 探针不得尝试测量它（含控制字符时返回 None 或走 is_ascii 快路径）。
#[test]
fn test_width_safe_backend_passes_osc8_symbols_untouched() {
    let (probe, calls) = make_probe(&[]);
    let mut backend = WidthSafeBackend::with_probe(TestBackend::new(20, 1), probe);
    // 英文链接文字：装饰后仍是纯 ASCII，走 `is_ascii()` 快路径。
    let ascii_link = "\x1b]8;;https://example.com\x1b\\visit\x1b]8;;\x1b\\";
    // 中文链接文字：含控制字符，探针必须放弃测量而非按字形列宽换掉。
    let cjk_link = "\x1b]8;;https://example.com\x1b\\查看\x1b]8;;\x1b\\";
    let symbols = [ascii_link, cjk_link];

    for (x, symbol) in symbols.iter().enumerate() {
        let mut cell = Cell::default();
        cell.set_symbol(symbol);
        assert!(backend.draw(std::iter::once((x as u16, 0, &cell))).is_ok());
    }

    for (x, symbol) in symbols.iter().enumerate() {
        assert_eq!(
            backend.inner.buffer()[(x as u16, 0)].symbol(),
            *symbol,
            "OSC8 序列不得被宽度适配改写，否则链接失效或整行错位"
        );
    }
    let calls = calls.borrow();
    let probed: Vec<&String> = calls.calls.iter().filter(|c| c.contains('\x1b')).collect();
    assert!(
        probed.is_empty(),
        "转义序列不得进入字形列宽探测，实际探测到：{probed:?}"
    );
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

#[test]
fn test_width_safe_backend_tool_result_prefix_falls_back_to_plus() {
    // #383：`⎿`（U+23BF）是工具结果行前缀（Bash/Read/Edit/Grep/Glob/AskUserQuestion），
    // 此前未收录于降级表，异常列宽终端会落到 `_ => '?'`，显示成无语义的问号。
    let (probe, _) = make_probe(&[("⎿", 2)]);
    let mut backend = WidthSafeBackend::with_probe(TestBackend::new(10, 1), probe);
    let cell = Cell::new("⎿");
    assert!(backend.draw([(0, 0, &cell)].into_iter()).is_ok());
    assert_eq!(
        backend.inner.buffer()[(0, 0)].symbol(),
        "+",
        "⎿ 应降级为 +（与 └ 等制表符一致），而非 ?"
    );
}

/// 回归：OSC 8 序列绝不能写进 `Cell` symbol。
///
/// `Buffer::diff` 用 `symbol.width()` 决定跳过后续多少个 cell，而序列载荷（URL）
/// 是可见字符——一旦进 buffer，链接后面的文字会被整段跳过、不落笔（线上表现：
/// 「URL 只剩首字母，同行内容错位/残留旧画面」）。装饰只能发生在输出层。
#[test]
fn test_hyperlink_decoration_stays_out_of_buffer() {
    use ratatui::{layout::Rect, TerminalOptions, Viewport};
    let (probe, _) = make_probe(&[]);
    let inner = TestBackend::new(4, 1);
    let mut terminal = Terminal::with_options(
        WidthSafeBackend::with_probe(inner, probe),
        TerminalOptions {
            viewport: Viewport::Fixed(Rect::new(0, 0, 4, 1)),
        },
    )
    .expect("创建固定视口");
    let mut buffer_symbol = String::new();
    terminal
        .draw(|f| {
            crate::terminal_backend::hyperlink::begin_frame(true);
            crate::terminal_backend::hyperlink::register(0, 0, "https://a.example");
            f.render_widget(Paragraph::new("ab"), f.area());
            buffer_symbol = f.buffer_mut()[(0, 0)].symbol().to_string();
        })
        .expect("绘制");
    let drawn = terminal.backend().inner.buffer();
    assert!(
        drawn[(0, 0)]
            .symbol()
            .starts_with("\x1b]8;;https://a.example\x07"),
        "输出侧必须包裹 OSC 8: {:?}",
        drawn[(0, 0)].symbol()
    );
    assert_eq!(
        drawn[(0, 0)].style().fg,
        Some(ratatui::style::Color::LightBlue),
        "链接前景色取 12 号槽（实测与 CC 观感一致）"
    );
    assert_eq!(
        drawn[(1, 0)].symbol(),
        "b",
        "链接后面的 cell 必须照常落笔，不能被 diff 跳过"
    );
    assert_eq!(
        buffer_symbol, "a",
        "buffer 里必须保持纯文本，序列不得参与宽度判定"
    );
}
