use super::*;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use std::time::{Duration, Instant};

async fn make_search_result_ready(app: &mut App) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while app.session_mgr.current().ui.at_mention.searching {
        app.poll_at_mention();
        assert!(Instant::now() < deadline, "候选搜索应及时完成");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

#[tokio::test]
async fn test_at_mention_keyboard_opens_completes_and_closes() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("sample.rs"), "").unwrap();
    let (mut app, _handle) = App::new_headless(80, 24).await;
    app.services.cwd = dir.path().to_string_lossy().into_owned();
    handle_key_event(
        &mut app,
        KeyEvent::new(KeyCode::Char('@'), KeyModifiers::NONE),
    )
    .expect("输入 @ 应成功");
    assert!(
        app.session_mgr.current().ui.at_mention.active,
        "单独 @ 应激活面板"
    );
    let action = handle_key_event(&mut app, KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
        .expect("搜索中 Enter 应正常处理");
    assert!(
        !matches!(action, Some(Action::Submit(_))),
        "搜索中不能误提交 @"
    );
    make_search_result_ready(&mut app).await;
    handle_key_event(&mut app, KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE))
        .expect("Tab 应补全路径");
    assert_eq!(
        app.session_mgr.current().ui.textarea.lines().join("\n"),
        "@sample.rs "
    );
    assert!(
        !app.session_mgr.current().ui.at_mention.active,
        "文件补全后应关闭面板"
    );
    handle_key_event(
        &mut app,
        KeyEvent::new(KeyCode::Char('@'), KeyModifiers::NONE),
    )
    .expect("再次输入 @ 应成功");
    assert!(app.session_mgr.current().ui.at_mention.active);
    handle_key_event(&mut app, KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))
        .expect("Esc 应关闭面板");
    assert!(!app.session_mgr.current().ui.at_mention.active);
}

#[tokio::test]
async fn test_at_mention_keyboard_searches_nested_file_after_default_list() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("src")).unwrap();
    std::fs::write(dir.path().join("src/needle.rs"), "").unwrap();
    let (mut app, _handle) = App::new_headless(80, 24).await;
    app.services.cwd = dir.path().to_string_lossy().into_owned();
    handle_key_event(
        &mut app,
        KeyEvent::new(KeyCode::Char('@'), KeyModifiers::NONE),
    )
    .expect("输入 @ 应成功");
    make_search_result_ready(&mut app).await;
    for ch in "needle".chars() {
        handle_key_event(
            &mut app,
            KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE),
        )
        .expect("输入关键词应成功");
    }
    make_search_result_ready(&mut app).await;
    assert!(
        app.session_mgr
            .current()
            .ui
            .at_mention
            .candidates
            .iter()
            .any(|candidate| candidate.path == "src/needle.rs"),
        "默认候选不能阻止子目录搜索"
    );
    handle_key_event(&mut app, KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
        .expect("Enter 应补全文件");
    assert_eq!(
        app.session_mgr.current().ui.textarea.lines().join("\n"),
        "@src/needle.rs "
    );
}
