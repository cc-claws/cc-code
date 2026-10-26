use super::*;
use ratatui::buffer::Cell;
use ratatui::style::{Color, Style};

fn cell(symbol: &str, style: Style) -> Cell {
    let mut cell = Cell::default();
    cell.set_symbol(symbol);
    cell.set_style(style);
    cell
}

/// 全局登记表是进程级状态，测试必须串行覆盖各分支：拆成多个 `#[test]`
/// 会并行跑同一个表（清空/登记互相踩），这里一个用例顺序走完。
#[test]
fn test_hyperlink_registry_wraps_registered_cells_only() {
    // 未启用：登记被忽略，也不包裹。
    begin_frame(false);
    register(3, 1, "https://a.example/x", true);
    assert_eq!(
        decorate(3, 1, &cell("h", Style::default())),
        None,
        "未启用时不得注入 OSC 8"
    );

    // 启用后：登记的 cell 被包裹并染成链接色，未登记/空白 cell 不受影响。
    begin_frame(true);
    register(3, 1, "https://a.example/x", true);
    let decorated = decorate(3, 1, &cell("h", Style::default())).expect("登记过的 cell 应被包裹");
    assert_eq!(
        decorated.symbol(),
        "\x1b]8;;https://a.example/x\x07h\x1b]8;;\x07",
        "包裹 = open(BEL 收尾) + 原 symbol + close，与 CC 的 OSC8_END 一致"
    );
    assert_eq!(
        decorated.style().fg,
        Some(Color::LightBlue),
        "链接前景色取 12 号槽（实测与 CC 观感一致）"
    );
    assert!(
        !decorated
            .style()
            .add_modifier
            .contains(ratatui::style::Modifier::UNDERLINED),
        "CC 不加人工下划线，悬停样式交给终端"
    );
    assert_eq!(
        decorate(4, 1, &cell("t", Style::default())),
        None,
        "同一行的其它 cell 不包裹"
    );
    assert_eq!(
        decorate(3, 2, &cell("x", Style::default())),
        None,
        "其它行不包裹"
    );
    assert_eq!(
        decorate(3, 1, &cell(" ", Style::default())),
        None,
        "空白 cell 不包裹，避免把空格变成热点"
    );

    // 新的一帧必须清空上一帧登记，否则滚动后旧坐标会挂上错误的链接。
    begin_frame(true);
    assert_eq!(
        decorate(3, 1, &cell("h", Style::default())),
        None,
        "跨帧不得残留登记"
    );
}

/// URL 里的控制字符必须剥离，否则能提前终止 OSC 8 序列注入任意终端指令。
#[test]
fn test_hyperlink_url_control_chars_are_stripped() {
    begin_frame(true);
    register(0, 0, "https://a.example/\u{7}evil", true);
    let decorated = decorate(0, 0, &cell("x", Style::default())).expect("应包裹");
    assert!(
        !decorated.symbol().contains("\u{7}evil\u{7}"),
        "URL 内的 BEL 不得冒充序列终结符: {:?}",
        decorated.symbol()
    );
    assert!(
        decorated
            .symbol()
            .starts_with("\x1b]8;;https://a.example/evil\x07"),
        "控制字符剥离后 URL 应保持完整: {:?}",
        decorated.symbol()
    );
}

/// `recolor = false` 的链接只包 OSC 8，必须保留渲染层给的前景色。
///
/// 回归护栏：附件引用行用正文前景色（白），输出层若照旧染成链接蓝，
/// 用户看到的就是蓝色而不是设计要求的白色。
#[test]
fn test_hyperlink_preserves_cell_color_when_recolor_disabled() {
    begin_frame(true);
    register(2, 0, "file:///tmp/a.png", false);
    let own_color = Style::default().fg(Color::Rgb(255, 255, 255));
    let decorated = decorate(2, 0, &cell("I", own_color)).expect("登记过的 cell 仍应包 OSC 8");
    assert_eq!(
        decorated.symbol(),
        "\x1b]8;;file:///tmp/a.png\x07I\x1b]8;;\x07",
        "不改色也必须保留可点击的 OSC 8 包裹"
    );
    assert_eq!(
        decorated.style().fg,
        Some(Color::Rgb(255, 255, 255)),
        "recolor=false 时前景色必须原样保留"
    );
}
