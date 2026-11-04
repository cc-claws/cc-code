use super::*;
use crate::theme::Theme;

struct DummyTheme;

impl Theme for DummyTheme {
    fn accent(&self) -> ratatui::style::Color {
        ratatui::style::Color::Cyan
    }
    fn success(&self) -> ratatui::style::Color {
        ratatui::style::Color::Green
    }
    fn warning(&self) -> ratatui::style::Color {
        ratatui::style::Color::Yellow
    }
    fn error(&self) -> ratatui::style::Color {
        ratatui::style::Color::Red
    }
    fn thinking(&self) -> ratatui::style::Color {
        ratatui::style::Color::Magenta
    }
    fn text(&self) -> ratatui::style::Color {
        ratatui::style::Color::White
    }
    fn muted(&self) -> ratatui::style::Color {
        ratatui::style::Color::Gray
    }
    fn dim(&self) -> ratatui::style::Color {
        ratatui::style::Color::DarkGray
    }
    fn border(&self) -> ratatui::style::Color {
        ratatui::style::Color::DarkGray
    }
    fn border_active(&self) -> ratatui::style::Color {
        ratatui::style::Color::Cyan
    }
    fn popup_bg(&self) -> ratatui::style::Color {
        ratatui::style::Color::Black
    }
    fn cursor_bg(&self) -> ratatui::style::Color {
        ratatui::style::Color::DarkGray
    }
    fn loading(&self) -> ratatui::style::Color {
        ratatui::style::Color::Cyan
    }
    fn syntax_theme_name(&self) -> &'static str {
        "Monokai Extended"
    }
}

const SAMPLE_GIT_DIFF: &str = r#"diff --git a/src/main.rs b/src/main.rs
index 8b8a75d5..0b065b82 100644
--- a/src/main.rs
+++ b/src/main.rs
@@ -1,5 +1,6 @@
 fn main() {
-    println!("hello");
+    let greeting = "hello world";
+    println!("{}", greeting);
 }
"#;

#[test]
fn test_is_patch_text_detects_git_diff() {
    assert!(is_patch_text(SAMPLE_GIT_DIFF), "应识别标准 git diff");
    assert!(
        !is_patch_text("random log text\nwithout diff"),
        "普通文本不应被识别"
    );
}

#[test]
fn test_strip_ansi_removes_escape_codes() {
    let colored = "\x1b[31m-removed\x1b[0m\n\x1b[32m+added\x1b[0m";
    let clean = strip_ansi(colored);
    assert_eq!(clean, "-removed\n+added");
}

#[test]
fn test_parse_patch_single_file_with_word_diff() {
    let parsed = parse_patch(SAMPLE_GIT_DIFF);
    assert_eq!(parsed.len(), 1, "应解析出 1 个文件");
    let (file_path, diff_result) = &parsed[0];
    assert_eq!(file_path, "src/main.rs");
    assert_eq!(diff_result.hunks.len(), 1);
    let hunk = &diff_result.hunks[0];
    assert_eq!(hunk.old_start, 1);
    assert_eq!(hunk.new_start, 1);
    // 应有 Context、Remove、Add 行
    let has_add = hunk.lines.iter().any(|l| matches!(l, DiffLine::Add { .. }));
    let has_remove = hunk
        .lines
        .iter()
        .any(|l| matches!(l, DiffLine::Remove { .. }));
    let has_context = hunk
        .lines
        .iter()
        .any(|l| matches!(l, DiffLine::Context { .. }));
    assert!(has_add && has_remove && has_context, "应包含增删与上下文行");
}

#[test]
fn test_parse_patch_multi_files() {
    let multi_diff = r#"diff --git a/foo.txt b/foo.txt
index 111..222 100644
--- a/foo.txt
+++ b/foo.txt
@@ -1,2 +1,2 @@
-a
+b
diff --git a/bar.txt b/bar.txt
index 333..444 100644
--- a/bar.txt
+++ b/bar.txt
@@ -1,2 +1,2 @@
-c
+d
"#;
    let parsed = parse_patch(multi_diff);
    assert_eq!(parsed.len(), 2, "应解析出 2 个文件");
    assert_eq!(parsed[0].0, "foo.txt");
    assert_eq!(parsed[1].0, "bar.txt");
}

#[test]
fn test_render_patch_applies_syntax_highlight() {
    let lines = render_patch(SAMPLE_GIT_DIFF, 80, &DummyTheme);
    assert!(lines.is_some(), "应成功渲染 patch");
    let rendered = lines.unwrap();
    // 第一行应为标题行
    let joined_first = rendered[0]
        .spans
        .iter()
        .map(|s| s.content.as_ref())
        .collect::<String>();
    assert!(joined_first.contains("src/main.rs"), "第一行应包含文件路径");

    #[cfg(feature = "markdown-highlight")]
    {
        // Add 行的代码片段应当有 Monokai 语法颜色（例如 let 为青色 #66D9EF）
        let mut found_let_color = false;
        for line in &rendered {
            for span in &line.spans {
                if span.content.contains("let")
                    && span.style.fg == Some(ratatui::style::Color::Rgb(102, 217, 239))
                {
                    found_let_color = true;
                    break;
                }
            }
        }
        assert!(
            found_let_color,
            "Add 行的 `let` 关键字应具有 Monokai 青色高亮"
        );
    }
}
