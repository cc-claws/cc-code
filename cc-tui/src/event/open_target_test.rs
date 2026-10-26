use super::local_file_target;

#[test]
fn test_local_file_target_returns_none_for_http_url() {
    assert!(
        local_file_target("https://example.com/a.md").is_none(),
        "非 file: 链接仍按 URL 交给浏览器"
    );
}

#[test]
fn test_local_file_target_falls_back_on_malformed_file_url() {
    // 空串与非 URL 输入不能构造出可打开的路径。
    assert!(local_file_target("").is_none());
    assert!(local_file_target("file:not a url").is_none());
}

#[cfg(windows)]
#[test]
fn test_local_file_target_decodes_windows_drive_path() {
    assert_eq!(
        local_file_target("file:///C:/a/b%20c.md").as_deref(),
        Some(r"C:\a\b c.md"),
        "百分号编码必须还原成可打开的空间分隔路径"
    );
}

#[cfg(unix)]
#[test]
fn test_local_file_target_decodes_unix_path() {
    assert_eq!(
        local_file_target("file:///tmp/a%20b.md").as_deref(),
        Some("/tmp/a b.md")
    );
}
