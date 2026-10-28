use std::io;
use std::ops::Range;

use ratatui::backend::{Backend, ClearType, TestBackend, WindowSize};
use ratatui::buffer::{Buffer, Cell};
use ratatui::layout::{Position, Rect, Size};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::Paragraph;
use ratatui::{Terminal, TerminalOptions, Viewport};
use unicode_width::UnicodeWidthStr;

use super::{WidthProbe, WidthSafeBackend};
use crate::terminal_backend::hyperlink;

struct MockProbe;

impl WidthProbe for MockProbe {
    fn width(&mut self, symbol: &str) -> Option<usize> {
        Some(symbol.width())
    }

    fn refresh(&mut self) -> bool {
        false
    }
}

struct MockBackend {
    screen: TestBackend,
    draws: Vec<Vec<(u16, u16, Cell)>>,
    fail_draw: bool,
    fail_flush: bool,
}

impl Backend for MockBackend {
    type Error = io::Error;

    fn draw<'a, I>(&mut self, content: I) -> io::Result<()>
    where
        I: Iterator<Item = (u16, u16, &'a Cell)>,
    {
        if std::mem::take(&mut self.fail_draw) {
            return Err(io::Error::other("模拟终端绘制失败"));
        }
        let cells: Vec<_> = content.map(|(x, y, cell)| (x, y, cell.clone())).collect();
        self.screen
            .draw(cells.iter().map(|(x, y, cell)| (*x, *y, cell)))
            .map_err(|never| -> io::Error { match never {} })?;
        self.draws.push(cells);
        Ok(())
    }

    fn append_lines(&mut self, count: u16) -> io::Result<()> {
        self.screen
            .append_lines(count)
            .map_err(|never| match never {})
    }

    fn hide_cursor(&mut self) -> io::Result<()> {
        self.screen.hide_cursor().map_err(|never| match never {})
    }

    fn show_cursor(&mut self) -> io::Result<()> {
        self.screen.show_cursor().map_err(|never| match never {})
    }

    fn get_cursor_position(&mut self) -> io::Result<Position> {
        self.screen
            .get_cursor_position()
            .map_err(|never| match never {})
    }

    fn set_cursor_position<P: Into<Position>>(&mut self, position: P) -> io::Result<()> {
        self.screen
            .set_cursor_position(position)
            .map_err(|never| match never {})
    }

    fn clear(&mut self) -> io::Result<()> {
        self.screen.clear().map_err(|never| match never {})
    }

    fn clear_region(&mut self, clear_type: ClearType) -> io::Result<()> {
        self.screen
            .clear_region(clear_type)
            .map_err(|never| match never {})
    }

    fn size(&self) -> io::Result<Size> {
        self.screen.size().map_err(|never| match never {})
    }

    fn window_size(&mut self) -> io::Result<WindowSize> {
        self.screen.window_size().map_err(|never| match never {})
    }

    fn flush(&mut self) -> io::Result<()> {
        if std::mem::take(&mut self.fail_flush) {
            return Err(io::Error::other("模拟终端刷新失败"));
        }
        self.screen.flush().map_err(|never| match never {})
    }
}

type MockTerminal = Terminal<WidthSafeBackend<MockBackend, MockProbe>>;

struct MockLink<'a> {
    columns: Range<u16>,
    target: &'a str,
    recolor: bool,
    id: (usize, usize, usize),
}

fn make_terminal(area: Rect) -> MockTerminal {
    let inner = MockBackend {
        screen: TestBackend::new(80, 12),
        draws: Vec::new(),
        fail_draw: false,
        fail_flush: false,
    };
    let mut backend = WidthSafeBackend::with_probe(inner, MockProbe);
    backend.clear_owned_lines_only();
    Terminal::with_options(
        backend,
        TerminalOptions {
            viewport: Viewport::Fixed(area),
        },
    )
    .expect("创建固定视口测试终端")
}

fn make_link(target: &str, columns: Range<u16>, recolor: bool) -> MockLink<'_> {
    let id = (0, usize::from(columns.start), usize::from(columns.end));
    MockLink {
        columns,
        target,
        recolor,
        id,
    }
}

fn make_draw(
    terminal: &mut MockTerminal,
    text: &str,
    style: Style,
    links: &[MockLink<'_>],
    enabled: bool,
) -> io::Result<()> {
    let links_frame = terminal.backend().hyperlink_frame();
    terminal
        .draw(|frame| {
            hyperlink::begin_frame(enabled);
            let area = frame.area();
            frame.render_widget(Paragraph::new(text).style(style), area);
            for link in links {
                for column in link.columns.clone() {
                    hyperlink::register(
                        area.x + column,
                        area.y,
                        link.target,
                        link.recolor,
                        link.id,
                    );
                }
            }
            links_frame.prepare(frame.buffer_mut());
        })
        .map(|_| ())
}

fn make_output_buffer(terminal: &MockTerminal) -> &Buffer {
    terminal.backend().inner.screen.buffer()
}

fn make_link_parts(symbol: &str) -> Option<(&str, &str)> {
    let (parameters, rest) = symbol.strip_prefix("\x1b]8;")?.split_once(';')?;
    let (target, _) = rest.split_once('\x07')?;
    Some((parameters, target))
}

#[test]
fn test_hyperlink_frame_links_unchanged_plain_text() {
    let mut terminal = make_terminal(Rect::new(0, 0, 60, 2));
    let text = "documentation";
    let target = "https://a.example/docs";
    let style = Style::default().fg(Color::Gray);
    make_draw(&mut terminal, text, style, &[], true).expect("绘制普通文字");
    make_draw(
        &mut terminal,
        text,
        style,
        &[make_link(target, 0..text.len() as u16, true)],
        true,
    )
    .expect("同文本切换为链接");
    for x in 0..text.len() as u16 {
        let cell = &make_output_buffer(&terminal)[(x, 0)];
        assert_eq!(cell.fg, Color::LightBlue, "每个链接字符都应变为蓝色");
        assert_eq!(
            make_link_parts(cell.symbol()).map(|(_, url)| url),
            Some(target),
            "相同字符也必须获得链接属性"
        );
    }
}

#[test]
fn test_hyperlink_frame_recolors_matching_substring() {
    let mut terminal = make_terminal(Rect::new(0, 0, 60, 2));
    let text = "https://github.com/cc-claws";
    let previous: String = text
        .chars()
        .enumerate()
        .map(|(index, character)| {
            if (14..18).contains(&index) {
                character
            } else {
                '_'
            }
        })
        .collect();
    let style = Style::default().fg(Color::Gray);
    make_draw(&mut terminal, &previous, style, &[], true).expect("绘制包含相同后缀的旧行");
    make_draw(
        &mut terminal,
        text,
        style,
        &[make_link(text, 0..text.len() as u16, true)],
        true,
    )
    .expect("绘制完整链接");
    for x in 0..text.len() as u16 {
        assert_eq!(
            make_output_buffer(&terminal)[(x, 0)].fg,
            Color::LightBlue,
            "包括未变化的 .com 在内，整段链接都应正确着色"
        );
    }
}

#[test]
fn test_hyperlink_frame_updates_target_for_unchanged_prefix() {
    let mut terminal = make_terminal(Rect::new(0, 0, 60, 2));
    let previous = "https://a.example/1";
    let current = "https://a.example/2";
    let style = Style::default().fg(Color::Gray);
    make_draw(
        &mut terminal,
        previous,
        style,
        &[make_link(previous, 0..previous.len() as u16, true)],
        true,
    )
    .expect("绘制旧目标");
    make_draw(
        &mut terminal,
        current,
        style,
        &[make_link(current, 0..current.len() as u16, true)],
        true,
    )
    .expect("更新链接目标");
    for x in 0..current.len() as u16 {
        assert_eq!(
            make_link_parts(make_output_buffer(&terminal)[(x, 0)].symbol()).map(|(_, url)| url),
            Some(current),
            "相同前缀不能继续指向旧地址"
        );
    }
}

#[test]
fn test_hyperlink_frame_removes_link_when_plain_style_is_already_blue() {
    let mut terminal = make_terminal(Rect::new(0, 0, 60, 2));
    let text = "open";
    let style = Style::default().fg(Color::LightBlue);
    make_draw(
        &mut terminal,
        text,
        style,
        &[make_link("https://a.example", 0..4, false)],
        true,
    )
    .expect("绘制保留原色的链接");
    make_draw(&mut terminal, text, style, &[], true).expect("移除链接属性");
    for (x, character) in text.chars().enumerate() {
        let cell = &make_output_buffer(&terminal)[(x as u16, 0)];
        assert_eq!(
            cell.symbol(),
            character.to_string(),
            "同色同字也必须移除旧链接"
        );
        assert_eq!(cell.fg, Color::LightBlue, "普通文字的原色应保留");
    }
}

#[test]
fn test_hyperlink_frame_preserves_own_color_and_selection_background() {
    let mut terminal = make_terminal(Rect::new(0, 0, 60, 2));
    let style = Style::default()
        .fg(Color::Rgb(210, 211, 212))
        .bg(Color::Rgb(35, 55, 75))
        .add_modifier(Modifier::BOLD);
    make_draw(&mut terminal, "image", style, &[], true).expect("绘制选中文字");
    make_draw(
        &mut terminal,
        "image",
        style,
        &[make_link("file:///tmp/image.png", 0..5, false)],
        true,
    )
    .expect("添加附件链接");
    for x in 0..5 {
        let cell = &make_output_buffer(&terminal)[(x, 0)];
        assert_eq!(cell.fg, Color::Rgb(210, 211, 212), "附件链接保留自身前景色");
        assert_eq!(cell.bg, Color::Rgb(35, 55, 75), "链接不得覆盖选区背景");
        assert!(
            cell.modifier.contains(Modifier::BOLD),
            "链接不得丢失原有样式"
        );
        assert!(
            !cell.modifier.contains(Modifier::UNDERLINED),
            "链接不得添加人工下划线"
        );
    }
}

#[test]
fn test_hyperlink_frame_disables_and_reenables_unchanged_links() {
    let mut terminal = make_terminal(Rect::new(0, 0, 60, 2));
    let text = "open";
    let target = "https://a.example";
    let style = Style::default().fg(Color::LightBlue);
    let links = [make_link(target, 0..4, false)];
    make_draw(&mut terminal, text, style, &links, true).expect("绘制初始链接");
    make_draw(&mut terminal, text, style, &links, false).expect("禁用链接");
    for x in 0..4 {
        assert!(
            make_link_parts(make_output_buffer(&terminal)[(x, 0)].symbol()).is_none(),
            "关闭后相同文字也不能残留链接"
        );
    }
    make_draw(&mut terminal, text, style, &links, true).expect("重新启用链接");
    for x in 0..4 {
        assert_eq!(
            make_link_parts(make_output_buffer(&terminal)[(x, 0)].symbol()).map(|(_, url)| url),
            Some(target),
            "重新启用后所有字符都应恢复链接"
        );
    }
}

#[test]
fn test_hyperlink_frame_never_repaints_wide_character_continuation() {
    let mut terminal = make_terminal(Rect::new(0, 0, 60, 2));
    let style = Style::default().fg(Color::Gray);
    make_draw(
        &mut terminal,
        "ab",
        style,
        &[make_link("https://a.example", 1..2, true)],
        true,
    )
    .expect("绘制即将被宽字覆盖的链接");
    make_draw(&mut terminal, "中", style, &[], true).expect("用宽字覆盖旧链接");
    let drawn = terminal.backend().inner.draws.last().expect("获取宽字输出");
    assert!(drawn
        .iter()
        .any(|(x, y, cell)| *x == 0 && *y == 0 && cell.symbol() == "中"));
    assert!(
        drawn.iter().all(|(x, y, _)| *x != 1 || *y != 0),
        "移除旧链接不能补画续格空格并擦掉宽字右半"
    );
    make_draw(
        &mut terminal,
        "中",
        style,
        &[make_link("https://a.example", 0..2, true)],
        true,
    )
    .expect("为宽字添加链接");
    let drawn = terminal
        .backend()
        .inner
        .draws
        .last()
        .expect("获取宽字链接输出");
    assert!(
        drawn.iter().all(|(x, y, _)| *x != 1 || *y != 0),
        "宽字链接只允许输出字形首格"
    );
    assert_eq!(
        make_link_parts(make_output_buffer(&terminal)[(0, 0)].symbol()).map(|(_, url)| url),
        Some("https://a.example"),
        "宽字首格应获得链接"
    );
}

#[test]
fn test_hyperlink_frame_handles_offset_viewport_clear_and_resize() {
    let mut terminal = make_terminal(Rect::new(5, 3, 40, 2));
    let outside = Cell::new("X");
    terminal
        .backend_mut()
        .inner
        .screen
        .draw([(0, 0, &outside), (70, 10, &outside)].into_iter())
        .expect("写入应用区域以外的宿主内容");
    let style = Style::default().fg(Color::Gray);
    let first = "https://a.example/1";
    let second = "https://a.example/2";
    make_draw(
        &mut terminal,
        "open",
        style,
        &[make_link(first, 0..4, true)],
        true,
    )
    .expect("绘制偏移视口链接");
    make_draw(
        &mut terminal,
        "open",
        style,
        &[make_link(second, 0..4, true)],
        true,
    )
    .expect("更新偏移视口链接");
    for x in 5..9 {
        assert_eq!(
            make_link_parts(make_output_buffer(&terminal)[(x, 3)].symbol()).map(|(_, url)| url),
            Some(second),
            "补画必须使用真实屏幕坐标"
        );
    }
    terminal.clear().expect("清理应用视口");
    terminal
        .resize(Rect::new(8, 5, 38, 2))
        .expect("移动并调整视口尺寸");
    make_draw(
        &mut terminal,
        "open",
        style,
        &[make_link(second, 0..4, true)],
        true,
    )
    .expect("清理和变更尺寸后重新绘制");
    for x in 8..12 {
        assert_eq!(
            make_link_parts(make_output_buffer(&terminal)[(x, 5)].symbol()).map(|(_, url)| url),
            Some(second),
            "清屏后不能因链接元数据相同而漏画"
        );
    }
    for position in [(0, 0), (70, 10)] {
        assert_eq!(
            make_output_buffer(&terminal)[position].symbol(),
            "X",
            "链接补画不能覆盖应用区域以外的内容"
        );
    }
}

#[test]
fn test_hyperlink_frame_isolates_backend_history_and_link_ids() {
    let mut first = make_terminal(Rect::new(0, 0, 60, 2));
    let mut second = make_terminal(Rect::new(0, 0, 60, 2));
    let target = "https://a.example";
    let style = Style::default().fg(Color::LightBlue);
    let links = [make_link(target, 0..4, false)];
    make_draw(&mut first, "open", style, &links, true).expect("绘制第一个后端链接");
    let first_id = make_link_parts(make_output_buffer(&first)[(0, 0)].symbol())
        .expect("第一个后端应有链接")
        .0
        .to_owned();
    make_draw(&mut second, "open", style, &[], true).expect("绘制第二个后端普通文字");
    make_draw(&mut first, "open", style, &[], true).expect("清除第一个后端链接");
    for x in 0..4 {
        assert!(
            make_link_parts(make_output_buffer(&first)[(x, 0)].symbol()).is_none(),
            "另一个后端的空登记不能覆盖本后端的前帧基线"
        );
    }
    make_draw(&mut second, "open", style, &links, true).expect("为第二个后端添加链接");
    let second_id = make_link_parts(make_output_buffer(&second)[(0, 0)].symbol())
        .expect("第二个后端应有链接")
        .0;
    assert_ne!(first_id, second_id, "不同后端的锚点身份不能互相串联");
}

#[test]
fn test_hyperlink_frame_preserves_distinct_adjacent_anchor_ids() {
    let mut terminal = make_terminal(Rect::new(0, 0, 60, 2));
    let target = "https://a.example";
    let style = Style::default().fg(Color::Gray);
    let mut links = [make_link(target, 0..2, true), make_link(target, 2..4, true)];
    make_draw(&mut terminal, "abcd", style, &links, true).expect("绘制同地址的相邻独立链接");
    let initial: Vec<_> = (0..4)
        .map(|x| {
            make_link_parts(make_output_buffer(&terminal)[(x, 0)].symbol())
                .expect("每个字符应有链接身份")
                .0
                .to_owned()
        })
        .collect();
    assert!(
        initial.iter().all(|id| id.starts_with("id=")),
        "必须输出显式锚点身份"
    );
    assert_eq!(initial[0], initial[1], "同一锚点的字符应共用身份");
    assert_eq!(initial[2], initial[3], "同一锚点的字符应共用身份");
    assert_ne!(initial[0], initial[2], "同地址的独立锚点不能合并");
    make_draw(&mut terminal, "Abcd", style, &links, true).expect("局部更新链接文字");
    for x in 0..4 {
        assert_eq!(
            make_link_parts(make_output_buffer(&terminal)[(x, 0)].symbol())
                .expect("局部重画后仍应有链接身份")
                .0,
            initial[x as usize],
            "局部重画不能改变同一锚点的身份"
        );
    }
    links[0].id = (1, 0, 2);
    make_draw(&mut terminal, "Abcd", style, &links, true).expect("仅替换第一个锚点的身份");
    let drawn = terminal
        .backend()
        .inner
        .draws
        .last()
        .expect("获取身份变化后的输出");
    assert_eq!(drawn.len(), 2, "只应补画身份发生变化的锚点");
    assert_ne!(
        make_link_parts(make_output_buffer(&terminal)[(0, 0)].symbol())
            .expect("替换后仍应有锚点")
            .0,
        initial[0],
        "相同文字和地址也必须更新锚点身份"
    );
    assert_eq!(
        make_link_parts(make_output_buffer(&terminal)[(2, 0)].symbol())
            .expect("第二个锚点应保持不变")
            .0,
        initial[2],
        "无关锚点身份不能改变"
    );
}

#[test]
fn test_hyperlink_frame_does_not_repaint_unchanged_links() {
    let mut terminal = make_terminal(Rect::new(0, 0, 60, 2));
    let style = Style::default().fg(Color::Gray);
    let links = [make_link("https://a.example", 0..4, true)];
    make_draw(&mut terminal, "open", style, &links, true).expect("绘制初始链接");
    make_draw(&mut terminal, "open", style, &links, true).expect("重复绘制静止帧");
    assert!(
        terminal
            .backend()
            .inner
            .draws
            .last()
            .expect("获取静止帧输出")
            .is_empty(),
        "无变化的链接不能每帧强制重画"
    );
}

#[test]
fn test_hyperlink_frame_retries_metadata_after_flush_failure() {
    let mut terminal = make_terminal(Rect::new(0, 0, 60, 2));
    let style = Style::default().fg(Color::Gray);
    let previous = [make_link("https://a.example/1", 0..4, true)];
    let current = [make_link("https://a.example/2", 0..4, true)];
    make_draw(&mut terminal, "open", style, &previous, true).expect("绘制成功基线");
    terminal.backend_mut().inner.fail_flush = true;
    assert!(make_draw(&mut terminal, "open", style, &current, true).is_err());
    make_draw(&mut terminal, "open", style, &current, true).expect("刷新失败后重试");
    let drawn = terminal.backend().inner.draws.last().expect("获取重试输出");
    assert_eq!(
        drawn.len(),
        4,
        "刷新失败不能提前推进链接基线，重试应补画全部目标变化格"
    );
    for (_, _, cell) in drawn {
        assert_eq!(
            make_link_parts(cell.symbol()).map(|(_, url)| url),
            Some("https://a.example/2"),
            "重试必须输出新目标"
        );
    }
}

#[test]
fn test_hyperlink_frame_retries_metadata_after_draw_failure() {
    let mut terminal = make_terminal(Rect::new(0, 0, 60, 2));
    let style = Style::default().fg(Color::Gray);
    let previous = [make_link("https://a.example/1", 0..4, true)];
    let current = [make_link("https://a.example/2", 0..4, true)];
    make_draw(&mut terminal, "open", style, &previous, true).expect("绘制成功基线");
    terminal.backend_mut().inner.fail_draw = true;
    assert!(make_draw(&mut terminal, "open", style, &current, true).is_err());
    make_draw(&mut terminal, "open", style, &current, true).expect("绘制失败后重试");
    let drawn = terminal.backend().inner.draws.last().expect("获取重试输出");
    assert_eq!(
        drawn.len(),
        4,
        "绘制失败不能提前推进链接基线，重试应补画全部目标变化格"
    );
    for (_, _, cell) in drawn {
        assert_eq!(
            make_link_parts(cell.symbol()).map(|(_, url)| url),
            Some("https://a.example/2"),
            "重试必须输出新目标"
        );
    }
}

#[test]
fn test_hyperlink_frame_does_not_consume_another_backend_prepared_frame() {
    let mut first = make_terminal(Rect::new(0, 0, 60, 2));
    let mut second = make_terminal(Rect::new(0, 0, 60, 2));
    let links_frame = first.backend().hyperlink_frame();
    let mut buffer = Buffer::empty(Rect::new(0, 0, 60, 2));
    buffer.set_string(0, 0, "open", Style::default().fg(Color::Gray));
    hyperlink::begin_frame(true);
    for x in 0..4 {
        hyperlink::register(x, 0, "https://a.example", true, (0, 0, 4));
    }
    links_frame.prepare(&mut buffer);
    second
        .backend_mut()
        .draw(std::iter::empty::<(u16, u16, &Cell)>())
        .expect("让另一个后端先处理空差分");
    second.backend_mut().flush().expect("刷新另一个后端");
    assert!(
        second
            .backend()
            .inner
            .draws
            .last()
            .expect("获取另一个后端的输出")
            .is_empty(),
        "后端不能输出其他后端准备的链接格子"
    );
    first
        .backend_mut()
        .draw(std::iter::empty::<(u16, u16, &Cell)>())
        .expect("原后端消费自己的准备帧");
    first.backend_mut().flush().expect("刷新原后端");
    let drawn = first.backend().inner.draws.last().expect("获取原后端输出");
    assert_eq!(drawn.len(), 4, "不匹配的后端不能消费掉原后端的准备帧");
    for (_, _, cell) in drawn {
        assert_eq!(
            make_link_parts(cell.symbol()).map(|(_, url)| url),
            Some("https://a.example"),
            "原后端仍应正常输出链接属性"
        );
    }
}

#[test]
fn test_hyperlink_frame_repaints_previous_target_after_failed_flush() {
    let mut terminal = make_terminal(Rect::new(0, 0, 60, 2));
    let style = Style::default().fg(Color::LightBlue);
    let previous = [make_link("https://a.example/1", 0..4, false)];
    let failed = [make_link("https://a.example/2", 0..4, false)];
    make_draw(&mut terminal, "open", style, &previous, true).expect("绘制成功基线");
    terminal.backend_mut().inner.fail_flush = true;
    assert!(make_draw(&mut terminal, "open", style, &failed, true).is_err());
    for x in 0..4 {
        assert_eq!(
            make_link_parts(make_output_buffer(&terminal)[(x, 0)].symbol()).map(|(_, url)| url),
            Some("https://a.example/2"),
            "失败前输出可能已经把部分终端格改成新目标"
        );
    }
    make_draw(&mut terminal, "open", style, &previous, true).expect("刷新失败后恢复原目标");
    let drawn = terminal.backend().inner.draws.last().expect("获取恢复输出");
    assert_eq!(drawn.len(), 4, "返回成功基线也必须重画输出状态不确定的格子");
    for (_, _, cell) in drawn {
        assert_eq!(
            make_link_parts(cell.symbol()).map(|(_, url)| url),
            Some("https://a.example/1"),
            "失败后回退必须重新输出原目标"
        );
    }
}

#[test]
fn test_hyperlink_frame_partial_clear_preserves_other_rows_metadata() {
    let mut terminal = make_terminal(Rect::new(0, 0, 60, 3));
    let style = Style::default().fg(Color::LightBlue);
    let links_frame = terminal.backend().hyperlink_frame();
    terminal
        .draw(|frame| {
            hyperlink::begin_frame(true);
            let area = frame.area();
            frame.render_widget(Paragraph::new("open\nopen").style(style), area);
            for y in 0..2 {
                for x in 0..4 {
                    hyperlink::register(x, y, "https://a.example", false, (usize::from(y), 0, 4));
                }
            }
            links_frame.prepare(frame.buffer_mut());
        })
        .expect("绘制两行独立链接");
    terminal
        .backend_mut()
        .set_cursor_position(Position::new(0, 0))
        .expect("定位需要局部清理的行");
    terminal
        .backend_mut()
        .clear_region(ClearType::UntilNewLine)
        .expect("仅清理第一行");
    make_draw(&mut terminal, "open\nopen", style, &[], true).expect("删除两行链接属性");
    for y in 0..2 {
        for (x, character) in "open".chars().enumerate() {
            assert_eq!(
                make_output_buffer(&terminal)[(x as u16, y)].symbol(),
                character.to_string(),
                "局部清理不能丢失其他行的旧链接信息，所有链接都应被正确移除"
            );
        }
    }
}

#[test]
fn test_hyperlink_frame_retries_wide_character_covering_dirty_continuation() {
    let mut terminal = make_terminal(Rect::new(0, 0, 60, 2));
    let style = Style::default().fg(Color::Gray);
    let links = [make_link("https://a.example", 1..2, true)];
    make_draw(&mut terminal, "ab", style, &links, true).expect("绘制将被宽字续格覆盖的链接");
    terminal.backend_mut().inner.fail_flush = true;
    assert!(make_draw(&mut terminal, "中", style, &[], true).is_err());
    make_draw(&mut terminal, "中", style, &[], true).expect("重试覆盖不确定链接格的宽字");
    let drawn = terminal
        .backend()
        .inner
        .draws
        .last()
        .expect("获取宽字重试输出");
    assert!(
        drawn
            .iter()
            .any(|(x, y, cell)| *x == 0 && *y == 0 && cell.symbol() == "中"),
        "不确定的链接格即使位于宽字续格，也必须通过首格重新输出整个宽字"
    );
    assert!(
        drawn.iter().all(|(x, y, _)| *x != 1 || *y != 0),
        "重试不能单独输出续格并擦掉宽字右半"
    );
}
