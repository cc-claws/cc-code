use super::*;
use cc_agent::messages::{BaseMessage, ContentBlock, MessageContent};

fn make_display_messages() -> Vec<MessageViewModel> {
    let message = BaseMessage::ai(MessageContent::blocks(vec![
        ContentBlock::Reasoning {
            text: (0..6).map(|i| format!("静态推理第 {i} 行\n")).collect(),
            signature: None,
            duration_ms: Some(1000),
        },
        ContentBlock::text("静态回答"),
    ]));
    let reasoning = MessageViewModel::from_base_message_with_cwd(&message, &[], None);
    let mut write = MessageViewModel::tool_block(
        "write-display".into(),
        "Write".into(),
        Some("file.rs".into()),
        false,
    );
    if let MessageViewModel::ToolBlock {
        diff_input,
        content,
        ..
    } = &mut write
    {
        *content = "File created successfully".into();
        *diff_input = Some(cc_widgets::DiffInput {
            file_path: "file.rs".into(),
            old_content: String::new(),
            new_content: "display-specific-diff".into(),
            is_new_file: true,
            is_deleted_file: false,
            is_binary: false,
        });
    }
    write.recompute_hash();
    let mut group = MessageViewModel::subagent_group("child".into(), "静态子任务".into());
    if let MessageViewModel::SubAgentGroup {
        recent_messages,
        is_running,
        total_steps,
        ..
    } = &mut group
    {
        *recent_messages = vec![reasoning.clone(), write.clone()];
        *is_running = false;
        *total_steps = 2;
    }
    group.recompute_hash();
    vec![reasoning, write, group]
}

async fn make_display_barrier(sender: &mpsc::Sender<RenderEvent>) {
    let (done, ready) = tokio::sync::oneshot::channel();
    // 宽度保持 80，不用强制重排掩盖旧 VM 的 cosmetic 缓存复用错误。
    sender
        .send(RenderEvent::ResizeAndWait(80, done))
        .await
        .expect("发送屏障");
    tokio::time::timeout(Duration::from_secs(3), ready)
        .await
        .expect("渲染应及时完成")
        .expect("渲染线程仍存活");
}

async fn make_display_snapshot(
    sender: &mpsc::Sender<RenderEvent>,
    cache: &Arc<RwLock<RenderCache>>,
    display: DisplayFlags,
    messages: &[MessageViewModel],
) -> (Vec<Line<'static>>, Vec<String>, usize) {
    sender
        .send(RenderEvent::WithDisplay {
            display,
            event: Box::new(RenderEvent::Rebuild(messages.to_vec())),
        })
        .await
        .expect("发送完整显示快照");
    make_display_barrier(sender).await;
    let cache = cache.read();
    (
        cache.lines.clone(),
        cache
            .wrap_map
            .iter()
            .map(|row| row.plain_text.clone())
            .collect(),
        cache.total_lines,
    )
}

#[tokio::test]
async fn test_viewport_display_cached_modes_match_fresh_render_in_both_strategies() {
    let messages = make_display_messages();
    for strategy in ["cache", "virtual"] {
        let (sender, cache, _) = spawn_render_thread(80);
        sender
            .send(RenderEvent::SetDetailStrategy(strategy.into()))
            .await
            .expect("选择渲染策略");
        let collapsed =
            make_display_snapshot(&sender, &cache, DisplayFlags::default(), &messages).await;
        for display in [
            DisplayFlags {
                detail_mode: true,
                ..Default::default()
            },
            DisplayFlags {
                diff_visible: true,
                ..Default::default()
            },
            DisplayFlags {
                detail_mode: true,
                diff_visible: true,
                show_tool_messages: true,
            },
        ] {
            let warmed = make_display_snapshot(&sender, &cache, display, &messages).await;
            let (fresh_sender, fresh_cache, _) = spawn_render_thread(80);
            fresh_sender
                .send(RenderEvent::SetDetailStrategy(strategy.into()))
                .await
                .expect("选择冷缓存渲染策略");
            let fresh =
                make_display_snapshot(&fresh_sender, &fresh_cache, display, &messages).await;
            assert_ne!(
                warmed.0, collapsed.0,
                "{strategy} 的显示切换必须改变真实渲染行"
            );
            assert_eq!(
                warmed, fresh,
                "{strategy} 的旧 VM 与双变体缓存不能复用另一显示状态"
            );
            let restored =
                make_display_snapshot(&sender, &cache, DisplayFlags::default(), &messages).await;
            assert_eq!(
                restored, collapsed,
                "{strategy} 切回折叠必须恢复行、复制文本与行数"
            );
        }
    }
}
