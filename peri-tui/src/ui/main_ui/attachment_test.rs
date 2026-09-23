use super::*;
use crate::app::PendingAttachment;

fn make_attachment() -> PendingAttachment {
    PendingAttachment {
        label: "clipboard_1.png".to_string(),
        media_type: "image/png".to_string(),
        base64_data: String::new(),
        size_bytes: 69 * 1024,
        image_id: 1,
    }
}

/// 归一化渲染文本：CJK 宽字符在 buffer 中带填充空格（"待 发"），断言前移除全部空格
fn squashed(text: &str) -> String {
    text.chars().filter(|c| *c != ' ').collect()
}

#[tokio::test]
async fn test_render_attachment_bar_title_follows_language() {
    // Arrange：英文 locale（默认）挂载一个附件
    let (mut app, mut handle) = App::new_headless(80, 6).await;
    app.session_mgr
        .current_mut()
        .metadata
        .pending_attachments
        .push(make_attachment());
    // Act：渲染附件栏
    assert!(handle
        .terminal
        .draw(|f| super::render_attachment_bar(f, &app, Rect::new(0, 0, 80, 6)))
        .is_ok());
    let snapshot = squashed(&handle.snapshot().join("\n"));
    // Assert：英文 locale 标题与 Del 提示必须为英文，不得出现中文
    assert!(
        snapshot.contains("PendingAttachments"),
        "英文 locale 标题应为英文，实际渲染: {snapshot}"
    );
    assert!(
        snapshot.contains("Del:removelastone"),
        "英文 locale Del 提示应为英文，实际渲染: {snapshot}"
    );
    assert!(
        !snapshot.contains("待发送附件"),
        "英文 locale 不应出现中文标题，实际渲染: {snapshot}"
    );
    assert!(
        !snapshot.contains("删除最后一张"),
        "英文 locale 不应出现中文 Del 提示，实际渲染: {snapshot}"
    );
}

#[tokio::test]
async fn test_render_attachment_bar_title_chinese_locale() {
    // Arrange：切换到中文 locale 并挂载一个附件
    let (mut app, mut handle) = App::new_headless(80, 6).await;
    app.session_mgr
        .current_mut()
        .metadata
        .pending_attachments
        .push(make_attachment());
    app.services.lc.switch("zh-CN").expect("切换中文 locale");
    // Act：渲染附件栏
    assert!(handle
        .terminal
        .draw(|f| super::render_attachment_bar(f, &app, Rect::new(0, 0, 80, 6)))
        .is_ok());
    let snapshot = squashed(&handle.snapshot().join("\n"));
    // Assert：中文 locale 标题与 Del 提示必须为中文
    assert!(
        snapshot.contains("待发送附件"),
        "中文 locale 标题应为中文，实际渲染: {snapshot}"
    );
    assert!(
        snapshot.contains("Del:删除最后一张"),
        "中文 locale Del 提示应为中文，实际渲染: {snapshot}"
    );
}
