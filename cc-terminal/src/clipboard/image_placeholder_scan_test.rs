use super::*;

#[test]
fn test_parse_placeholders_rejects_overflow_and_keeps_following_valid_reference() {
    let overflow = format!("[Image #{}0]", usize::MAX);
    assert_eq!(
        parse_single_placeholder(&overflow),
        None,
        "超大编号不得溢出或崩溃"
    );
    assert_eq!(
        parse_placeholders(&format!("中文 {overflow} [Image #2]")),
        vec![2]
    );
}

#[test]
fn test_parse_placeholders_scans_long_chinese_content_without_copying_each_suffix() {
    let content = format!("{} [Image #42]", "长消息中文正文".repeat(10_000));
    assert_eq!(
        parse_placeholders(&content),
        vec![42],
        "长消息渲染应能扫描尾部图片引用"
    );
}

#[test]
fn test_parse_placeholders_preserves_leading_zero_and_nested_prefix_behavior() {
    assert_eq!(parse_single_placeholder("[Image #001]"), Some((1, 12)));
    assert_eq!(
        parse_placeholders("[Image #001]中[Image #invalid [Image #2]"),
        vec![1, 2]
    );
}
