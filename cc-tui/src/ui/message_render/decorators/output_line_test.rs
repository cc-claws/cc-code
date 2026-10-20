use super::*;

// ── try_format_json：逐行 JSON 美化 ────────────────────────────────────────

/// 单行 JSON 对象展开成 2 空格缩进（对齐 CC `jsonStringify(parsed, null, 2)`）。
#[test]
fn test_try_format_json_expands_object_with_two_spaces() {
    let pretty = try_format_json(r#"{"a":1,"b":[2,3]}"#).expect("合法 JSON 应美化");
    assert_eq!(pretty, "{\n  \"a\": 1,\n  \"b\": [\n    2,\n    3\n  ]\n}");
}

#[test]
fn test_try_format_json_existing_indent_is_normalized() {
    // 输入已有多余空白也算「同一解析结果」，会统一重排成 2 空格
    let pretty = try_format_json(r#"{ "a" :  1 }"#).expect("合法 JSON 应美化");
    assert_eq!(pretty, "{\n  \"a\": 1\n}");
}

/// 精度守卫：CC 的守卫是为 JS `Number` 设计的（超 2^53 丢精度）；Rust 的
/// serde_json 默认用 `i64`/`u64` 保留整数，**不会**因大整数丢精度，这里锁定该差异——
/// 大整数必须原样美化，不能被丢弃或降级成浮点。
#[test]
fn test_try_format_json_preserves_large_integers() {
    let pretty = try_format_json(r#"{"id":9007199254740993}"#).expect("大整数应可安全美化");
    assert!(
        pretty.contains("9007199254740993"),
        "u64 范围整数必须逐位保留: {pretty}"
    );
}

/// 守卫仍对「重序列化会改写原文」的情况生效：`1.10` 会被归一成 `1.1`，
/// 属于篡改命令输出 → 放弃美化。
#[test]
fn test_try_format_json_rejects_number_rewrite() {
    assert_eq!(try_format_json(r#"{"v":1.10}"#), None);
}

/// 超出 u64/i64 的整数会被 serde_json 降级成 f64，归一化比较不等 → 放弃。
#[test]
fn test_try_format_json_rejects_out_of_range_integer() {
    let line = r#"{"id":123456789012345678901234567890}"#;
    assert_eq!(
        try_format_json(line),
        None,
        "超 u64 的整数会丢精度，必须原样保留"
    );
}

#[test]
fn test_try_format_json_rejects_non_json_lines() {
    for line in [
        "plain text",
        "Error: something failed",
        "{not json}",
        "[unclosed",
        "",
        "   ",
    ] {
        assert_eq!(try_format_json(line), None, "非 JSON 行不得美化: {line:?}");
    }
}

/// 顶层标量美化前后逐字节相同，快速路径直接跳过（不出 `Some`）。
#[test]
fn test_try_format_json_skips_scalars() {
    for line in ["123", r#""just a string""#, "true", "null"] {
        assert_eq!(try_format_json(line), None, "顶层标量无需美化: {line}");
    }
}

/// JSON 允许 `\/` 转义，序列化时不产出它；归一化后应视为等价并美化。
#[test]
fn test_try_format_json_accepts_slash_escape() {
    let pretty = try_format_json(r#"{"url":"a\/b"}"#).expect("不应因 \\/ 差异放弃");
    assert!(pretty.contains("\"url\": \"a/b\""), "实际: {pretty}");
}

/// 数组顶层同样美化。
#[test]
fn test_try_format_json_expands_array() {
    let pretty = try_format_json(r#"[1,2]"#).expect("顶层数组应美化");
    assert_eq!(pretty, "[\n  1,\n  2\n]");
}

// ── try_json_format_content：逐行批量 ──────────────────────────────────────

/// 命中任意一行即返回整段替换文本，未命中的行原样保留。
#[test]
fn test_try_json_format_content_mixes_json_and_plain_lines() {
    let content = "started\n{\"ok\":true}\ndone";
    let out = try_json_format_content(content).expect("含 JSON 行应返回替换文本");
    assert_eq!(out, "started\n{\n  \"ok\": true\n}\ndone");
}

/// 没有任何 JSON 行时返回 `None`，调用方可继续零拷贝借用原文。
#[test]
fn test_try_json_format_content_returns_none_without_json() {
    assert_eq!(try_json_format_content("line one\nline two"), None);
    assert_eq!(try_json_format_content(""), None);
}

/// 超过 `MAX_JSON_FORMAT_LENGTH` 整体跳过（CC 用 JS `length`，此处按字符数）。
#[test]
fn test_try_json_format_content_skips_oversized_content() {
    let mut content = String::from("{\"a\":1}\n");
    content.push_str(&"x".repeat(MAX_JSON_FORMAT_LENGTH));
    assert_eq!(
        try_json_format_content(&content),
        None,
        "超长内容应整体跳过美化"
    );
}

// ── wrap_plain_text：按显示列折行 ─────────────────────────────────────────

#[test]
fn test_wrap_plain_text_breaks_long_line_on_width() {
    let wrapped = wrap_plain_text(&"a".repeat(25), 10);
    assert_eq!(wrapped, vec!["aaaaaaaaaa", "aaaaaaaaaa", "aaaaa"]);
}

#[test]
fn test_wrap_plain_text_counts_cjk_as_two_columns() {
    // 中文每字 2 列：宽度 4 只能放 2 个字
    let wrapped = wrap_plain_text("你好世界", 4);
    assert_eq!(wrapped, vec!["你好", "世界"]);
}

#[test]
fn test_wrap_plain_text_trims_trailing_space_per_line() {
    assert_eq!(
        wrap_plain_text("  indented  \nnext  ", 80),
        vec!["  indented", "next"]
    );
}

// ── truncate_wrapped：3 行截断 + CC 特例 ─────────────────────────────────

#[test]
fn test_truncate_wrapped_keeps_three_lines_and_counts_rest() {
    let content = "l1\nl2\nl3\nl4\nl5";
    let out = truncate_wrapped(content, 80, MAX_LINES_TO_SHOW);
    assert_eq!(out.above_the_fold, "l1\nl2\nl3");
    assert_eq!(out.remaining_lines, 2);
}

/// CC 特例：折叠后只剩 1 行时**直接显示第 4 行**，不出 `+1 line` 提示。
#[test]
fn test_truncate_wrapped_shows_fourth_line_when_only_one_remains() {
    let out = truncate_wrapped("l1\nl2\nl3\nl4", 80, MAX_LINES_TO_SHOW);
    assert_eq!(out.above_the_fold, "l1\nl2\nl3\nl4");
    assert_eq!(out.remaining_lines, 0, "只剩一行时不再提示");
}

#[test]
fn test_truncate_wrapped_short_content_is_untouched() {
    let out = truncate_wrapped("l1\nl2", 80, MAX_LINES_TO_SHOW);
    assert_eq!(out.above_the_fold, "l1\nl2");
    assert_eq!(out.remaining_lines, 0);
}

#[test]
fn test_truncate_wrapped_empty_content() {
    let out = truncate_wrapped("   \n\n  ", 80, MAX_LINES_TO_SHOW);
    assert_eq!(out.above_the_fold, "");
    assert_eq!(out.remaining_lines, 0);
}

/// 单行超宽会先折行再计入 3 行预算。
#[test]
fn test_truncate_wrapped_counts_wrapped_segments() {
    let content = "a".repeat(200);
    let out = truncate_wrapped(&content, 30, MAX_LINES_TO_SHOW);
    let shown = out.above_the_fold.lines().count();
    assert_eq!(shown, MAX_LINES_TO_SHOW, "超宽单行折行后同样只留 3 行");
    assert!(out.remaining_lines > 0, "其余行应被计量");
}

// ── decorate：完整装饰管线 ────────────────────────────────────────────────

/// JSON 美化与截断在同一管线里协作：先用美化后的行数做截断预算。
#[test]
fn test_decorate_formats_json_before_truncating() {
    let content = r#"{"a":1}"#;
    let out = decorate(content, 80, MAX_LINES_TO_SHOW);
    assert!(
        out.above_the_fold.contains("\"a\": 1"),
        "应先美化再截断: {:?}",
        out.above_the_fold
    );
    assert!(out.above_the_fold.starts_with('{'), "美化后应有层级缩进");
}

#[test]
fn test_decorate_passes_through_plain_text() {
    let out = decorate("hello\nworld", 80, MAX_LINES_TO_SHOW);
    assert_eq!(out.above_the_fold, "hello\nworld");
    assert_eq!(out.remaining_lines, 0);
}

/// 无 URL 时不值得走着色路径。
#[test]
fn test_contains_url() {
    assert!(contains_url("see https://example.com/x for details"));
    assert!(!contains_url("no links here, see example.com"));
}
