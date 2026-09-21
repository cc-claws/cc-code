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

/// 复现：表格含尾部空列时，旧算法「最后一列取剩余」让空列吞掉
/// 整数截断余量，挤压真正有内容的列。
///
/// 空列（ideal=0）不应分到任何剩余宽度。
#[test]
fn parse_table_trailing_empty_column_not_hoarding() {
    // 3 个内容列（ideal 均 > min=10，触发按比例分配剩余空间）+ 1 个尾部空列
    // max_width=45 → available=32，min_sum=30，remaining=2
    // 旧算法截断余数滚给空列；新算法累积分配给 extra 最大的列
    let md = "| 长标题列内容示例 | 中等内容列xx | 短内容x |  |\n\
              | --- | --- | --- | --- |\n\
              | 长标题列内容示例数据更长一些些 | 中等内容列xx数据 | 短内容x |  |";
    let text = parse_markdown(md, &default_theme(), 45);
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
    // 段内 fill 数 = 列宽 + 2（左右各一个空格位）；旧算法会吞掉截断余量使空列变宽
    let last_col_fill = segments[3].chars().filter(|c| *c == '─').count();
    assert_eq!(
        last_col_fill, 2,
        "尾部空列宽度应为 0（fill=2），边框: {border}"
    );
}

/// 多内容列按 extras 累积比例分配剩余宽度，整数除法不把余数
/// 滚给最后一列——extra 最大的列应拿到完整份额。
#[test]
fn parse_table_extra_share_not_truncated_to_last() {
    // 列宽（ideal/min/extra）: col1=20/10/10, col2=15/10/5, col3=12/10/2, col4空=0/0/0
    // max_width=45 → available=32, min_sum=30, remaining=2
    // 新算法: col3 应拿到 2*17/17 - 2*15/17 = 1 → 宽 11（旧算法 col3 只有 10，余数给空列）
    let md = "| AaaaaaaaaaBbbbbbbbbb | CccccccccDdddd | EeeeeeeFfff |  |\n\
              | --- | --- | --- | --- |\n\
              | AaaaaaaaaaBbbbbbbbbbX | CccccccccDddddY | EeeeeeeFfffZ |  |";
    let text = parse_markdown(md, &default_theme(), 45);
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
    assert_eq!(
        widths,
        vec![11, 10, 11, 0],
        "剩余空间应按累积比例分给内容列，空列为 0，边框: {border}"
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
                line.spans.iter().map(|s| s.content.as_ref()).collect::<String>()
            );
        }
    }
}

/// Demo 1：无序列表长内容折行后续行应与首行文字左边缘对齐（悬挂缩进）
#[test]
fn test_unordered_list_hanging_indent() {
    // bullet "• " 占 2 列（• U+2022 宽度 1 + 空格 1）
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
            .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect::<String>())
            .collect::<Vec<_>>()
    );

    // 首行应有 bullet "•"
    let first_line: String = text.lines[0]
        .spans
        .iter()
        .map(|s| s.content.as_ref())
        .collect();
    assert!(
        first_line.contains('•'),
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
    // 嵌套列表：父项 "• " 2列，子项 "  • " 4列
    let md = "- 父项\n  - 子项内容非常长需要折行的文字这里继续写更多内容以确保超出宽度";
    let text = parse_markdown(md, &default_theme(), 30);

    // 找到子项的行（以 "  •" 开头的行）
    let mut child_first_line_idx = None;
    for (i, line) in text.lines.iter().enumerate() {
        let line_str: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        if line_str.contains("  •") && line_str.contains("子项") {
            child_first_line_idx = Some(i);
            break;
        }
    }
    let child_idx = child_first_line_idx.expect("应有子项行");

    // 子项续行的缩进应 ≥ 4 列（"  " 层级缩进 + "• " bullet 宽度）
    if child_idx + 1 < text.lines.len() {
        let next_line: String = text.lines[child_idx + 1]
            .spans
            .iter()
            .map(|s| s.content.as_ref())
            .collect();
        if !next_line.is_empty() && !next_line.contains('•') {
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

    // 所有非空行都应有引用前缀 "▍"
    for (i, line) in text.lines.iter().enumerate() {
        let line_str: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        if line_str.is_empty() {
            continue;
        }
        assert!(
            line_str.contains('▍'),
            "引用块第 {} 行应包含引用前缀 '▍'，实际: {:?}",
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
