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
    let lines: Vec<String> = (0..250).map(|i| format!("- [Line {}](file{}.md)", i, i)).collect();
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
fn test_build_auto_memory_prompt_format() {
    let prompt = build_auto_memory_prompt("~/.cc-code/projects/D--code-peri/memory/", Some("- [Line 1](file.md) — test hook"));
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
    let prompt = build_auto_memory_prompt(r"C:\Users\user\.claude\projects\D--code-peri\memory\", None);
    assert!(prompt.contains("Your MEMORY.md is currently empty."));
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
