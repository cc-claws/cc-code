use super::*;
use ratatui::{backend::TestBackend, Terminal};

fn make_candidates() -> Vec<super::super::file_search::FileCandidate> {
    (0..15)
        .map(|index| super::super::file_search::FileCandidate {
            path: format!("file_{index:02}.rs"),
            display: format!("file_{index:02}.rs"),
            is_dir: false,
            score: 1,
        })
        .collect()
}

fn make_popup_snapshot(
    state: &AtMentionState,
    width: u16,
    input_y: u16,
    language: &str,
) -> Vec<String> {
    let mut terminal = Terminal::new(TestBackend::new(width, input_y + 2)).unwrap();
    let lc = LcRegistry::new(Some(language));
    terminal
        .draw(|frame| {
            let input_area = Rect::new(0, input_y, width, 2);
            frame.render_widget(Paragraph::new("INPUT"), input_area);
            render_at_mention_popup(frame, state, input_area, &lc);
        })
        .expect("面板应正常渲染");
    let buffer = terminal.backend().buffer();
    (0..buffer.area.height)
        .map(|row| {
            let mut line = String::new();
            let mut column = 0;
            while column < buffer.area.width {
                let symbol = buffer[(column, row)].symbol();
                line.push_str(symbol);
                // 双宽字符后的占位格不是正文，不能插入空格污染中文快照。
                column += UnicodeWidthStr::width(symbol).max(1) as u16;
            }
            line
        })
        .collect()
}

#[test]
fn test_popup_opens_immediately_and_displays_empty_states() {
    let mut state = AtMentionState::new();
    state.activate(String::new(), 0);
    assert!(
        make_popup_snapshot(&state, 60, 15, "en")
            .join("\n")
            .contains("Searching…"),
        "@ 激活后应立即显示面板"
    );
    state.update_candidates(Vec::new());
    assert!(make_popup_snapshot(&state, 60, 15, "zh-CN")
        .join("\n")
        .contains("当前目录无候选"));
    state.activate("missing".into(), 0);
    state.update_candidates(Vec::new());
    assert!(make_popup_snapshot(&state, 60, 15, "en")
        .join("\n")
        .contains("No matching files."));
}

#[test]
fn test_popup_limits_height_and_keeps_input_visible() {
    let mut state = AtMentionState::new();
    state.activate(String::new(), 0);
    state.update_candidates(make_candidates());
    let lines = make_popup_snapshot(&state, 60, 15, "en");
    assert_eq!(
        lines.iter().filter(|line| line.contains("file_")).count(),
        MAX_VIEWPORT
    );
    assert!(lines[3].contains("@ 1–10/15"), "标题应显示可见范围");
    assert!(lines[15].starts_with("INPUT"), "面板不能覆盖输入框");
}

#[test]
fn test_popup_small_terminal_keeps_selected_candidate_visible() {
    let mut state = AtMentionState::new();
    state.activate(String::new(), 0);
    state.update_candidates(make_candidates());
    state.selected = 14;
    let lines = make_popup_snapshot(&state, 60, 4, "en");
    assert_eq!(
        lines.iter().filter(|line| line.contains("file_")).count(),
        2
    );
    assert!(lines[2].contains("❯") && lines[2].contains("file_14.rs"));
    assert!(lines[4].starts_with("INPUT"));
    let tiny = make_popup_snapshot(&state, 60, 2, "en");
    assert!(
        !tiny.iter().any(|line| line.contains("file_")),
        "高度不足时不覆盖输入框"
    );
}

#[test]
fn test_truncate_cjk_with_narrow_width() {
    assert_eq!(
        truncate_middle("你好世界", 3),
        "你",
        "窄终端仍按显示列宽截断"
    );
}

#[test]
fn test_truncate_short() {
    // 短于 max_width 不截断
    assert_eq!(truncate_middle("abc", 10), "abc");
}

#[test]
fn test_truncate_long() {
    let result = truncate_middle("abcdefghijklmnopqrstuvwxyz", 10);
    let width = UnicodeWidthStr::width(result.as_str());
    assert_eq!(width, 10, "截断后宽度应等于 max_width");
    assert!(result.contains("..."), "应包含省略号");
}

#[test]
fn test_truncate_cjk() {
    // 每个汉字占 2 列，CJK 字符不可拆分，实际宽度可能小于 max_width
    let result = truncate_middle("你好世界测试数据", 10);
    let width = UnicodeWidthStr::width(result.as_str());
    assert!(width <= 10, "CJK 截断后宽度不应超过 max_width");
    assert!(result.contains("..."));
}

#[test]
fn test_truncate_exact() {
    // 刚好等于 max_width
    assert_eq!(truncate_middle("abcde", 5), "abcde");
}
