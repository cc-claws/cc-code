use super::*;
use ratatui::crossterm::event::{KeyEvent, MouseEvent};
use std::collections::VecDeque;

#[test]
fn test_simulated_paste_long_text_keeps_final_enter_inside_paste() {
    let mut queue: VecDeque<_> = std::iter::repeat_n(make_key(KeyCode::Char('甲')), 5000)
        .chain([make_key(KeyCode::Enter)])
        .collect();
    let mut pending = VecDeque::new();
    let result = detect_simulated_paste(make_key(KeyCode::Char('甲')), &mut pending, |_| {
        Ok(queue.pop_front())
    })
    .expect("检测成功");
    assert_eq!(result, Event::Paste(format!("{}\n", "甲".repeat(5001))));
    assert!(pending.is_empty(), "尾部换行不能变成单独的提交按键");
}

fn make_key(code: KeyCode) -> Event {
    Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
}

#[test]
fn test_simulated_paste_preserves_mouse_and_shortcut_boundaries() {
    for boundary in [
        Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 79,
            row: 5,
            modifiers: KeyModifiers::NONE,
        }),
        Event::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)),
        Event::Resize(80, 24),
    ] {
        let mut queue = VecDeque::from([
            make_key(KeyCode::Char('好')),
            boundary.clone(),
            make_key(KeyCode::Char('后')),
        ]);
        let mut pending = VecDeque::new();
        let result = detect_simulated_paste(make_key(KeyCode::Char('你')), &mut pending, |_| {
            Ok(queue.pop_front())
        })
        .expect("检测成功");
        assert_eq!(result, Event::Paste("你好".into()));
        assert_eq!(
            pending,
            VecDeque::from([boundary]),
            "边界事件不能被粘贴吞掉"
        );
        assert_eq!(queue.pop_front(), Some(make_key(KeyCode::Char('后'))));
    }
}

#[test]
fn test_simulated_paste_multiline_and_single_key_are_preserved() {
    let mut queue = VecDeque::from([make_key(KeyCode::Enter), make_key(KeyCode::Char('乙'))]);
    let mut pending = VecDeque::new();
    assert_eq!(
        detect_simulated_paste(make_key(KeyCode::Char('甲')), &mut pending, |_| Ok(
            queue.pop_front()
        ))
        .expect("检测成功"),
        Event::Paste("甲\n乙".into())
    );
    assert_eq!(
        detect_simulated_paste(make_key(KeyCode::Enter), &mut pending, |_| Ok(None))
            .expect("检测成功"),
        make_key(KeyCode::Enter)
    );
}

#[test]
fn test_simulated_paste_hover_between_text_and_enter_does_not_submit() {
    let hover = Event::Mouse(MouseEvent {
        kind: MouseEventKind::Moved,
        column: 79,
        row: 5,
        modifiers: KeyModifiers::NONE,
    });
    let mut queue = VecDeque::from([
        make_key(KeyCode::Char('乙')),
        hover.clone(),
        make_key(KeyCode::Enter),
    ]);
    let mut pending = VecDeque::new();
    let event = detect_simulated_paste(make_key(KeyCode::Char('甲')), &mut pending, |_| {
        Ok(queue.pop_front())
    })
    .expect("检测成功");
    assert_eq!(
        event,
        Event::Paste("甲乙\n".into()),
        "移动鼠标不能让粘贴换行变为提交"
    );
    assert_eq!(pending, VecDeque::from([hover]), "悬停位置仍必须送交 UI");
}
