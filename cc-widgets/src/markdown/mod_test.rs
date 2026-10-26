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
    // 对齐 CC：标题前不额外留空行（消息开头直接是标题），标题后留 1 空行
    let line = &text.lines[0];
    let heading_found = line.spans.iter().any(|s| s.content.contains("Hello"));
    assert!(heading_found, "Expected 'Hello' in heading output");
    // 对齐 CC：H1 = bold + italic + underline，不带颜色
    let has_bold = line
        .spans
        .iter()
        .any(|s| s.style.add_modifier.contains(Modifier::BOLD));
    assert!(has_bold, "Expected BOLD modifier on heading");
    let has_italic = line
        .spans
        .iter()
        .any(|s| s.style.add_modifier.contains(Modifier::ITALIC));
    assert!(has_italic, "H1 应为 bold+italic+underline（对齐 CC）");
    let has_underline = line
        .spans
        .iter()
        .any(|s| s.style.add_modifier.contains(Modifier::UNDERLINED));
    assert!(has_underline, "H1 应带下划线（对齐 CC）");
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
        line_str.contains("- ") && line_str.contains("item1")
    });
    assert!(has_bullet1, "对齐 CC：无序列表用 `- `，实际缺 bullet");
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
        .any(|l| l.spans.iter().any(|s| s.content.contains("▎")));
    assert!(has_prefix, "对齐 CC：引用前缀用 ▎");
}

#[test]
fn parse_horizontal_rule() {
    let text = parse_markdown("---", &default_theme(), 80);
    assert!(!text.lines.is_empty());
    // 对齐 CC：hr 固定输出 `---`
    let has_rule = text
        .lines
        .iter()
        .any(|l| l.spans.iter().any(|s| s.content.contains("---")));
    assert!(has_rule, "对齐 CC：hr 应输出 ---");
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

/// 复现：表格含尾部空列时，空列不应分到任何多余宽度（空列宽至少为 MIN_COLUMN_WIDTH=3）。
#[test]
fn parse_table_trailing_empty_column_not_hoarding() {
    let md = "| 长标题列内容示例 | 中等内容列xx | 短内容x |  |\n\
              | --- | --- | --- | --- |\n\
              | 长标题列内容示例数据更长一些些 | 中等内容列xx数据 | 短内容x |  |";
    // 宽视口下走横向框线表，验证列宽分配行为
    let text = parse_markdown(md, &default_theme(), 80);
    let border: String = text
        .lines
        .iter()
        .find(|l| l.spans.iter().any(|s| s.content.contains('┌')))
        .expect("应有表格顶边框")
        .spans
        .iter()
        .map(|s| s.content.as_ref())
        .collect();

    // 顶边框形如 ┌─...─┬─...─┬─...─┬┐，按 ┬ 分段，段内 fill 数 = 列宽 + 2
    let segments: Vec<&str> = border.split('┬').collect();
    assert_eq!(segments.len(), 4, "应有 4 列，边框: {border}");
    let last_col_fill = segments[3].chars().filter(|c| *c == '─').count();
    // 对齐 CC：空列下限为 MIN_COLUMN_WIDTH = 3，+2 空格位 = 5 填字符
    assert_eq!(
        last_col_fill, 5,
        "尾部空列宽度应等于下限 3（fill=5），边框: {border}"
    );
}

/// 多内容列按 extras 比例分配剩余宽度。
#[test]
fn parse_table_extra_share_not_truncated_to_last() {
    let md = "| AaaaaaaaaaBbbbbbbbbb | CccccccccDdddd | EeeeeeeFfff |  |\n\
              | --- | --- | --- | --- |\n\
              | AaaaaaaaaaBbbbbbbbbbX | CccccccccDddddY | EeeeeeeFfffZ |  |";
    let text = parse_markdown(md, &default_theme(), 80);
    let border: String = text
        .lines
        .iter()
        .find(|l| l.spans.iter().any(|s| s.content.contains('┌')))
        .expect("应有表格顶边框")
        .spans
        .iter()
        .map(|s| s.content.as_ref())
        .collect();

    let segments: Vec<&str> = border.split('┬').collect();
    assert_eq!(segments.len(), 4, "应有 4 列，边框: {border}");
    let widths: Vec<usize> = segments
        .iter()
        .map(|s| s.chars().filter(|c| *c == '─').count().saturating_sub(2))
        .collect();
    // 宽视口下 4 列都能完全容纳，空列取下限 3
    assert_eq!(
        widths,
        vec![21, 15, 12, 3],
        "宽视口下各列应等于理想宽度，空列为下限 3，边框: {border}"
    );
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
fn test_parse_markdown_with_links_retains_url() {
    let md = "请查看 [#222](https://github.com/cc-claws/cc-code/pull/222) 详情";
    let doc = parse_markdown_with_links(md, &default_theme(), 80);
    assert_eq!(doc.links.len(), 1, "应正确提取出 1 个超链接");
    let link = &doc.links[0];
    assert_eq!(
        link.url, "https://github.com/cc-claws/cc-code/pull/222",
        "链接 URL 必须完整保留，不能被丢弃"
    );
    let line = &doc.text.lines[link.line];
    let plain: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
    use unicode_segmentation::UnicodeSegmentation;
    let label: String = plain
        .graphemes(true)
        .skip(link.g_start)
        .take(link.g_end - link.g_start)
        .collect();
    assert_eq!(
        label, "#222",
        "链接命中区标签文本应与 Markdown 锚文本精确一致"
    );
}

#[test]
fn test_parse_markdown_multiple_links() {
    let md = "链接1: [Doc](https://example.com/doc) 以及链接2: [Repo](https://github.com)";
    let doc = parse_markdown_with_links(md, &default_theme(), 80);
    assert_eq!(doc.links.len(), 2, "应提取出 2 个超链接");
    assert_eq!(doc.links[0].url, "https://example.com/doc");
    assert_eq!(doc.links[1].url, "https://github.com");
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

/// 回归测试：包含超长无空格路径和多列的复杂表格在各种视口宽度下绝对不溢出 max_width
#[test]
fn parse_table_long_path_never_overflows_max_width() {
    let md = r#"| # | 本地文件路径 | 拟定 GitHub Issue 标题 | 状态 / 优先级 | 核心要点 |
|---|---|---|---|---|
| 1 | spec/issues/2026-09-20-tui-terminal-title-dynamic-status-surface-alignment.md | feat(tui): 对齐 OpenAI Codex 规范的动态终端标题 (Status Surface) 体系 | Open / 中 | 解决当前写死 CC Code - Running 导致的多 Tab 无法辨识、缺少 Action Required 审批阻塞提醒、OSC 写入未去重问题；对齐 Codex codex-rs 状态表面架构。 |
| 2 | spec/issues/2026-09-20-cmd-status-bar-glyph-ghosting-and-visual-adaptation.md | fix(tui): Windows 传统控制台 (CMD) 底部状态栏字符残影与跨平台视觉自适应 | 已立项 / 高 | 解决 Windows CMD (新宋体 CP936) 下状态栏 █░进度条、? Emoji、✓、· 歧义宽度导致的列宽偏移、文字撕裂与无法擦除的幽灵残影。 |"#;

    for max_width in [60, 80, 100, 120, 150] {
        let text = parse_markdown(md, &default_theme(), max_width);
        for (idx, line) in text.lines.iter().enumerate() {
            let line_w: usize = line.spans.iter().map(|s| s.content.width()).sum();
            assert!(
                line_w <= max_width,
                "在 max_width={} 下，第 {} 行宽度 {} 超出限制: {:?}",
                max_width,
                idx,
                line_w,
                line.spans
                    .iter()
                    .map(|s| s.content.as_ref())
                    .collect::<String>()
            );
        }
    }
}

/// Demo 1：无序列表长内容折行后续行应与首行文字左边缘对齐（悬挂缩进）
#[test]
fn test_unordered_list_hanging_indent() {
    // bullet "- " 占 2 列（`-` 宽 1 + 空格 1）
    // max_width=30，文本需要超过 28 列才会折行
    let md = "- 结论摘要非系统问题历史绑定正常命中订单与日志证据齐全";
    let text = parse_markdown(md, &default_theme(), 30);

    // 应该产生多行（因为内容超宽）
    assert!(
        text.lines.len() >= 2,
        "长列表项应折成多行，实际 {} 行: {:?}",
        text.lines.len(),
        text.lines
            .iter()
            .map(|l| l
                .spans
                .iter()
                .map(|s| s.content.as_ref())
                .collect::<String>())
            .collect::<Vec<_>>()
    );

    // 首行应有 bullet "•"
    let first_line: String = text.lines[0]
        .spans
        .iter()
        .map(|s| s.content.as_ref())
        .collect();
    assert!(
        first_line.contains("- "),
        "首行应包含 bullet，实际: {:?}",
        first_line
    );

    // 续行应以空格开头（悬挂缩进），宽度 = bullet 显示宽度
    for i in 1..text.lines.len() {
        let line: String = text.lines[i]
            .spans
            .iter()
            .map(|s| s.content.as_ref())
            .collect();
        if line.is_empty() {
            continue; // 跳过空行
        }
        let leading_spaces: usize = line.chars().take_while(|c| *c == ' ').count();
        assert!(
            leading_spaces >= 2,
            "续行 {} 应有 ≥2 列空格缩进（悬挂缩进），实际开头: {:?}",
            i,
            &line[..line.len().min(10)]
        );
    }
}

/// Demo 2：有序列表长内容折行后续行缩进应与序号后文字对齐
#[test]
fn test_ordered_list_hanging_indent() {
    // "1. " 占 3 列，max_width=30，文本需要超过 27 列
    let md = "1. 第一步打开配置文件并检查所有环境变量确保配置正确后重启";
    let text = parse_markdown(md, &default_theme(), 30);

    assert!(
        text.lines.len() >= 2,
        "长有序列表项应折成多行，实际 {} 行",
        text.lines.len()
    );

    // 首行应有序号 "1."
    let first_line: String = text.lines[0]
        .spans
        .iter()
        .map(|s| s.content.as_ref())
        .collect();
    assert!(first_line.contains("1."), "首行应包含序号 '1.'");

    // 续行缩进 = "1. " 的宽度 = 3
    for i in 1..text.lines.len() {
        let line: String = text.lines[i]
            .spans
            .iter()
            .map(|s| s.content.as_ref())
            .collect();
        if line.is_empty() {
            continue;
        }
        let leading_spaces: usize = line.chars().take_while(|c| *c == ' ').count();
        assert!(
            leading_spaces >= 3,
            "有序列表续行 {} 应有 ≥3 列空格缩进，实际开头: {:?}",
            i,
            &line[..line.len().min(10)]
        );
    }
}

/// Demo 3：嵌套列表各级悬挂缩进逐层叠加
#[test]
fn test_nested_list_hanging_indent() {
    // 嵌套列表：父项 "- " 2列，子项 "  - " 4列
    let md = "- 父项\n  - 子项内容非常长需要折行的文字这里继续写更多内容以确保超出宽度";
    let text = parse_markdown(md, &default_theme(), 30);

    // 找到子项的行（以 "  -" 开头的行）
    let mut child_first_line_idx = None;
    for (i, line) in text.lines.iter().enumerate() {
        let line_str: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        if line_str.contains("  -") && line_str.contains("子项") {
            child_first_line_idx = Some(i);
            break;
        }
    }
    let child_idx = child_first_line_idx.expect("应有子项行");

    // 子项续行的缩进应 ≥ 4 列（"  " 层级缩进 + "- " bullet 宽度）
    if child_idx + 1 < text.lines.len() {
        let next_line: String = text.lines[child_idx + 1]
            .spans
            .iter()
            .map(|s| s.content.as_ref())
            .collect();
        if !next_line.is_empty() && !next_line.contains("- ") {
            let leading_spaces: usize = next_line.chars().take_while(|c| *c == ' ').count();
            assert!(
                leading_spaces >= 4,
                "嵌套子项续行应有 ≥4 列缩进，实际 {} 列，行内容: {:?}",
                leading_spaces,
                next_line
            );
        }
    }
}

/// Demo 4：引用块长内容折行后续行应有引用前缀对齐
#[test]
fn test_blockquote_hanging_indent() {
    let md = "> 引用内容非常长需要折行的文字这里继续写更多内容以确保超出指定宽度限制";
    let text = parse_markdown(md, &default_theme(), 30);

    assert!(
        text.lines.len() >= 2,
        "长引用块应折成多行，实际 {} 行",
        text.lines.len()
    );

    // 所有非空行都应有引用前缀 "▎"
    for (i, line) in text.lines.iter().enumerate() {
        let line_str: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        if line_str.is_empty() {
            continue;
        }
        assert!(
            line_str.contains('▎'),
            "引用块第 {} 行应包含引用前缀 '▎'，实际: {:?}",
            i,
            line_str
        );
    }
}

/// 验证每行宽度不超过 max_width
#[test]
fn test_hanging_indent_respects_max_width() {
    let md = "- 这是一个非常长的列表项内容需要在指定宽度内折行并保持悬挂缩进对齐效果";
    for max_width in [20, 30, 40, 50, 60] {
        let text = parse_markdown(md, &default_theme(), max_width);
        for (idx, line) in text.lines.iter().enumerate() {
            let line_w: usize = line.spans.iter().map(|s| s.content.width()).sum();
            assert!(
                line_w <= max_width,
                "在 max_width={} 下，第 {} 行宽度 {} 超出限制: {:?}",
                max_width,
                idx,
                line_w,
                line.spans
                    .iter()
                    .map(|s| s.content.as_ref())
                    .collect::<String>()
            );
        }
    }
}

// ── 超链接命中区：结构与边界场景 ──────────────────────────────

/// 辅助：按链接命中区从渲染结果中提取标签文本
fn extract_link_label(doc: &MarkdownDoc, hit: &LinkHit) -> String {
    use unicode_segmentation::UnicodeSegmentation;
    let plain: String = doc.text.lines[hit.line]
        .spans
        .iter()
        .map(|s| s.content.as_ref())
        .collect();
    plain
        .graphemes(true)
        .skip(hit.g_start)
        .take(hit.g_end - hit.g_start)
        .collect()
}

#[test]
fn test_link_hit_in_list_item() {
    let doc = parse_markdown_with_links(
        "- 见 [#1](https://example.com/1) 说明",
        &default_theme(),
        80,
    );
    assert_eq!(doc.links.len(), 1, "列表项内链接应被提取");
    assert_eq!(doc.links[0].url, "https://example.com/1");
    assert_eq!(
        extract_link_label(&doc, &doc.links[0]),
        "#1",
        "项目符号偏移后命中区应仍精确"
    );
}

#[test]
fn test_link_hit_in_blockquote() {
    let doc = parse_markdown_with_links("> 见 [#1](https://example.com/1)", &default_theme(), 80);
    assert_eq!(doc.links.len(), 1, "引用块内链接应被提取");
    assert_eq!(
        extract_link_label(&doc, &doc.links[0]),
        "#1",
        "引用前缀偏移后命中区应仍精确"
    );
}

#[test]
fn test_link_hit_multiple_same_line() {
    let doc = parse_markdown_with_links(
        "[A](https://a.com) 和 [B](https://b.com)",
        &default_theme(),
        80,
    );
    assert_eq!(doc.links.len(), 2);
    assert_eq!(extract_link_label(&doc, &doc.links[0]), "A");
    assert_eq!(doc.links[0].url, "https://a.com");
    assert_eq!(extract_link_label(&doc, &doc.links[1]), "B");
    assert_eq!(doc.links[1].url, "https://b.com");
}

#[test]
fn test_link_hit_adjacent_no_separator() {
    let doc =
        parse_markdown_with_links("[A](https://a.com)[B](https://b.com)", &default_theme(), 80);
    assert_eq!(doc.links.len(), 2, "紧邻的两个链接应分别提取");
    assert_eq!(extract_link_label(&doc, &doc.links[0]), "A");
    assert_eq!(extract_link_label(&doc, &doc.links[1]), "B");
}

#[test]
fn test_link_hit_bold_inside() {
    let doc = parse_markdown_with_links("**[#1](https://example.com/1)**", &default_theme(), 80);
    assert_eq!(doc.links.len(), 1);
    assert_eq!(
        extract_link_label(&doc, &doc.links[0]),
        "#1",
        "加粗包裹的链接命中区应精确"
    );
}

#[test]
fn test_link_hit_cjk_label() {
    let doc = parse_markdown_with_links("[中文链接](https://example.com)", &default_theme(), 80);
    assert_eq!(doc.links.len(), 1);
    assert_eq!(
        extract_link_label(&doc, &doc.links[0]),
        "中文链接",
        "CJK 标签的 grapheme 范围应正确"
    );
}

#[test]
fn test_link_hit_autolink() {
    let doc = parse_markdown_with_links("见 <https://example.com/x> 说明", &default_theme(), 80);
    assert_eq!(doc.links.len(), 1, "自动链接应被提取");
    assert_eq!(doc.links[0].url, "https://example.com/x");
    assert_eq!(
        extract_link_label(&doc, &doc.links[0]),
        "https://example.com/x"
    );
}

#[test]
fn test_link_hit_wrapped_across_lines_all_share_url() {
    // 窄宽度下列表项内的长链接标签必然折行，拆出的多段命中区都要指向同一 URL
    let md = "- [verylonglinklabeltextthatwillcertainlywrap](https://example.com/x) tail";
    for max_width in [20, 30, 40] {
        let doc = parse_markdown_with_links(md, &default_theme(), max_width);
        assert!(
            !doc.links.is_empty(),
            "max_width={max_width} 应至少提取一个命中区"
        );
        for hit in &doc.links {
            assert_eq!(
                hit.url, "https://example.com/x",
                "折行拆分的命中区应共享同一 URL"
            );
        }
    }
}

#[test]
fn test_link_hit_plain_text_has_no_links() {
    let doc = parse_markdown_with_links("没有任何链接的普通文本", &default_theme(), 80);
    assert!(doc.links.is_empty(), "纯文本不应产生链接命中区");
}

/// 对齐 CC：`---` 水平线固定输出 3 个连字符，与终端宽度无关。
#[test]
fn test_horizontal_rule_fills_available_width() {
    for width in [30usize, 40, 80, 120, 200] {
        let doc = parse_markdown("上文\n\n---\n\n下文", &default_theme(), width);
        let rule_line = doc
            .lines
            .iter()
            .find(|l| {
                let t: String = l.spans.iter().map(|s| s.content.as_ref()).collect();
                t == "---"
            })
            .unwrap_or_else(|| panic!("宽度 {width} 下应产生 `---` 水平线"));
        let text: String = rule_line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(
            text, "---",
            "对齐 CC：hr 与宽度无关，恒为 ---（width={width}）"
        );
    }
}

/// 对齐 CC：每个列表项独占一行，嵌套项不得与外层文字挤在同一行
#[test]
fn test_nested_list_items_are_on_separate_lines() {
    let text = parse_markdown(
        "- 外层一\n  - 内层一\n  - 内层二\n- 外层二\n",
        &default_theme(),
        80,
    );
    let lines: Vec<String> = text
        .lines
        .iter()
        .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
        .collect();
    assert_eq!(
        lines,
        vec!["- 外层一", "  - 内层一", "  - 内层二", "- 外层二"],
        "嵌套列表每项应独占一行，实际: {lines:?}"
    );
}

/// 对齐 CC：消息以标题开头时不留前置空行
#[test]
fn test_heading_at_start_has_no_leading_blank_line() {
    let text = parse_markdown("## 标题\n\n正文\n", &default_theme(), 80);
    let lines: Vec<String> = text
        .lines
        .iter()
        .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
        .collect();
    assert_eq!(lines, vec!["标题", "", "正文"], "实际: {lines:?}");
}

/// 对齐 CC：块间空行只来自源文本，多个空行压成 1 个
#[test]
fn test_block_spacing_follows_source_blank_lines() {
    let text = parse_markdown(
        "## 标题\n\n第一段文字。\n\n- 项一\n- 项二\n\n```rust\nfn main() {}\n```\n\n结尾段。\n",
        &default_theme(),
        80,
    );
    let lines: Vec<String> = text
        .lines
        .iter()
        .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
        .collect();
    assert_eq!(
        lines,
        vec![
            "标题",
            "",
            "第一段文字。",
            "",
            "- 项一",
            "- 项二",
            "",
            "fn main() {}",
            "",
            "结尾段。"
        ],
        "块间空行应与 CC 一致，实际: {lines:?}"
    );
}

/// 对齐 CC：源文本块间没有空行时不留空行
#[test]
fn test_no_blank_line_between_blocks_without_source_blank() {
    let list = parse_markdown("文字\n- 项一\n- 项二\n", &default_theme(), 80);
    let lines: Vec<String> = list
        .lines
        .iter()
        .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
        .collect();
    assert_eq!(lines, vec!["文字", "- 项一", "- 项二"], "实际: {lines:?}");

    let code = parse_markdown("文字\n```js\nx\n```\n", &default_theme(), 80);
    let lines: Vec<String> = code
        .lines
        .iter()
        .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
        .collect();
    assert_eq!(lines, vec!["文字", "x"], "实际: {lines:?}");
}

/// 对齐 CC：loose list 的项间空行不渲染（源空行属于 list token 内部）
#[test]
fn test_loose_list_has_no_blank_line_between_items() {
    let text = parse_markdown("- 项一\n\n- 项二\n", &default_theme(), 80);
    let lines: Vec<String> = text
        .lines
        .iter()
        .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
        .collect();
    assert_eq!(lines, vec!["- 项一", "- 项二"], "实际: {lines:?}");
}

/// 渲染为纯文本行（用于与 CC 的行结构对拍）
fn render_md_lines(src: &str) -> Vec<String> {
    let text = parse_markdown(src, &default_theme(), 80);
    text.lines
        .iter()
        .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
        .collect()
}

/// 与 CC（marked@17 + `formatToken`）逐语法对拍：块间空行只来自源文本。
///
/// 已知的**有意差异**（不在本测试断言范围内，改符号会影响全局观感）：
/// - 列表/引用前缀已对齐 CC：`- ` / `▎ `（CC 的引用条是 `▎`，不是 `│`）
/// - 表格：peri 用 box drawing，CC 用 `| a | b |`
/// - 图片：peri 显示 alt，CC 显示 URL
/// - 三级嵌套列表缩进：CC 逐层累加到 6 空格，peri 每层固定 2 空格
/// - CC 自身缺陷：`loose list` 丢 bullet、删除线内容被吞，peri 保留正确行为
#[test]
fn test_markdown_structure_matches_cc() {
    let cases: Vec<(&str, &str, Vec<&str>)> = vec![
        ("h1", "# 一级标题\n正文\n", vec!["一级标题", "", "正文"]),
        (
            "h2_h3",
            "## 二级\n\n### 三级\n\n正文\n",
            vec!["二级", "", "三级", "", "正文"],
        ),
        (
            "multi_para",
            "第一段。\n\n第二段。\n\n第三段。\n",
            vec!["第一段。", "", "第二段。", "", "第三段。"],
        ),
        ("ul_tight", "- 项一\n- 项二\n", vec!["- 项一", "- 项二"]),
        (
            "ul_after_para",
            "文字\n- 项一\n- 项二\n",
            vec!["文字", "- 项一", "- 项二"],
        ),
        (
            "ul_with_blank_around",
            "文字\n\n- 项一\n- 项二\n\n文字2\n",
            vec!["文字", "", "- 项一", "- 项二", "", "文字2"],
        ),
        (
            "ol_start3",
            "3. 第三\n4. 第四\n",
            vec!["3. 第三", "4. 第四"],
        ),
        (
            "nested_2",
            "- 外层一\n  - 内层一\n  - 内层二\n- 外层二\n",
            vec!["- 外层一", "  - 内层一", "  - 内层二", "- 外层二"],
        ),
        ("loose_list", "- 项一\n\n- 项二\n", vec!["- 项一", "- 项二"]),
        (
            "fence_lang",
            "```rust\nfn main() {}\n```\n",
            vec!["fn main() {}"],
        ),
        (
            "fence_multi",
            "```js\nlet a = 1\nlet b = 2\n```\n",
            vec!["let a = 1", "let b = 2"],
        ),
        (
            "fence_after_para",
            "文字\n```js\nx\n```\n",
            vec!["文字", "x"],
        ),
        (
            "fence_blank_around",
            "文字\n\n```js\nx\n```\n\n文字2\n",
            vec!["文字", "", "x", "", "文字2"],
        ),
        (
            "inline_styles",
            "这里有 `code`、**粗体**、*斜体*、~~删除线~~。\n",
            // CC 主动禁用删除线（模型常把 ~100 当约等于），peri 同样关闭，
            // 因此 `~~删除线~~` 原样保留
            vec!["这里有 code、粗体、斜体、~~删除线~~。"],
        ),
        (
            "link_inline",
            "看 [文档](https://example.com) 了解。\n",
            vec!["看 文档 了解。"],
        ),
        (
            "autolink",
            "访问 <https://example.com> 即可。\n",
            vec!["访问 https://example.com 即可。"],
        ),
        (
            "blockquote",
            "> 引用一行\n> 引用二行\n",
            vec!["▎ 引用一行", "▎ 引用二行"],
        ),
        (
            "blockquote_multi_para",
            "> 第一段\n>\n> 第二段\n",
            vec!["▎ 第一段", "", "▎ 第二段"],
        ),
        (
            "blockquote_with_list",
            "> - 项一\n> - 项二\n",
            vec!["▎ - 项一", "▎ - 项二"],
        ),
        ("hardbreak", "第一行  \n第二行\n", vec!["第一行", "第二行"]),
        ("soft_lines", "第一行\n第二行\n", vec!["第一行", "第二行"]),
        (
            "bq_soft",
            "> 第一行\n> 第二行\n",
            vec!["▎ 第一行", "▎ 第二行"],
        ),
        (
            "mixed",
            "## 步骤\n\n1. 第一步\n2. 第二步\n\n```bash\nnpm i\n```\n\n完成。\n",
            vec![
                "步骤",
                "",
                "1. 第一步",
                "2. 第二步",
                "",
                "npm i",
                "",
                "完成。",
            ],
        ),
    ];
    for (name, src, expected) in cases {
        assert_eq!(render_md_lines(src), expected, "case: {name}");
    }
}

/// 表格：peri 用 box drawing（CC 用 `| a | b |`），但前后空行结构必须一致
#[test]
fn test_table_spacing_matches_cc() {
    let lines = render_md_lines("文字\n\n| a | b |\n| --- | --- |\n| 1 | 2 |\n\n文字2\n");
    assert_eq!(
        lines,
        vec![
            "文字",
            "",
            "┌─────┬─────┐",
            "│  a  │  b  │",
            "├─────┼─────┤",
            "│ 1   │ 2   │",
            "└─────┴─────┘",
            "",
            "文字2",
        ],
        "实际: {lines:?}"
    );
}

/// 图片：对齐 CC `formatToken` 的 `image` 分支，输出 URL 而非 alt 文本
#[test]
fn test_image_spacing_matches_cc() {
    assert_eq!(
        render_md_lines("![图](https://example.com/a.png)\n"),
        vec!["https://example.com/a.png"]
    );
}

/// 水平线：对齐 CC `formatToken` 的 `hr` 分支输出 `---`。
///
/// 有意偏离：CC 的 hr token 不带 EOL、下个块也不补空行，会输出 `---文字`；
/// 这里保留 peri 的块分隔（hr 与后文分行）。
#[test]
fn test_hr_spacing_matches_cc() {
    // 有意偏离：CC 的 hr token 吞掉后续换行，`---\n\n正文` 之间没有空行。
    // peri 保留源文本空行，避免 `---` 与正文视觉粘连。
    let lines = render_md_lines("文字\n\n---\n\n文字2\n");
    assert_eq!(lines.len(), 5, "实际: {lines:?}");
    assert_eq!(lines[0], "文字");
    assert_eq!(lines[1], "");
    assert_eq!(lines[2], "---", "对齐 CC：hr 固定输出 ---");
    assert_eq!(lines[3], "", "hr 后按源文本保留空行（CC 缺陷不复制）");
    assert_eq!(lines[4], "文字2");

    // 文档以 hr 开头：无前置空行；hr 与后文分行（CC 会粘成 `---文字`）
    let lines = render_md_lines("---\n文字\n");
    assert_eq!(lines.len(), 2, "实际: {lines:?}");
    assert_eq!(lines[0], "---");
    assert_eq!(lines[1], "文字");
}

// ── 表格内链接命中区（Issue #703）─────────────────────────────

#[test]
fn test_table_link_hit_issue_example() {
    // Issue 原例 + 前置段落（验证表格起始行偏移换算）
    let md = "前言\n\n| 项目 | 状态 | 详情 |\n| --- | --- | --- |\n| Pull Request | [#702 MERGED](https://github.com/cc-claws/cc-code/pull/702) | Merge commit: 735c0406 |\n| 关联 Issue | [#701 CLOSED](https://github.com/cc-claws/cc-code/issues/701) | 自动完成关闭 |";
    let doc = parse_markdown_with_links(md, &default_theme(), 80);
    assert_eq!(doc.links.len(), 2, "表格内两个链接都应被提取");
    assert_eq!(
        doc.links[0].url,
        "https://github.com/cc-claws/cc-code/pull/702"
    );
    assert_eq!(
        extract_link_label(&doc, &doc.links[0]),
        "#702 MERGED",
        "第一行链接命中区应精确落在锚文本上"
    );
    assert_eq!(
        doc.links[1].url,
        "https://github.com/cc-claws/cc-code/issues/701"
    );
    assert_eq!(
        extract_link_label(&doc, &doc.links[1]),
        "#701 CLOSED",
        "第二行链接命中区应精确落在锚文本上"
    );
}

#[test]
fn test_table_link_hit_header_and_two_in_one_cell() {
    let md = "| [H](https://h.com) | B |\n| --- | --- |\n| [A](https://a.com) 和 [B](https://b.com) | x |";
    let doc = parse_markdown_with_links(md, &default_theme(), 80);
    assert_eq!(doc.links.len(), 3, "表头链接 + 同格双链接都应被提取");
    // 顺序：表头行在前，同格内按源码顺序
    assert_eq!(extract_link_label(&doc, &doc.links[0]), "H");
    assert_eq!(doc.links[0].url, "https://h.com");
    assert_eq!(extract_link_label(&doc, &doc.links[1]), "A");
    assert_eq!(doc.links[1].url, "https://a.com");
    assert_eq!(extract_link_label(&doc, &doc.links[2]), "B");
    assert_eq!(doc.links[2].url, "https://b.com");
}

#[test]
fn test_table_link_hit_alignments() {
    // 长表头拉出 padding，覆盖左 / 居中 / 右三种对齐的列偏移
    let md = "| LongHeaderL | LongHeaderC | LongHeaderR |\n| --- | :---: | ---: |\n| [A](https://a.com) | [B](https://b.com) | [C](https://c.com) |";
    let doc = parse_markdown_with_links(md, &default_theme(), 80);
    assert_eq!(doc.links.len(), 3);
    assert_eq!(extract_link_label(&doc, &doc.links[0]), "A");
    assert_eq!(
        extract_link_label(&doc, &doc.links[1]),
        "B",
        "居中列的左 padding 应计入命中区"
    );
    assert_eq!(
        extract_link_label(&doc, &doc.links[2]),
        "C",
        "右对齐列的左 padding 应计入命中区"
    );
}

#[test]
fn test_table_link_hit_cjk_label() {
    // CJK 显示宽 2 列、grapheme 1 个：前缀换算必须按 grapheme 计，不能按列宽
    let md = "| 项目 | 状态 |\n| --- | --- |\n| 中文说明 | [中文链接](https://example.com) |";
    let doc = parse_markdown_with_links(md, &default_theme(), 80);
    assert_eq!(doc.links.len(), 1);
    assert_eq!(doc.links[0].url, "https://example.com");
    assert_eq!(
        extract_link_label(&doc, &doc.links[0]),
        "中文链接",
        "CJK 前缀列后命中区仍应精确"
    );
}

#[test]
fn test_table_link_hit_wrapped_across_lines() {
    // 窄视口下无空格长标签硬断词折行，拆出的多段应共享 URL 且拼回原标签
    let md = "| 项目 | 详情 |\n| --- | --- |\n| 链接 | [verylonglinklabeltextthatwraps](https://example.com/x) |";
    let doc = parse_markdown_with_links(md, &default_theme(), 30);
    assert!(
        doc.links.len() >= 2,
        "折行后链接应拆成多段命中区，实际 {:?}",
        doc.links
    );
    for hit in &doc.links {
        assert_eq!(hit.url, "https://example.com/x");
    }
    let labels: String = doc
        .links
        .iter()
        .map(|h| extract_link_label(&doc, h))
        .collect();
    assert_eq!(labels, "verylonglinklabeltextthatwraps");
}

#[test]
fn test_table_link_hit_vertically_centered_cell() {
    // 第二列折成 3 行，单行链接格垂直居中落在中间行：行号必须含垂直偏移
    let md = "| 链接 | 说明 |\n| --- | --- |\n| [L](https://l.com) | aaaa bbbb cccc dddd eeee ffff gggg hhhh iiii jjjj kkkk llll |";
    let doc = parse_markdown_with_links(md, &default_theme(), 40);
    assert_eq!(doc.links.len(), 1);
    assert_eq!(
        extract_link_label(&doc, &doc.links[0]),
        "L",
        "垂直居中后命中区行号应指向链接实际所在行"
    );
}

#[test]
fn test_table_link_hit_url_as_label() {
    // 锚文本即 URL 时走退化分支：显示 URL 本身，且应可点
    let md =
        "| 项目 | 详情 |\n| --- | --- |\n| 链接 | [https://example.com/x](https://example.com/x) |";
    let doc = parse_markdown_with_links(md, &default_theme(), 80);
    assert_eq!(doc.links.len(), 1);
    assert_eq!(doc.links[0].url, "https://example.com/x");
    assert_eq!(
        extract_link_label(&doc, &doc.links[0]),
        "https://example.com/x"
    );
}

#[test]
fn test_table_link_hit_mailto_excluded() {
    let md =
        "| 联系 | 邮箱 |\n| --- | --- |\n| 作者 | [mail@example.com](mailto:mail@example.com) |";
    let doc = parse_markdown_with_links(md, &default_theme(), 80);
    assert!(
        doc.links.is_empty(),
        "mailto 在表格内同样只显示邮箱、不产生命中区"
    );
}

#[test]
fn test_table_without_links_has_no_hits() {
    let md = "| A | B |\n| --- | --- |\n| 1 | 2 |";
    let doc = parse_markdown_with_links(md, &default_theme(), 80);
    assert!(doc.links.is_empty(), "无链接表格不应产生命中区");
}
