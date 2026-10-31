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
    style.bg == Some(theme::cur().selection_bg)
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
fn test_highlight_line_spans_preserves_grapheme_across_style_spans() {
    let spans = vec![Span::from("Ae"), Span::from("\u{301}👩‍💻Z")];
    let highlighted = highlight_line_spans(spans, 1, 3);
    let selected: String = highlighted
        .iter()
        .filter(|span| has_selection_bg(span.style))
        .map(|span| span.content.as_ref())
        .collect();
    assert_eq!(selected, "e\u{301}👩‍💻", "选区必须与复制使用同一字素边界");
}

#[test]
fn test_presented_viewport_waits_for_complete_window_without_changing_target() {
    use crate::ui::render_thread::{VirtualLayoutSnapshot, WrappedLineInfo};
    let mut cache = RenderCache::new();
    cache.total_lines = 1_000;
    cache.virtual_layout = Some(std::sync::Arc::new(VirtualLayoutSnapshot::default()));
    cache.wrap_map = vec![WrappedLineInfo {
        line_idx: 0,
        visual_row_start: 100,
        visual_row_end: 140,
        plain_text: String::new(),
        char_widths: Vec::new(),
        copy_prefix_graphemes: 0,
    }];
    assert_eq!(
        presented_viewport_offset(&cache, 800, 110, 10),
        110,
        "未物化目标继续画旧窗口"
    );
    assert_eq!(
        presented_viewport_offset(&cache, 135, 110, 10),
        110,
        "仅覆盖半个视口也不能切过去"
    );
    assert_eq!(
        presented_viewport_offset(&cache, 120, 110, 10),
        120,
        "覆盖完整视口即可切换"
    );
    assert_eq!(
        presented_viewport_offset(&cache, 800, 0, 10),
        100,
        "旧位置已淘汰时限制到当前窗口"
    );
    cache.virtual_layout = None;
    assert_eq!(
        presented_viewport_offset(&cache, 800, 110, 10),
        800,
        "全量模式沿用原行为"
    );
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
    // 未发布 tip 时 tip 行不占位：空行(1) + spinner(1) + 分隔空行(1) + 5 个 todo 项 + 1 个截断行 + trailing(1) = 10 行
    assert_eq!(
        spinner_extra_count(&app),
        10,
        "无 tip 时应为 10 行（tip 行不占位）"
    );
    // 发布一条 tip 后：再 + tip(1) = 11 行封顶
    app.spinner_tip_state.on_turn_start(|| {
        Some(crate::ui::tips::TipCandidate {
            id: "tip-0".to_string(),
            text: "x".to_string(),
            cooldown_sessions: 0,
            priority: 0,
            label: None,
        })
    });
    app.spinner_tip_state.on_turn_end();
    assert_eq!(
        spinner_extra_count(&app),
        11,
        "超过 5 个 todo 时行数应封顶在 11 行"
    );
}

/// 压缩跑道：固定 12 格，小人从右向左单向奔跑并穿墙回卷；先跑，跑久了改走。
#[test]
fn test_compact_runner_line_advances_along_fixed_track() {
    fn render(offset_ms: u64) -> (String, usize) {
        let started = std::time::Instant::now() - std::time::Duration::from_millis(offset_ms);
        let line = compact_runner_line(started);
        let text: String = line.spans.iter().map(|sp| sp.content.as_ref()).collect();
        assert_eq!(
            line.spans[0].content.as_ref(),
            COMPACT_TRACK_INDENT,
            "跑道首 span 应为缩进"
        );
        let glyph = if text.contains('🚶') {
            '🚶'
        } else {
            '🏃'
        };
        let pos = line
            .spans
            .iter()
            .position(|sp| sp.content.contains(glyph))
            .expect("应存在小人");
        // 减去缩进 span，换算成跑道格号
        (text, pos - 1)
    }

    let (early, early_pos) = render(0);
    let (later, later_pos) = render(450);
    let (tired, _) = render(25_000);

    assert_eq!(
        early.matches('🏃').count(),
        1,
        "同一时刻只有一个小人: {early}"
    );
    assert_eq!(
        later.matches('🏃').count(),
        1,
        "同一时刻只有一个小人: {later}"
    );
    assert_eq!(early.matches('·').count(), 11, "跑道固定 12 格: {early}");
    assert_eq!(early_pos, 11, "起始帧位于最右端");
    assert_eq!(later_pos, 8, "跑：每 150ms 向左前进一格 (11 -> 8)");
    assert_eq!(
        later.matches('•').count(),
        3,
        "身后的 3 格留下足迹: {later}"
    );
    assert_eq!(
        later.matches('·').count(),
        8,
        "身前的 8 格为待跑点: {later}"
    );

    // 穿墙单向循环验证：跑过一轮（1800ms）后重新从右端进场
    let (_, wrap_pos) = render(1800);
    assert_eq!(wrap_pos, 11, "跑完一轮后穿墙回卷至最右端");

    assert_eq!(
        tired.matches('🚶').count(),
        1,
        "跑满 20 秒应改走路: {tired}"
    );
    assert_eq!(tired.matches('🏃').count(), 0, "走路时不再跑: {tired}");
}

/// 螃蟹跑道：固定 12 格，横向爬行并双向折返，契合横行特性。
#[test]
fn test_compact_crab_line_follows_bidirectional_cycle() {
    fn render(offset_ms: u64) -> (String, usize) {
        let started = std::time::Instant::now() - std::time::Duration::from_millis(offset_ms);
        let line = compact_crab_line(started);
        let text: String = line.spans.iter().map(|sp| sp.content.as_ref()).collect();
        assert_eq!(
            line.spans[0].content.as_ref(),
            COMPACT_TRACK_INDENT,
            "跑道首 span 应为缩进"
        );
        let pos = line
            .spans
            .iter()
            .position(|sp| sp.content.contains('🦀'))
            .expect("应存在螃蟹");
        (text, pos - 1)
    }

    let (early, early_pos) = render(0);
    let (later, later_pos) = render(480);
    let (tired, _) = render(25_000);

    assert_eq!(
        early.matches('🦀').count(),
        1,
        "同一时刻只有一只螃蟹: {early}"
    );
    assert_eq!(
        later.matches('🦀').count(),
        1,
        "同一时刻只有一只螃蟹: {later}"
    );
    assert_eq!(early_pos, 0, "起始帧位于最左端");
    assert_eq!(later_pos, 3, "爬：每 160ms 向右前进一格 (0 -> 3)");
    assert_eq!(later.matches('•').count(), 3, "爬过的格子留下足迹: {later}");

    // 与飞碟一致的双向节奏：爬到头后折返，而不是瞬跳回起点。
    let (_, turn_pos) = render(2100); // 帧 13：终点停留，仍在最右
    let (_, back_pos) = render(2900); // 帧 18：已折返
    assert_eq!(turn_pos, 11, "抵达右端后停留");
    assert!(
        back_pos < turn_pos,
        "到头后应折返: {back_pos} vs {turn_pos}"
    );

    assert_eq!(
        tired.matches('🦀').count(),
        1,
        "爬满 20 秒仍显示螃蟹: {tired}"
    );
}

/// 飞碟跑道：14 格、40ms/帧，前进 → 终点停留 → 后退 → 起点停留（对齐 MiMo）。
#[test]
fn test_compact_ufo_line_follows_reference_cycle() {
    fn render(offset_ms: u64) -> (String, usize) {
        let started = std::time::Instant::now() - std::time::Duration::from_millis(offset_ms);
        let line = compact_ufo_line(started);
        let text: String = line.spans.iter().map(|sp| sp.content.as_ref()).collect();
        assert_eq!(
            line.spans[0].content.as_ref(),
            COMPACT_TRACK_INDENT,
            "跑道首 span 应为缩进"
        );
        let pos = line
            .spans
            .iter()
            .position(|sp| sp.content.contains('🛸'))
            .expect("应存在飞碟");
        (text, pos - 1)
    }

    // 帧 13（520ms）：前进到最右；帧 15（600ms）：终点停留仍在最右
    let (_, end_pos) = render(530);
    let (_, hold_end_pos) = render(600);
    // 帧 22（880ms）：已开始后退；帧 37（1480ms）：起点停留
    let (_, back_pos) = render(900);
    let (start_text, start_pos) = render(1500);

    assert_eq!(end_pos, 13, "前进到第 14 格");
    assert_eq!(hold_end_pos, 13, "终点停留期间位置不变");
    assert_eq!(back_pos, 12, "停留结束后开始后退");
    assert_eq!(start_pos, 0, "回到起点");
    assert_eq!(start_text.matches('🛸').count(), 1, "同一时刻只有一个飞碟");

    // 尾迹 5 格（trailSteps = 6 → dd ∈ 1..=5）
    let (end_text, _) = render(530);
    let trail = end_text
        .chars()
        .filter(|c| matches!(c, '·' | '∙' | '˙'))
        .count();
    assert_eq!(trail, 5, "终点处尾迹应为 5 格: {end_text}");
}
