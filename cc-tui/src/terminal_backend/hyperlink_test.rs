use super::*;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};

fn make_buffer(x: u16, y: u16, symbol: &str, style: Style) -> Buffer {
    let mut buffer = Buffer::empty(Rect::new(0, 0, 10, 4));
    buffer[(x, y)].set_symbol(symbol).set_style(style);
    buffer
}

fn make_prepared_frame(previous: &Frame, buffer: &mut Buffer) -> Frame {
    previous.prepare(buffer);
    take_prepared_frame(previous).expect("应生成当前帧").frame
}

#[test]
fn test_hyperlink_registry_wraps_registered_cells_only() {
    let baseline = Frame::default();
    let mut buffer = make_buffer(3, 1, "h", Style::default());
    begin_frame(false);
    register(3, 1, "https://a.example/x", true, (0, 0, 1));
    let frame = make_prepared_frame(&baseline, &mut buffer);
    assert_eq!(
        frame.decorate(3, 1, &buffer[(3, 1)]),
        None,
        "未启用时不得注入 OSC 8"
    );
    begin_frame(true);
    register(3, 1, "https://a.example/x", true, (0, 0, 1));
    let frame = make_prepared_frame(&baseline, &mut buffer);
    assert_eq!(buffer[(3, 1)].symbol(), "h", "差分 buffer 必须保持纯文本");
    assert_eq!(
        buffer[(3, 1)].fg,
        Color::LightBlue,
        "链接颜色必须在差分前生效"
    );
    let decorated = frame
        .decorate(3, 1, &buffer[(3, 1)])
        .expect("登记过的格子应被包裹");
    assert_eq!(
        decorated.symbol(),
        format!(
            "\x1b]8;id=cc-code-{}-{}-0-0-1;https://a.example/x\x07h\x1b]8;;\x07",
            std::process::id(),
            frame.namespace
        ),
        "同一命中区必须使用显式锚点，并在字符后关闭链接"
    );
    assert!(
        !decorated.modifier.contains(Modifier::UNDERLINED),
        "下划线交由终端悬停状态处理"
    );
    assert_eq!(
        frame.decorate(4, 1, &Cell::new("t")),
        None,
        "同一行其他格子不包裹"
    );
    assert_eq!(frame.decorate(3, 2, &Cell::new("x")), None, "其他行不包裹");
    assert_eq!(
        frame.decorate(3, 1, &Cell::new(" ")),
        None,
        "空白格子不得成为热点"
    );
    begin_frame(true);
    let frame = make_prepared_frame(&frame, &mut buffer);
    assert_eq!(
        frame.decorate(3, 1, &buffer[(3, 1)]),
        None,
        "每帧必须重新登记链接"
    );
}

#[test]
fn test_hyperlink_url_control_chars_are_stripped() {
    let baseline = Frame::default();
    let mut buffer = make_buffer(0, 0, "x", Style::default());
    begin_frame(true);
    register(
        0,
        0,
        "https://a.example/\u{7}evil\u{1b}\\\n",
        true,
        (0, 0, 1),
    );
    let frame = make_prepared_frame(&baseline, &mut buffer);
    let decorated = frame
        .decorate(0, 0, &buffer[(0, 0)])
        .expect("登记过的格子应被包裹");
    assert!(
        decorated.symbol().contains(";https://a.example/evil\\\x07"),
        "URI 内控制字符必须被剥离"
    );
    assert_eq!(
        decorated.symbol().matches('\x07').count(),
        2,
        "URI 不得额外注入 OSC 终结符"
    );
    assert_eq!(
        decorated.symbol().matches('\x1b').count(),
        2,
        "URI 不得额外注入终端命令"
    );
}

#[test]
fn test_hyperlink_preserves_cell_color_when_recolor_disabled() {
    let baseline = Frame::default();
    let own_color = Color::Rgb(255, 255, 255);
    let mut buffer = make_buffer(
        2,
        0,
        "I",
        Style::default().fg(own_color).bg(Color::DarkGray),
    );
    begin_frame(true);
    register(2, 0, "file:///tmp/a.png", false, (0, 2, 3));
    let frame = make_prepared_frame(&baseline, &mut buffer);
    let decorated = frame
        .decorate(2, 0, &buffer[(2, 0)])
        .expect("登记过的格子应被包裹");
    assert!(
        decorated
            .symbol()
            .contains(";file:///tmp/a.png\x07I\x1b]8;;\x07"),
        "保留附件文件链接"
    );
    assert_eq!(buffer[(2, 0)].fg, own_color, "差分前保留附件自身颜色");
    assert_eq!(decorated.fg, own_color, "输出层不得再次改色");
    assert_eq!(decorated.bg, Color::DarkGray, "选区背景必须保留");
}
