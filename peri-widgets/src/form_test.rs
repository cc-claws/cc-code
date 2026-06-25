use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum TestField {
    A,
    B,
    C,
}

impl FormField for TestField {
    fn next(self) -> Self {
        match self {
            Self::A => Self::B,
            Self::B => Self::C,
            Self::C => Self::A,
        }
    }
    fn prev(self) -> Self {
        match self {
            Self::A => Self::C,
            Self::B => Self::A,
            Self::C => Self::B,
        }
    }
    fn label(self) -> &'static str {
        match self {
            Self::A => "A",
            Self::B => "B",
            Self::C => "C",
        }
    }
}

#[test]
fn form_state_field_navigation() {
    let fields = [TestField::A, TestField::B, TestField::C];
    let mut state = FormState::with_active(&fields, TestField::A);
    assert_eq!(state.active_field(), TestField::A);
    state.next_field();
    assert_eq!(state.active_field(), TestField::B);
    state.next_field();
    assert_eq!(state.active_field(), TestField::C);
    state.next_field();
    assert_eq!(state.active_field(), TestField::A); // wraps
    state.prev_field();
    assert_eq!(state.active_field(), TestField::C); // wraps back
}

#[test]
fn form_state_text_editing() {
    let mut state = FormState::new([TestField::A, TestField::B, TestField::C].into_iter());
    state.handle_char('h');
    state.handle_char('i');
    assert_eq!(state.active_input().value(), "hi");
    state.handle_backspace();
    assert_eq!(state.active_input().value(), "h");
}

#[test]
fn form_state_independent_fields() {
    let mut state = FormState::new([TestField::A, TestField::B, TestField::C].into_iter());
    state.handle_char('h');
    state.handle_char('i');
    state.next_field();
    state.handle_char('x');
    state.prev_field();
    assert_eq!(state.active_input().value(), "hi");
}

#[test]
fn form_state_cursor_movement() {
    let mut state = FormState::new([TestField::A, TestField::B, TestField::C].into_iter());
    state.handle_char('a');
    state.handle_char('b');
    state.handle_cursor_home();
    state.handle_char('X');
    assert_eq!(state.active_input().value(), "Xab");
}

#[test]
fn form_state_paste() {
    let mut state = FormState::new([TestField::A, TestField::B, TestField::C].into_iter());
    state.handle_paste("hello");
    assert_eq!(state.active_input().value(), "hello");
}

#[test]
fn form_state_set_active() {
    let mut state = FormState::new([TestField::A, TestField::B, TestField::C].into_iter());
    state.set_active(TestField::C);
    assert_eq!(state.active_field(), TestField::C);
    assert_eq!(state.input(TestField::A).value(), "");
}
