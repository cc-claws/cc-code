//! textarea 内嵌图片占位符 `[Image #N]` 的格式与扫描。
//!
//! 粘贴图片时在 textarea 当前光标位置插入 `[Image #N]`，让用户能在文本中混排
//! 图片、控制图片在 prompt 中的位置。提交时扫描 textarea 中的占位符，按出现顺序
//! 决定哪些附件被发送；用户在 textarea 中删掉的占位符对应附件不会发送。
//!
//! 编号 N 是 SessionMetadata.next_image_id 单调递增的稳定 ID——不会因为
//! 删除/重排而变化，避免"删 #1 后 #2 重命名为 #1 但 textarea 文本未同步"的错位。

/// 占位符格式：`[Image #N]`，N 为 1 起的正整数。
pub fn format_placeholder(image_id: usize) -> String {
    format!("[Image #{image_id}]")
}

/// 扫描文本，按出现顺序返回所有 `[Image #N]` 占位符中的 image_id。
///
/// 部分残留（如 `[Image` 或 `Image #1]`）不匹配，不会返回。
/// 重复出现的同一 image_id 会被多次返回（调用方按需去重）。
pub fn parse_placeholders(text: &str) -> Vec<usize> {
    // 消息区会反复扫描历史：只检查标记起点，避免逐字符复制剩余全文。
    text.match_indices("[Image #")
        .filter_map(|(offset, _)| {
            text.get(offset..)
                .and_then(parse_placeholder_with_len)
                .map(|(id, _)| id)
        })
        .collect()
}

/// 验证字符串恰好是一个 `[Image #N]` 占位符（不多不少）。
/// 是则返回 `(image_id, 占位符字符长度)`，否则 None。
pub fn parse_single_placeholder(s: &str) -> Option<(usize, usize)> {
    let (id, consumed) = parse_placeholder_with_len(s)?;
    if consumed == s.chars().count() {
        Some((id, consumed))
    } else {
        None
    }
}

/// 解析开头的占位符，同时返回实际字符长度（含前导零）。
fn parse_placeholder_with_len(s: &str) -> Option<(usize, usize)> {
    let prefix = "[Image #";
    let mut image_id = 0usize;
    for (index, character) in s.strip_prefix(prefix)?.chars().enumerate() {
        if character == ']' {
            return (index > 0).then_some((image_id, prefix.len() + index + 1));
        }
        if !character.is_ascii_digit() {
            return None;
        }
        image_id = image_id
            .checked_mul(10)?
            .checked_add(character as usize - '0' as usize)?;
    }
    None
}

#[cfg(test)]
#[path = "image_placeholder_scan_test.rs"]
mod scan_tests;

/// 从 `pending_attachments` 中筛出 textarea 中仍存在的占位符对应附件。
///
/// 按 textarea 出现顺序返回 `(image_id, &T)`，未在 textarea 中出现的项被丢弃。
/// 重复占位符会重复返回同一附件（罕见情况，调用方按需 dedup）。
pub fn filter_by_placeholders<'a, T: 'a>(
    text: &str,
    attachments: &'a [T],
    image_id_of: impl Fn(&T) -> usize,
) -> Vec<&'a T> {
    let ids = parse_placeholders(text);
    let mut result = Vec::with_capacity(ids.len());
    for id in ids {
        if let Some(att) = attachments.iter().find(|a| image_id_of(a) == id) {
            result.push(att);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_placeholder() {
        assert_eq!(format_placeholder(1), "[Image #1]");
        assert_eq!(format_placeholder(42), "[Image #42]");
    }

    #[test]
    fn test_parse_single_placeholder_in_text() {
        assert_eq!(parse_placeholders("look at [Image #1]"), vec![1]);
    }

    #[test]
    fn test_parse_multiple_placeholders_in_order() {
        assert_eq!(
            parse_placeholders("a [Image #2] b [Image #1] c"),
            vec![2, 1]
        );
    }

    #[test]
    fn test_parse_placeholders_returns_empty_when_none() {
        assert!(parse_placeholders("hello world").is_empty());
    }

    #[test]
    fn test_parse_placeholders_ignores_partial_or_malformed() {
        assert!(parse_placeholders("half [Image broken").is_empty());
        assert!(parse_placeholders("Image #1]").is_empty());
        assert!(parse_placeholders("[Image #]").is_empty());
    }

    #[test]
    fn test_parse_placeholders_consecutive_without_separator() {
        assert_eq!(parse_placeholders("[Image #1][Image #2]"), vec![1, 2]);
    }

    #[test]
    fn test_parse_placeholders_advances_correctly_with_multibyte_chars() {
        assert_eq!(parse_placeholders("图 [Image #3] 片"), vec![3]);
    }

    #[test]
    fn test_filter_by_placeholders_matches_attachments_in_order() {
        let atts = vec![10, 20, 30];
        let result = filter_by_placeholders("[Image #20] then [Image #10]", &atts, |x| *x);
        assert_eq!(result, vec![&20, &10]);
    }

    #[test]
    fn test_filter_by_placeholders_discards_unmentioned_attachments() {
        let atts = vec![10, 20, 30];
        let result = filter_by_placeholders("only [Image #10]", &atts, |x| *x);
        assert_eq!(result, vec![&10]);
    }

    #[test]
    fn test_filter_by_placeholders_returns_empty_when_no_placeholders() {
        let atts = vec![10, 20];
        let result = filter_by_placeholders("no images here", &atts, |x| *x);
        assert!(result.is_empty());
    }

    #[test]
    fn test_filter_by_placeholders_skips_when_attachment_missing() {
        let atts = vec![10];
        let result = filter_by_placeholders("[Image #99]", &atts, |x| *x);
        assert!(result.is_empty());
    }

    #[test]
    fn test_parse_single_placeholder_exact_match() {
        assert_eq!(parse_single_placeholder("[Image #1]"), Some((1, 10)));
        assert_eq!(parse_single_placeholder("[Image #42]"), Some((42, 11)));
    }

    #[test]
    fn test_parse_single_placeholder_rejects_partial_or_malformed() {
        assert_eq!(parse_single_placeholder("[Image"), None);
        assert_eq!(parse_single_placeholder("Image #1]"), None);
        assert_eq!(parse_single_placeholder("[Image #1"), None);
        assert_eq!(parse_single_placeholder("[Image #1] extra"), None);
        assert_eq!(parse_single_placeholder("[Image #]"), None);
    }
}
