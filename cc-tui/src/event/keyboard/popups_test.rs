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
