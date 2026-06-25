use super::*;

#[tokio::test]
async fn test_paste_text_into_textarea_multiline_appends_placeholder() {
    let (mut app, _handle) = App::new_headless(80, 24).await;
    app.session_mgr
        .current_mut()
        .ui
        .textarea
        .insert_str("测试输入1+1");
    let pasted = "bulky_follow/list 接口正常返回数据了，code: 0，返回了多条记录。\n\
这个 token 对应的用户是 叶炜朋（staff_id: 124727592321575781）。\n\
follow 场景本来就没问题，因为 $skipPermission = true 已经跳过了权限过滤。\n\
之前看不到数据的是 selection 场景，已经修复并提交了。\n\
部署测试环境后，selection 列表也应该能正常看到数据了。\n\
单纯复制预期: [Pasted text #1 +7 lines]\n\
预期用户界面看到,然后实际发给agent要全部发过去";

    app.paste_text_into_textarea(pasted);

    let visible = app.session_mgr.current().ui.textarea.lines().join("\n");
    assert_eq!(
        visible, "测试输入1+1 [Pasted text #1 +7 lines]",
        "已有输入后粘贴多行文本应追加一个占位符，且不覆盖原输入"
    );
    assert_eq!(
        app.expand_pasted_text(&visible),
        format!("测试输入1+1 {}", pasted),
        "提交前应能把占位符还原为完整粘贴内容"
    );
}

#[tokio::test]
async fn test_paste_text_into_textarea_single_line_keeps_plain_text() {
    let (mut app, _handle) = App::new_headless(80, 24).await;

    app.paste_text_into_textarea("hello");

    assert_eq!(
        app.session_mgr.current().ui.textarea.lines(),
        ["hello"],
        "单行粘贴应保持原始可见文本"
    );
    assert!(
        app.session_mgr.current().ui.pasted_text_blocks.is_empty(),
        "单行粘贴不应创建占位符映射"
    );
}

#[tokio::test]
async fn test_submit_message_expands_pasted_text_but_displays_placeholder() {
    let (mut app, _handle) = App::new_headless(80, 24).await;
    app.paste_text_into_textarea("line1\nline2\nline3");
    let visible = app.session_mgr.current().ui.textarea.lines().join("\n");

    app.submit_message(visible.clone());

    assert_eq!(
        app.session_mgr
            .current()
            .metadata
            .last_human_message
            .as_deref(),
        Some(visible.as_str()),
        "用户消息显示应保留占位符，避免大段粘贴撑满 UI"
    );
    assert_eq!(
        app.session_mgr
            .current()
            .messages
            .last_submitted_text
            .as_deref(),
        Some("line1\nline2\nline3"),
        "实际提交给 agent 的文本应展开为完整粘贴内容"
    );
    assert!(
        app.session_mgr.current().ui.pasted_text_blocks.is_empty(),
        "提交当前 draft 后应清理占位符映射"
    );
}
