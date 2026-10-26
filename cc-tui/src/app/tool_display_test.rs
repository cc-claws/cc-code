#[test]
fn test_format_tool_name_new_names() {
    assert_eq!(format_tool_name("Read"), "Read");
    assert_eq!(format_tool_name("Write"), "Write");
    assert_eq!(format_tool_name("Edit"), "Update");
    assert_eq!(format_tool_name("Glob"), "Glob");
    assert_eq!(format_tool_name("Grep"), "Grep");
    assert_eq!(format_tool_name("Bash"), "Bash");
    assert_eq!(format_tool_name("TodoWrite"), "Todo");
    assert_eq!(format_tool_name("AskUserQuestion"), "Ask");
    assert_eq!(format_tool_name("Agent"), "Agent");
    assert_eq!(format_tool_name("Skill"), "Skill");
    assert_eq!(format_tool_name_for_stats("Skill"), "Skill");
}

#[test]
fn test_format_tool_args_skill_only_shows_name() {
    let input = serde_json::json!({"skill": "commit", "args": "-m \"fix 中文\""});
    assert_eq!(
        format_tool_args("Skill", &input, None).as_deref(),
        Some("commit"),
        "对齐 CC：技能工具头仅展示名称，参数不进入摘要"
    );
    let input = serde_json::json!({"skill": "example-skill", "args": "  "});
    assert_eq!(
        format_tool_args("Skill", &input, None).as_deref(),
        Some("example-skill"),
        "空白参数不应留下多余分隔符"
    );
    assert_eq!(
        format_tool_args("Skill", &serde_json::json!({"args": "orphan"}), None),
        None,
        "缺少技能名时不应把 args 误认为名称"
    );
}

#[test]
fn test_format_tool_args_skill_sanitizes_and_truncates_unicode() {
    let input = serde_json::json!({
        "skill": format!("查询技能 \u{1b}[31m{}\n参数", "中文".repeat(TOOL_ARGS_MAX_CHARS)),
        "args": "参数不会显示"
    });
    let result = format_tool_args("Skill", &input, None).expect("应生成技能摘要");
    assert!(result.starts_with("查询技能 中文"), "应保留完整中文字符");
    assert!(result.ends_with('…'), "超长参数应以省略号闭合");
    assert!(
        !result.contains('\u{1b}') && !result.contains('\n'),
        "终端控制序列不能进入摘要"
    );
    assert_eq!(result.chars().count(), TOOL_ARGS_MAX_CHARS + 1);
}

#[test]
fn test_format_tool_name_with_input_edit_variants() {
    // CC: old_string 非空 → Update
    let edit_input = serde_json::json!({
        "file_path": "src/main.rs",
        "old_string": "old",
        "new_string": "new"
    });
    assert_eq!(
        format_tool_name_with_input("Edit", Some(&edit_input)),
        "Update"
    );
    // CC: old_string 为空字符串 → Create（等价新建文件）
    let create_input = serde_json::json!({
        "file_path": "src/main.rs",
        "old_string": "",
        "new_string": "content"
    });
    assert_eq!(
        format_tool_name_with_input("Edit", Some(&create_input)),
        "Create"
    );
    // CC: 无 input → Update
    assert_eq!(format_tool_name_with_input("Edit", None), "Update");
    // 其他工具不受影响
    let write_input = serde_json::json!({"file_path": "a.txt", "content": "x"});
    assert_eq!(
        format_tool_name_with_input("Write", Some(&write_input)),
        "Write"
    );
}

#[test]
fn test_format_tool_args_grep_uses_pattern() {
    let input = serde_json::json!({"pattern": "needle", "output_mode": "content"});
    let result = format_tool_args("Grep", &input, None);
    assert!(result.is_some(), "Grep 工具应返回 pattern 摘要");
    assert!(result.unwrap().contains("needle"), "应包含 pattern 内容");
}

#[test]
fn test_format_tool_args_bash_uses_command() {
    let input = serde_json::json!({"command": "cargo test"});
    let result = format_tool_args("Bash", &input, None);
    assert!(result.is_some());
    assert!(result.unwrap().contains("cargo test"));
}

#[test]
fn test_format_tool_args_bash_strips_terminal_control_sequences() {
    let input = serde_json::json!({
        "command": "echo ok \u{1b}[<555;106;49M \u{1b}[31mred\u{1b}[0m done"
    });
    let result = format_tool_args("Bash", &input, None).expect("Bash 应返回 command 摘要");
    assert!(
        !result.contains('\u{1b}'),
        "不应保留 ESC 控制字符: {result:?}"
    );
    assert!(
        !result.contains("[<555;106;49M"),
        "不应保留 SGR 鼠标坐标序列: {result:?}"
    );
    assert!(result.contains("red"), "应保留普通文本内容: {result:?}");
    assert!(result.contains("done"), "应保留普通文本内容: {result:?}");
}

#[test]
fn test_sanitize_display_text_strips_osc_sequence() {
    let result = sanitize_display_text("before\u{1b}]0;bad title\u{7}after");
    assert_eq!(result, "beforeafter", "OSC 序列不应进入 TUI 文本");
}

#[test]
fn test_sanitize_display_text_preserving_newlines_keeps_script_indentation() {
    let script = "if true; then\r\n\t\u{1b}[31mecho 中文\u{1b}[0m\nfi";
    let result = sanitize_display_text_preserving_newlines(script);
    assert_eq!(
        result, "if true; then\n    echo 中文\nfi",
        "正文应保留脚本换行与缩进，并移除终端颜色控制序列"
    );
    assert_eq!(
        sanitize_display_text(script),
        "if true; then   echo 中文 fi",
        "既有单行摘要行为不变"
    );
}

#[test]
fn test_sanitize_display_text_preserving_newlines_removes_multiline_control_sequences() {
    let result = sanitize_display_text_preserving_newlines(
        "before\u{1b}]0;hidden\nOSC title\u{7}after\n\u{1b}Ppayload\nmore\u{1b}\\done",
    );
    assert_eq!(
        result, "beforeafter\ndone",
        "跨行 OSC 和字符串控制序列应整体移除，不能泄漏隐藏载荷"
    );
}

#[test]
fn test_old_tool_names_not_matched() {
    // 验证旧工具名不再被匹配（fallback 到 to_pascal）
    assert_eq!(format_tool_name("bash"), "Bash"); // fallback
    assert_eq!(format_tool_name("read_file"), "ReadFile"); // fallback to_pascal
    assert_eq!(format_tool_name("write_file"), "WriteFile"); // fallback to_pascal
    assert_eq!(format_tool_name("search_files_rg"), "SearchFilesRg"); // fallback to_pascal
    assert_eq!(format_tool_name("launch_agent"), "LaunchAgent"); // fallback to_pascal
}

#[test]
fn test_read_write_edit_file_path_not_truncated() {
    // Read/Write/Edit 的 file_path 完整显示不截断
    let path = "/home/user/projects/my-app/src/components/header.rs";
    let input = serde_json::json!({"file_path": path});
    let result = format_tool_args("Read", &input, Some("/home/user/projects/my-app/"));
    assert_eq!(
        result.unwrap(),
        "src/components/header.rs",
        "file_path 应完整显示不截断"
    );

    let result = format_tool_args("Write", &input, Some("/home/user/"));
    assert_eq!(
        result.unwrap(),
        "projects/my-app/src/components/header.rs",
        "file_path 应完整显示不截断"
    );

    let result = format_tool_args("Edit", &input, None);
    // 无 cwd 时按 CC `getDisplayPath` 保留绝对路径（旧实现退化取 basename）
    assert_eq!(result.unwrap(), path);
}

#[test]
fn test_bash_truncates_at_max_chars() {
    // Bash 命令仅保留一个防御性上限（详细模式需完整展示命令，故上限远大于旧值）
    let cmd = "a".repeat(BASH_COMMAND_MAX_CHARS + 100);
    let input = serde_json::json!({"command": cmd});
    let result = format_tool_args("Bash", &input, None).unwrap();
    assert_eq!(
        result.chars().count(),
        BASH_COMMAND_MAX_CHARS + 1,
        "超长 Bash 命令应截断到上限字符 + …"
    );
    assert!(result.ends_with('…'), "超长 Bash 命令应以 … 结尾");
    assert!(result.starts_with('a'), "Bash 命令应保留前缀内容");
}

#[test]
fn test_bash_command_under_max_chars_not_truncated() {
    // 数十 KB 以内的命令不应被截断，保证详细模式可完整折行展示
    let cmd = "git log --graph --oneline ".repeat(50);
    let input = serde_json::json!({"command": cmd.clone()});
    let result = format_tool_args("Bash", &input, None).unwrap();
    assert_eq!(result, cmd, "未超过上限的 Bash 命令应完整保留");
}

#[test]
fn test_websearch_and_webfetch_not_truncated_at_sixty_chars() {
    let query =
        "what is the best way to handle visual line wrapping in rust terminal ui and ratatui";
    assert!(query.len() > 60);
    let input = serde_json::json!({"query": query});
    let result = format_tool_args("WebSearch", &input, None).unwrap();
    assert_eq!(
        result, query,
        "超过 60 字符的 WebSearch query 不应被过早截断"
    );

    let url = "https://example.com/very/long/url/path/with/lots/of/parameters?query=rust&framework=ratatui&mode=detail";
    assert!(url.len() > 60);
    let fetch_input = serde_json::json!({"url": url});
    let fetch_result = format_tool_args("WebFetch", &fetch_input, None).unwrap();
    assert_eq!(
        fetch_result, url,
        "超过 60 字符的 WebFetch url 不应被过早截断"
    );
}

#[test]
fn test_glob_truncates_at_200() {
    let pattern = "p".repeat(300);
    let input = serde_json::json!({"pattern": pattern, "path": "/tmp"});
    let result = format_tool_args("Glob", &input, None).unwrap();
    assert_eq!(
        result.chars().count(),
        201,
        "Glob pattern 应截断到 200 字符 + …"
    );
    assert!(result.ends_with('…'), "超长 Glob pattern 应以 … 结尾");
}

#[test]
fn test_read_fallback_to_path_alias() {
    // LLM 有时发 path 而非 file_path，应回退读取
    let input = serde_json::json!({"path": "/home/user/project/src/main.rs"});
    let result = format_tool_args("Read", &input, Some("/home/user/project/"));
    assert_eq!(result.as_deref(), Some("src/main.rs"), "应回退到 path 字段");

    // 无 cwd 时 CC `getDisplayPath` 保留绝对路径，不再退化成 basename
    let input = serde_json::json!({"path": "/tmp/test.txt"});
    let result = format_tool_args("Write", &input, None);
    assert_eq!(
        result.as_deref(),
        Some("/tmp/test.txt"),
        "Write 也应回退到 path"
    );

    let input = serde_json::json!({"path": "/tmp/edit.rs"});
    let result = format_tool_args("Edit", &input, None);
    assert_eq!(
        result.as_deref(),
        Some("/tmp/edit.rs"),
        "Edit 也应回退到 path"
    );
}

#[test]
fn test_read_file_path_takes_priority_over_path() {
    // file_path 存在时优先使用 file_path
    let input = serde_json::json!({"file_path": "/a/real.rs", "path": "/b/alias.rs"});
    let result = format_tool_args("Read", &input, None);
    assert_eq!(
        result.as_deref(),
        Some("/a/real.rs"),
        "file_path 应优先于 path"
    );
}

#[test]
fn test_glob_pattern_not_path_stripped() {
    // Glob pattern 不应走 strip_cwd（pattern 不是文件路径）
    let input = serde_json::json!({"pattern": "app/admin/**/*.php"});
    let result = format_tool_args("Glob", &input, Some("/home/user/project/"));
    assert_eq!(
        result.as_deref(),
        Some("app/admin/**/*.php"),
        "Glob pattern 不应被路径剥离"
    );
}

#[test]
fn test_grep_truncates_at_200() {
    let pattern = "r".repeat(300);
    let input = serde_json::json!({"pattern": pattern});
    let result = format_tool_args("Grep", &input, None).unwrap();
    assert_eq!(
        result.chars().count(),
        201,
        "Grep pattern 应截断到 200 字符 + …"
    );
    assert!(result.ends_with('…'), "超长 Grep pattern 应以 … 结尾");
}

#[test]
fn test_read_offset_limit_shows_range() {
    // offset=100, limit=50 → 显示 100-149（参考实现的 ` · lines 起-止` 文案）
    let input = serde_json::json!({"file_path": "/tmp/main.rs", "offset": 100, "limit": 50});
    let result = format_tool_args("Read", &input, None);
    assert_eq!(result.as_deref(), Some("/tmp/main.rs · lines 100-149"));
}

#[test]
fn test_read_offset_only_shows_open_range() {
    // 只有 offset 无 limit → 显示 from line 起（参考实现的 verbose 文案）
    let input = serde_json::json!({"file_path": "/tmp/main.rs", "offset": 855});
    let result = format_tool_args("Read", &input, None);
    assert_eq!(result.as_deref(), Some("/tmp/main.rs · from line 855"));
}

#[test]
fn test_read_no_offset_limit_shows_path_only() {
    // 无 offset/limit → 纯路径，不追加行号
    let input = serde_json::json!({"file_path": "/tmp/main.rs"});
    let result = format_tool_args("Read", &input, None);
    assert_eq!(result.as_deref(), Some("/tmp/main.rs"));
}

#[test]
fn test_read_offset_1_still_shows_range() {
    // 对齐参考实现：offset 或 limit 任一存在就显示，offset=1 不省略
    let input = serde_json::json!({"file_path": "/tmp/main.rs", "offset": 1, "limit": 32});
    let result = format_tool_args("Read", &input, None);
    assert_eq!(result.as_deref(), Some("/tmp/main.rs · lines 1-32"));
}

#[test]
fn test_read_limit_only_starts_from_line_one() {
    // 只有 limit 无 offset → 起始行缺省为 1
    let input = serde_json::json!({"file_path": "/tmp/main.rs", "limit": 32});
    let result = format_tool_args("Read", &input, None);
    assert_eq!(result.as_deref(), Some("/tmp/main.rs · lines 1-32"));
}

#[test]
fn test_read_offset_limit_with_cwd() {
    let input = serde_json::json!({"file_path": "/home/user/project/src/main.rs", "offset": 10, "limit": 20});
    let result = format_tool_args("Read", &input, Some("/home/user/project/"));
    assert_eq!(result.as_deref(), Some("src/main.rs · lines 10-29"));
}

#[test]
fn test_read_summary_keeps_filename_with_colon_digits() {
    // 文件名自带 `:123` 时，紧凑摘要不得把它当成行号范围剥掉
    let input = serde_json::json!({"file_path": "/tmp/a:123"});
    let result = format_tool_args("Read", &input, None);
    assert_eq!(result.as_deref(), Some("/tmp/a:123"));
}

#[test]
fn test_build_diff_input_edit_uses_full_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sample.rs");
    let before = "line1\nline2\nline3\nline4\nold line\nline6\nline7\nline8\nline9\nline10\n";
    let after = before.replace("old line", "new line");
    std::fs::write(&path, &after).unwrap();

    let input = serde_json::json!({
        "file_path": path.to_string_lossy(),
        "old_string": "old line",
        "new_string": "new line"
    });
    let di = build_diff_input("Edit", &input, None, None).expect("应构造 DiffInput");
    assert!(!di.is_new_file, "Edit 应标记为更新");
    assert!(
        di.old_content.contains("old line") && di.old_content.contains("line1"),
        "old_content 应为编辑前全文，实际: {}",
        di.old_content
    );
    assert!(
        di.new_content.contains("new line") && di.new_content.contains("line10"),
        "new_content 应为编辑后全文，实际: {}",
        di.new_content
    );
    assert_eq!(
        di.old_content.lines().count(),
        10,
        "旧全文应包含全部 10 行（含上下文）"
    );
}

#[test]
fn test_build_diff_input_edit_falls_back_when_file_missing() {
    let input = serde_json::json!({
        "file_path": "no_such_dir/no_such_file.rs",
        "old_string": "old line",
        "new_string": "new line"
    });
    let di = build_diff_input("Edit", &input, None, None).expect("应回退片段 diff");
    assert_eq!(di.old_content, "old line", "文件不可读时应回退片段 diff");
    assert_eq!(di.new_content, "new line");
}

#[test]
fn test_build_diff_input_edit_falls_back_when_new_string_absent() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sample.rs");
    std::fs::write(&path, "totally different content\n").unwrap();

    let input = serde_json::json!({
        "file_path": path.to_string_lossy(),
        "old_string": "old line",
        "new_string": "new line"
    });
    let di = build_diff_input("Edit", &input, None, None).expect("应回退片段 diff");
    assert_eq!(
        di.old_content, "old line",
        "磁盘内容不含 new_string 时应回退片段 diff"
    );
}

/// 端到端：点击编辑的 diff 渲染应带上下文行与真实行号（对齐 CC structuredPatch 展示）
#[test]
fn test_edit_diff_render_shows_context_lines() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sample.rs");
    let before = concat!(
        "fn main() {\n",
        "    let a = 1;\n",
        "    let b = 2;\n",
        "    let c = 3;\n",
        "    let d = 4;\n",
        "    let target = 5;\n",
        "    let e = 6;\n",
        "    let f = 7;\n",
        "    let g = 8;\n",
        "}\n",
    );
    let after = before.replace("let target = 5;", "let target = 50;");
    std::fs::write(&path, &after).unwrap();

    let input = serde_json::json!({
        "file_path": path.to_string_lossy(),
        "old_string": "    let target = 5;",
        "new_string": "    let target = 50;"
    });
    let di = build_diff_input("Edit", &input, None, None).expect("应构造全文 DiffInput");
    let lines = cc_widgets::diff::render_diff(&di, 80, &cc_widgets::DarkTheme);
    let text: String = lines
        .iter()
        .map(|l| {
            l.spans
                .iter()
                .map(|s| s.content.as_ref())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");
    println!("\n===== 渲染结果 =====\n{text}\n====================");

    assert!(
        text.contains("let b = 2;") && text.contains("let f = 7;"),
        "应包含改动行前后各 3 行上下文（第 3-9 行）:\n{text}"
    );
    assert!(
        text.contains("let target = 50;") && text.contains("let target = 5;"),
        "应包含改动前后内容:\n{text}"
    );
    assert!(
        text.contains(" 6 "),
        "行号应为真实文件行号（第 6 行）:\n{text}"
    );
    assert!(
        text.contains("Added 1 line, removed 1 line"),
        "应含摘要行:\n{text}"
    );
}

/// P1-1 回归：replace_all 无法从编辑后内容区分"原有的 new_string"与"本次生成的"，
/// 反推可能得到错误的旧全文，必须回退片段 diff。
#[test]
fn test_build_diff_input_replace_all_ambiguous_uses_snippet() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sample.txt");
    // 编辑前 `bar foo`，编辑后 `bar bar`（replace_all: foo → bar）
    std::fs::write(&path, "bar bar\n").unwrap();
    let input = serde_json::json!({
        "file_path": path.to_string_lossy(),
        "old_string": "foo",
        "new_string": "bar",
        "replace_all": true
    });
    let di = build_diff_input("Edit", &input, None, None).expect("应构造 DiffInput");
    assert_eq!(
        di.old_content, "foo",
        "replace_all 反推存在歧义时应回退片段 diff，实际 old_content: {:?}",
        di.old_content
    );
}

/// Write 覆盖已有文件：用 ToolStart 抓取的旧内容做真 diff（对齐 CC FileWriteTool）
#[test]
fn test_build_diff_input_write_existing_file_uses_old_content() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.txt");
    std::fs::write(&path, "line1\nline2\n").unwrap();
    let cwd = dir.path().to_str().unwrap();
    let input = serde_json::json!({
        "file_path": "a.txt",
        "content": "line1\nline2 changed\n"
    });
    cache_write_old_content("tc-write-update", Some("line1\nline2\n".into()));
    let di = build_diff_input("Write", &input, Some(cwd), Some("tc-write-update"))
        .expect("应构造 DiffInput");
    assert!(!di.is_new_file, "覆盖已有文件应按 update 渲染");
    assert_eq!(di.old_content, "line1\nline2\n");
    assert_eq!(di.new_content, "line1\nline2 changed\n");
}

/// Write 目标不存在：仍按 create 全绿视图渲染
#[test]
fn test_build_diff_input_write_new_file_stays_create() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = dir.path().to_str().unwrap();
    let input = serde_json::json!({"file_path": "new.txt", "content": "hello\n"});
    cache_write_old_content("tc-write-create", None);
    let di = build_diff_input("Write", &input, Some(cwd), Some("tc-write-create"))
        .expect("应构造 DiffInput");
    assert!(di.is_new_file, "文件原本不存在应按 create 渲染");
    assert!(di.old_content.is_empty());
}

/// 无 ToolStart 记录（历史回放）：回退 create 视图，不误判为 update
#[test]
fn test_build_diff_input_write_without_cache_falls_back_to_create() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.txt"), "old\n").unwrap();
    let cwd = dir.path().to_str().unwrap();
    let input = serde_json::json!({"file_path": "a.txt", "content": "new\n"});
    let di = build_diff_input("Write", &input, Some(cwd), Some("tc-write-miss"))
        .expect("应构造 DiffInput");
    assert!(di.is_new_file, "缓存未命中时应回退 create 视图");
}

/// 写入内容与旧内容一致：不产生空 diff，保留工具结果行
#[test]
fn test_build_diff_input_write_identical_content_returns_none() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.txt"), "same\n").unwrap();
    let cwd = dir.path().to_str().unwrap();
    let input = serde_json::json!({"file_path": "a.txt", "content": "same\n"});
    cache_write_old_content("tc-write-same", Some("same\n".into()));
    assert!(
        build_diff_input("Write", &input, Some(cwd), Some("tc-write-same")).is_none(),
        "内容未变化时不应渲染空 diff"
    );
}

/// append 写：旧内容 + 追加内容组成真 diff
#[test]
fn test_build_diff_input_write_append_uses_concatenated_content() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.txt"), "a\n").unwrap();
    let cwd = dir.path().to_str().unwrap();
    let input = serde_json::json!({"file_path": "a.txt", "content": "b\n", "append": true});
    cache_write_old_content("tc-write-append", Some("a\n".into()));
    let di = build_diff_input("Write", &input, Some(cwd), Some("tc-write-append"))
        .expect("应构造 DiffInput");
    assert!(!di.is_new_file);
    assert_eq!(di.old_content, "a\n");
    assert_eq!(di.new_content, "a\nb\n");
}
