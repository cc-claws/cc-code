use super::*;

fn make_prompt(count: usize) -> HitlBatchPrompt {
    let (sender, _) = tokio::sync::oneshot::channel();
    let items = (0..count)
        .map(|index| BatchItem {
            tool_name: format!("Tool{index}"),
            input: serde_json::json!({}),
        })
        .collect();
    HitlBatchPrompt::new(items, sender)
}

#[test]
fn test_approval_choices_follow_arrows_and_stop_at_boundaries() {
    let mut prompt = make_prompt(1);
    prompt.move_choice(-1);
    assert_eq!(prompt.choices, [ApprovalChoice::Once]);
    prompt.move_choice(1);
    assert_eq!(prompt.choices, [ApprovalChoice::Session]);
    prompt.move_choice(1);
    assert_eq!(prompt.choices, [ApprovalChoice::Reject]);
    prompt.move_choice(1);
    assert_eq!(
        prompt.choices,
        [ApprovalChoice::Reject],
        "末项不能意外循环到同意"
    );
    prompt.move_choice(-1);
    assert_eq!(prompt.choices, [ApprovalChoice::Session]);
}

#[test]
fn test_approval_tool_navigation_preserves_each_choice() {
    let mut prompt = make_prompt(2);
    prompt.move_choice(1);
    prompt.move_cursor(1);
    prompt.move_choice(2);
    assert_eq!(
        prompt.choices,
        [ApprovalChoice::Session, ApprovalChoice::Reject]
    );
    prompt.move_cursor(-1);
    assert_eq!(prompt.cursor, 0);
    assert_eq!(prompt.choices[0], ApprovalChoice::Session);
    prompt.move_cursor(-1);
    assert_eq!(prompt.cursor, 1, "反向切换支持从首项回到末项");
}

#[test]
fn test_approval_scroll_follows_selected_option_row() {
    let mut prompt = make_prompt(4);
    prompt.last_visible_height = 5;
    prompt.move_cursor(3);
    prompt.move_choice(2);
    let selected_row = 3 * 5 + 4;
    assert!(selected_row >= prompt.scroll_offset as usize);
    assert!(selected_row < prompt.scroll_offset as usize + 5);
    prompt.move_cursor(1);
    assert_eq!(prompt.cursor, 0);
    assert_eq!(prompt.scroll_offset, 0);
}

#[test]
fn test_approval_single_item_does_not_scroll_when_all_options_fit() {
    let mut prompt = make_prompt(1);
    prompt.last_visible_height = 5;
    prompt.move_choice(2);
    assert_eq!(prompt.scroll_offset, 0, "内容可见时应完整保留三个选项");
}

#[test]
fn test_approval_empty_prompt_navigation_is_safe() {
    let mut prompt = make_prompt(0);
    prompt.move_cursor(1);
    prompt.move_choice(1);
    assert_eq!(prompt.cursor, 0);
    assert!(prompt.choices.is_empty());
}

#[test]
fn test_approval_option_identifiers_preserve_session_scope() {
    assert_eq!(ApprovalChoice::Once.option_id(), "allow_once");
    assert_eq!(ApprovalChoice::Session.option_id(), "allow_always");
    assert_eq!(ApprovalChoice::Reject.option_id(), "reject_once");
}
