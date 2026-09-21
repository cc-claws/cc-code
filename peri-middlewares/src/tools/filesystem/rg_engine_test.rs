use super::*;
use std::path::{Path, PathBuf};

#[test]
fn test_resolve_rg_returns_cached_result() {
    // 第一次调用会触发探测，第二次应返回缓存结果
    let first = resolve_rg();
    let second = resolve_rg();
    // 两者应指向同一个结果（要么都是 Some 要么都是 None）
    assert_eq!(first.is_some(), second.is_some(), "rg 探测结果应一致");
}

#[test]
fn test_build_grep_args_basic() {
    let parsed = ParsedArgs {
        pattern: "needle".to_string(),
        path: Some("src/".to_string()),
        glob_filters: vec![],
        _type_filters: vec![],
        _type_excludes: vec![],
        output_mode: OutputMode::Default,
        before_context: 0,
        after_context: 0,
        case_insensitive: false,
        whole_word: false,
        multiline: false,
        line_number: true,
        invert_match: false,
        fixed_strings: false,
        max_depth: None,
    };
    let args = build_grep_args(&parsed, &PathBuf::from("/project/src"), "/project");
    let args_str = args.join(" ");
    assert!(args_str.contains("--no-heading"), "应含 --no-heading: {args_str}");
    assert!(args_str.contains("--line-number"), "应含 --line-number: {args_str}");
    assert!(args_str.contains("--hidden"), "应含 --hidden: {args_str}");
    assert!(args_str.contains("--max-columns 500"), "应含 --max-columns: {args_str}");
    assert!(args_str.contains("!.claude"), "应排除 .claude: {args_str}");
    assert!(args_str.contains("!.worktrees"), "应排除 .worktrees: {args_str}");
    assert!(args_str.contains("needle"), "应含 pattern: {args_str}");
    // smart-case: 全小写模式应自动 -i
    assert!(args_str.contains("-i"), "全小写模式应启用 smart-case: {args_str}");
}

#[test]
fn test_build_grep_args_smart_case_uppercase() {
    let parsed = ParsedArgs {
        pattern: "Needle".to_string(),
        path: None,
        glob_filters: vec![],
        _type_filters: vec![],
        _type_excludes: vec![],
        output_mode: OutputMode::FilesOnly,
        before_context: 0,
        after_context: 0,
        case_insensitive: false,
        whole_word: false,
        multiline: false,
        line_number: true,
        invert_match: false,
        fixed_strings: false,
        max_depth: None,
    };
    let args = build_grep_args(&parsed, &PathBuf::from("/project"), "/project");
    let args_str = args.join(" ");
    // 含大写，不应启用 -i
    assert!(!args_str.contains("-i"), "含大写模式不应启用 -i: {args_str}");
    assert!(args_str.contains("-l"), "FilesOnly 应含 -l: {args_str}");
}

#[test]
fn test_build_grep_args_context() {
    let parsed = ParsedArgs {
        pattern: "test".to_string(),
        path: None,
        glob_filters: vec!["*.rs".to_string()],
        _type_filters: vec![],
        _type_excludes: vec![],
        output_mode: OutputMode::Default,
        before_context: 2,
        after_context: 3,
        case_insensitive: true,
        whole_word: true,
        multiline: false,
        line_number: true,
        invert_match: false,
        fixed_strings: false,
        max_depth: Some(3),
    };
    let args = build_grep_args(&parsed, &PathBuf::from("/project"), "/project");
    let args_str = args.join(" ");
    assert!(args_str.contains("-C 3"), "对称上下文应取较大值: {args_str}");
    assert!(args_str.contains("-i"), "显式 -i: {args_str}");
    assert!(args_str.contains("-w"), "whole_word: {args_str}");
    assert!(args_str.contains("--max-depth 3"), "max_depth: {args_str}");
    assert!(args_str.contains("*.rs"), "glob filter: {args_str}");
}

#[test]
fn test_build_grep_args_all_flags() {
    let parsed = ParsedArgs {
        pattern: "test".to_string(),
        path: None,
        glob_filters: vec![],
        _type_filters: vec![],
        _type_excludes: vec![],
        output_mode: OutputMode::CountOnly,
        before_context: 0,
        after_context: 0,
        case_insensitive: false,
        whole_word: false,
        multiline: true,
        line_number: true,
        invert_match: true,
        fixed_strings: true,
        max_depth: None,
    };
    let args = build_grep_args(&parsed, &PathBuf::from("/project"), "/project");
    let args_str = args.join(" ");
    assert!(args_str.contains("-c"), "CountOnly 应含 -c: {args_str}");
    assert!(args_str.contains("-F"), "fixed_strings: {args_str}");
    assert!(args_str.contains("-v"), "invert_match: {args_str}");
    assert!(args_str.contains("-U"), "multiline: {args_str}");
    assert!(args_str.contains("--multiline-dotall"), "multiline dotall: {args_str}");
}

#[test]
fn test_build_glob_args() {
    let args = build_glob_args("**/*.rs", &PathBuf::from("/project"));
    let args_str = args.join(" ");
    assert!(args_str.contains("--files"), "应含 --files: {args_str}");
    assert!(args_str.contains("--hidden"), "应含 --hidden: {args_str}");
    assert!(args_str.contains("!.git"), "应排除 .git: {args_str}");
    assert!(args_str.contains("!.claude"), "应排除 .claude: {args_str}");
    assert!(args_str.contains("!.worktrees"), "应排除 .worktrees: {args_str}");
    assert!(args_str.contains("!node_modules"), "应排除 node_modules: {args_str}");
    assert!(args_str.contains("!target"), "应排除 target: {args_str}");
    assert!(args_str.contains("**/*.rs"), "应含 pattern: {args_str}");
}

#[test]
fn test_convert_rg_line() {
    let cwd = Path::new("/project");
    let line = "/project/src/main.rs:42: fn main() {";
    let result = convert_rg_line(line, cwd);
    assert_eq!(result, "src/main.rs:42: fn main() {");
}

#[test]
fn test_convert_rg_line_non_matching_prefix() {
    let cwd = Path::new("/project");
    let line = "/other/file.rs:10: hello";
    let result = convert_rg_line(line, cwd);
    // 不匹配的绝对路径应保持原样
    assert_eq!(result, "/other/file.rs:10: hello");
}

#[test]
fn test_convert_rg_line_relative() {
    let cwd = Path::new("/project");
    let line = "src/main.rs:5: hello world";
    let result = convert_rg_line(line, cwd);
    assert_eq!(result, "src/main.rs:5: hello world");
}

#[test]
fn test_format_grep_output_default_no_matches() {
    let result = format_grep_output(OutputMode::Default, &[], 0, 250, 0, false);
    assert_eq!(result, "No matches found");
}

#[test]
fn test_format_grep_output_files_only() {
    let items = vec!["a.txt".to_string(), "b.txt".to_string()];
    let result = format_grep_output(OutputMode::FilesOnly, &items, 2, 250, 0, false);
    assert!(result.starts_with("Found 2 files\n"), "FilesOnly 格式: {result}");
    assert!(result.contains("a.txt"), "应含 a.txt: {result}");
    assert!(result.contains("b.txt"), "应含 b.txt: {result}");
}

#[test]
fn test_format_grep_output_files_only_truncated() {
    let items = vec!["a.txt".to_string(), "b.txt".to_string()];
    let result = format_grep_output(OutputMode::FilesOnly, &items, 5, 2, 0, true);
    assert!(result.starts_with("Found 2 files limit: 2"), "截断分页: {result}");
}

#[test]
fn test_format_grep_output_count() {
    let items = vec!["a.txt:3".to_string(), "b.txt:1".to_string()];
    let result = format_grep_output(OutputMode::CountOnly, &items, 2, 250, 0, false);
    assert!(result.contains("a.txt:3"), "count 行: {result}");
    assert!(result.contains("Found 4 total occurrences across 2 files."), "count summary: {result}");
}

#[test]
fn test_format_grep_output_count_no_matches() {
    let result = format_grep_output(OutputMode::CountOnly, &[], 0, 250, 0, false);
    assert_eq!(
        result, "No matches found\n\nFound 0 total occurrences across 0 files.",
        "count 无匹配格式"
    );
}
