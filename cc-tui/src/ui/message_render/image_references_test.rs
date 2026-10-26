use super::*;
use crate::ui::message_view::MessageViewModel;
use unicode_segmentation::UnicodeSegmentation;

fn make_image_paths() -> Vec<(usize, PathBuf)> {
    vec![
        (1, std::env::temp_dir().join("image one.png")),
        (2, std::env::temp_dir().join("image-two.png")),
    ]
}

#[test]
fn test_image_reference_link_matches_label_and_body_line_offset() {
    let mut lines = vec![Line::from("正文第一行"), Line::from("正文第二行")];
    let mut links = Vec::new();
    let paths = make_image_paths();
    append(
        &mut lines,
        &mut links,
        "[Image #1] 看下图片",
        &paths,
        80,
        true,
    );
    assert_eq!(lines.len(), 3, "附件应紧跟正文，不插空行");
    assert_eq!(lines[2].to_string(), "  ⎿  [Image #1]");
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].line, 2);
    assert_eq!((links[0].g_start, links[0].g_end), (5, 15));
    assert_eq!(
        links[0].url,
        url::Url::from_file_path(&paths[0].1)
            .expect("绝对路径应可转 URL")
            .to_string()
    );
    assert!(
        links[0].url.contains("image%20one.png"),
        "空格必须由 URL 库编码"
    );
    assert!(lines[2].spans[1]
        .style
        .add_modifier
        .contains(Modifier::UNDERLINED));
    assert!(
        lines[2].spans.iter().all(|span| span.style.bg.is_none()),
        "附件行不得继承用户气泡底色"
    );
}

#[test]
fn test_image_references_are_sorted_and_repeated_identifiers_are_deduplicated() {
    let mut lines = Vec::new();
    let mut links = Vec::new();
    append(
        &mut lines,
        &mut links,
        "[Image #2] [Image #1] [Image #2]",
        &make_image_paths(),
        80,
        true,
    );
    assert_eq!(
        lines.iter().map(Line::to_string).collect::<Vec<_>>(),
        vec!["  ⎿  [Image #1]", "  ⎿  [Image #2]"]
    );
    assert_eq!(
        links.iter().map(|hit| hit.line).collect::<Vec<_>>(),
        vec![0, 1]
    );
}

#[test]
fn test_image_reference_without_path_stays_visible_without_link_or_underline() {
    let mut lines = Vec::new();
    let mut links = Vec::new();
    append(&mut lines, &mut links, "恢复历史 [Image #7]", &[], 80, true);
    assert_eq!(lines[0].to_string(), "  ⎿  [Image #7]");
    assert!(links.is_empty(), "无路径不得构造猜测链接");
    assert!(lines[0]
        .spans
        .iter()
        .all(|span| !span.style.add_modifier.contains(Modifier::UNDERLINED)));
}

#[test]
fn test_image_reference_disabled_hyperlinks_stays_plain() {
    let mut lines = Vec::new();
    let mut links = Vec::new();
    append(
        &mut lines,
        &mut links,
        "[Image #1]",
        &make_image_paths(),
        80,
        false,
    );
    assert_eq!(lines[0].to_string(), "  ⎿  [Image #1]");
    assert!(links.is_empty());
    assert!(
        lines[0]
            .spans
            .iter()
            .all(|span| !span.style.add_modifier.contains(Modifier::UNDERLINED)),
        "关闭超链接时不得留下可点击的样式"
    );
}

#[test]
fn test_image_references_ignore_text_without_placeholders() {
    let mut lines = vec![Line::from("纯文字")];
    let mut links = Vec::new();
    append(
        &mut lines,
        &mut links,
        "纯文字 [Image 未完成",
        &make_image_paths(),
        80,
        true,
    );
    assert_eq!(lines.len(), 1);
    assert!(links.is_empty());
}

#[test]
fn test_image_reference_wrapping_keeps_hanging_indent_and_link_coordinates() {
    let mut lines = Vec::new();
    let mut links = Vec::new();
    append(
        &mut lines,
        &mut links,
        "[Image #1]",
        &make_image_paths(),
        12,
        true,
    );
    assert!(lines.len() > 1, "窄终端应把引用拆成续行");
    assert_eq!(lines[0].spans[0].content, BranchPrefix::PREFIX);
    assert!(lines[1..]
        .iter()
        .all(|line| line.spans[0].content == BranchPrefix::HANG));
    assert!(lines.iter().all(|line| line.width() <= 12));
    assert_eq!(links.len(), lines.len(), "每个可见分段都应可点");
    for hit in links {
        let text = lines[hit.line].to_string();
        let label: String = text
            .graphemes(true)
            .skip(hit.g_start)
            .take(hit.g_end - hit.g_start)
            .collect();
        assert!(!label.is_empty() && "[Image #1]".contains(&label));
        assert_eq!(hit.g_start, BranchPrefix::WIDTH);
    }
}

#[test]
fn test_user_bubble_adds_image_rows_in_compact_and_detail_modes() {
    let mut message = MessageViewModel::user_with_expanded(
        "[Pasted text #1 +2 lines] [Image #1]".into(),
        "展开第一行\n展开第二行 [Image #1]".into(),
    );
    message.set_image_ids(vec![1]);
    message.set_image_paths(&make_image_paths().into_iter().collect());
    for detail in [false, true] {
        let (lines, links) =
            super::super::render_view_model_with_links(&message, Some(1), 80, detail, 0);
        assert_eq!(
            lines.last().expect("必须有附件行").to_string(),
            "  ⎿  [Image #1]"
        );
        assert_eq!(links.len(), usize::from(crate::ui::osc8::enabled()));
        assert!(
            lines[0].spans.iter().all(|span| span.style.bg.is_some()),
            "正文继续保留气泡底色"
        );
    }
}

#[cfg(windows)]
#[test]
fn test_image_reference_windows_path_uses_file_url() {
    let mut lines = Vec::new();
    let mut links = Vec::new();
    append(
        &mut lines,
        &mut links,
        "[Image #1]",
        &[(1, PathBuf::from(r"C:\a\b.png"))],
        80,
        true,
    );
    assert_eq!(links[0].url, "file:///C:/a/b.png");
}
