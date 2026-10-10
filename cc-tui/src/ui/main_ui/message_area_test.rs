use super::*;
use crate::ui::theme;

fn make_app() -> App {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(App::new())
}

/// 检查 span 是否有选区背景色
fn has_selection_bg(style: Style) -> bool {
    matches!(style.bg, Some(theme::SELECTION_BG))
}

#[test]
fn test_spinner_extra_count_reserves_blank_line_before_recap() {
    let mut app = make_app();
    // 无 recap：空行(1) + 总结行(1) + trailing(1)
    assert_eq!(spinner_extra_count(&app), 3, "无 recap 时总是 3 行");
    // 有 recap：额外多出分隔空行(1) + recap 行(1)
    app.session_mgr.current_mut().latest_recap = Some("测试摘要".to_string());
    assert_eq!(
        spinner_extra_count(&app),
        5,
        "recap 与总结行间应保留分隔空行"
    );
}

#[test]
fn test_highlight_line_spans_full_span() {
    let spans = vec![Span::from("Hello"), Span::from("World")];
    let result = highlight_line_spans(spans, 0, 10);
    assert_eq!(result.len(), 2);
    assert!(has_selection_bg(result[0].style));
    assert!(has_selection_bg(result[1].style));
}

#[test]
fn test_highlight_line_spans_partial_start() {
    let spans = vec![Span::from("Hello")];
    let result = highlight_line_spans(spans, 3, 10);
    // 前 3 字符原样，后 2 字符选区背景
    assert_eq!(result.len(), 2);
    assert!(!has_selection_bg(result[0].style));
    assert!(has_selection_bg(result[1].style));
    assert_eq!(result[0].content, "Hel");
    assert_eq!(result[1].content, "lo");
}

#[test]
fn test_highlight_line_spans_partial_both() {
    let spans = vec![Span::from("Hello")];
    let result = highlight_line_spans(spans, 1, 4);
    assert_eq!(result.len(), 3);
    assert_eq!(result[0].content, "H");
    assert!(!has_selection_bg(result[0].style));
    assert_eq!(result[1].content, "ell");
    assert!(has_selection_bg(result[1].style));
    assert_eq!(result[2].content, "o");
    assert!(!has_selection_bg(result[2].style));
}

#[test]
fn test_highlight_line_spans_multi_span() {
    let spans = vec![Span::from("Hel"), Span::from("lo Wo"), Span::from("rld")];
    let result = highlight_line_spans(spans, 2, 8);
    // 选中范围 char 2..8 = "llo Wo"
    // span0 "Hel": 前 2 原样 + 后 1 选区背景
    // span1 "lo Wo": 全部选区背景
    // span2 "rld": 不在选区（span2 starts at char 8）
    assert_eq!(result.len(), 4);
    assert_eq!(result[0].content, "He");
    assert!(!has_selection_bg(result[0].style));
    assert_eq!(result[1].content, "l");
    assert!(has_selection_bg(result[1].style));
    assert_eq!(result[2].content, "lo Wo");
    assert!(has_selection_bg(result[2].style));
    assert_eq!(result[3].content, "rld");
    assert!(!has_selection_bg(result[3].style));
}

#[test]
fn test_highlight_line_spans_outside() {
    let spans = vec![Span::from("Hello")];
    let result = highlight_line_spans(spans, 10, 15);
    assert_eq!(result.len(), 1);
    assert!(!has_selection_bg(result[0].style));
    assert_eq!(result[0].content, "Hello");
}

#[test]
fn test_todo_render_line_count_boundaries() {
    assert_eq!(todo_render_line_count(0), 0, "空任务列表行数为0");
    assert_eq!(todo_render_line_count(1), 1, "单个任务占1行");
    assert_eq!(todo_render_line_count(5), 5, "5个任务占5行");
    assert_eq!(todo_render_line_count(6), 6, "6个任务占5行+1行折叠统计");
    assert_eq!(todo_render_line_count(8), 6, "8个任务占5行+1行折叠统计");
    assert_eq!(todo_render_line_count(20), 6, "20个任务依然最多占6行");
}

#[test]
fn test_format_hidden_todos_summary_cases() {
    let pending_items = vec![
        TodoItem {
            content: "Task A".to_string(),
            status: TodoStatus::Pending,
            active_form: None,
        },
        TodoItem {
            content: "Task B".to_string(),
            status: TodoStatus::Pending,
            active_form: None,
        },
        TodoItem {
            content: "Task C".to_string(),
            status: TodoStatus::InProgress,
            active_form: None,
        },
    ];
    assert_eq!(
        format_hidden_todos_summary(&pending_items),
        "    ... +3 pending",
        "全未完成项应显示 pending 统计"
    );

    let completed_items = vec![
        TodoItem {
            content: "Task D".to_string(),
            status: TodoStatus::Completed,
            active_form: None,
        },
        TodoItem {
            content: "Task E".to_string(),
            status: TodoStatus::Completed,
            active_form: None,
        },
    ];
    assert_eq!(
        format_hidden_todos_summary(&completed_items),
        "    ... +2 completed",
        "全已完成项应显示 completed 统计"
    );

    let mixed_items = vec![
        TodoItem {
            content: "Task F".to_string(),
            status: TodoStatus::Pending,
            active_form: None,
        },
        TodoItem {
            content: "Task G".to_string(),
            status: TodoStatus::Completed,
            active_form: None,
        },
    ];
    assert_eq!(
        format_hidden_todos_summary(&mixed_items),
        "    ... +1 pending, +1 completed",
        "混合项应同时显示两类统计"
    );
}

#[test]
fn test_spinner_extra_count_clamps_with_many_todos() {
    let mut app = make_app();
    app.session_mgr.current_mut().ui.loading = true;
    for i in 0..8 {
        app.session_mgr.current_mut().todo_items.push(TodoItem {
            content: format!("Task {i}"),
            status: TodoStatus::Pending,
            active_form: None,
        });
    }
    // 8 个 todo 时：空行(1) + spinner(1) + tip(1) + 分隔空行(1) + 5 个 todo 项 + 1 个截断行 + trailing(1) = 11 行
    assert_eq!(
        spinner_extra_count(&app),
        11,
        "超过 5 个 todo 时行数应封顶在 11 行"
    );
}
