//! 把粘贴的字符串识别为文件路径，统一归一化成 `PathBuf`。
//!
//! 参照 openai/codex `clipboard_paste.rs:normalize_pasted_path` 处理 4 种形态：
//!
//! 1. `file://` URL：用 `url::Url::parse` 解析后取 path 段
//! 2. Windows drive 路径（`C:\\...`）：WSL 下转 `/mnt/c/...`，其他平台保留
//! 3. UNC 路径（`\\\\server\\share\\...`）：保留原样作为 UNC `PathBuf`
//! 4. shell 转义的单路径（`My\\ File.png`）：手写反斜杠转义解析（避免引入 shlex）
//!
//! 多行文本、包含空白的非路径字符串、明显不是路径的内容均返回 `None`。

use std::path::PathBuf;

/// 把粘贴的文本归一化为 `PathBuf`，无法识别为路径返回 `None`。
///
/// 调用方应在「单行粘贴」时调用；多行文本直接走普通粘贴流程。
pub fn normalize_pasted_path(pasted: &str) -> Option<PathBuf> {
    let trimmed = pasted.trim();
    if trimmed.is_empty() || trimmed.lines().count() != 1 {
        return None;
    }

    let stripped = strip_outer_quotes(trimmed);
    if stripped.is_empty() {
        return None;
    }

    // 始终拒绝 shell 元字符：避免把 `$VAR`、`;rm`、`|cat` 这类命令当路径
    if contains_shell_metacharacters(stripped) {
        return None;
    }

    if let Some(path) = parse_file_url(stripped) {
        return Some(path);
    }

    // `\\` 开头的字符串：UNC 路径或拒绝，不当作 shell 转义处理
    if stripped.starts_with("\\\\") {
        return parse_unc_path(stripped);
    }

    if let Some(path) = parse_windows_drive_path(stripped) {
        return Some(path);
    }

    parse_shell_escaped_path(stripped)
}

/// 去掉单/双引号包裹（仅在两端匹配时）。
fn strip_outer_quotes(s: &str) -> &str {
    let bytes = s.as_bytes();
    if bytes.len() < 2 {
        return s;
    }
    match (bytes[0], bytes[bytes.len() - 1]) {
        (b'"', b'"') | (b'\'', b'\'') => &s[1..s.len() - 1],
        _ => s,
    }
}

/// `file:///tmp/x.png` 或 `file:///C:/Users/...` → filesystem path。
fn parse_file_url(s: &str) -> Option<PathBuf> {
    let lowered = s.to_ascii_lowercase();
    if !lowered.starts_with("file://") {
        return None;
    }

    let url = url::Url::parse(s).ok()?;
    if url.scheme() != "file" {
        return None;
    }

    // url::Url::to_file_path 只在 host 为空或 localhost 时返回 Ok
    match url.to_file_path() {
        Ok(path) => Some(path),
        Err(()) => {
            // host 不为空（如 file://server/share/...），fallback 用 path 段拼
            let raw = url.path();
            Some(PathBuf::from(raw))
        }
    }
}

/// `\\server\share\file.png` → 保留为 PathBuf（Windows UNC / Samba）。
fn parse_unc_path(s: &str) -> Option<PathBuf> {
    if !s.starts_with("\\\\") {
        return None;
    }
    // 至少要有 \\server\share 结构
    let after = &s[2..];
    let mut components = after.split(['\\', '/']).filter(|c| !c.is_empty());
    let first = components.next()?;
    let second = components.next()?;
    if first.is_empty() || second.is_empty() {
        return None;
    }
    Some(PathBuf::from(s))
}

/// `C:\Users\...` 或 `C:/Users/...` → WSL 下转 `/mnt/c/...`，其他平台原样保留。
fn parse_windows_drive_path(s: &str) -> Option<PathBuf> {
    let bytes = s.as_bytes();
    if bytes.len() < 3 {
        return None;
    }
    let drive = bytes[0];
    if !drive.is_ascii_alphabetic() {
        return None;
    }
    if bytes[1] != b':' {
        return None;
    }
    if bytes[2] != b'\\' && bytes[2] != b'/' {
        return None;
    }

    #[cfg(target_os = "linux")]
    {
        // WSL 下把 C:\... → /mnt/c/...
        let drive_letter = drive.to_ascii_lowercase() as char;
        let mut result = PathBuf::from(format!("/mnt/{drive_letter}"));
        for component in s[2..]
            .trim_start_matches(['\\', '/'])
            .split(['\\', '/'])
            .filter(|c| !c.is_empty())
        {
            result.push(component);
        }
        Some(result)
    }

    #[cfg(not(target_os = "linux"))]
    {
        // 非 Linux（含 Windows 本地、macOS）：原样保留
        Some(PathBuf::from(s))
    }
}

/// 解析 shell 反斜杠转义的单路径。
///
/// 例：`/home/user/My\ File.png` → `/home/user/My File.png`。
/// 遇到真正的 shell 特殊字符（`$`、`` ` ``、`|`、`>`、`<`、`;`、`&`、`*`、`?`、`(`、`)`）
/// 视为「不是单纯路径」，返回 `None`，避免误把 shell 命令当路径。
fn parse_shell_escaped_path(s: &str) -> Option<PathBuf> {
    if !s.contains('\\') {
        // 无转义，要求至少看起来像路径（包含路径分隔符或扩展名）
        return looks_like_path(s).then(|| PathBuf::from(s));
    }

    if contains_shell_metacharacters(s) {
        return None;
    }

    let mut decoded = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.next() {
                Some(escaped) => decoded.push(escaped),
                None => return None, // 末尾裸反斜杠，不当路径
            }
        } else {
            decoded.push(ch);
        }
    }

    // 解码后允许包含空白，但禁止再出现反斜杠（除非就是路径分隔符）
    if decoded.is_empty() {
        return None;
    }
    Some(PathBuf::from(decoded))
}

fn looks_like_path(s: &str) -> bool {
    if s.contains('/') || s.contains('\\') {
        return true;
    }
    // 看起来像 file.ext
    if let Some(dot) = s.rfind('.') {
        dot > 0 && s[dot + 1..].chars().all(|c| c.is_ascii_alphanumeric())
    } else {
        false
    }
}

fn contains_shell_metacharacters(s: &str) -> bool {
    s.chars().any(|c| {
        matches!(
            c,
            '$' | '`'
                | '|'
                | '>'
                | '<'
                | ';'
                | '&'
                | '*'
                | '?'
                | '('
                | ')'
                | '['
                | ']'
                | '{'
                | '}'
                | '!'
                | '~'
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_url_解析为本地路径() {
        assert_eq!(
            normalize_pasted_path("file:///tmp/example.png"),
            Some(PathBuf::from("/tmp/example.png"))
        );
    }

    #[test]
    fn file_url_带主机名_取_path_段() {
        let p = normalize_pasted_path("file://localhost/tmp/x.png");
        assert_eq!(p, Some(PathBuf::from("/tmp/x.png")));
    }

    #[test]
    fn 空字符串_返回_none() {
        assert_eq!(normalize_pasted_path(""), None);
        assert_eq!(normalize_pasted_path("   "), None);
    }

    #[test]
    fn 多行文本_返回_none() {
        assert_eq!(normalize_pasted_path("line1\nline2"), None);
    }

    #[test]
    fn 带引号包裹_识别路径() {
        assert_eq!(
            normalize_pasted_path("\"/tmp/my file.png\""),
            Some(PathBuf::from("/tmp/my file.png"))
        );
    }

    #[test]
    fn unc_路径_保留() {
        let p = normalize_pasted_path(r"\\server\share\file.jpg");
        assert!(p.is_some());
        assert_eq!(p.unwrap().to_string_lossy(), r"\\server\share\file.jpg");
    }

    #[test]
    fn unc_缺少_share_返回_none() {
        assert_eq!(normalize_pasted_path(r"\\server"), None);
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn windows_drive_在_wsl_转_mnt() {
        assert_eq!(
            normalize_pasted_path(r"C:\Temp\example.png"),
            Some(PathBuf::from("/mnt/c/Temp/example.png"))
        );
    }

    #[test]
    #[cfg(not(target_os = "linux"))]
    fn windows_drive_非_linux_原样保留() {
        assert_eq!(
            normalize_pasted_path(r"C:\Temp\example.png"),
            Some(PathBuf::from(r"C:\Temp\example.png"))
        );
    }

    #[test]
    fn shell_转义_反斜杠空格() {
        assert_eq!(
            normalize_pasted_path(r"/home/user/My\ File.png"),
            Some(PathBuf::from("/home/user/My File.png"))
        );
    }

    #[test]
    fn shell_转义_末尾裸反斜杠_返回_none() {
        assert_eq!(normalize_pasted_path(r"/tmp/foo\"), None);
    }

    #[test]
    fn shell_元字符_拒绝() {
        assert_eq!(normalize_pasted_path("/tmp/foo$bar"), None);
        assert_eq!(normalize_pasted_path("/tmp/foo;rm"), None);
        assert_eq!(normalize_pasted_path("/tmp/foo|cat"), None);
    }

    #[test]
    fn 普通单词_不含路径分隔符_无扩展名_返回_none() {
        assert_eq!(normalize_pasted_path("hello"), None);
    }

    #[test]
    fn 普通单词_带扩展名_识别为路径() {
        assert_eq!(
            normalize_pasted_path("readme.md"),
            Some(PathBuf::from("readme.md"))
        );
    }

    #[test]
    fn 相对路径_识别() {
        assert_eq!(
            normalize_pasted_path("./src/main.rs"),
            Some(PathBuf::from("./src/main.rs"))
        );
    }

    #[test]
    fn 去掉引号后再识别_file_url() {
        assert_eq!(
            normalize_pasted_path("\"file:///tmp/x.png\""),
            Some(PathBuf::from("/tmp/x.png"))
        );
    }
}
