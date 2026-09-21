use super::*;
use base64::Engine as _;
use ratatui::crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

fn make_image(dir: &std::path::Path) -> std::path::PathBuf {
    let path = dir.join("本机 图片.JPG");
    image::RgbImage::from_pixel(2, 2, image::Rgb([12, 34, 56]))
        .save_with_format(&path, image::ImageFormat::Jpeg)
        .expect("构造本机 JPEG");
    path
}

#[tokio::test]
async fn test_image_paste_event_quoted_path_and_file_url_idle_and_busy() {
    let dir = tempfile::tempdir().unwrap();
    let path = make_image(dir.path());
    for loading in [false, true] {
        for pasted in [
            format!("\"{}\"", path.display()),
            url::Url::from_file_path(&path)
                .expect("生成 file URL")
                .to_string(),
        ] {
            let (mut app, _handle) = App::new_headless(80, 24).await;
            app.session_mgr.current_mut().ui.loading = loading;
            app.session_mgr
                .current_mut()
                .ui
                .textarea
                .insert_str("分析这张图 ");
            assert!(crate::event::handle_event(&mut app, Event::Paste(pasted))
                .await
                .is_ok());
            assert_eq!(
                app.session_mgr.current().ui.textarea.lines(),
                ["分析这张图 [Image #1]"]
            );
            let attachments = &app.session_mgr.current().metadata.pending_attachments;
            assert_eq!(attachments.len(), 1);
            assert_eq!(attachments[0].media_type, "image/png");
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(&attachments[0].base64_data)
                .expect("附件编码正确");
            assert!(
                image::load_from_memory(&bytes).is_ok(),
                "应添加真正图片数据"
            );
        }
    }
}

#[tokio::test]
async fn test_image_paste_enter_queues_image_with_message_not_path() {
    let dir = tempfile::tempdir().unwrap();
    let path = make_image(dir.path());
    let (mut app, _handle) = App::new_headless(80, 24).await;
    app.session_mgr.current_mut().ui.loading = true;
    assert!(crate::event::handle_event(
        &mut app,
        Event::Paste(path.to_string_lossy().into_owned())
    )
    .await
    .is_ok());
    assert!(crate::event::handle_event(
        &mut app,
        Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
    )
    .await
    .is_ok());
    let queue = &app.session_mgr.current().messages.pending_messages;
    assert_eq!(queue.len(), 1);
    assert_eq!(queue[0].text, "[Image #1]");
    assert_eq!(queue[0].attachments.len(), 1);
    assert_eq!(
        queue[0].content().content_blocks().len(),
        2,
        "提交内容应包含文字和图片两个 block"
    );
    assert!(app
        .session_mgr
        .current()
        .metadata
        .pending_attachments
        .is_empty());
}

#[tokio::test]
async fn test_image_paste_relative_path_uses_project_directory() {
    let dir = tempfile::tempdir().unwrap();
    let path = make_image(dir.path());
    let (mut app, _handle) = App::new_headless(80, 24).await;
    app.services.cwd = dir.path().to_string_lossy().into_owned();
    app.paste_text_into_textarea(
        path.file_name()
            .expect("文件名存在")
            .to_str()
            .expect("UTF-8 文件名"),
    );
    assert_eq!(
        app.session_mgr.current().metadata.pending_attachments.len(),
        1
    );
}

#[tokio::test]
async fn test_image_paste_nonimage_missing_corrupt_and_multiline_remain_text() {
    let dir = tempfile::tempdir().unwrap();
    let text_path = dir.path().join("说明.txt");
    let corrupt = dir.path().join("损坏.png");
    std::fs::write(&text_path, b"plain text").expect("构造文本文件");
    std::fs::write(&corrupt, b"not png").expect("构造坏图片");
    let image_path = make_image(dir.path());
    for text in [
        "正常文本".to_string(),
        text_path.to_string_lossy().into_owned(),
        corrupt.to_string_lossy().into_owned(),
        dir.path()
            .join("missing.png")
            .to_string_lossy()
            .into_owned(),
        format!("图片路径：\n{}", image_path.display()),
    ] {
        let (mut app, _handle) = App::new_headless(80, 24).await;
        app.paste_text_into_textarea(&text);
        let visible = app.session_mgr.current().ui.textarea.lines().join("\n");
        assert_eq!(
            app.expand_pasted_text(&visible),
            text,
            "未转换的内容不能丢失"
        );
        assert!(app
            .session_mgr
            .current()
            .metadata
            .pending_attachments
            .is_empty());
    }
}

#[tokio::test]
async fn test_image_paste_shell_draft_keeps_image_path_as_text() {
    let dir = tempfile::tempdir().unwrap();
    let path = make_image(dir.path());
    let (mut app, _handle) = App::new_headless(80, 24).await;
    app.session_mgr
        .current_mut()
        .ui
        .textarea
        .insert_str("  !type ");
    app.paste_text_into_textarea(path.to_str().expect("UTF-8 路径"));
    assert_eq!(
        app.session_mgr.current().ui.textarea.lines(),
        [format!("  !type {}", path.display())]
    );
    assert!(app
        .session_mgr
        .current()
        .metadata
        .pending_attachments
        .is_empty());
}

#[tokio::test]
async fn test_image_paste_parentheses_in_copied_absolute_filename() {
    let dir = tempfile::tempdir().unwrap();
    let source = make_image(dir.path());
    let path = dir.path().join("本机 图片 (1).jpg");
    std::fs::rename(source, &path).expect("构造带括号图片名");
    let (mut app, _handle) = App::new_headless(80, 24).await;
    app.paste_text_into_textarea(&format!("\"{}\"", path.display()));
    assert_eq!(
        app.session_mgr.current().ui.textarea.lines(),
        ["[Image #1]"]
    );
    assert_eq!(
        app.session_mgr.current().metadata.pending_attachments.len(),
        1
    );
}

#[tokio::test]
async fn test_image_paste_setup_field_receives_path_without_chat_attachment() {
    use crate::app::setup_wizard::{FormField, FormMode, SetupStep};
    let dir = tempfile::tempdir().unwrap();
    let path = make_image(dir.path());
    let pasted = path.to_string_lossy().into_owned();
    let (mut app, _handle) = App::new_headless(80, 24).await;
    app.open_setup_wizard();
    let wizard = app.global_ui.setup_wizard.as_mut().expect("设置面板已打开");
    wizard.step = SetupStep::Form;
    wizard.form_mode = FormMode::Edit;
    wizard.form_focus = FormField::ApiKey;
    assert!(
        crate::event::handle_event(&mut app, Event::Paste(pasted.clone()))
            .await
            .is_ok()
    );
    let wizard = app.global_ui.setup_wizard.as_ref().expect("设置面板仍打开");
    assert_eq!(
        wizard.providers[0].api_key, pasted,
        "设置字段应收到原始路径文本"
    );
    assert!(app
        .session_mgr
        .current()
        .metadata
        .pending_attachments
        .is_empty());
    assert_eq!(app.session_mgr.current().ui.textarea.lines(), [""]);
}
