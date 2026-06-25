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
    let mut result = Vec::new();
    let mut cursor = 0;
    let bytes = text.as_bytes();

    while cursor < bytes.len() {
        let remaining = &text[cursor..];
        if let Some(id) = parse_placeholder_at_start(remaining) {
            result.push(id);
            // 跳过整个占位符
            let placeholder_len = format_placeholder(id).len();
            cursor += placeholder_len;
        } else {
            // 跳过一个 UTF-8 字符
            let next_char_len = utf8_char_len(bytes[cursor]);
            cursor += next_char_len;
        }
    }

    result
}

/// 检查字符串是否以 `[Image #N]` 开头，是则返回 N，否则返回 None。
fn parse_placeholder_at_start(s: &str) -> Option<usize> {
    parse_placeholder_with_len(s).map(|(id, _)| id)
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

/// 与 `parse_placeholder_at_start` 同语义，但额外返回占位符字符长度。
fn parse_placeholder_with_len(s: &str) -> Option<(usize, usize)> {
    let prefix = "[Image #";
    let s_chars: Vec<char> = s.chars().collect();
    let prefix_chars: Vec<char> = prefix.chars().collect();

    if s_chars.len() < prefix_chars.len() + 2 {
        return None;
    }
    if s_chars[..prefix_chars.len()] != prefix_chars[..] {
        return None;
    }

    let mut digits_len = 0usize;
    let mut image_id = 0usize;
    let mut i = prefix_chars.len();
    while i < s_chars.len() && s_chars[i].is_ascii_digit() {
        image_id = image_id * 10 + (s_chars[i] as usize - '0' as usize);
        digits_len += 1;
        i += 1;
    }
    if digits_len == 0 || i >= s_chars.len() || s_chars[i] != ']' {
        return None;
    }

    Some((image_id, i + 1))
}

/// 返回 UTF-8 字符的首字节所对应的字符长度（字节为单位）。
fn utf8_char_len(first_byte: u8) -> usize {
    if first_byte < 0x80 {
        1
    } else if first_byte >> 5 == 0b110 {
        2
    } else if first_byte >> 4 == 0b1110 {
        3
    } else if first_byte >> 3 == 0b11110 {
        4
    } else {
        // 无效 UTF-8 首字节，按 1 推进避免死循环
        1
    }
}

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
    fn 格式化_占位符() {
        assert_eq!(format_placeholder(1), "[Image #1]");
        assert_eq!(format_placeholder(42), "[Image #42]");
    }

    #[test]
    fn 解析_单个占位符() {
        assert_eq!(parse_placeholders("look at [Image #1]"), vec![1]);
    }

    #[test]
    fn 解析_多个占位符_按顺序() {
        assert_eq!(
            parse_placeholders("a [Image #2] b [Image #1] c"),
            vec![2, 1]
        );
    }

    #[test]
    fn 解析_无占位符_返回空() {
        assert!(parse_placeholders("hello world").is_empty());
    }

    #[test]
    fn 解析_部分残留_不匹配() {
        assert!(parse_placeholders("half [Image broken").is_empty());
        assert!(parse_placeholders("Image #1]").is_empty());
        assert!(parse_placeholders("[Image #]").is_empty());
    }

    #[test]
    fn 解析_连续占位符_无分隔() {
        assert_eq!(parse_placeholders("[Image #1][Image #2]"), vec![1, 2]);
    }

    #[test]
    fn 解析_多字节字符_正确推进() {
        assert_eq!(parse_placeholders("图 [Image #3] 片"), vec![3]);
    }

    #[test]
    fn 过滤_按占位符顺序匹配附件() {
        let atts = vec![10, 20, 30];
        let result = filter_by_placeholders("[Image #20] then [Image #10]", &atts, |x| *x);
        assert_eq!(result, vec![&20, &10]);
    }

    #[test]
    fn 过滤_未在文本中出现_被丢弃() {
        let atts = vec![10, 20, 30];
        let result = filter_by_placeholders("only [Image #10]", &atts, |x| *x);
        assert_eq!(result, vec![&10]);
    }

    #[test]
    fn 过滤_无占位符_返回空() {
        let atts = vec![10, 20];
        let result = filter_by_placeholders("no images here", &atts, |x| *x);
        assert!(result.is_empty());
    }

    #[test]
    fn 过滤_占位符无对应附件_跳过() {
        let atts = vec![10];
        let result = filter_by_placeholders("[Image #99]", &atts, |x| *x);
        assert!(result.is_empty());
    }

    #[test]
    fn parse_single_完整匹配() {
        assert_eq!(parse_single_placeholder("[Image #1]"), Some((1, 10)));
        assert_eq!(parse_single_placeholder("[Image #42]"), Some((42, 11)));
    }

    #[test]
    fn parse_single_部分残留_不匹配() {
        assert_eq!(parse_single_placeholder("[Image"), None);
        assert_eq!(parse_single_placeholder("Image #1]"), None);
        assert_eq!(parse_single_placeholder("[Image #1"), None);
        assert_eq!(parse_single_placeholder("[Image #1] extra"), None);
        assert_eq!(parse_single_placeholder("[Image #]"), None);
    }
}
