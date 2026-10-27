// memory 模块的单元测试

#[test]
fn test_sanitize_project_path_windows_and_posix() {
    // Windows 风格路径
    let win = sanitize_project_path(r"D:\code\peri");
    assert_eq!(win, "D--code-peri");

    let win_verbatim = sanitize_project_path(r"\\?\D:\code\peri");
    assert_eq!(win_verbatim, "D--code-peri");

    // POSIX 风格路径
    let posix = sanitize_project_path("/home/user/project");
    assert_eq!(posix, "-home-user-project");
}

#[test]
fn test_sanitize_project_path_length_limit() {
    let long_name = "a".repeat(250);
    let sanitized = sanitize_project_path(&long_name);
    assert!(sanitized.len() > 200);
    assert_eq!(&sanitized[..200], &"a".repeat(200));
    assert!(sanitized.contains('-'));
}

#[test]
fn test_truncate_entrypoint_content_within_limits() {
    let raw = "- [Title](file.md) — one line description\n- [Second](second.md) — second line";
    let res = truncate_entrypoint_content(raw);
    assert!(!res.was_line_truncated);
    assert!(!res.was_byte_truncated);
    assert_eq!(res.content, raw);
    assert_eq!(res.line_count, 2);
}

#[test]
fn test_truncate_entrypoint_content_line_cap() {
    let lines: Vec<String> = (0..250)
        .map(|i| format!("- [Line {}](file{}.md)", i, i))
        .collect();
    let raw = lines.join("\n");
    let res = truncate_entrypoint_content(&raw);
    assert!(res.was_line_truncated);
    assert!(res.content.contains("> WARNING: MEMORY.md is 250 lines"));
    // 截断后内容应包含前 200 行
    assert!(res.content.contains("- [Line 0]"));
    assert!(res.content.contains("- [Line 199]"));
    assert!(!res.content.contains("- [Line 200]"));
}

#[test]
fn test_truncate_entrypoint_content_byte_cap() {
    // 构造单行极长（超过 25,000 字节）的异常内容
    let line = format!("- [Long]({}) — {}", "x".repeat(10_000), "y".repeat(20_000));
    let raw = format!("{}\n- [Second](second.md)", line);
    let res = truncate_entrypoint_content(&raw);
    assert!(res.was_byte_truncated);
    assert!(res.content.contains("> WARNING: MEMORY.md is"));
    assert!(res.content.contains("index entries are too long"));
}

#[test]
fn test_truncate_entrypoint_content_multibyte_at_byte_limit() {
    let raw = "记".repeat(MAX_ENTRYPOINT_BYTES);
    let res = truncate_entrypoint_content(&raw);
    let retained = res
        .content
        .split("\n\n> WARNING:")
        .next()
        .expect("截断结果应包含正文前缀");
    assert!(res.was_byte_truncated, "多字节超长索引应标记字节截断");
    assert!(
        retained.len() <= MAX_ENTRYPOINT_BYTES,
        "正文应在字符边界遵守原字节预算"
    );
    assert_eq!(
        retained,
        "记".repeat(MAX_ENTRYPOINT_BYTES / "记".len()),
        "超长单行应保留预算内的完整字符"
    );
}

#[test]
fn test_truncate_entrypoint_content_multibyte_after_complete_line() {
    let raw = format!("a\n{}", "记".repeat(MAX_ENTRYPOINT_BYTES));
    let res = truncate_entrypoint_content(&raw);
    assert!(res.was_byte_truncated, "多字节超长索引应标记字节截断");
    assert!(
        res.content.starts_with("a\n\n> WARNING:"),
        "字节边界落在多字节字符内时仍应优先保留完整行"
    );
}

#[test]
fn test_build_auto_memory_prompt_format() {
    let prompt = build_auto_memory_prompt(
        "~/.cc-code/projects/D--code-peri/memory/",
        Some("- [Line 1](file.md) — test hook"),
    );
    assert!(prompt.contains("# auto memory"));
    assert!(prompt.contains("`~/.cc-code/projects/D--code-peri/memory/`"));
    assert!(prompt.contains("<name>user</name>"));
    assert!(prompt.contains("<name>feedback</name>"));
    assert!(prompt.contains("<name>project</name>"));
    assert!(prompt.contains("<name>reference</name>"));
    assert!(prompt.contains("## What NOT to save in memory"));
    assert!(prompt.contains("## How to save memories"));
    assert!(prompt.contains("## When to access memories"));
    assert!(prompt.contains("## Before recommending from memory"));
    assert!(prompt.contains("## MEMORY.md"));
    assert!(prompt.contains("- [Line 1](file.md) — test hook"));
}

#[test]
fn test_get_auto_memory_display_dir() {
    let cwd = r"D:\code\peri";
    let display = get_auto_memory_display_dir(cwd);
    assert_eq!(display, "~/.cc-code/projects/D--code-peri/memory/");
    assert!(!display.contains("Users"));
    assert!(!display.contains("user"));
}

#[test]
fn test_build_auto_memory_prompt_empty_index() {
    let prompt =
        build_auto_memory_prompt(r"C:\Users\user\.claude\projects\D--code-peri\memory\", None);
    assert!(prompt.contains("Your MEMORY.md is currently empty."));
}

#[test]
fn test_build_auto_memory_instructions_excludes_index() {
    let instructions = build_auto_memory_instructions("/project/memory");
    assert!(
        instructions.contains("# auto memory"),
        "应保留自动记忆规则标题"
    );
    assert!(
        instructions.contains("## Types of memory"),
        "应保留记忆类型规则"
    );
    assert!(
        instructions.contains("## Memory and other forms of persistence"),
        "应保留末尾持久化规则"
    );
    assert!(
        !instructions.contains("## MEMORY.md"),
        "纯规则不应包含索引正文段"
    );
    assert!(
        !instructions.contains("Your MEMORY.md is currently empty."),
        "纯规则不应声明索引为空"
    );
    assert!(
        !instructions.contains("MEMORY_INDEX_SENTINEL"),
        "纯规则不应包含索引正文"
    );
}

#[test]
fn test_build_auto_memory_prompt_includes_index_once() {
    let entrypoint = "\n  - [MEMORY_INDEX_SENTINEL](topic.md) — 测试索引\n\n";
    let prompt = build_auto_memory_prompt("/project/memory", Some(entrypoint));
    assert_eq!(
        prompt.matches("MEMORY_INDEX_SENTINEL").count(),
        1,
        "合成提示词应只包含一份索引正文"
    );
    assert_eq!(
        prompt.matches("## MEMORY.md").count(),
        1,
        "应只包含一个索引正文段"
    );
    assert!(
        prompt.ends_with("\n\n## MEMORY.md\n\n- [MEMORY_INDEX_SENTINEL](topic.md) — 测试索引"),
        "应保留旧接口的索引标题、分隔符和首尾空白处理"
    );
    assert!(
        !prompt.contains("Your MEMORY.md is currently empty."),
        "非空索引不应包含空索引提示"
    );
}

#[test]
fn test_build_auto_memory_prompt_blank_index_matches_empty_index() {
    let empty_prompt = build_auto_memory_prompt("/project/memory", None);
    for entrypoint in ["", " \r\n\t"] {
        let prompt = build_auto_memory_prompt("/project/memory", Some(entrypoint));
        assert_eq!(
            prompt, empty_prompt,
            "缺省、空和纯空白索引的渲染结果应保持一致"
        );
    }
    assert!(empty_prompt.ends_with("\n\nYour MEMORY.md is currently empty. When you save new memories, they will appear here."), "应保留旧接口的完整空索引提示");
    assert!(
        !empty_prompt.contains("## MEMORY.md"),
        "空索引不应包含索引正文标题"
    );
}

#[test]
fn test_build_auto_memory_instructions_preserves_directory_separator() {
    let cases = [
        ("/project/memory", "/project/memory/"),
        ("/project/memory/", "/project/memory/"),
        (r"C:\project\memory", r"C:\project\memory/"),
        (r"C:\project\memory\", r"C:\project\memory\"),
    ];
    for (memory_dir, expected_dir) in cases {
        let instructions = build_auto_memory_instructions(memory_dir);
        let prompt = build_auto_memory_prompt(memory_dir, None);
        let directory_text = format!("at `{expected_dir}`.");
        assert!(
            instructions.contains(&directory_text),
            "应按旧契约补齐或保留目录末尾分隔符：{memory_dir}"
        );
        assert!(
            prompt.contains(&directory_text),
            "兼容接口应保持相同的目录显示：{memory_dir}"
        );
    }
}

#[test]
fn test_is_auto_mem_path() {
    let cwd = std::env::current_dir().unwrap();
    let mem_dir = get_auto_memory_dir(&cwd.to_string_lossy());
    let memory_file = mem_dir.join("user_role.md");
    assert!(is_auto_mem_path(&memory_file, &cwd.to_string_lossy()));

    let outside_file = std::path::Path::new("D:/code/some_other_file.txt");
    assert!(!is_auto_mem_path(outside_file, &cwd.to_string_lossy()));
}
