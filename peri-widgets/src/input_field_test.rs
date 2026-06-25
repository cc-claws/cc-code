use super::*;

#[test]
fn input_state_insert_and_backspace() {
    let mut s = InputState::new();
    s.insert('a');
    s.insert('b');
    s.insert('c');
    assert_eq!(s.value(), "abc");
    s.backspace();
    assert_eq!(s.value(), "ab");
    assert_eq!(s.cursor(), 2);
}

#[test]
fn input_state_cursor_movement() {
    let mut s = InputState::with_value("abc".into());
    s.cursor_end();
    assert_eq!(s.cursor(), 3);
    s.cursor_left();
    assert_eq!(s.cursor(), 2);
    s.cursor_home();
    assert_eq!(s.cursor(), 0);
}

#[test]
fn input_state_delete_at_cursor() {
    let mut s = InputState::with_value("abc".into());
    s.cursor_home(); // cursor at 0
    s.cursor_right(); // cursor at 1 ('b')
    s.delete();
    assert_eq!(s.value(), "ac");
}

#[test]
fn input_state_paste() {
    let mut s = InputState::new();
    s.paste("hello");
    assert_eq!(s.value(), "hello");
    assert_eq!(s.cursor(), 5);
}

#[test]
fn input_state_utf8_multibyte() {
    let mut s = InputState::new();
    s.insert('中');
    s.insert('文');
    assert_eq!(s.value(), "中文");
    s.cursor_left();
    s.cursor_left();
    assert_eq!(s.cursor(), 0);
    s.insert('你');
    assert_eq!(s.value(), "你中文");
}

#[test]
fn input_state_masked_display() {
    let s = InputState::with_value("sk-1234567890".into()).masked(true);
    let display = s.display_text('•');
    assert_eq!(display, "sk-1••••7890");
}

#[test]
fn input_state_masked_short() {
    let s = InputState::with_value("abc".into()).masked(true);
    let display = s.display_text('•');
    assert_eq!(display, "•••");
}

#[test]
fn input_field_to_line_focused() {
    let s = InputState::with_value("test".into());
    let field = InputField::new("Name").focused(true);
    let line = field.to_line(&s);
    let line_str: String = line.spans.iter().map(|s| s.content.clone()).collect();
    assert!(
        line_str.contains('│'),
        "Expected cursor char, got: {}",
        line_str
    );
}

#[test]
fn input_field_to_line_unfocused() {
    let s = InputState::with_value("test".into());
    let field = InputField::new("Name").focused(false);
    let line = field.to_line(&s);
    let line_str: String = line.spans.iter().map(|s| s.content.clone()).collect();
    assert!(
        !line_str.contains('│'),
        "Expected no cursor char, got: {}",
        line_str
    );
}

#[test]
fn input_field_to_line_cursor_hidden() {
    let s = InputState::with_value("test".into());
    let style = InputFieldStyle {
        cursor_visible: false,
        ..Default::default()
    };
    let field = InputField::new("Name").focused(true).style(style);
    let line = field.to_line(&s);
    let line_str: String = line.spans.iter().map(|s| s.content.clone()).collect();
    assert!(
        !line_str.contains('│'),
        "Expected no cursor when hidden, got: {}",
        line_str
    );
}
