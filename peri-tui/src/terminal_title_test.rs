use super::*;

#[test]
fn test_terminal_title_format_idle_unnamed() {
    let item = TerminalTitleItem::new(TerminalTitleStatusKind::Idle, "peri", None, None);
    assert_eq!(item.format_title(), "peri", "未命名时 Idle 展示项目名");
}

#[test]
fn test_terminal_title_format_idle_named() {
    let item = TerminalTitleItem::new(TerminalTitleStatusKind::Idle, "peri", Some("fix-bug"), None);
    assert_eq!(item.format_title(), "fix-bug", "已命名时 Idle 直接展示主题");
}

#[test]
fn test_terminal_title_format_working_unnamed() {
    let item = TerminalTitleItem::new(TerminalTitleStatusKind::Working, "peri", None, Some("⠋"));
    assert_eq!(item.format_title(), "⠋ peri", "未命名时 Working 为 ⠋ project");
}

#[test]
fn test_terminal_title_format_working_named() {
    let item = TerminalTitleItem::new(TerminalTitleStatusKind::Working, "peri", Some("fix-bug"), Some("⠙"));
    assert_eq!(item.format_title(), "⠙ fix-bug", "已命名时 Working 为 ⠙ thread");
}

#[test]
fn test_terminal_title_format_thinking_unnamed() {
    let item = TerminalTitleItem::new(TerminalTitleStatusKind::Thinking, "peri", None, Some("⠋"));
    assert_eq!(item.format_title(), "⠋ peri", "未命名时 Thinking 为 ⠋ project");
}

#[test]
fn test_terminal_title_format_thinking_named() {
    let item = TerminalTitleItem::new(TerminalTitleStatusKind::Thinking, "peri", Some("fix-bug"), Some("⠋"));
    assert_eq!(item.format_title(), "⠋ fix-bug", "已命名时 Thinking 为 ⠋ thread");
}

#[test]
fn test_terminal_title_format_action_required_unnamed() {
    let item = TerminalTitleItem::new(TerminalTitleStatusKind::ActionRequired, "peri", None, None);
    assert_eq!(item.format_title(), "[ ! ] Action Required  peri", "未命名时 ActionRequired");
}

#[test]
fn test_terminal_title_format_action_required_named() {
    let item = TerminalTitleItem::new(TerminalTitleStatusKind::ActionRequired, "peri", Some("fix-bug"), None);
    assert_eq!(item.format_title(), "[ ! ] Action Required  fix-bug", "已命名时 ActionRequired 为 [ ! ] Action Required  thread");
}

#[test]
fn test_terminal_title_format_done_unnamed() {
    let item = TerminalTitleItem::new(TerminalTitleStatusKind::Done, "peri", None, None);
    assert_eq!(item.format_title(), "✴ peri", "未命名完成态带菊花 ✴ project");
}

#[test]
fn test_terminal_title_format_done_named() {
    let item = TerminalTitleItem::new(TerminalTitleStatusKind::Done, "peri", Some("fix-bug"), None);
    assert_eq!(item.format_title(), "✴ fix-bug", "已命名完成态带菊花 ✴ thread");
}

#[test]
fn test_sanitize_title_filters_control_characters() {
    let raw = "peri\x00\x07\x1b[31m\x7F | fix\r\n";
    let cleaned = sanitize_title(raw);
    assert_eq!(cleaned, "peri[31m | fix", "未能正确过滤控制字符与回车换行");
}

#[test]
fn test_sanitize_title_collapses_whitespace_runs() {
    // 对齐 Codex 官方测试 sanitizes_terminal_title
    let raw = "  Project\t|\nWorking\x1b\x07 |   Thread  ";
    let cleaned = sanitize_title(raw);
    assert_eq!(cleaned, "Project | Working | Thread", "未将空白符折叠为单空格并去除首尾空格");
}

#[test]
fn test_sanitize_title_filters_bidi_and_zero_width() {
    // 对齐 Codex 官方测试 strips_invisible_format_chars_from_terminal_title
    let raw = "Pro\u{202E}j\u{2066}e\u{200F}c\u{061C}t\u{200B} \u{FEFF}T\u{2060}itle";
    let cleaned = sanitize_title(raw);
    assert_eq!(cleaned, "Project Title", "未能正确过滤 Bidi 与零宽字符");
}

#[test]
fn test_sanitize_title_hard_cap_240_chars() {
    // 对齐 Codex 官方测试 truncates_terminal_title
    let raw = "a".repeat(300);
    let cleaned = sanitize_title(&raw);
    assert_eq!(cleaned.len(), MAX_TERMINAL_TITLE_CHARS, "未能将标题硬截断至 240 字符");
}

#[test]
fn test_truncate_terminal_title_part_grapheme_boundary() {
    let raw = "a".repeat(30);
    let truncated = truncate_terminal_title_part(&raw, 24);
    assert_eq!(truncated.chars().count(), 24, "字符截断长度不符合预期");
    assert!(truncated.ends_with("..."), "截断未以 ... 结尾");
}

#[test]
fn test_truncate_terminal_title_part_chinese_graphemes() {
    let raw = "这是一个超长并且带有中文和Emoji的会话标题名称测试字符串🚀🚀🚀";
    let truncated = truncate_terminal_title_part(raw, 10);
    assert!(truncated.ends_with("..."), "中文截断未以 ... 结尾");
    assert!(truncated.chars().count() <= 10, "中文截断字符数超出限制");
}

#[test]
fn test_extract_thread_title_chinese_simple() {
    let prompt = "帮我实现动态终端标题功能，参考 codex-rs";
    let title = extract_thread_title(prompt);
    assert_eq!(title.as_deref(), Some("实现动态终端标题功能"), "中文前缀剥离与标点截断不符合预期");
}

#[test]
fn test_extract_thread_title_cjk_numbered_list_no_panic() {
    // 关键回归测试：包含多字节 UTF-8 顿号 '、' 的有序列表前缀，绝不能发生字节切片 panic
    let prompt = "1、帮我修复系统登录死锁问题";
    let title = extract_thread_title(prompt);
    assert_eq!(title.as_deref(), Some("修复系统登录死锁问题"), "中文顿号有序列表前缀剥离不符合预期");

    let prompt_dot = "2. 帮我重构订单模块";
    let title_dot = extract_thread_title(prompt_dot);
    assert_eq!(title_dot.as_deref(), Some("重构订单模块"), "英文点号有序列表前缀剥离不符合预期");
}

#[test]
fn test_extract_thread_title_chinese_with_url() {
    let prompt = "https://github.com/cc-claws/cc-code/issues/162 实现这个功能吧";
    let title = extract_thread_title(prompt);
    assert_eq!(title.as_deref(), Some("实现这个功能吧"), "URL 过滤与中文提取不符合预期");
}

#[test]
fn test_extract_thread_title_english_simple() {
    let prompt = "Please fix the issue where terminal title flickers on windows";
    let title = extract_thread_title(prompt);
    assert_eq!(title.as_deref(), Some("fix the issue where terminal"), "英文前缀过滤与词数截断不符合预期");
}

#[test]
fn test_extract_thread_title_code_block_skipped() {
    let prompt = "```rust\nfn main() {}\n```\n帮我分析这段代码";
    let title = extract_thread_title(prompt);
    assert_eq!(title.as_deref(), Some("分析这段代码"), "代码块标记跳过与中文提取不符合预期");
}

#[test]
fn test_extract_thread_title_system_reminder_filtered() {
    let prompt = "<system-reminder>\nsome reminder\n</system-reminder>\n修复编译错误";
    let title = extract_thread_title(prompt);
    assert_eq!(title.as_deref(), Some("修复编译错误"), "system-reminder 标签过滤不符合预期");
}

#[test]
fn test_extract_thread_title_empty_or_whitespace() {
    assert_eq!(extract_thread_title(""), None, "空字符串应返回 None");
    assert_eq!(extract_thread_title("   \n\t  "), None, "纯空白字符应返回 None");
}
