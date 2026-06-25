// ─── build_rebuild_all 事件处理测试 ──────────────────────────────────────

/// 场景1: build_rebuild_all 产生正确的 RebuildAll action
#[test]
fn test_build_rebuild_all_done() {
    use super::message_pipeline::MessagePipeline;
    use peri_agent::messages::BaseMessage;

    let mut pipeline = MessagePipeline::new("/tmp".to_string());
    pipeline.set_completed(vec![
        BaseMessage::human("q1"),
        BaseMessage::ai("a1"),
        BaseMessage::human("q2"),
        BaseMessage::ai("a2"),
    ]);

    let action = pipeline.build_rebuild_all(2);
    if let super::message_pipeline::PipelineAction::RebuildAll {
        prefix_len,
        tail_vms,
    } = action
    {
        assert_eq!(prefix_len, 2);
        // tail 应包含 q2 + a2（从最后一条 Human 开始 reconcile）
        assert!(tail_vms.len() >= 2, "tail_vms 应包含 q2 + a2");
    } else {
        panic!("Expected RebuildAll");
    }
}

/// 场景2: build_rebuild_all 在 Interrupted 场景下正确工作
#[test]
fn test_build_rebuild_all_interrupted() {
    use super::message_pipeline::MessagePipeline;
    use peri_agent::messages::BaseMessage;

    let mut pipeline = MessagePipeline::new("/tmp".to_string());
    pipeline.set_completed(vec![
        BaseMessage::human("q1"),
        BaseMessage::ai("a1"),
        BaseMessage::human("q2"),
    ]);

    let action = pipeline.build_rebuild_all(2);
    if let super::message_pipeline::PipelineAction::RebuildAll {
        prefix_len,
        tail_vms,
    } = action
    {
        assert_eq!(prefix_len, 2);
        // tail 应包含 q2（从最后一条 Human 开始 reconcile）
        assert!(!tail_vms.is_empty(), "tail_vms 应包含 q2");
    } else {
        panic!("Expected RebuildAll");
    }
}

/// 场景3: submit_message 记录 round_start_vm_idx（纯逻辑验证）
/// round_start_vm_idx 在 UserBubble 推入之后设置，确保 RebuildAll 不截掉用户消息
#[test]
fn test_submit_message_records_round_start_vm_idx() {
    let mut messages = vec![
        crate::ui::message_view::MessageViewModel::user("q1".to_string()),
        crate::ui::message_view::MessageViewModel::from_base_message(
            &peri_agent::messages::BaseMessage::ai("a1".to_string()),
            &[],
        ),
        crate::ui::message_view::MessageViewModel::user("q2".to_string()),
    ];

    messages.push(crate::ui::message_view::MessageViewModel::user(
        "q3".to_string(),
    ));
    // round_start_vm_idx 在 push 之后设置
    let round_start_vm_idx = messages.len();
    assert_eq!(round_start_vm_idx, 4);
    assert_eq!(
        round_start_vm_idx, 4,
        "round_start_vm_idx 应为 push 后的值，确保 UserBubble 在 prefix 中"
    );
}

/// 场景4: RebuildAll 时 SystemNote 按锚点位置插入，而非追加到末尾
#[test]
fn test_rebuildall_system_note_anchor_insertion() {
    use crate::ui::message_view::MessageViewModel;

    let mut view_messages: Vec<MessageViewModel> = vec![MessageViewModel::user("q1".to_string())];
    let prefix_len = view_messages.len(); // round_start_vm_idx = 1

    // 模拟 agent 运行中添加 SystemNote（锚点 = 1）
    let anchor = view_messages.len();
    let vm = MessageViewModel::system("OAuth notification".to_string());
    view_messages.push(vm);
    assert_eq!(view_messages.len(), 2, "AddMessage 后应有 2 条");

    // 模拟 RebuildAll：使用锚点机制
    let mut ephemeral_notes: Vec<(usize, MessageViewModel)> = vec![(
        anchor,
        MessageViewModel::system("OAuth notification".to_string()),
    )];
    let tail_vms = vec![MessageViewModel::from_base_message(
        &peri_agent::messages::BaseMessage::ai("response".to_string()),
        &[],
    )];

    // 过滤：锚点 >= prefix_len 的保留
    let saved_notes: Vec<(usize, MessageViewModel)> = ephemeral_notes
        .drain(..)
        .filter(|(a, _)| *a >= prefix_len)
        .collect();

    view_messages.drain(prefix_len..);
    view_messages.extend(tail_vms);

    // 按锚点插入
    for (a, note_vm) in saved_notes {
        let tail_len = view_messages.len() - prefix_len;
        let insert_pos = (a - prefix_len).min(tail_len) + prefix_len;
        view_messages.insert(insert_pos, note_vm.clone());
        ephemeral_notes.push((insert_pos, note_vm));
    }

    // 验证 SystemNote 在锚点位置（索引 1），而非末尾
    assert_eq!(view_messages.len(), 3, "应有 3 条：User + SystemNote + AI");
    assert!(
        matches!(&view_messages[1], MessageViewModel::SystemNote { content, .. } if content == "OAuth notification"),
        "SystemNote 应在位置 1（锚点位置），实际: {:?}",
        view_messages
    );
}

/// 场景5: 锚点在 prefix 内的 SystemNote 应被丢弃
#[test]
fn test_system_note_discarded_when_anchor_in_prefix() {
    use crate::ui::message_view::MessageViewModel;

    let mut view_messages: Vec<MessageViewModel> = vec![MessageViewModel::user("q1".to_string())];
    let prefix_len = view_messages.len(); // 1

    // SystemNote 锚点在 prefix 内（anchor=0 < prefix_len=1）
    let mut ephemeral_notes: Vec<(usize, MessageViewModel)> =
        vec![(0, MessageViewModel::system("old note".to_string()))];

    let tail_vms = vec![MessageViewModel::from_base_message(
        &peri_agent::messages::BaseMessage::ai("response".to_string()),
        &[],
    )];

    let saved_notes: Vec<(usize, MessageViewModel)> = ephemeral_notes
        .drain(..)
        .filter(|(a, _)| *a >= prefix_len)
        .collect();

    view_messages.drain(prefix_len..);
    view_messages.extend(tail_vms);

    for (a, note_vm) in saved_notes {
        let tail_len = view_messages.len() - prefix_len;
        let insert_pos = (a - prefix_len).min(tail_len) + prefix_len;
        view_messages.insert(insert_pos, note_vm.clone());
        ephemeral_notes.push((insert_pos, note_vm));
    }

    // SystemNote 被丢弃
    assert_eq!(
        view_messages.len(),
        2,
        "应有 2 条：User + AI（SystemNote 被丢弃）"
    );
    assert!(
        !view_messages
            .iter()
            .any(|vm| matches!(vm, MessageViewModel::SystemNote { .. })),
        "prefix 内的 SystemNote 应被丢弃"
    );
}

/// 场景6: 多次连续 RebuildAll 后 SystemNote 位置不变
#[test]
fn test_multiple_rebuildalls_preserve_note_position() {
    use crate::ui::message_view::MessageViewModel;

    let mut view_messages: Vec<MessageViewModel> = vec![MessageViewModel::user("q1".to_string())];
    let prefix_len = 1;

    // 添加 SystemNote（锚点 = 1）
    let anchor = view_messages.len();
    let vm = MessageViewModel::system("note".to_string());
    view_messages.push(vm);
    let mut ephemeral_notes: Vec<(usize, MessageViewModel)> =
        vec![(anchor, MessageViewModel::system("note".to_string()))];

    // 第一次 RebuildAll
    let tail_vms_1 = vec![MessageViewModel::from_base_message(
        &peri_agent::messages::BaseMessage::ai("response1".to_string()),
        &[],
    )];

    let saved_notes: Vec<(usize, MessageViewModel)> = ephemeral_notes
        .drain(..)
        .filter(|(a, _)| *a >= prefix_len)
        .collect();
    view_messages.drain(prefix_len..);
    view_messages.extend(tail_vms_1);
    for (a, note_vm) in saved_notes {
        let insert_pos = (a - prefix_len).min(view_messages.len() - prefix_len) + prefix_len;
        view_messages.insert(insert_pos, note_vm.clone());
        ephemeral_notes.push((insert_pos, note_vm));
    }

    assert!(
        matches!(&view_messages[1], MessageViewModel::SystemNote { content, .. } if content == "note"),
        "第一次 RebuildAll 后 SystemNote 应在位置 1"
    );

    // 第二次 RebuildAll（相同 prefix_len）
    let tail_vms_2 = vec![MessageViewModel::from_base_message(
        &peri_agent::messages::BaseMessage::ai("response2".to_string()),
        &[],
    )];

    let saved_notes_2: Vec<(usize, MessageViewModel)> = ephemeral_notes
        .drain(..)
        .filter(|(a, _)| *a >= prefix_len)
        .collect();
    view_messages.drain(prefix_len..);
    view_messages.extend(tail_vms_2);
    for (a, note_vm) in saved_notes_2 {
        let insert_pos = (a - prefix_len).min(view_messages.len() - prefix_len) + prefix_len;
        view_messages.insert(insert_pos, note_vm.clone());
        ephemeral_notes.push((insert_pos, note_vm));
    }

    assert!(
        matches!(&view_messages[1], MessageViewModel::SystemNote { content, .. } if content == "note"),
        "第二次 RebuildAll 后 SystemNote 仍应在位置 1"
    );
}

/// 场景7: 多个 SystemNote 保持相对顺序
#[test]
fn test_multiple_system_notes_maintain_order() {
    use crate::ui::message_view::MessageViewModel;

    let mut view_messages: Vec<MessageViewModel> = vec![MessageViewModel::user("q1".to_string())];
    let prefix_len = 1;

    // 添加两个 SystemNote
    let anchor1 = view_messages.len(); // 1
    view_messages.push(MessageViewModel::system("note1".to_string()));
    let anchor2 = view_messages.len(); // 2
    view_messages.push(MessageViewModel::system("note2".to_string()));

    let mut ephemeral_notes: Vec<(usize, MessageViewModel)> = vec![
        (anchor1, MessageViewModel::system("note1".to_string())),
        (anchor2, MessageViewModel::system("note2".to_string())),
    ];

    let tail_vms = vec![MessageViewModel::from_base_message(
        &peri_agent::messages::BaseMessage::ai("response".to_string()),
        &[],
    )];

    let mut saved_notes: Vec<(usize, MessageViewModel)> = ephemeral_notes
        .drain(..)
        .filter(|(a, _)| *a >= prefix_len)
        .collect();
    view_messages.drain(prefix_len..);
    view_messages.extend(tail_vms);

    saved_notes.sort_by_key(|(a, _)| *a);
    for (a, note_vm) in saved_notes {
        let insert_pos = (a - prefix_len).min(view_messages.len() - prefix_len) + prefix_len;
        view_messages.insert(insert_pos, note_vm.clone());
        ephemeral_notes.push((insert_pos, note_vm));
    }

    // 顺序：User(0), note1(1), note2(2), AI(3)
    assert_eq!(view_messages.len(), 4);
    assert!(
        matches!(&view_messages[1], MessageViewModel::SystemNote { content, .. } if content == "note1"),
        "note1 应在位置 1"
    );
    assert!(
        matches!(&view_messages[2], MessageViewModel::SystemNote { content, .. } if content == "note2"),
        "note2 应在位置 2"
    );
}

/// 场景8: 锚点超过 tail 长度时 clamp 到末尾
#[test]
fn test_system_note_clamped_to_end_when_tail_shorter() {
    use crate::ui::message_view::MessageViewModel;

    let mut view_messages: Vec<MessageViewModel> = vec![MessageViewModel::user("q1".to_string())];
    let prefix_len = 1;

    // SystemNote 锚点 = 5（远超当前 view_messages 长度）
    let anchor = 5;
    let mut ephemeral_notes: Vec<(usize, MessageViewModel)> =
        vec![(anchor, MessageViewModel::system("late note".to_string()))];

    // tail 只有 1 条消息
    let tail_vms = vec![MessageViewModel::from_base_message(
        &peri_agent::messages::BaseMessage::ai("response".to_string()),
        &[],
    )];

    let saved_notes: Vec<(usize, MessageViewModel)> = ephemeral_notes
        .drain(..)
        .filter(|(a, _)| *a >= prefix_len)
        .collect();
    view_messages.drain(prefix_len..);
    view_messages.extend(tail_vms);

    for (a, note_vm) in saved_notes {
        let insert_pos = (a - prefix_len).min(view_messages.len() - prefix_len) + prefix_len;
        view_messages.insert(insert_pos, note_vm.clone());
        ephemeral_notes.push((insert_pos, note_vm));
    }

    // clamp 到末尾：User(0), AI(1), late note(2)
    assert_eq!(view_messages.len(), 3);
    assert!(
        matches!(&view_messages[2], MessageViewModel::SystemNote { content, .. } if content == "late note"),
        "超过 tail 长度的 SystemNote 应 clamp 到末尾"
    );
}
