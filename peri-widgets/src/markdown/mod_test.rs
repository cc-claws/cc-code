use unicode_width::UnicodeWidthStr;

use super::*;
use ratatui::style::Modifier;

fn default_theme() -> DefaultMarkdownTheme {
    DefaultMarkdownTheme
}

#[test]
fn parse_empty_input() {
    let text = parse_markdown("", &default_theme(), 80);
    // Empty input may produce an empty line
    assert!(
        text.lines.len() <= 1,
        "Expected at most 1 line for empty input, got {}",
        text.lines.len()
    );
}

#[test]
fn parse_heading() {
    let text = parse_markdown("# Hello", &default_theme(), 80);
    // 标题前后各有一个空行，所以 Hello 在 index 1
    let line = &text.lines[1];
    let heading_found = line.spans.iter().any(|s| s.content.contains("Hello"));
    assert!(heading_found, "Expected 'Hello' in heading output");
    let has_bold = line
        .spans
        .iter()
        .any(|s| s.style.add_modifier == Modifier::BOLD);
    assert!(has_bold, "Expected BOLD modifier on heading");
}

#[test]
fn parse_code_block() {
    let text = parse_markdown("```rust\nfn main() {}\n```", &default_theme(), 80);
    assert_eq!(
        text.lines.len(),
        1,
        "单行代码块只应产生一行，got {} lines: {:?}",
        text.lines.len(),
        text.lines
    );
    // 单行代码块：只着色，无 [lang] 和 │ 前缀
    let line = &text.lines[0];
    let has_code_color = line
        .spans
        .iter()
        .any(|s| s.style.fg == Some(default_theme().code()) && s.content.contains("fn main"));
    assert!(has_code_color, "Expected code text with code color");
    let no_prefix = !line.spans.iter().any(|s| s.content.contains('│'));
    assert!(no_prefix, "Single-line code block should not have │ prefix");
}

#[test]
fn parse_inline_code() {
    let text = parse_markdown("`hello`", &default_theme(), 80);
    assert!(!text.lines.is_empty());
    let has_code = text.lines.iter().any(|l| {
        l.spans
            .iter()
            .any(|s| s.content.contains("hello") && s.style.fg == Some(default_theme().code()))
    });
    assert!(has_code, "Expected inline code with code color");
}

#[test]
fn parse_bold_italic() {
    let text = parse_markdown("**bold** *italic*", &default_theme(), 80);
    assert!(!text.lines.is_empty());
    let line = &text.lines[0];
    let has_bold = line
        .spans
        .iter()
        .any(|s| s.style.add_modifier == Modifier::BOLD);
    assert!(has_bold, "Expected BOLD modifier");
    let has_italic = line
        .spans
        .iter()
        .any(|s| s.style.add_modifier == Modifier::ITALIC);
    assert!(has_italic, "Expected ITALIC modifier");
}

#[test]
fn parse_link() {
    let text = parse_markdown("[text](url)", &default_theme(), 80);
    assert!(!text.lines.is_empty());
    let has_link = text.lines.iter().any(|l| {
        l.spans
            .iter()
            .any(|s| s.content.contains("text") && s.style.fg == Some(default_theme().link()))
    });
    assert!(has_link, "Expected link text with link color");
}

#[test]
fn parse_unordered_list() {
    let text = parse_markdown("- item1\n- item2", &default_theme(), 80);
    assert!(text.lines.len() >= 2);
    let has_bullet1 = text.lines.iter().any(|l| {
        let line_str: String = l.spans.iter().map(|s| s.content.clone()).collect();
        line_str.contains("•") && line_str.contains("item1")
    });
    assert!(has_bullet1, "Expected bullet • and item1");
    let has_bullet2 = text.lines.iter().any(|l| {
        let line_str: String = l.spans.iter().map(|s| s.content.clone()).collect();
        line_str.contains("item2")
    });
    assert!(has_bullet2, "Expected item2");
}

#[test]
fn parse_ordered_list() {
    let text = parse_markdown("1. first\n2. second", &default_theme(), 80);
    assert!(text.lines.len() >= 2);
    let has_1 = text.lines.iter().any(|l| {
        let line_str: String = l.spans.iter().map(|s| s.content.clone()).collect();
        line_str.contains("1.") && line_str.contains("first")
    });
    assert!(has_1, "Expected '1. first'");
    let has_2 = text.lines.iter().any(|l| {
        let line_str: String = l.spans.iter().map(|s| s.content.clone()).collect();
        line_str.contains("2.") && line_str.contains("second")
    });
    assert!(has_2, "Expected '2. second'");
}

#[test]
fn parse_blockquote() {
    let text = parse_markdown("> quoted", &default_theme(), 80);
    assert!(!text.lines.is_empty());
    let has_prefix = text
        .lines
        .iter()
        .any(|l| l.spans.iter().any(|s| s.content.contains("▍")));
    assert!(has_prefix, "Expected blockquote prefix ▍");
}

#[test]
fn parse_horizontal_rule() {
    let text = parse_markdown("---", &default_theme(), 80);
    assert!(!text.lines.is_empty());
    let has_rule = text
        .lines
        .iter()
        .any(|l| l.spans.iter().any(|s| s.content.contains("─")));
    assert!(has_rule, "Expected horizontal rule ─");
}

#[test]
fn parse_table() {
    let text = parse_markdown(
        "| H1 | H2 |\n| --- | --- |\n| A | B |",
        &default_theme(),
        80,
    );
    assert!(text.lines.len() >= 3);
    let has_border = text.lines.iter().any(|l| {
        l.spans
            .iter()
            .any(|s| s.content.contains("┌") || s.content.contains("├") || s.content.contains("└"))
    });
    assert!(has_border, "Expected table box-drawing borders");
}

#[test]
fn parse_table_with_cjk() {
    let text = parse_markdown(
        "| 列1 | 列2 |\n| --- | --- |\n| 中文内容 | 更多中文 |",
        &default_theme(),
        80,
    );
    assert!(text.lines.len() >= 3);
    // CJK 字符应该正确对齐
    let has_content = text.lines.iter().any(|l| {
        let line_str: String = l.spans.iter().map(|s| s.content.clone()).collect();
        line_str.contains("中文内容") || line_str.contains("更多中文")
    });
    assert!(has_content, "Expected CJK content in table");
}

#[test]
fn parse_table_with_wrap() {
    let text = parse_markdown(
        "| 短 | 非常长的单元格内容需要自动换行 |\n| --- | --- |\n| A | B |",
        &default_theme(),
        40, // 限制宽度以触发换行
    );
    assert!(text.lines.len() >= 4, "Table should wrap long content");
}

/// 复现 Issue #2026-05-18：4 列表格，第一列 CJK 被比例缩放压到极窄
///
/// 场景：Col 2 有很长字符串（如 URL），比例缩放后 Col 2 占大头，
/// Col 1 (CJK) 只剩 ~4 显示列，每行仅 2 个中文字，不可读。
#[test]
fn parse_table_cjk_first_column_too_narrow() {
    // 4 列: CJK 列 + 长路径列 + 2 短列
    let md = "| 中文列 | 文件路径 | 状态 | 大小 |\n| --- | --- | --- | --- |\n| 这是一个需要测试的中文内容 | /very/long/path/that/takes/all/the/width/proportionally/and/squeezes/other/columns | OK | 1K |";
    let text = parse_markdown(md, &default_theme(), 50);
    assert!(text.lines.len() >= 3, "Table should render");

    // 计算第一列的视觉宽度（取数据行中第一列的行宽）
    // 渲染内容中识别 CJK 数据
    let first_col_content: Vec<String> = text
        .lines
        .iter()
        .flat_map(|l| {
            l.spans
                .iter()
                .filter(|s| s.content.contains("中文") || s.content.contains("需要测试"))
                .map(|s| s.content.to_string())
        })
        .collect();
    assert!(
        !first_col_content.is_empty(),
        "CJK content should appear in output"
    );

    // 关键断言：第一列应当有足够宽度，不应被压缩到每行仅 1-2 个 CJK 字符
    // 如果第一列宽度 ≤ 6 显示列，CJK 每行 ≤ 3 字（CJK=2 列宽），不可读
    // 收集第一列数据行的 Span 并检查其视觉宽度
    for line in &text.lines {
        let mut in_first_col = false;
        let mut col_width = 0usize;
        for span in &line.spans {
            if span.content == "│" {
                if !in_first_col {
                    in_first_col = true;
                } else {
                    // 遇到第二个 │，第一列结束
                    break;
                }
            } else if in_first_col && span.content != " " {
                col_width += span.content.width();
            }
        }
        if col_width > 0 {
            // 第一列至少需要 10 显示列才有基本可读性（CJK 每行 ≥ 5 字）
            assert!(
                col_width >= 10,
                "First CJK column width is {} (display cols), should be >= 10. Rendered lines: {:#?}",
                col_width,
                text.lines.iter().map(|l| {
                    l.spans.iter().map(|s| s.content.clone()).collect::<String>()
                }).collect::<Vec<_>>()
            );
            break; // 只检查第一行有内容的行
        }
    }
}

#[test]
fn parse_code_block_with_language() {
    let text = parse_markdown("```rust\nfn main() {}\n```", &default_theme(), 80);
    assert!(!text.lines.is_empty());
    let all: String = text
        .lines
        .iter()
        .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref()))
        .collect();
    // 不再输出 [lang] 标签
    assert!(
        !all.contains("[rust]"),
        "Should not have language tag, got: {all:?}"
    );
    assert!(all.contains("fn main"), "Should contain code content");
}

#[test]
fn parse_markdown_respects_width() {
    // 测试不同宽度的渲染
    let text_wide = parse_markdown(
        "| A | B |\n| --- | --- |\n| 内容 | 更多内容 |",
        &default_theme(),
        100,
    );
    let text_narrow = parse_markdown(
        "| A | B |\n| --- | --- |\n| 内容 | 更多内容 |",
        &default_theme(),
        30,
    );

    // 窄版本应该有更多行（因为换行）
    assert!(
        text_narrow.lines.len() >= text_wide.lines.len(),
        "Narrower width should result in more lines"
    );
}

#[cfg(feature = "markdown-highlight")]
#[test]
fn parse_multiline_code_block_rust_highlight() {
    let text = parse_markdown(
        "```rust\nfn main() {\n    println!(\"hello\");\n}\n```",
        &default_theme(),
        80,
    );
    // 3 行代码内容
    assert!(text.lines.len() >= 3, "多行代码块应至少产生 3 行");
    // 验证非单行模式：有代码内容
    let has_content = text.lines.iter().any(|l| {
        let line_str: String = l.spans.iter().map(|s| s.content.as_ref()).collect();
        line_str.contains("fn main")
    });
    assert!(has_content, "多行代码块应有代码内容");
    // 验证语法高亮产生了多种颜色（不全是统一 text 颜色）
    let all_colors: std::collections::HashSet<_> = text
        .lines
        .iter()
        .flat_map(|l| l.spans.iter().filter_map(|s| s.style.fg))
        .collect();
    assert!(
        all_colors.len() > 1,
        "语法高亮应产生多种颜色，实际颜色数: {}",
        all_colors.len()
    );
}

#[cfg(feature = "markdown-highlight")]
#[test]
fn parse_multiline_code_block_unknown_lang_fallback() {
    let text = parse_markdown(
        "```unknown_lang_xyz\ncode here\nmore code\n```",
        &default_theme(),
        80,
    );
    assert!(text.lines.len() >= 2, "未识别语言仍应输出代码行");
    // 回退模式：每行应有代码内容
    let has_content = text.lines.iter().any(|l| {
        let line_str: String = l.spans.iter().map(|s| s.content.as_ref()).collect();
        line_str.contains("code here")
    });
    assert!(has_content, "回退模式应有代码内容");
    // 回退模式：所有代码文本使用统一 text 颜色
    let code_spans: Vec<_> = text
        .lines
        .iter()
        .flat_map(|l| {
            l.spans
                .iter()
                .filter(|s| !s.content.contains('│') && !s.content.trim().is_empty())
        })
        .collect();
    for span in &code_spans {
        assert_eq!(
            span.style.fg,
            Some(default_theme().text()),
            "回退模式代码应使用 text 颜色"
        );
    }
}

#[cfg(feature = "markdown-highlight")]
#[test]
fn parse_multiline_code_block_no_lang_fallback() {
    let text = parse_markdown("```\ncode here\nmore code\n```", &default_theme(), 80);
    assert!(text.lines.len() >= 2, "省略语言标签仍应输出代码行");
    let has_content = text
        .lines
        .iter()
        .any(|l| l.spans.iter().any(|s| s.content.contains("code here")));
    assert!(has_content, "回退模式应有代码内容");
}

/// 集成测试：parse_markdown 多次调用应产生相同结果（幂等性）
#[test]
fn parse_markdown_cache_hit_on_repeat() {
    // Arrange: 同一内容调用两次
    let content = "# 缓存测试\n\n这是一段用于测试缓存命中的 Markdown 文本。";
    let theme = default_theme();
    let width = 80;

    // Act: 两次调用
    let result1 = parse_markdown(content, &theme, width);
    let result2 = parse_markdown(content, &theme, width);

    // Assert: 两次结果完全一致（幂等性）
    assert_eq!(
        result1.lines.len(),
        result2.lines.len(),
        "两次解析结果行数应一致"
    );
    for (a, b) in result1.lines.iter().zip(result2.lines.iter()) {
        let text_a: String = a.spans.iter().map(|s| s.content.as_ref()).collect();
        let text_b: String = b.spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(text_a, text_b, "两次解析对应行内容应一致");
    }
}

/// 集成测试：不同宽度产生不同渲染结果
#[test]
fn parse_markdown_different_width_different_cache_entry() {
    // Arrange
    let content = "| A | B |\n| --- | --- |\n| 内容 | 更多内容 |";
    let theme = default_theme();

    // Act: 用两个不同宽度调用
    let r1 = parse_markdown(content, &theme, 80);
    let r2 = parse_markdown(content, &theme, 40);

    // Assert: 不同宽度应产生不同的渲染结果（行数或内容不同）
    let different = r1.lines.len() != r2.lines.len()
        || r1.lines.iter().zip(r2.lines.iter()).any(|(a, b)| {
            a.spans
                .iter()
                .map(|s| s.content.as_ref())
                .collect::<String>()
                != b.spans
                    .iter()
                    .map(|s| s.content.as_ref())
                    .collect::<String>()
        });
    assert!(different, "不同宽度应产生不同渲染结果");
}

/// 集成测试：空字符串返回空结果
#[test]
fn parse_markdown_empty_not_cached() {
    // Act
    let result = parse_markdown("", &default_theme(), 80);

    // Assert: 空字符串直接返回空 Text
    assert!(
        result.lines.is_empty() || result.lines.iter().all(|l| l.spans.is_empty()),
        "空字符串应返回空结果"
    );
}
