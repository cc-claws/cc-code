use super::*;
use ratatui::{backend::TestBackend, buffer::Buffer, Terminal};

fn make_input_buffer(
    textarea: &TextArea<'static>,
    mode: TextareaShellMode,
    focused: bool,
    cursor_visible: bool,
) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(24, 8)).expect("创建输入测试终端");
    terminal
        .draw(|frame| {
            render_textarea(
                frame,
                textarea,
                Rect::new(0, 0, 24, 8),
                mode,
                focused,
                cursor_visible,
            );
        })
        .expect("绘制输入框");
    terminal.backend().buffer().clone()
}

fn make_cursor_position(buffer: &Buffer) -> (u16, u16) {
    let cursors: Vec<_> = (0..buffer.area.height)
        .flat_map(|row| (0..buffer.area.width).map(move |column| (column, row)))
        .filter(|position| buffer[*position].symbol() == "│")
        .collect();
    assert_eq!(cursors.len(), 1, "聚焦输入框应恰好显示一个竖线光标");
    cursors[0]
}

#[test]
fn test_long_draft_cursor_stays_visible_while_moving_vertically() {
    let mut textarea = crate::app::build_textarea(false);
    textarea.insert_str(
        (0..200)
            .map(|row| format!("row-{row:03}"))
            .collect::<Vec<_>>()
            .join("\n"),
    );
    let original_lines = textarea.lines().to_vec();
    let bottom = make_input_buffer(&textarea, TextareaShellMode::None, true, true);
    let (_, bottom_y) = make_cursor_position(&bottom);
    assert_eq!(bottom_y, 6, "末行光标应在六行内容区底部可见");

    textarea.move_cursor(CursorMove::Up);
    let up = make_input_buffer(&textarea, TextareaShellMode::None, true, true);
    assert_eq!(
        make_cursor_position(&up).1,
        6,
        "上移后内部滚动仍应使光标可见"
    );
    textarea.move_cursor(CursorMove::Down);
    let down = make_input_buffer(&textarea, TextareaShellMode::None, true, true);
    assert_eq!(make_cursor_position(&down).1, 6, "下移回到底部时光标仍可见");

    textarea.move_cursor(CursorMove::Top);
    let top = make_input_buffer(&textarea, TextareaShellMode::None, true, true);
    assert_eq!(
        make_cursor_position(&top).1,
        1,
        "移动到首行后内部窗口应随光标滚动"
    );
    assert_eq!(textarea.lines(), original_lines, "渲染不得改动草稿内容");
}

#[test]
fn test_long_line_cursor_stays_visible_while_scrolling_horizontally() {
    let mut textarea = crate::app::build_textarea(false);
    textarea.insert_str("abcdef".repeat(30));
    let end = make_input_buffer(&textarea, TextareaShellMode::None, true, true);
    assert_eq!(
        make_cursor_position(&end).0,
        23,
        "长行尾部光标应水平滚动到可见区域"
    );
    textarea.move_cursor(CursorMove::Back);
    let back = make_input_buffer(&textarea, TextareaShellMode::None, true, true);
    assert_eq!(
        make_cursor_position(&back).0,
        23,
        "长行左移后光标仍在可见区域"
    );
    textarea.move_cursor(CursorMove::Forward);
    let forward = make_input_buffer(&textarea, TextareaShellMode::None, true, true);
    assert_eq!(
        make_cursor_position(&forward).0,
        23,
        "右移后光标仍在输入框内"
    );
    textarea.move_cursor(CursorMove::Head);
    let head = make_input_buffer(&textarea, TextareaShellMode::None, true, true);
    assert_eq!(
        make_cursor_position(&head).0,
        1,
        "回到行首后竖线位于左侧内边距（文本左缘）"
    );
}

#[test]
fn test_shell_display_prefix_and_cursor_keep_draft_intact() {
    let mut textarea = crate::app::build_textarea(false);
    textarea.insert_str("!echo hello");
    let cursor = textarea.cursor();
    let buffer = make_input_buffer(&textarea, TextareaShellMode::Command, true, true);
    assert_eq!(
        make_cursor_position(&buffer),
        (12, 1),
        "隐藏前缀后光标同步左移一列"
    );
    let visible: String = (2..24).map(|column| buffer[(column, 1)].symbol()).collect();
    assert!(visible.starts_with("echo hello"), "展示只隐藏 shell 前缀");
    assert!(!visible.contains('!'), "展示副本不显示 shell 前缀");
    assert_eq!(
        textarea.lines(),
        ["!echo hello"],
        "真实草稿保留前缀供提交识别"
    );
    assert_eq!(textarea.cursor(), cursor, "展示不改变真实光标");

    textarea.insert_str(" argument".repeat(20));
    let long = make_input_buffer(&textarea, TextareaShellMode::Command, true, true);
    assert!(
        make_cursor_position(&long).0 < 24,
        "长 shell 命令水平滚动后光标可见"
    );
}

#[test]
fn test_unfocused_or_blinking_cursor_does_not_leave_reverse_cells() {
    let mut textarea = crate::app::build_textarea(false);
    textarea.insert_str("draft");
    for (focused, visible) in [(false, true), (true, false)] {
        let buffer = make_input_buffer(&textarea, TextareaShellMode::None, focused, visible);
        assert!(
            buffer
                .content
                .iter()
                .all(|cell| !cell.modifier.contains(Modifier::REVERSED)),
            "失焦或光标灭相时不得显示反色光标"
        );
    }
}

#[test]
fn test_rebuilt_textarea_keeps_row_limits_and_horizontal_scroll() {
    for disabled in [false, true] {
        let textarea = crate::app::build_textarea(disabled);
        assert_eq!(textarea.min_rows(), 3, "所有重建路径保留三行最小高度");
        assert_eq!(textarea.max_rows(), 8, "所有重建路径保留八行最大高度");
        assert_eq!(
            textarea.wrap_mode(),
            tui_textarea::WrapMode::None,
            "保留长行水平滚动，本次不引入软换行"
        );
    }
}
