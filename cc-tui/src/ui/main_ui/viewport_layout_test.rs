use super::*;
use crate::{app::MessageViewModel, terminal_backend::viewport::ViewportState, ui::main_ui};
use ratatui::{backend::TestBackend, layout::Size, Terminal, TerminalOptions, Viewport};

async fn make_messages(app: &mut App, text: &str) {
    app.session_mgr.current().messages.invalidate_layout();
    let messages = vec![MessageViewModel::user(text.into())];
    app.session_mgr.current_mut().messages.view_messages = messages.clone();
    let sender = app.session_mgr.current().messages.render_tx.clone();
    sender
        .send(crate::ui::render_thread::RenderEvent::Rebuild(messages))
        .await
        .expect("提交本地渲染快照");
    let (done, ready) = tokio::sync::oneshot::channel();
    sender
        .send(crate::ui::render_thread::RenderEvent::ResizeAndWait(
            80, done,
        ))
        .await
        .expect("同步渲染");
    ready.await.expect("渲染完成");
}

/// 启动期系统提示不算会话历史。
///
/// 回归：hooks 未信任提示在首帧之前入列，曾被判成「已有历史」，
/// 导致欢迎卡与吉祥物在整个会话里都不显示（#783 与 #652 的门控相撞）。
#[tokio::test]
async fn test_startup_notice_keeps_welcome_visible() {
    // Arrange
    let (mut app, _) = App::new_headless(120, 40).await;
    app.push_toned_system_note(
        r#"本项目 hooks 未加载：D:\code\peri 未被信任。执行 `cc-code trust-project "D:\code\peri"` 放行（或设 CC_CODE_TRUST_PROJECT_HOOKS=1）。"#.to_string(),
        crate::ui::message_view::NoteTone::Warning,
    );
    let session = app.session_mgr.current().metadata.session_id;
    // Act
    app.global_ui
        .viewport
        .sync_welcome(session, has_conversation_history(&app));
    // Assert
    assert!(
        !has_conversation_history(&app),
        "纯系统提示不应算作会话历史"
    );
    assert!(
        app.global_ui.viewport.startup_welcome,
        "启动期提示不得把会话判成旧会话"
    );
    assert!(welcome_visible(&app), "欢迎卡与吉祥物应可见");
    assert!(welcome_rows(&app, 80) > 0, "欢迎卡应占用行数");

    // 真实用户消息才算历史
    app.session_mgr
        .current_mut()
        .messages
        .view_messages
        .push(MessageViewModel::user("hi".into()));
    assert!(has_conversation_history(&app), "真实用户消息应计入会话历史");
}

#[tokio::test]
async fn test_compact_idle_height_is_stable_and_input_follows_card() {
    for (width, height) in [(80, 24), (120, 40), (40, 24)] {
        let (mut app, _) = App::new_headless(width, height).await;
        app.global_ui.viewport = ViewportState::growing(3);
        app.global_ui.viewport.terminal_size = Size::new(width, height);
        app.global_ui.viewport.startup_welcome = true;
        let area = Rect::new(0, 0, width, height);
        let required = required_rows(&mut app, area);
        assert!(
            (10..=12).contains(&required),
            "常规首帧只占必要行数，实际 {required}"
        );
        let plan = app
            .global_ui
            .viewport
            .prepare(required, Size::new(width, height));
        let mut terminal = Terminal::with_options(
            TestBackend::new(width, height),
            TerminalOptions {
                viewport: Viewport::Fixed(plan.area),
            },
        )
        .expect("固定视口");
        for _ in 0..3 {
            terminal
                .draw(|frame| main_ui::render(frame, &mut app))
                .expect("绘制紧凑布局");
            assert_eq!(
                required_rows(&mut app, area),
                required,
                "已分配空白不能反向计入内容"
            );
        }
        let input = app
            .session_mgr
            .current()
            .ui
            .textarea_area
            .expect("输入区存在");
        assert_eq!(input.y, plan.area.y + welcome::welcome_height(&app, width));
        assert_eq!(input.bottom() + 2, plan.area.bottom(), "状态栏紧跟输入框");
        assert!(!app.global_ui.viewport.fullscreen_latched);
    }
}

#[tokio::test]
async fn test_draft_borrows_message_rows_without_growing_viewport() {
    let (mut app, _) = App::new_headless(80, 40).await;
    app.global_ui.viewport = ViewportState::growing(2);
    let terminal = Rect::new(0, 0, 80, 40);
    let first = required_rows(&mut app, terminal);
    app.global_ui.viewport.prepare(first, Size::new(80, 40));
    app.session_mgr
        .current_mut()
        .ui
        .textarea
        .insert_str("第一行\n第二行\nthird");
    let multiline = required_rows(&mut app, terminal);
    assert_eq!(multiline, first, "多行草稿不得推动视口增长");
    let plan = app.global_ui.viewport.prepare(multiline, Size::new(80, 40));
    let heights = BottomHeights::measure(&app, terminal);
    let input = input_rows(
        heights.input,
        40,
        plan.area.height,
        heights.total() - heights.input,
    );
    assert!(input > INPUT_MIN_ROWS, "多行草稿应借用已有消息区的行");
    let cursor = app.session_mgr.current().ui.textarea.cursor();
    app.session_mgr.current_mut().ui.textarea.delete_char();
    let shorter = required_rows(&mut app, terminal);
    let plan = app.global_ui.viewport.prepare(shorter, Size::new(80, 40));
    assert_eq!(plan.area.height, first, "删除后仍保留原始视口高度");
    assert_eq!(
        app.session_mgr.current().ui.textarea.cursor(),
        (cursor.0, cursor.1 - 1)
    );
}

#[test]
fn test_input_rows_preserves_message_space_and_fits_tiny_areas() {
    let others = 2;
    assert_eq!(
        input_rows(8, 40, others + 11, others),
        8,
        "保留三行消息后可借八行"
    );
    for area in [4, 6, 24] {
        let input = input_rows(200, 40, area, others);
        assert!(
            input + others < area,
            "区域 {area} 至少留一行消息且不得溢出"
        );
    }
    assert_eq!(
        input_rows(8, 40, 4, others),
        1,
        "极小区域允许输入框低于三行"
    );
    assert_eq!(input_rows(8, 40, 6, others), 3, "空间紧张时退回三行输入");
    assert_eq!(
        input_rows(8, 40, 2, others),
        0,
        "底栏已占满时不能继续分配输入"
    );
}

#[test]
fn test_input_rows_uses_host_cap_and_keeps_fullscreen_message_rows() {
    for (terminal, cap) in [(6, 3), (16, 5), (24, 8), (40, 8), (u16::MAX, 8)] {
        assert_eq!(
            input_rows(200, terminal, 40, 2),
            cap,
            "输入上限只取宿主高度并封顶八行"
        );
    }
    for terminal in [6, 16, 24, 40] {
        let input = input_rows(200, terminal, terminal, 2);
        assert!(input <= INPUT_MAX_ROWS, "全屏输入不得超过八行");
        if terminal >= INPUT_MIN_ROWS + MIN_MESSAGE_ROWS + 2 {
            assert!(
                terminal - 2 - input >= MIN_MESSAGE_ROWS,
                "足够空间时保留三行消息"
            );
        }
    }
}

#[tokio::test]
async fn test_long_draft_keeps_growth_baseline_from_first_frame() {
    for height in [16, 24, 40] {
        let (mut app, _) = App::new_headless(80, height).await;
        app.global_ui.viewport = ViewportState::growing(0);
        let terminal = Rect::new(0, 0, 80, height);
        let baseline = required_rows(&mut app, terminal);
        app.session_mgr
            .current_mut()
            .ui
            .textarea
            .insert_str(&["draft"; 200].join("\n"));
        assert_eq!(
            required_rows(&mut app, terminal),
            baseline,
            "两百行草稿与单行所需高度相同"
        );
        let plan = app
            .global_ui
            .viewport
            .prepare(baseline, Size::new(80, height));
        assert_eq!(plan.area.height, baseline, "首次绘制只分配基线高度");
        assert!(!plan.enter_fullscreen, "草稿不得假性锁定全屏");
        assert_eq!(
            BottomHeights::measure_for_growth(&app, terminal).input,
            INPUT_MIN_ROWS
        );
        app.session_mgr.current_mut().ui.textarea = crate::app::build_textarea(false);
        let cleared = required_rows(&mut app, terminal);
        let cleared_plan = app
            .global_ui
            .viewport
            .prepare(cleared, Size::new(80, height));
        assert_eq!(
            cleared_plan.area.height, baseline,
            "清空后没有草稿产生的留白"
        );
    }
}

#[tokio::test]
async fn test_first_message_keeps_welcome_and_real_message_coordinates() {
    let (mut app, _) = App::new_headless(80, 40).await;
    app.global_ui.viewport = ViewportState::growing(4);
    app.global_ui.viewport.startup_welcome = true;
    make_messages(&mut app, "first message").await;
    let required = required_rows(&mut app, Rect::new(0, 0, 80, 40));
    let plan = app.global_ui.viewport.prepare(required, Size::new(80, 40));
    let mut terminal = Terminal::with_options(
        TestBackend::new(80, 40),
        TerminalOptions {
            viewport: Viewport::Fixed(plan.area),
        },
    )
    .expect("固定视口");
    terminal
        .draw(|frame| main_ui::render(frame, &mut app))
        .expect("绘制第一条消息");
    let messages = app
        .session_mgr
        .current()
        .ui
        .messages_area
        .expect("消息热区");
    assert_eq!(messages.y, 4, "欢迎与消息共用完整滚动区域");
    assert_eq!(
        app.session_mgr.current().ui.message_prefix_rows,
        welcome::welcome_height(&app, 80) as usize + 1
    );
    let input = app
        .session_mgr
        .current()
        .ui
        .textarea_area
        .expect("输入热区");
    assert_eq!(messages.bottom(), input.y);
    let buffer = terminal.backend().buffer();
    let gap_row = messages.y + welcome::welcome_height(&app, 80);
    assert!(
        (messages.x..messages.right()).all(|column| buffer[(column, gap_row)].symbol() == " "),
        "欢迎卡与首条消息之间保留一行空白"
    );
    let first_message: String = (messages.x..messages.right())
        .map(|column| buffer[(column, gap_row + 1)].symbol())
        .collect();
    assert!(first_message.contains("first message"), "消息紧随间隔行");
    let text: String = buffer.content.iter().map(|cell| cell.symbol()).collect();
    assert!(text.contains("CC Code"));
    assert_eq!(text.matches("first message").count(), 1, "消息只能出现一次");
    assert_eq!(
        app.session_mgr.current().messages.view_messages.len(),
        1,
        "欢迎不注入消息历史"
    );
}

#[tokio::test]
async fn test_width_reflow_precedes_height_decision_and_fullscreen_keeps_welcome() {
    let (mut app, _) = App::new_headless(80, 40).await;
    app.global_ui.viewport = ViewportState::growing(0);
    app.global_ui.viewport.startup_welcome = true;
    make_messages(&mut app, &"中文界面长内容 mixed words ".repeat(12)).await;
    let wide = required_rows(&mut app, Rect::new(0, 0, 80, 40));
    prepare_width(&mut app, 40).await;
    assert_eq!(
        app.session_mgr.current().messages.render_cache.read().width,
        40
    );
    let narrow = required_rows(&mut app, Rect::new(0, 0, 40, 40));
    assert!(narrow > wide, "中文换行后的视觉行数应参与高度判定");
    app.global_ui
        .viewport
        .prepare(narrow, Size::new(40, narrow));
    assert!(app.global_ui.viewport.fullscreen_latched);
    assert!(welcome_visible(&app), "全屏保留欢迎前缀并参与滚动");
    app.session_mgr.current_mut().messages.view_messages.clear();
    assert!(welcome_visible(&app), "全屏清空会话后仍显示紧凑欢迎内容");
    assert!(
        app.global_ui.viewport.fullscreen_latched,
        "清空内容不会解锁全屏"
    );
}

#[tokio::test]
async fn test_hint_popup_stays_inside_nonzero_viewport() {
    let (mut app, _) = App::new_headless(80, 40).await;
    app.global_ui.viewport = ViewportState::growing(6);
    app.session_mgr.current_mut().ui.textarea.insert_str("/");
    let required = required_rows(&mut app, Rect::new(0, 0, 80, 40));
    let plan = app.global_ui.viewport.prepare(required, Size::new(80, 40));
    let mut terminal = Terminal::with_options(
        TestBackend::new(80, 40),
        TerminalOptions {
            viewport: Viewport::Fixed(plan.area),
        },
    )
    .expect("固定视口");
    terminal
        .draw(|frame| main_ui::render(frame, &mut app))
        .expect("非零起点的候选浮层不越界");
    for row in 0..plan.area.y {
        assert!(
            (0..80).all(|col| terminal.backend().buffer()[(col, row)].symbol() == " "),
            "应用不能改写起点上方内容"
        );
    }
}

#[tokio::test]
async fn test_hint_popup_expand_then_dismiss_leaves_no_blank_gap() {
    // 覆盖不同终端高度，尤其测试高度 <= 27 的标准终端（如 24 行），确保不被假性全屏击穿
    for height in [24, 40] {
        let (mut app, _) = App::new_headless(80, height).await;
        app.global_ui.viewport = ViewportState::growing(0);
        let terminal_rect = Rect::new(0, 0, 80, height);

        // 1. 基线：无 `/` 时的 required_rows
        let baseline = required_rows(&mut app, terminal_rect);
        let plan_base = app
            .global_ui
            .viewport
            .prepare(baseline, Size::new(80, height));
        let mut term = Terminal::with_options(
            TestBackend::new(80, height),
            TerminalOptions {
                viewport: Viewport::Fixed(plan_base.area),
            },
        )
        .expect("固定视口");
        term.draw(|frame| main_ui::render(frame, &mut app))
            .expect("基线绘制");
        let initial_input_area = app
            .session_mgr
            .current()
            .ui
            .textarea_area
            .expect("输入框已分配");
        assert!(!app.global_ui.viewport.fullscreen_latched);

        // 2. 输入 `/` → 浮层出现：required_rows 不膨胀，不锁死全屏
        app.session_mgr.current_mut().ui.textarea.insert_str("/");
        let with_hint = required_rows(&mut app, terminal_rect);
        assert_eq!(
            with_hint, baseline,
            "命令提示浮层作为 Overlay 不撑大持久视口行数"
        );
        let plan_hint = app
            .global_ui
            .viewport
            .prepare(with_hint, Size::new(80, height));
        assert_eq!(plan_hint.area.height, baseline, "视口高度保持基线高度不变");
        assert!(
            !app.global_ui.viewport.fullscreen_latched,
            "输入 / 绝不触发全屏锁死"
        );
        term.resize(plan_hint.area).expect("调整视口");
        term.draw(|frame| main_ui::render(frame, &mut app))
            .expect("浮层绘制");

        // 3. 删除 `/` → 浮层消失：高度依然与基线严格一致，输入框位置完全不移动
        app.session_mgr.current_mut().ui.textarea.delete_char();
        let dismissed = required_rows(&mut app, terminal_rect);
        assert_eq!(dismissed, baseline, "删除后保持基线高度");
        let plan_dismiss = app
            .global_ui
            .viewport
            .prepare(dismissed, Size::new(80, height));
        assert_eq!(
            plan_dismiss.area.height, baseline,
            "视口高度完全不变，绝对不产生任何空白"
        );
        assert_eq!(
            app.global_ui.viewport.allocated_rows, baseline,
            "allocated_rows 不膨胀"
        );
        term.resize(plan_dismiss.area).expect("调整视口");
        term.draw(|frame| main_ui::render(frame, &mut app))
            .expect("恢复绘制");
        let final_input_area = app
            .session_mgr
            .current()
            .ui
            .textarea_area
            .expect("输入框已分配");
        assert_eq!(
            final_input_area.y, initial_input_area.y,
            "输入框 Y 坐标完全不移动，欢迎卡与输入框之间绝无空白行"
        );
    }
}

#[tokio::test]
async fn test_wrapped_todo_and_recap_use_actual_visual_rows() {
    let (mut app, _) = App::new_headless(80, 40).await;
    make_messages(&mut app, "question").await;
    app.session_mgr.current_mut().latest_recap = Some("长摘要 ".repeat(50));
    let lines = message_area::visible_tail(&app);
    assert!(
        message_area::visual_line_count(&lines, 40) > lines.len(),
        "摘要必须按显示列换行计数"
    );
    app.session_mgr.current_mut().ui.loading = true;
    app.session_mgr
        .current_mut()
        .todo_items
        .push(cc_middlewares::prelude::TodoItem {
            content: "很长的待办事项 ".repeat(30),
            status: cc_middlewares::prelude::TodoStatus::InProgress,
            active_form: Some("执行中".into()),
        });
    let lines = message_area::visible_tail(&app);
    assert!(
        message_area::visual_line_count(&lines, 40) > lines.len(),
        "待办项必须按显示列换行计数"
    );
}

#[tokio::test]
async fn test_empty_viewport_measures_and_renders_activity() {
    for subagent in [false, true] {
        let (mut app, _) = App::new_headless(80, 40).await;
        app.global_ui.viewport = ViewportState::growing(3);
        let idle = required_rows(&mut app, Rect::new(0, 0, 80, 40));
        if subagent {
            app.handle_subagent_lifecycle(&serde_json::json!({
                "event": "started", "instanceId": "child", "agentType": "general-purpose",
                "taskPreview": "empty active child", "isBackground": true,
            }));
            app.session_mgr.current_mut().focused_instance_id = Some("child".into());
        } else {
            app.session_mgr.current_mut().latest_recap = Some("empty activity recap".into());
        }
        let required = required_rows(&mut app, Rect::new(0, 0, 80, 40));
        assert!(required > idle, "无正文时仍需为活动附属内容预留行数");
        let tail = message_area::visible_tail(&app);
        let marker = if subagent {
            app.session_mgr
                .current()
                .viewport_spinner()
                .verb()
                .to_string()
        } else {
            "empty activity recap".to_string()
        };
        assert!(!tail.is_empty(), "活动附属行必须存在");
        let plan = app.global_ui.viewport.prepare(required, Size::new(80, 40));
        let mut terminal = Terminal::with_options(
            TestBackend::new(80, 40),
            TerminalOptions {
                viewport: Viewport::Fixed(plan.area),
            },
        )
        .expect("固定视口");
        terminal
            .draw(|frame| main_ui::render(frame, &mut app))
            .expect("绘制空正文活动");
        let text: String = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(text.contains("CC Code"), "欢迎卡仍应可见");
        assert!(text.contains(&marker), "无正文时活动内容也应绘制");
        assert!(
            app.session_mgr.current().current_view_messages().is_empty(),
            "绘制不写消息历史"
        );
    }
}

#[tokio::test]
async fn test_tiny_terminal_renders_without_coordinates_outside_frame() {
    for (width, height) in [(1, 1), (2, 2), (10, 3), (40, 8)] {
        let (mut app, _) = App::new_headless(width, height).await;
        app.global_ui.viewport = ViewportState::growing(0);
        app.global_ui.viewport.terminal_size = Size::new(width, height);
        let required = required_rows(&mut app, Rect::new(0, 0, width, height));
        let plan = app
            .global_ui
            .viewport
            .prepare(required, Size::new(width, height));
        assert!(
            app.global_ui.viewport.fullscreen_latched,
            "首帧无法容纳时应全屏"
        );
        let mut terminal = Terminal::with_options(
            TestBackend::new(width, height),
            TerminalOptions {
                viewport: Viewport::Fixed(plan.area),
            },
        )
        .expect("极小视口");
        terminal
            .draw(|frame| main_ui::render(frame, &mut app))
            .expect("极小终端不应越界");
        let input = app
            .session_mgr
            .current()
            .ui
            .textarea_area
            .expect("输入区域");
        assert!(input.right() <= width && input.bottom() <= height);
    }
}

#[tokio::test]
async fn test_panel_uses_host_height_and_closing_shrinks_back() {
    let (mut app, _) = App::new_headless(80, 40).await;
    app.global_ui.viewport = ViewportState::growing(2);
    let host = Rect::new(0, 0, 80, 40);
    let initial = required_rows(&mut app, host);
    app.global_ui.viewport.prepare(initial, Size::new(80, 40));
    app.open_memory_panel();
    let panel_rows = BottomHeights::measure(&app, host).panel;
    assert!(
        panel_rows > initial * 3 / 5,
        "面板不能被当前紧凑区域递归压缩"
    );
    let required = required_rows(&mut app, host);
    let transient = BottomHeights::measure(&app, host).panel;
    let expanded =
        app.global_ui
            .viewport
            .prepare_with_transient(required, transient, Size::new(80, 40));
    app.close_memory_panel();
    let required = required_rows(&mut app, host);
    let transient = BottomHeights::measure(&app, host).panel;
    let closed =
        app.global_ui
            .viewport
            .prepare_with_transient(required, transient, Size::new(80, 40));
    assert!(
        closed.area.height < expanded.area.height,
        "关闭面板后视口应回收面板高度，页面继续紧贴内容"
    );
    assert_eq!(
        closed.area.height, initial,
        "关闭面板后高度恢复到打开前的紧凑高度"
    );
    assert!(
        closed.shrink_top + closed.shrink_bottom > 0,
        "回收行必须上报给绘制层清理，避免留白"
    );
}

#[tokio::test]
async fn test_fullscreen_transition_preserves_welcome_and_message_history() {
    let (mut app, _) = App::new_headless(80, 40).await;
    app.global_ui.viewport = ViewportState::growing(38);
    app.global_ui.viewport.startup_welcome = true;
    make_messages(&mut app, "first message\nsecond message").await;
    let required = required_rows(&mut app, Rect::new(0, 0, 80, 40));
    let initial = app.global_ui.viewport.prepare(required, Size::new(80, 40));
    assert!(!initial.enter_fullscreen, "靠近屏底不能提前切全屏");
    let plan = app
        .global_ui
        .viewport
        .prepare(required, Size::new(80, required));
    assert!(plan.enter_fullscreen, "已占用高度达到窗口高度时切换");
    let mut terminal = Terminal::with_options(
        TestBackend::new(80, required),
        TerminalOptions {
            viewport: Viewport::Fixed(plan.area),
        },
    )
    .expect("全屏视口");
    terminal
        .draw(|frame| main_ui::render(frame, &mut app))
        .expect("切换后首帧");
    let text: String = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect();
    assert!(text.contains("CC Code"), "切换后的首帧保留欢迎卡");
    assert!(text.contains("first message"), "切换后的首帧保留原消息");
    assert_eq!(
        app.session_mgr.current().ui.scroll_offset,
        0,
        "内容恰好容纳时无需滚动"
    );
    assert_eq!(
        app.session_mgr.current().messages.view_messages.len(),
        1,
        "欢迎不写入历史"
    );
    assert!(
        !app.session_mgr
            .current()
            .messages
            .render_cache
            .read()
            .lines
            .iter()
            .any(|line| line.to_string().contains("CC Code")),
        "欢迎不写入消息缓存"
    );
}

#[tokio::test]
async fn test_welcome_message_and_tail_scroll_as_continuous_content() {
    use ratatui::{
        text::Text,
        widgets::{Paragraph, Wrap},
    };
    for width in [80, 56] {
        let (mut app, mut handle) = App::new_headless(width, 40).await;
        app.global_ui.viewport.startup_welcome = true;
        make_messages(
            &mut app,
            &(0..12)
                .map(|row| format!("message row {row:02}\n"))
                .collect::<String>(),
        )
        .await;
        app.session_mgr.current_mut().latest_recap = Some("tail recap".into());
        let text_width = width - 1;
        let (done, ready) = tokio::sync::oneshot::channel();
        app.session_mgr
            .current()
            .messages
            .render_tx
            .send(crate::ui::render_thread::RenderEvent::ResizeAndWait(
                text_width, done,
            ))
            .await
            .expect("同步正文宽度");
        ready.await.expect("正文宽度就绪");
        app.session_mgr.current_mut().messages.last_resize_width = Some(text_width);
        let prefix = welcome_rows(&app, text_width);
        let mut lines = app
            .session_mgr
            .current()
            .messages
            .render_cache
            .read()
            .lines
            .clone();
        lines.extend(message_area::visible_tail(&app));
        let total = prefix + message_area::visual_line_count(&lines, text_width);
        assert!(
            total < 40 && total > 8,
            "完整参考内容应能放入参考帧并超过滚动视口"
        );
        handle
            .terminal
            .draw(|frame| {
                welcome::render_welcome_slice(
                    frame,
                    &app,
                    Rect::new(0, 0, text_width, prefix as u16),
                    0,
                );
                frame.render_widget(
                    Paragraph::new(Text::from(lines)).wrap(Wrap { trim: false }),
                    Rect::new(0, prefix as u16, text_width, 40 - prefix as u16),
                );
            })
            .expect("绘制完整内容参考帧");
        let complete = handle.terminal.backend().buffer().clone();
        for offset in 0..=total - 8 {
            app.session_mgr
                .current()
                .messages
                .render_cache
                .write()
                .scroll_anchor =
                (offset < prefix).then_some(crate::ui::render_thread::ScrollAnchor::default());
            app.session_mgr.current_mut().ui.scroll_follow = false;
            app.session_mgr.current_mut().ui.scroll_offset = offset;
            handle
                .terminal
                .draw(|frame| {
                    message_area::render_messages(
                        frame,
                        &mut app,
                        Rect::default(),
                        Rect::new(0, 3, width, 8),
                    )
                })
                .expect("绘制滚动切片");
            assert_eq!(
                app.session_mgr.current().ui.scroll_offset,
                offset,
                "浏览欢迎时消息重建不能推走前缀"
            );
            assert_eq!(app.session_mgr.current().ui.message_prefix_rows, prefix);
            for row in 0..8 {
                for column in 0..text_width {
                    assert_eq!(
                        handle.terminal.backend().buffer()[(column, 3 + row)],
                        complete[(column, offset as u16 + row)],
                        "宽度{width}偏移{offset}必须与完整内容连续对应"
                    );
                }
            }
        }
    }
}

#[tokio::test]
async fn test_width_preparation_recovers_full_width_at_mascot_boundary() {
    let (mut app, _) = App::new_headless(56, 40).await;
    app.global_ui.viewport = ViewportState::growing(0);
    app.global_ui.viewport.startup_welcome = true;
    make_messages(&mut app, &"x".repeat(54)).await;
    prepare_width(&mut app, 55).await;
    assert_eq!(
        app.session_mgr
            .current()
            .messages
            .render_cache
            .read()
            .total_lines,
        2,
        "窄屏正文分成两行"
    );
    let height = welcome_rows(&app, 56) as u16
        + 1
        + BottomHeights::measure(&app, Rect::new(0, 0, 56, 40)).total();
    app.global_ui.viewport.terminal_size = Size::new(56, height);
    app.global_ui.viewport.fullscreen_latched = true;
    let mut terminal = Terminal::new(TestBackend::new(56, height)).expect("全宽临界帧");
    for _ in 0..3 {
        prepare_width(&mut app, 56).await;
        assert_eq!(
            app.session_mgr.current().messages.render_cache.read().width,
            56,
            "全宽恰好容纳时不能沿用窄缓存预测"
        );
        terminal
            .draw(|frame| main_ui::render(frame, &mut app))
            .expect("临界宽度首帧");
        assert!(
            app.session_mgr
                .current()
                .ui
                .message_scrollbar_area
                .is_none(),
            "全宽恰好容纳，无需滚动条"
        );
        assert_eq!(
            app.session_mgr.current().ui.message_prefix_rows,
            8,
            "恢复宽度后保留完整吉祥物与消息间距"
        );
    }
    make_messages(&mut app, &"many message rows\n".repeat(30)).await;
    prepare_width(&mut app, 56).await;
    let version = app
        .session_mgr
        .current()
        .messages
        .render_cache
        .read()
        .version;
    assert_eq!(
        app.session_mgr.current().messages.render_cache.read().width,
        55
    );
    for _ in 0..3 {
        prepare_width(&mut app, 56).await;
        assert_eq!(
            app.session_mgr
                .current()
                .messages
                .render_cache
                .read()
                .version,
            version,
            "内容不变时不能反复宽窄重排"
        );
    }
}
