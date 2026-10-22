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
    // 对齐 CC：不渲染 @@ hunk header，改为摘要行 "Added N line, removed M line"
    let all: String = lines
        .iter()
        .map(|l| {
            l.spans
                .iter()
                .map(|s| s.content.as_ref())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        !all.contains("@@"),
        "不应渲染 @@ hunk header（CC 用摘要行替代），实际:\n{all}"
    );
    assert!(
        all.contains("Added 1 line") && all.contains("removed 1 line"),
        "应渲染 Added/removed 摘要行，实际:\n{all}"
    );
    // 对齐 CC：摘要行是普通 <Text>（无前景色），仅数字加粗
    let summary = lines
        .iter()
        .find(|l| {
            let joined: String = l.spans.iter().map(|s| s.content.as_ref()).collect();
            joined.contains("Added 1 line")
        })
        .expect("应找到摘要行");
    for span in &summary.spans {
        assert_eq!(
            span.style.fg, None,
            "摘要行不应有前景色，实际 span: {:?}",
            span.content
        );
    }
    for span in &summary.spans {
        let is_number = span.content.as_ref() == "1";
        let is_bold = span
            .style
            .add_modifier
            .contains(ratatui::style::Modifier::BOLD);
        assert_eq!(
            is_bold, is_number,
            "仅数字应加粗，实际 span: {:?}",
            span.content
        );
    }
}

#[test]
fn test_render_diff_add_line_is_green() {
    let input = make_basic_input();
    let theme = DarkTheme;
    let lines = render_diff_impl(&input, 80, &theme);
    // 找到包含 "new" 的行（语法高亮会把内容拆成多个 span，需拼接后匹配）
    let add_line = lines.iter().find(|l| {
        let joined: String = l.spans.iter().map(|s| s.content.as_ref()).collect();
        joined.contains("println!(\"new\")")
    });
    assert!(add_line.is_some(), "应有包含 new 内容的行");
    let add_line = add_line.unwrap();
    // 语法高亮后 Add 行内容 span 使用语法色 + 绿色背景；gutter 仍为 diff_add 前景
    let add_color = theme.diff_add();
    let add_bg = theme.diff_add_bg();
    let has_green = add_line.spans.iter().any(|s| s.style.fg == Some(add_color));
    let has_green_bg = add_line.spans.iter().any(|s| s.style.bg == Some(add_bg));
    assert!(
        has_green || has_green_bg,
        "新增行应有 diff_add 前景或绿底背景"
    );
}

/// 删除行三档配色对齐 CC `color-diff`（`packages/color-diff-napi/src/index.ts`）：
///
/// - `marker === '-'` 的代码正文走 `defaultStyle(theme)` → `theme.foreground`（正文色）；
/// - marker `-` 与行号走 `deleteDecoration`（暗红）；
/// - 整行底色走 `deleteLine`（深红）。
///
/// 旧实现三处共用 `diff_remove` 亮红，删除行代码整行发红，与 CC 的「白字压深红底」不符。
#[test]
fn test_render_diff_remove_line_splits_decoration_and_body_color() {
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

    let body = remove_line
        .spans
        .iter()
        .find(|s| s.content.contains("println!(\"old\")"))
        .expect("应有正文 span");
    assert_eq!(
        body.style.fg,
        Some(theme.text()),
        "删除行代码正文应使用正文色（CC defaultStyle/foreground）"
    );
    assert_eq!(
        body.style.bg,
        Some(theme.diff_remove_bg()),
        "删除行正文应压在 deleteLine 深红底上"
    );

    let gutter = &remove_line.spans[0];
    assert_eq!(
        gutter.style.fg,
        Some(theme.diff_remove_decoration()),
        "marker 与行号应用 deleteDecoration 暗红，不能与正文同色"
    );
    assert!(
        remove_line
            .spans
            .iter()
            .all(|s| s.style.fg != Some(theme.diff_remove())),
        "删除行不应再出现旧口径的 diff_remove 亮红"
    );
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
    // 对齐 CC color-diff：`+` 行的 marker 与行号用 addDecoration，
    // 代码正文走语法高亮（无语法时回退 diff_add）。两类行都要数到。
    let deco = theme.diff_add_decoration();
    let add_fg = theme.diff_add();
    let green_lines: Vec<_> = lines
        .iter()
        .filter(|l| {
            l.spans
                .iter()
                .any(|s| s.style.fg == Some(deco) || s.style.fg == Some(add_fg))
        })
        .collect();
    assert!(
        green_lines.len() >= 2,
        "新文件应有多行带新增色的内容（marker/行号或正文），实际有 {} 行",
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

// ── diff 语法高亮渲染测试（对齐 CC：Add/Context 行带语法色，Remove 行不带）──

/// 拼接一行的全部 span 文本
fn joined_text(line: &Line) -> String {
    line.spans
        .iter()
        .map(|s| s.content.as_ref())
        .collect::<Vec<_>>()
        .join("")
}

/// 使用 Monokai Extended 主题的测试主题
struct MonokaiTestTheme;

impl crate::theme::Theme for MonokaiTestTheme {
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

/// Rust diff：Add 行的 fn token 应带青色语法色 #66D9EF
#[cfg(feature = "markdown-highlight")]
#[test]
fn test_render_diff_add_line_has_syntax_color() {
    let input = DiffInput {
        file_path: "main.rs".to_string(),
        old_content: String::new(),
        new_content: "fn main() {}\n".to_string(),
        is_new_file: true,
        is_deleted_file: false,
        is_binary: false,
    };
    let lines = render_diff_impl(&input, 80, &MonokaiTestTheme);
    let add_line = lines
        .iter()
        .find(|l| joined_text(l).contains("fn main"))
        .expect("应找到含 fn main 的行");
    let has_cyan = add_line
        .spans
        .iter()
        .any(|s| s.style.fg == Some(ratatui::style::Color::Rgb(102, 217, 239)));
    assert!(
        has_cyan,
        "Add 行 fn token 应为青色 #66D9EF（Monokai storage 色）"
    );
    // 绿色行背景必须保留（语法色不能打断整行底色）
    let green_bg = crate::theme::Theme::diff_add_bg(&MonokaiTestTheme);
    let has_green_bg = add_line.spans.iter().any(|s| s.style.bg == Some(green_bg));
    assert!(has_green_bg, "Add 行应保留 diff 绿色背景");
}

/// 词级 diff 的 Add 行：对齐 CC，语法高亮与词级底色并存
#[cfg(feature = "markdown-highlight")]
#[test]
fn test_render_diff_word_diff_line_keeps_syntax_color() {
    let input = DiffInput {
        file_path: "main.rs".to_string(),
        old_content: "let foo = 1;\n".to_string(),
        new_content: "let bar = 1;\n".to_string(),
        is_new_file: false,
        is_deleted_file: false,
        is_binary: false,
    };
    let lines = render_diff_impl(&input, 80, &MonokaiTestTheme);
    let add_line = lines
        .iter()
        .find(|l| joined_text(l).contains("let bar"))
        .expect("应找到含 let bar 的 Add 行");
    let add_fg = crate::theme::Theme::diff_add(&MonokaiTestTheme);
    let word_bg = crate::theme::Theme::diff_add_word_bg(&MonokaiTestTheme);
    let bar_span = add_line
        .spans
        .iter()
        .find(|s| s.content.contains("bar"))
        .expect("变更片段 bar 应存在于 Add 行");
    assert_eq!(
        bar_span.style.bg,
        Some(word_bg),
        "变更片段应叠加词级深色背景"
    );
    assert!(
        bar_span.style.fg.is_some() && bar_span.style.fg != Some(add_fg),
        "变更片段应保留语法前景色，而非统一的 diff_add 前景"
    );
}

/// 词级 diff 行在语言未识别时回退纯色渲染，不 panic
#[cfg(feature = "markdown-highlight")]
#[test]
fn test_render_diff_word_diff_unknown_lang_falls_back() {
    let input = DiffInput {
        file_path: "Makefile".to_string(),
        old_content: "all: old\n".to_string(),
        new_content: "all: new\n".to_string(),
        is_new_file: false,
        is_deleted_file: false,
        is_binary: false,
    };
    let lines = render_diff_impl(&input, 80, &MonokaiTestTheme);
    let add_line = lines
        .iter()
        .find(|l| joined_text(l).contains("all: new"))
        .expect("应找到含 all: new 的 Add 行");
    let word_bg = crate::theme::Theme::diff_add_word_bg(&MonokaiTestTheme);
    assert!(
        add_line.spans.iter().any(|s| s.style.bg == Some(word_bg)),
        "回退路径仍应保留词级背景"
    );
}

/// Remove 行不做语法高亮：内容 span 应为统一的 diff_remove 前景色
#[cfg(feature = "markdown-highlight")]
#[test]
fn test_render_diff_remove_line_has_no_syntax_color() {
    let input = DiffInput {
        file_path: "main.rs".to_string(),
        old_content: "fn old_func() {}\n".to_string(),
        new_content: String::new(),
        is_new_file: false,
        is_deleted_file: true,
        is_binary: false,
    };
    let lines = render_diff_impl(&input, 80, &MonokaiTestTheme);
    let remove_line = lines
        .iter()
        .find(|l| joined_text(l).contains("fn old_func"))
        .expect("应找到含 fn old_func 的行");
    // 对齐 CC color-diff `marker === '-'` 分支：删除行**不做语法高亮**，
    // 正文统一用正文色（`theme.text()`），因此不会混入 Monokai 的语法色。
    let body_fg = crate::theme::Theme::text(&MonokaiTestTheme);
    let content_spans: Vec<_> = remove_line
        .spans
        .iter()
        .filter(|s| s.content.contains("old_func") || s.content.contains("fn"))
        .collect();
    assert!(!content_spans.is_empty(), "应存在含代码内容的内容 span");
    for s in &content_spans {
        assert_eq!(
            s.style.fg,
            Some(body_fg),
            "Remove 行内容不应带语法色，应为正文色"
        );
    }
}

/// 未识别语言（不在 CC 白名单的扩展名）应回退纯文本渲染，不 panic
#[cfg(feature = "markdown-highlight")]
#[test]
fn test_render_diff_unknown_lang_falls_back() {
    let input = DiffInput {
        file_path: "notes.txt".to_string(),
        old_content: String::new(),
        new_content: "all:\n\techo hi\n".to_string(),
        is_new_file: true,
        is_deleted_file: false,
        is_binary: false,
    };
    let lines = render_diff_impl(&input, 80, &MonokaiTestTheme);
    let add_line = lines
        .iter()
        .find(|l| joined_text(l).contains("echo hi"))
        .expect("应找到含 echo hi 的行");
    let has_green_bg = add_line
        .spans
        .iter()
        .any(|s| s.style.bg == Some(crate::theme::Theme::diff_add_bg(&MonokaiTestTheme)));
    assert!(has_green_bg, "未识别语言回退后仍应保留绿底");
}

/// 对齐 CC color-diff：行号 gutter 为单行号，marker + 空格 + 右对齐行号 + 空格，无竖线
#[test]
fn test_render_diff_gutter_matches_cc_single_line_number() {
    let input = make_basic_input();
    let lines = render_diff_impl(&input, 80, &DarkTheme);
    let texts: Vec<String> = lines.iter().map(joined_text).collect();
    // Add 行（第 2 行）："+ 2 " 前缀
    assert!(
        texts.iter().any(|t| t.starts_with("+ 2 ")),
        "Add 行 gutter 应为 \"+ 2 \"，实际:\n{}",
        texts.join("\n")
    );
    // Remove 行（旧文件第 2 行）："- 2 " 前缀
    assert!(
        texts.iter().any(|t| t.starts_with("- 2 ")),
        "Remove 行 gutter 应为 \"- 2 \"，实际:\n{}",
        texts.join("\n")
    );
    // Context 行（第 1 行）：两空格 + 行号 + 空格
    assert!(
        texts.iter().any(|t| t.starts_with("  1 ")),
        "Context 行 gutter 应为 \"  1 \"，实际:\n{}",
        texts.join("\n")
    );
    // 不应再出现旧的竖线分隔符
    assert!(
        !texts.iter().any(|t| t.contains('│')),
        "diff 行不应含竖线分隔符，实际:\n{}",
        texts.join("\n")
    );
}

/// 新建文件不输出 "Added N lines" 摘要行（与 Write 工具结果的 "Wrote N lines" 重复）
#[test]
fn test_render_new_file_has_no_added_summary() {
    let input = DiffInput {
        file_path: "new_file.rs".to_string(),
        old_content: String::new(),
        new_content: "line1\nline2\n".to_string(),
        is_new_file: true,
        is_deleted_file: false,
        is_binary: false,
    };
    let lines = render_diff_impl(&input, 80, &DarkTheme);
    let all = lines.iter().map(joined_text).collect::<Vec<_>>().join("\n");
    assert!(
        !all.contains("Added "),
        "新建文件不应输出 Added 摘要行（Write 结果已显示 Wrote N lines），实际:\n{all}"
    );
}

// ── 新建文件代码预览（对齐 CC FileWriteTool create 分支）────────────────────

/// 代码预览不应带 diff 底色（Add 绿底/Remove 红底）
#[test]
fn test_render_new_file_code_has_no_diff_background() {
    let content = "fn main() {\n    println!(\"hi\");\n}\n";
    let lines = crate::diff::render_new_file_code("main.rs", content, 80, &DarkTheme, 10);
    assert_eq!(lines.len(), 3, "3 行内容应渲染 3 行");
    for line in &lines {
        for span in &line.spans {
            assert_eq!(
                span.style.bg, None,
                "代码预览不应设置 diff 背景色，实际 span: {:?}",
                span.content
            );
        }
    }
}

/// 超过 max_lines 时应截断并提示剩余行数
#[test]
fn test_render_new_file_code_truncates_with_hint() {
    let content: String = (1..=15).map(|i| format!("line {i}\n")).collect();
    let lines = crate::diff::render_new_file_code("main.rs", &content, 80, &DarkTheme, 10);
    assert_eq!(lines.len(), 11, "10 行内容 + 1 行提示");
    let last = joined_text(&lines[10]);
    assert!(
        last.contains("+5 lines") && last.contains("ctrl+o to expand"),
        "截断提示应为 '… +5 lines (ctrl+o to expand)'，实际: {last}"
    );
}

/// 全量模式（usize::MAX）不截断
#[test]
fn test_render_new_file_code_full_mode_no_truncation() {
    let content: String = (1..=15).map(|i| format!("line {i}\n")).collect();
    let lines = crate::diff::render_new_file_code("main.rs", &content, 80, &DarkTheme, usize::MAX);
    assert_eq!(lines.len(), 15, "全量模式应渲染全部 15 行");
    assert!(
        !lines
            .iter()
            .any(|l| joined_text(l).contains("ctrl+o to expand")),
        "全量模式不应出现截断提示"
    );
}

/// 语法高亮应生效（Rust 关键字带语法色）
#[cfg(feature = "markdown-highlight")]
#[test]
fn test_render_new_file_code_applies_syntax_highlight() {
    let theme = MonokaiTestTheme;
    let lines = crate::diff::render_new_file_code("main.rs", "fn main() {}\n", 80, &theme, 10);
    let fn_span = lines[0]
        .spans
        .iter()
        .find(|s| s.content.contains("fn"))
        .expect("应找到 fn token");
    assert_eq!(
        fn_span.style.fg,
        Some(ratatui::style::Color::Rgb(102, 217, 239)),
        "fn 应为 Monokai 青色"
    );
}

/// P1-2 回归：渲染缓存 key 必须包含语言指纹，
/// 内容相同但扩展名不同的两个文件不得串用语法高亮结果。
#[cfg(feature = "markdown-highlight")]
#[test]
fn test_render_cache_key_includes_language() {
    let make = |path: &str| DiffInput {
        file_path: path.to_string(),
        old_content: String::new(),
        new_content: "let y = 2;".to_string(),
        is_new_file: false,
        is_deleted_file: false,
        is_binary: false,
    };
    let has_rust_cyan = |lines: &[Line]| {
        lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .any(|s| s.style.fg == Some(ratatui::style::Color::Rgb(102, 217, 239)))
    };
    let rust_lines = crate::diff::render_diff(&make("sample.rs"), 80, &MonokaiTestTheme);
    assert!(has_rust_cyan(&rust_lines), "rs 文件应有 Rust 语法色");
    let text_lines = crate::diff::render_diff(&make("sample.txt"), 80, &MonokaiTestTheme);
    assert!(
        !has_rust_cyan(&text_lines),
        "txt 文件不应复用 rs 文件的语法高亮缓存"
    );
}

// ── 语言选择（对齐 CC detectLanguage）测试 ─────────────────────────────────

/// 文件名表：Makefile / Dockerfile / CMakeLists.txt / Rakefile / Gemfile
#[cfg(feature = "markdown-highlight")]
#[test]
fn test_detect_language_by_filename() {
    assert_eq!(
        detect_language("Makefile", None),
        Some("makefile".to_string())
    );
    assert_eq!(
        detect_language("Dockerfile", None),
        Some("dockerfile".to_string())
    );
    assert_eq!(
        detect_language("CMakeLists.txt", None),
        Some("cmake".to_string())
    );
    assert_eq!(detect_language("Rakefile", None), Some("ruby".to_string()));
    assert_eq!(detect_language("Gemfile", None), Some("ruby".to_string()));
    // CC 的 stem 规则：Makefile.bak 的 stem 仍是 Makefile
    assert_eq!(
        detect_language("Makefile.bak", None),
        Some("makefile".to_string())
    );
}

/// 扩展名白名单：CC 支持的语言命中，白名单外返回 None
#[cfg(feature = "markdown-highlight")]
#[test]
fn test_detect_language_by_extension() {
    assert_eq!(detect_language("src/main.rs", None), Some("rs".to_string()));
    // .md 不做语法高亮（对齐 CC：hljs markdown 的 section/strong 未映射主题色，实际与纯文本一致）
    assert_eq!(detect_language("a.md", None), None);
    assert_eq!(detect_language("a.yml", None), Some("yml".to_string()));
    // .h 归为 C，避免 syntect 命中 Objective-C
    assert_eq!(detect_language("a.h", None), Some("c".to_string()));
    // 白名单外的扩展名不做高亮
    assert_eq!(detect_language("notes.txt", None), None);
    assert_eq!(detect_language("data.xyz", None), None);
}

/// shebang / `<?php` / `<?xml` 首行检测
#[cfg(feature = "markdown-highlight")]
#[test]
fn test_detect_language_by_shebang() {
    assert_eq!(
        detect_language("script", Some("#!/bin/bash")),
        Some("bash".to_string())
    );
    assert_eq!(
        detect_language("script", Some("#!/usr/bin/env python3")),
        Some("python".to_string())
    );
    assert_eq!(
        detect_language("script", Some("#!/usr/bin/env node")),
        Some("js".to_string())
    );
    assert_eq!(
        detect_language("script", Some("#!/usr/bin/ruby")),
        Some("ruby".to_string())
    );
    assert_eq!(detect_language("a.php", None), Some("php".to_string()));
    assert_eq!(
        detect_language("noext", Some("<?php echo 1;")),
        Some("php".to_string())
    );
    assert_eq!(
        detect_language("noext", Some("<?xml version=\"1.0\"?>")),
        Some("xml".to_string())
    );
}

/// 首行 BOM 不影响 shebang 识别
#[cfg(feature = "markdown-highlight")]
#[test]
fn test_detect_language_strips_bom() {
    assert_eq!(
        detect_language("script", Some("\u{feff}#!/bin/bash")),
        Some("bash".to_string())
    );
}

/// two-face 语法集：TypeScript 可高亮（syntect 默认集缺失该语法）
#[cfg(feature = "markdown-highlight")]
#[test]
fn test_render_diff_typescript_has_syntax_color() {
    let input = DiffInput {
        file_path: "app.ts".to_string(),
        old_content: String::new(),
        new_content: "const answer: number = 42;\n".to_string(),
        is_new_file: true,
        is_deleted_file: false,
        is_binary: false,
    };
    let lines = render_diff_impl(&input, 80, &MonokaiTestTheme);
    let add_line = lines
        .iter()
        .find(|l| joined_text(l).contains("const answer"))
        .expect("应找到含 const answer 的行");
    let add_fg = crate::theme::Theme::diff_add(&MonokaiTestTheme);
    assert!(
        add_line
            .spans
            .iter()
            .any(|s| s.style.fg.is_some() && s.style.fg != Some(add_fg)),
        "TypeScript 的 Add 行应带语法色"
    );
}

/// two-face 语法集：Dockerfile 可高亮（文件名表 + syntect 默认集缺失时曾回退纯文本）
#[cfg(feature = "markdown-highlight")]
#[test]
fn test_render_diff_dockerfile_has_syntax_color() {
    let input = DiffInput {
        file_path: "Dockerfile".to_string(),
        old_content: String::new(),
        new_content: "FROM rust:1\nRUN cargo build\n".to_string(),
        is_new_file: true,
        is_deleted_file: false,
        is_binary: false,
    };
    let lines = render_diff_impl(&input, 80, &MonokaiTestTheme);
    let add_line = lines
        .iter()
        .find(|l| joined_text(l).contains("FROM rust"))
        .expect("应找到含 FROM rust 的行");
    let add_fg = crate::theme::Theme::diff_add(&MonokaiTestTheme);
    assert!(
        add_line
            .spans
            .iter()
            .any(|s| s.style.fg.is_some() && s.style.fg != Some(add_fg)),
        "Dockerfile 的 Add 行应带语法色"
    );
}

/// 提取一行中参与语法着色的 span 前景色（跳过 diff 行的 gutter 与背景填充）
#[cfg(feature = "markdown-highlight")]
fn content_fgs(line: &Line<'static>, skip_first: bool) -> Vec<Option<Color>> {
    line.spans
        .iter()
        .skip(if skip_first { 1 } else { 0 })
        .filter(|s| !s.content.trim().is_empty())
        .map(|s| s.style.fg)
        .collect()
}

/// 回归：新建文件预览与 diff 行共用逐行无状态高亮（对齐 CC `ColorFile`/`ColorDiff`
/// 共用 `highlightLine` 的单行口径）。多行块注释的中间行在两侧都不应被识别为注释。
#[cfg(feature = "markdown-highlight")]
#[test]
fn test_render_new_file_code_matches_diff_line_highlight() {
    let theme = MonokaiTestTheme;
    let content = "/*\n * comment line two\n */\nfn main() {}\n";
    let preview = crate::diff::render_new_file_code("probe.rs", content, 80, &theme, usize::MAX);
    let input = DiffInput {
        file_path: "probe.rs".to_string(),
        old_content: String::new(),
        new_content: content.to_string(),
        is_new_file: true,
        is_deleted_file: false,
        is_binary: false,
    };
    let diff_lines = render_diff_impl(&input, 80, &theme);
    // diff 第 0 行是文件标题，其后与 preview 逐行对应
    assert_eq!(preview.len(), 4);
    for i in 0..preview.len() {
        let diff_line = &diff_lines[i + 1];
        // 两侧首 span 都是行号 gutter（preview 无 marker，diff 有 marker）
        assert_eq!(
            content_fgs(&preview[i], true),
            content_fgs(diff_line, true),
            "第 {i} 行预览与 diff 的语法前景色应一致，preview: {:?}, diff: {:?}",
            preview[i],
            diff_line
        );
    }
    let comment_fg = Color::Rgb(117, 113, 94);
    assert!(
        !content_fgs(&preview[1], true).contains(&Some(comment_fg)),
        "块注释中间行按单行高亮，不应沿用上一行的注释状态"
    );
}

/// 对齐 CC `ColorFile`：新建文件预览带行号 gutter，宽度 = 总行数位数 + 2
#[test]
fn test_render_new_file_code_has_line_number_gutter() {
    let content = "fn a() {}\nfn b() {}\nfn c() {}\n";
    let lines = crate::diff::render_new_file_code("main.rs", content, 80, &DarkTheme, usize::MAX);
    assert_eq!(lines.len(), 3);
    for (i, line) in lines.iter().enumerate() {
        assert_eq!(
            line.spans[0].content.as_ref(),
            format!(" {} ", i + 1),
            "第 {i} 行应为 ' 行号 ' 形式的 gutter"
        );
        assert_eq!(
            line.spans[0].style.fg,
            Some(DarkTheme.dim()),
            "行号应使用 dim 色（对齐 CC 行号 DIM）"
        );
    }
}

/// 对齐 CC `FileWriteTool`：空内容显示 "(No content)" 占位
#[test]
fn test_render_new_file_code_empty_content_placeholder() {
    let lines = crate::diff::render_new_file_code("a.txt", "", 80, &DarkTheme, 10);
    assert_eq!(lines.len(), 1, "空文件只渲染一行占位");
    let text: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
    assert_eq!(text, " 1 (No content)");
}

/// 对齐 CC `HighlightedCode/Fallback.tsx`：预览内容同样行首 tab → 2 空格
#[test]
fn test_render_new_file_code_converts_leading_tabs() {
    let lines = crate::diff::render_new_file_code("a.txt", "\techo hi\n", 80, &DarkTheme, 10);
    let text: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
    assert_eq!(text, " 1   echo hi", "行首 tab 应转为 2 空格");
}

/// 行号宽度按总行数位数：3 位行号时 gutter 宽 5 列并右对齐
#[test]
fn test_render_new_file_code_gutter_width_follows_line_count() {
    let content: String = (1..=100).map(|i| format!("l{i}\n")).collect();
    let lines =
        crate::diff::render_new_file_code("notes.txt", &content, 80, &DarkTheme, usize::MAX);
    assert_eq!(lines[0].spans[0].content.as_ref(), "   1 ");
    assert_eq!(lines[99].spans[0].content.as_ref(), " 100 ");
}

/// 对齐 CC `wrapText`：新建预览超宽行折行，续行行号位补空格，不丢字符
#[test]
fn test_render_new_file_code_wraps_long_line_with_blank_continuation_gutter() {
    let content = format!("{}\n", "y".repeat(100));
    let lines =
        crate::diff::render_new_file_code("notes.txt", &content, 40, &DarkTheme, usize::MAX);
    assert!(lines.len() > 1, "100 列内容在 40 列宽下应折行");
    let joined: String = lines
        .iter()
        .map(|l| {
            l.spans
                .iter()
                .skip(1)
                .map(|s| s.content.as_ref())
                .collect::<String>()
        })
        .collect();
    assert_eq!(joined, "y".repeat(100), "折行不得丢失字符");
    // 总行数 1 → n=1 → gutter 宽 3 列，首行 " 1 "，续行 3 空格
    assert_eq!(lines[0].spans[0].content.as_ref(), " 1 ");
    for line in &lines[1..] {
        assert_eq!(line.spans[0].content.as_ref(), "   ");
        assert!(line_display_width(line) <= 40, "折行后不得超过 width");
    }
}

/// 对齐 CC：`.md` 不做语法高亮，内容回退为无前景色，与 `.txt` 输出逐字一致
#[cfg(feature = "markdown-highlight")]
#[test]
fn test_render_new_file_code_md_falls_back_to_plain_text() {
    let content = "# 标题\n\n普通段落文字\n\n**粗体**\n";
    let md =
        crate::diff::render_new_file_code("notes.md", content, 80, &MonokaiTestTheme, usize::MAX);
    for line in &md {
        assert!(
            content_fgs(line, true).iter().all(|fg| fg.is_none()),
            "md 预览不应带语法前景色，实际: {:?}",
            content_fgs(line, true)
        );
    }
    let txt =
        crate::diff::render_new_file_code("notes.txt", content, 80, &MonokaiTestTheme, usize::MAX);
    assert_eq!(
        md.iter().map(joined_text).collect::<Vec<_>>(),
        txt.iter().map(joined_text).collect::<Vec<_>>(),
        "md 与 txt 的预览输出应完全一致（都不着色）"
    );
}

/// 一行的显示宽度（CJK 占 2 列）
fn line_display_width(line: &Line<'static>) -> usize {
    line.spans
        .iter()
        .map(|s| unicode_width::UnicodeWidthStr::width(s.content.as_ref()))
        .sum()
}

#[test]
fn test_wrap_spans_splits_at_display_width() {
    let rows = wrap_spans(vec![Span::raw("abcdefghij")], 4);
    let texts: Vec<String> = rows
        .iter()
        .map(|r| r.iter().map(|s| s.content.as_ref()).collect())
        .collect();
    assert_eq!(texts, vec!["abcd", "efgh", "ij"]);
}

#[test]
fn test_wrap_spans_keeps_wide_chars_intact() {
    let rows = wrap_spans(vec![Span::raw("宽宽宽")], 5);
    let widths: Vec<usize> = rows
        .iter()
        .map(|r| {
            r.iter()
                .map(|s| unicode_width::UnicodeWidthStr::width(s.content.as_ref()))
                .sum()
        })
        .collect();
    assert_eq!(widths, vec![4, 2], "宽字符不得被切断，每行不超过 5 列");
}

#[test]
fn test_wrap_spans_forces_progress_on_narrow_width() {
    let rows = wrap_spans(vec![Span::raw("宽宽")], 1);
    assert_eq!(
        rows.len(),
        2,
        "单字符超宽时强制单字符换行，不得死循环或丢字符"
    );
}

/// 对齐 CC `wrapText`：改动行折行后每段 padding 到 width，续行 marker 位保留
#[test]
fn test_render_diff_wrapped_add_line_pads_and_indents_continuation() {
    let content = "x".repeat(200);
    let input = DiffInput {
        file_path: "test.rs".to_string(),
        old_content: String::new(),
        new_content: format!("{content}\n"),
        is_new_file: true,
        is_deleted_file: false,
        is_binary: false,
    };
    let lines = render_diff_impl(&input, 40, &DarkTheme);
    // 第 0 行是文件标题，其余为 Add 行
    let add_lines: Vec<&Line<'static>> = lines.iter().skip(1).collect();
    assert!(add_lines.len() > 1, "200 列内容在 40 列宽下应折成多行");
    let joined: String = add_lines
        .iter()
        .map(|l| {
            l.spans
                .iter()
                .skip(1)
                .map(|s| s.content.as_ref())
                .collect::<String>()
        })
        .collect();
    assert_eq!(joined.trim_end(), content, "折行不得丢失字符");
    for line in &add_lines {
        assert_eq!(
            line_display_width(line),
            40,
            "改动行每段应 padding 到 width"
        );
    }
    assert_eq!(
        add_lines[1].spans[0].content.as_ref(),
        "+   ",
        "续行 gutter 应为 marker + 空格占位（n=1 时共 4 列）"
    );
}

/// 对齐 CC `wrapText`：上下文行折行但不 padding（背景不延伸到边界）
#[test]
fn test_render_diff_wrapped_context_line_is_not_padded() {
    let shared = "c".repeat(100);
    let input = DiffInput {
        file_path: "test.txt".to_string(),
        old_content: format!("{shared}\nold\n"),
        new_content: format!("{shared}\nnew\n"),
        is_new_file: false,
        is_deleted_file: false,
        is_binary: false,
    };
    let lines = render_diff_impl(&input, 40, &DarkTheme);
    let context_lines: Vec<&Line<'static>> = lines
        .iter()
        .skip(1)
        .filter(|l| {
            let content: String = l.spans.iter().skip(1).map(|s| s.content.as_ref()).collect();
            !content.trim().is_empty() && content.trim().chars().all(|c| c == 'c')
        })
        .collect();
    assert!(
        context_lines.len() > 1,
        "100 列上下文行在 40 列宽下应折行，实际 {} 行",
        context_lines.len()
    );
    let joined: String = context_lines
        .iter()
        .map(|l| {
            l.spans
                .iter()
                .skip(1)
                .map(|s| s.content.as_ref())
                .collect::<String>()
        })
        .collect();
    assert_eq!(joined, shared, "上下文行折行不得丢失字符");
    assert!(
        context_lines.iter().any(|l| line_display_width(l) < 40),
        "上下文行不应 padding 到 width"
    );
}

/// 回归：折行必须是线性的。曾经的实现在每个切分段都对剩余全串统计宽度
/// 并 `split_off` 复制，500 KB 单行要 42 s（O(n²)）；线性实现约 0.3 s（debug）。
#[test]
fn test_render_diff_long_line_wrapping_stays_linear() {
    let content = format!("{}\n", "x".repeat(500_000));
    let input = DiffInput {
        file_path: "test.txt".to_string(),
        old_content: String::new(),
        new_content: content,
        is_new_file: true,
        is_deleted_file: false,
        is_binary: false,
    };
    let t0 = std::time::Instant::now();
    let lines = render_diff_impl(&input, 80, &DarkTheme);
    let elapsed = t0.elapsed();
    // 500 KB / 76 列 ≈ 6500 行
    assert!(
        lines.len() > 6_000,
        "超长单行应折成数千行，实际 {}",
        lines.len()
    );
    assert!(
        elapsed < std::time::Duration::from_secs(5),
        "折行应为线性复杂度，500 KB 单行不得耗时 {elapsed:?}"
    );
}
