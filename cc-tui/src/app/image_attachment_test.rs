use super::*;
use base64::{engine::general_purpose::STANDARD, Engine as _};

fn make_image(image_id: usize, data: &str) -> PendingAttachment {
    PendingAttachment {
        label: format!("clipboard_{image_id}.png"),
        media_type: "image/png".into(),
        base64_data: data.into(),
        size_bytes: 5,
        image_id,
    }
}

#[tokio::test]
async fn test_image_attachment_submit_saves_only_active_images_and_preserves_display() {
    let (mut app, _) = App::new_headless(80, 24).await;
    app.session_mgr
        .current_mut()
        .metadata
        .title_generation_attempted = true;
    let encoded = STANDARD.encode(b"image");
    app.session_mgr.current_mut().metadata.pending_attachments =
        vec![make_image(1, &encoded), make_image(2, &encoded)];
    let session_id = app.session_mgr.current().metadata.session_id;
    app.submit_message("[Image #1] 看这张图片".into());
    let session = app.session_mgr.current();
    assert_eq!(
        session.metadata.image_paths.len(),
        1,
        "删掉占位符的图片不能写盘"
    );
    let path = session.metadata.image_paths[&1].clone();
    assert_eq!(
        path,
        std::env::temp_dir()
            .join("cc-code-images")
            .join(session_id.to_string())
            .join("1.png")
    );
    assert_eq!(std::fs::read(&path).expect("读取已提交的图片"), b"image");
    assert!(!path.with_file_name("2.png").exists());
    assert_eq!(
        session.metadata.last_human_message.as_deref(),
        Some("[Image #1] 看这张图片"),
        "用户文字不得追加图片数量尾巴"
    );
    assert!(
        matches!(&session.messages.view_messages[0], MessageViewModel::UserBubble { content, .. } if content == "[Image #1] 看这张图片")
    );
    std::fs::remove_file(&path).expect("清理本测试图片");
    std::fs::remove_dir(path.parent().expect("会话临时目录")).expect("清理本测试空目录");
}

#[tokio::test]
async fn test_image_attachment_decode_failure_preserves_user_message() {
    let (mut app, _) = App::new_headless(80, 24).await;
    app.session_mgr
        .current_mut()
        .metadata
        .title_generation_attempted = true;
    app.session_mgr.current_mut().metadata.pending_attachments =
        vec![make_image(1, "invalid base64!")];
    app.submit_message("[Image #1] 图片仍需发送".into());
    let session = app.session_mgr.current();
    assert!(
        session.metadata.image_paths.is_empty(),
        "写盘失败不能登记不可用路径"
    );
    assert!(
        session.metadata.pending_attachments.is_empty(),
        "落盘失败不能阻止附件进入提交链路"
    );
    assert_eq!(
        session.metadata.last_human_message.as_deref(),
        Some("[Image #1] 图片仍需发送")
    );
    assert!(
        matches!(&session.messages.view_messages[0], MessageViewModel::UserBubble { content, .. } if content == "[Image #1] 图片仍需发送")
    );
}

#[tokio::test]
async fn test_image_attachment_render_rebuild_preserves_session_links() {
    let (mut app, _) = App::new_headless(80, 24).await;
    let path = std::env::temp_dir().join("session-image.png");
    app.session_mgr
        .current_mut()
        .metadata
        .image_paths
        .insert(1, path.clone());
    let mut message = MessageViewModel::user("[Image #1]".into());
    message.set_image_ids(vec![1]);
    app.session_mgr
        .current_mut()
        .messages
        .view_messages
        .push(message);
    app.render_rebuild();
    let (done, ready) = tokio::sync::oneshot::channel();
    app.session_mgr
        .current()
        .messages
        .render_tx
        .send(RenderEvent::ResizeAndWait(80, done))
        .await
        .expect("发送渲染屏障");
    ready.await.expect("渲染快照应可用");
    let cache = app.session_mgr.current().messages.render_cache.read();
    assert_eq!(cache.total_lines, 2, "正文和附件都应参与布局");
    if crate::ui::osc8::enabled() {
        assert_eq!(cache.links.len(), 1, "重建用户消息后也必须补回路径");
        assert_eq!(
            cache.links[0].url,
            url::Url::from_file_path(path)
                .expect("绝对路径可转 URL")
                .to_string()
        );
    } else {
        assert!(cache.links.is_empty());
    }
}
