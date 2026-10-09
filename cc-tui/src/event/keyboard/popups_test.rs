use super::*;
use crate::app::{ApprovalChoice, HitlBatchPrompt, InteractionPrompt};
use cc_middlewares::hitl::{BatchItem, HitlDecision};

fn make_input(key: Key, shift: bool) -> Input {
    Input {
        key,
        ctrl: false,
        alt: false,
        shift,
    }
}

#[tokio::test]
async fn test_approval_keyboard_arrows_choose_and_tab_switches_tools() {
    let (mut app, _) = App::new_headless(100, 30).await;
    let (sender, _) = tokio::sync::oneshot::channel();
    let items = vec!["Bash", "Edit"]
        .into_iter()
        .map(|tool_name| BatchItem {
            tool_name: tool_name.to_string(),
            input: serde_json::json!({}),
        })
        .collect();
    app.session_mgr.current_mut().agent.interaction_prompt = Some(InteractionPrompt::Approval(
        HitlBatchPrompt::new(items, sender),
    ));
    handle_popups(&mut app, &make_input(Key::Down, false));
    handle_popups(&mut app, &make_input(Key::Tab, false));
    handle_popups(&mut app, &make_input(Key::Down, false));
    handle_popups(&mut app, &make_input(Key::Down, false));
    handle_popups(&mut app, &make_input(Key::Tab, true));
    handle_popups(&mut app, &make_input(Key::Char(' '), false));
    let Some(InteractionPrompt::Approval(prompt)) =
        &app.session_mgr.current().agent.interaction_prompt
    else {
        panic!("审批弹窗应仍存在");
    };
    assert_eq!(prompt.cursor, 0);
    assert_eq!(
        prompt.choices,
        [ApprovalChoice::Session, ApprovalChoice::Reject],
        "上下键选择、Tab 切换且保留每项选择，空格不再循环"
    );
}

#[tokio::test]
async fn test_approval_keyboard_enter_submits_selected_rejection() {
    let (mut app, _) = App::new_headless(100, 30).await;
    let (sender, receiver) = tokio::sync::oneshot::channel();
    let items = vec![BatchItem {
        tool_name: "Bash".to_string(),
        input: serde_json::json!({"command":"custom-build"}),
    }];
    app.session_mgr.current_mut().agent.interaction_prompt = Some(InteractionPrompt::Approval(
        HitlBatchPrompt::new(items, sender),
    ));
    handle_popups(&mut app, &make_input(Key::Down, false));
    handle_popups(&mut app, &make_input(Key::Down, false));
    handle_popups(&mut app, &make_input(Key::Enter, false));
    assert!(
        matches!(receiver.await, Ok(decisions) if matches!(decisions.as_slice(), [HitlDecision::Reject])),
        "Enter 应提交当前选择的拒绝"
    );
    assert!(app.session_mgr.current().agent.interaction_prompt.is_none());
}

#[tokio::test]
async fn test_approval_keyboard_long_script_arrows_do_not_scroll_commands() {
    let (mut app, mut handle) = App::new_headless(120, 20).await;
    let (sender, receiver) = tokio::sync::oneshot::channel();
    let command = (0..40)
        .map(|index| format!("printf visible_line_{index:02}"))
        .collect::<Vec<_>>()
        .join("\n");
    app.services.lc = crate::i18n::LcRegistry::new(Some("en"));
    app.session_mgr.current_mut().agent.interaction_prompt =
        Some(InteractionPrompt::Approval(HitlBatchPrompt::new(
            vec![BatchItem {
                tool_name: "Bash".to_string(),
                input: serde_json::json!({"command":command}),
            }],
            sender,
        )));
    handle
        .terminal
        .draw(|frame| crate::ui::main_ui::render(frame, &mut app))
        .expect("初始审批面板应渲染成功");
    let initial_command_lines: Vec<_> = handle
        .snapshot()
        .into_iter()
        .filter(|line| line.contains("printf visible_line_"))
        .collect();
    assert!(!initial_command_lines.is_empty(), "长脚本应仍展示首部内容");
    for key in [Key::Down, Key::Down, Key::Up, Key::Char(' ')] {
        handle_popups(&mut app, &make_input(key, false));
        handle
            .terminal
            .draw(|frame| crate::ui::main_ui::render(frame, &mut app))
            .expect("选择审批项后应渲染成功");
        let command_lines: Vec<_> = handle
            .snapshot()
            .into_iter()
            .filter(|line| line.contains("printf visible_line_"))
            .collect();
        assert_eq!(
            command_lines, initial_command_lines,
            "上下键只选择审批项，不能抢占为命令滚动"
        );
    }
    let Some(InteractionPrompt::Approval(prompt)) =
        &app.session_mgr.current().agent.interaction_prompt
    else {
        panic!("选择过程中审批应仍存在");
    };
    assert_eq!(prompt.choices, [ApprovalChoice::Session]);
    assert_eq!(
        prompt.choices[0].option_id(),
        "allow_always",
        "本次会话同意的 ACP 选项 ID 必须保持原样"
    );
    handle_popups(&mut app, &make_input(Key::Enter, false));
    assert!(
        matches!(receiver.await, Ok(decisions) if matches!(decisions.as_slice(), [HitlDecision::Approve])),
        "选择本次会话同意后 Enter 仍应提交放行结果"
    );
}

#[tokio::test]
async fn test_approval_keyboard_enter_preserves_mixed_batch_decisions() {
    let (mut app, _) = App::new_headless(120, 30).await;
    let (sender, receiver) = tokio::sync::oneshot::channel();
    let items = (0..3)
        .map(|index| BatchItem {
            tool_name: "Bash".to_string(),
            input: serde_json::json!({"command":format!("printf item_{index}")}),
        })
        .collect();
    app.session_mgr.current_mut().agent.interaction_prompt = Some(InteractionPrompt::Approval(
        HitlBatchPrompt::new(items, sender),
    ));
    handle_popups(&mut app, &make_input(Key::Tab, false));
    handle_popups(&mut app, &make_input(Key::Down, false));
    handle_popups(&mut app, &make_input(Key::Tab, false));
    handle_popups(&mut app, &make_input(Key::Down, false));
    handle_popups(&mut app, &make_input(Key::Down, false));
    handle_popups(&mut app, &make_input(Key::Enter, false));
    assert!(
        matches!(receiver.await, Ok(decisions) if matches!(decisions.as_slice(), [HitlDecision::Approve, HitlDecision::Approve, HitlDecision::Reject])),
        "Enter 应按顺序提交本次同意、会话同意和拒绝，不改变结果语义"
    );
    assert!(app.session_mgr.current().agent.interaction_prompt.is_none());
}

#[tokio::test]
async fn test_approval_keyboard_escape_rejects_entire_batch() {
    let (mut app, _) = App::new_headless(120, 30).await;
    let (sender, receiver) = tokio::sync::oneshot::channel();
    let items = (0..3)
        .map(|index| BatchItem {
            tool_name: "Bash".to_string(),
            input: serde_json::json!({"command":format!("printf item_{index}")}),
        })
        .collect();
    app.session_mgr.current_mut().agent.interaction_prompt = Some(InteractionPrompt::Approval(
        HitlBatchPrompt::new(items, sender),
    ));
    handle_popups(&mut app, &make_input(Key::Down, false));
    handle_popups(&mut app, &make_input(Key::Tab, false));
    handle_popups(&mut app, &make_input(Key::Esc, false));
    assert!(
        matches!(receiver.await, Ok(decisions) if decisions.len() == 3 && decisions.iter().all(|decision| matches!(decision, HitlDecision::Reject))),
        "Esc 应拒绝全部工具，不能留下原本同意的工具"
    );
    assert!(app.session_mgr.current().agent.interaction_prompt.is_none());
}
