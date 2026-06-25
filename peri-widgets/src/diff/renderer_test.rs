use super::*;
use crate::theme::DarkTheme;

fn make_basic_input() -> DiffInput {
    DiffInput {
        file_path: "test.rs".to_string(),
        old_content: "fn main() {\n    println!(\"old\");\n}\n".to_string(),
        new_content: "fn main() {\n    println!(\"new\");\n}\n".to_string(),
        is_new_file: false,
        is_deleted_file: false,
        is_binary: false,
    }
}

#[test]
fn test_render_diff_produces_lines() {
    let input = make_basic_input();
    let theme = DarkTheme;
    let lines = render_diff_impl(&input, 80, &theme);
    assert!(!lines.is_empty(), "基本 diff 应产出非��行");
}

#[test]
fn test_render_diff_hunk_header_is_cyan() {
    let input = make_basic_input();
    let theme = DarkTheme;
    let lines = render_diff_impl(&input, 80, &theme);
    // 找到 hunk header 行，颜色应为 Cyan（DarkTheme.diff_hunk 返回 Color::Cyan）
    let hunk_line = lines
        .iter()
        .find(|l| l.spans.iter().any(|s| s.content.starts_with("@@")));
    assert!(hunk_line.is_some(), "应有 @@ 开头的 hunk header 行");
    let hunk_line = hunk_line.unwrap();
    let has_cyan = hunk_line
        .spans
        .iter()
        .any(|s| s.style.fg == Some(theme.diff_hunk()));
    assert!(has_cyan, "hunk header 颜色应为 diff_hunk");
}

#[test]
fn test_render_diff_add_line_is_green() {
    let input = make_basic_input();
    let theme = DarkTheme;
    let lines = render_diff_impl(&input, 80, &theme);
    // 找到包含 "new" 的行，gutter 应为绿色
    let add_line = lines.iter().find(|l| {
        l.spans
            .iter()
            .any(|s| s.content.contains("println!(\"new\")"))
    });
    assert!(add_line.is_some(), "应有包含 new 内容的行");
    let add_line = add_line.unwrap();
    let add_color = theme.diff_add();
    let has_green = add_line.spans.iter().any(|s| s.style.fg == Some(add_color));
    assert!(has_green, "新增行应有 diff_add 颜色");
}

#[test]
fn test_render_diff_remove_line_is_red() {
    let input = make_basic_input();
    let theme = DarkTheme;
    let lines = render_diff_impl(&input, 80, &theme);
    let remove_line = lines.iter().find(|l| {
        l.spans
            .iter()
            .any(|s| s.content.contains("println!(\"old\")"))
    });
    assert!(remove_line.is_some(), "应有包含 old 内容的行");
    let remove_line = remove_line.unwrap();
    let remove_color = theme.diff_remove();
    let has_red = remove_line
        .spans
        .iter()
        .any(|s| s.style.fg == Some(remove_color));
    assert!(has_red, "删除行应有 diff_remove 颜色");
}

#[test]
fn test_render_new_file_all_green() {
    let input = DiffInput {
        file_path: "new_file.rs".to_string(),
        old_content: String::new(),
        new_content: "line1\nline2\nline3\n".to_string(),
        is_new_file: true,
        is_deleted_file: false,
        is_binary: false,
    };
    let theme = DarkTheme;
    let lines = render_diff_impl(&input, 80, &theme);
    // 应有标题行 + hunk header + 至少 3 条内容行
    assert!(lines.len() >= 2, "新文件应有标题 + 内容行");
    let add_color = theme.diff_add();
    let green_lines: Vec<_> = lines
        .iter()
        .filter(|l| l.spans.iter().any(|s| s.style.fg == Some(add_color)))
        .collect();
    assert!(
        green_lines.len() >= 2,
        "新文件应有多行绿色内容，实际有 {} 行",
        green_lines.len()
    );
}

#[test]
fn test_render_new_file_shows_tail_lines() {
    let input = DiffInput {
        file_path: "new_file.rs".to_string(),
        old_content: String::new(),
        new_content: (0..12)
            .map(|idx| format!("line {idx:02}"))
            .collect::<Vec<_>>()
            .join("\n"),
        is_new_file: true,
        is_deleted_file: false,
        is_binary: false,
    };
    let theme = DarkTheme;
    let lines = render_diff_impl(&input, 80, &theme);
    let text: String = lines
        .iter()
        .flat_map(|line| line.spans.iter().map(|span| span.content.as_ref()))
        .collect::<Vec<_>>()
        .join("");

    assert!(text.contains("line 11"), "新文件 diff 应显示尾部代码行");
    assert!(
        !text.contains("more lines not shown"),
        "新文件 diff 不应在详细视图中隐藏代码行"
    );
}

#[test]
fn test_render_truncated_diff() {
    let input = DiffInput {
        file_path: "big.txt".to_string(),
        old_content: "x".repeat(600_000),
        new_content: "y".repeat(600_000),
        is_new_file: false,
        is_deleted_file: false,
        is_binary: false,
    };
    let theme = DarkTheme;
    let lines = render_diff_impl(&input, 80, &theme);
    assert_eq!(lines.len(), 1, "截断 diff 应只有一行");
    let text: String = lines[0].spans.iter().map(|s| &*s.content).collect();
    assert!(
        text.contains("too large"),
        "截断信息应包含 'too large'，实际: {}",
        text
    );
}

#[test]
fn test_render_binary_file() {
    let input = DiffInput {
        file_path: "image.png".to_string(),
        old_content: String::new(),
        new_content: String::new(),
        is_new_file: false,
        is_deleted_file: false,
        is_binary: true,
    };
    let theme = DarkTheme;
    let lines = render_diff_impl(&input, 80, &theme);
    assert_eq!(lines.len(), 1, "二进制 diff 应只有一行");
    let text: String = lines[0].spans.iter().map(|s| &*s.content).collect();
    assert!(
        text.contains("Binary"),
        "二进制信息应包含 'Binary'，实际: {}",
        text
    );
}

/// 模拟 Edit 工具单行替换场景：old_string 和 new_string 各一行
#[test]
fn test_render_diff_single_line_edit_no_concat() {
    let input = DiffInput {
        file_path: "cache.rs".to_string(),
        old_content: "const CACHE_CAPACITY: usize = 256;".to_string(),
        new_content: "const CACHE_CAPACITY: usize = 1024;".to_string(),
        is_new_file: false,
        is_deleted_file: false,
        is_binary: false,
    };
    let theme = DarkTheme;
    let lines = render_diff_impl(&input, 80, &theme);
    let texts: Vec<String> = lines
        .iter()
        .map(|l| {
            l.spans
                .iter()
                .map(|s| s.content.as_ref())
                .collect::<Vec<_>>()
                .join("")
        })
        .collect();
    let all = texts.join("\n");
    assert!(
        !all.contains("256;1024"),
        "old/new 不应拼在同一行，实际输出:\n{}",
        all
    );
    assert!(all.contains("256"), "应包含旧值 256，实际:\n{}", all);
    assert!(all.contains("1024"), "应包含新值 1024，实际:\n{}", all);
}
