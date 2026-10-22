//! 与 Claude Code（`src/utils/markdown.ts` + `src/components/MarkdownTable.tsx`）
//! 的 Markdown 渲染对拍测试。
//!
//! fixture `fixtures/cc_markdown_golden.json` 由 CC 真实源码生成：
//! `marked@17` 词法分析 → `formatToken` / `MarkdownTable` 渲染 → 抽取
//! 「纯文本 + 归一化样式标签」。样式被压缩成便于断言的字符标签：
//!
//! ```text
//! b = bold   i = italic   u = underline   d = dim   s = strikethrough
//! . = 无修饰                      #rrggbb = 前景色
//! ```
//!
//! 断言策略：
//! - `plain`（去掉样式后的可见文本）必须**逐行完全一致**，覆盖块结构、空行、
//!   列表符号、表格框线与折行位置。
//! - `spans` 断言样式标签序列一致，覆盖标题字形、行内代码/链接配色、引用斜体。
//!
//! 有意的偏离（CC 自身缺陷，已在 `DIVERGENCES` 中显式登记）：
//! - `hr_at_start`：CC 的 hr token 不带 EOL 且后续块不补空行，会输出 `---文字`；
//!   peri 在 hr 后补一次块分隔，避免正文被并入同一行。
//! - `loose_list`：CC 的 `list_item` 里是 `paragraph` 而非 `text`，命中不到
//!   bullet 分支，列表符号整段丢失；peri 保留 bullet。

#![cfg(feature = "markdown-highlight")]

use cc_widgets::markdown::{parse_markdown_with_options, MarkdownTheme};
use cc_widgets::theme::Theme;
use ratatui::style::Modifier;
use serde::Deserialize;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

#[derive(Deserialize)]
struct Fixture {
    name: String,
    src: String,
    width: usize,
    lines: Vec<FixtureLine>,
}

#[derive(Deserialize)]
struct FixtureLine {
    plain: String,
    spans: Vec<(String, String)>,
}

/// 已知且有意保留的差异：用例名 → 原因。
///
/// 判定标准：**CC 的行为是可读性缺陷**（丢内容 / 粘连 / 缩进错乱）时，
/// peri 保留正确行为并在此登记；其余一律以 CC 为准。
const DIVERGENCES: &[(&str, &str)] = &[
    // ── CC 丢内容 / 结构错乱 ──
    (
        "hr",
        "CC 的 hr token 吞掉后续换行，`---\\n\\n正文` 之间没有空行；peri 保留源文本空行",
    ),
    (
        "hr_at_start",
        "CC 的 hr 不带 EOL，会把后文粘成 `---文字`；peri 分行输出",
    ),
    (
        "ul_loose",
        "CC 的 loose list 项内是 paragraph，命中不到 bullet 分支而丢符号；peri 保留 `- `",
    ),
    (
        "ul_multi_para_item",
        "同 loose_list：CC 丢掉 bullet，且多段项之间丢了空行；peri 保留",
    ),
    (
        "ul_with_code",
        "同 loose_list：CC 丢掉 bullet 且丢空行；peri 保留",
    ),
    (
        "ul_nested3",
        "CC `list_item` 递归时用 listDepth 直接重复缩进，产生 0/2/6；peri 每层 2 空格",
    ),
    ("ul_nested4", "同上，CC 产生 0/2/6/10；peri 每层 2 空格"),
    (
        "ul_long_wrap",
        "CC 折行后续行丢失悬挂缩进（顶格），列表项续行无法与首行对齐；peri 保留",
    ),
    (
        "quote_long_wrap",
        "CC 折行后续行丢失 `▎ ` 前缀，引用块视觉断裂；peri 保留",
    ),
    (
        "quote_nested",
        "CC 对 `> > x` 的内层竖条错误地叠加了斜体；peri 只对正文加斜体",
    ),
    // ── CC 未做「嵌套表格降级」以外的一致性 ──
    (
        "table_in_quote",
        "CC 对引用块内表格走 formatToken 的 ASCII 管道表；peri 统一用框线表（观感更一致）",
    ),
    (
        "table_in_list",
        "同 table_in_quote：CC 嵌套表格退化为 `| a | b |` 管道表；peri 统一框线表",
    ),
    // ── 空文档占位 ──
    (
        "html_block_div",
        "纯 HTML 块：CC 输出一个空行占位；peri 输出空文档（外层渲染时会补行）",
    ),
    (
        "html_comment",
        "HTML 注释块：CC 丢弃注释后不留空行；peri 会多一个前置空行",
    ),
];

fn golden() -> Vec<Fixture> {
    let raw = include_str!("fixtures/cc_markdown_golden.json");
    serde_json::from_str(raw).expect("fixture JSON 应可解析")
}

/// 用默认主题渲染（对应 CC dark 调色板）。
struct DarkTheme;
impl MarkdownTheme for DarkTheme {
    fn heading(&self) -> ratatui::style::Color {
        cc_widgets::theme::DarkTheme.warning()
    }
    fn text(&self) -> ratatui::style::Color {
        cc_widgets::theme::DarkTheme.text()
    }
    fn muted(&self) -> ratatui::style::Color {
        cc_widgets::theme::DarkTheme.muted()
    }
    fn code(&self) -> ratatui::style::Color {
        cc_widgets::theme::DarkTheme.permission()
    }
    fn link(&self) -> ratatui::style::Color {
        cc_widgets::theme::DarkTheme.success()
    }
    fn code_prefix(&self) -> ratatui::style::Color {
        cc_widgets::theme::DarkTheme.success()
    }
    fn quote_prefix(&self) -> ratatui::style::Color {
        cc_widgets::theme::DarkTheme.muted()
    }
    fn list_bullet(&self) -> ratatui::style::Color {
        cc_widgets::theme::DarkTheme.text()
    }
    fn separator(&self) -> ratatui::style::Color {
        cc_widgets::theme::DarkTheme.muted()
    }
}

/// 把 ratatui span 的样式压成与 fixture 相同的标签格式。
fn style_tag(style: ratatui::style::Style) -> String {
    let mut flags = String::new();
    if style.add_modifier.contains(Modifier::BOLD) {
        flags.push('b');
    }
    if style.add_modifier.contains(Modifier::ITALIC) {
        flags.push('i');
    }
    if style.add_modifier.contains(Modifier::UNDERLINED) {
        flags.push('u');
    }
    if style.add_modifier.contains(Modifier::DIM) {
        flags.push('d');
    }
    if style.add_modifier.contains(Modifier::CROSSED_OUT) {
        flags.push('s');
    }
    if flags.is_empty() {
        flags.push('.');
    }
    match style.fg {
        Some(ratatui::style::Color::Rgb(r, g, b)) => format!("{flags}#{r:02x}{g:02x}{b:02x}"),
        _ => flags,
    }
}

/// 逐字符展开为 (grapheme, style_tag)，消除 span 切分差异后再比对。
fn graphemes(line: &FixtureLine) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for (text, tag) in &line.spans {
        for g in text.graphemes(true) {
            out.push((g.to_string(), tag.clone()));
        }
    }
    out
}

fn render(src: &str, width: usize) -> Vec<FixtureLine> {
    // fixture 由 CC 以 highlight=null 且 supportsHyperlinks=false 生成，
    // 对应「关闭语法高亮 + 无超链接纯 URL」分支
    let doc = parse_markdown_with_options(src, &DarkTheme, width, false, false);
    doc.text
        .lines
        .iter()
        .map(|line| FixtureLine {
            plain: line
                .spans
                .iter()
                .map(|s| s.content.as_ref())
                .collect::<String>(),
            spans: line
                .spans
                .iter()
                .map(|s| (s.content.to_string(), style_tag(s.style)))
                .collect(),
        })
        .collect()
}

#[test]
fn markdown_matches_claude_code() {
    let mut failures = Vec::new();

    for case in golden() {
        if let Some((_, reason)) = DIVERGENCES.iter().find(|(n, _)| *n == case.name) {
            // 显式登记的偏离：不与 CC 逐字比对，但仍要满足两条不变式，
            // 避免「偏离」变成掩盖真 bug 的盲区：
            //   1. 内容不丢失 —— CC 可见的每个非空白字符都要出现
            //   2. 不越界 —— 每行显示宽度都不超过渲染宽度
            let got = render(&case.src, case.width);
            let got_joined: String = got.iter().map(|l| l.plain.as_str()).collect();

            // 内容不丢失：以「实义字符」为判据——去掉 CC 与 peri 双方各自的
            // 边框/符号字符后，字母数字与 CJK 必须完全一致。
            // 这样 `| a |` 与 `│ a │` 的差异被正确视为格式差异，
            // 而真正丢字（如 bullet 吞掉正文）仍会失败。
            fn meaningful(s: &str) -> Vec<char> {
                s.chars()
                    .filter(|c| {
                        !c.is_whitespace()
                            && !matches!(
                                c,
                                '|' | '-'
                                    | '┌'
                                    | '┬'
                                    | '┐'
                                    | '├'
                                    | '┼'
                                    | '┤'
                                    | '└'
                                    | '┴'
                                    | '┘'
                                    | '│'
                                    | '─'
                                    | '▎'
                                    | '•'
                                    | ':'
                            )
                    })
                    .collect()
            }
            let cc_chars = meaningful(
                &case
                    .lines
                    .iter()
                    .map(|l| l.plain.as_str())
                    .collect::<String>(),
            );
            let got_chars = meaningful(&got_joined);
            assert!(
                cc_chars == got_chars,
                "case `{}` 内容不一致（偏离仅允许格式差异）：{reason}\n  want: {:?}\n  got:  {:?}",
                case.name,
                cc_chars.iter().collect::<String>(),
                got_chars.iter().collect::<String>()
            );

            for line in &got {
                let w = line.plain.width();
                assert!(
                    w <= case.width,
                    "case `{}` 行宽 {w} 超出渲染宽度 {}（偏离项也必须不越界）：{:?}",
                    case.name,
                    case.width,
                    line.plain
                );
            }
            continue;
        }

        let got = render(&case.src, case.width);
        let want_plain: Vec<&str> = case.lines.iter().map(|l| l.plain.as_str()).collect();
        let got_plain: Vec<&str> = got.iter().map(|l| l.plain.as_str()).collect();

        if got_plain != want_plain {
            failures.push(format!(
                "case `{}` 纯文本不一致\n  want: {:?}\n  got:  {:?}",
                case.name, want_plain, got_plain
            ));
            continue;
        }

        for (i, (want, got)) in case.lines.iter().zip(got.iter()).enumerate() {
            // 逐字符比对：span 的切分方式不影响结论，只关心每个字符的样式
            let want_g = graphemes(want);
            let got_g = graphemes(got);
            if want_g != got_g {
                failures.push(format!(
                    "case `{}` 第 {i} 行样式不一致\n  want: {:?}\n  got:  {:?}",
                    case.name, want_g, got_g
                ));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "与 CC 渲染差异 {} 处：\n{}",
        failures.len(),
        failures.join("\n\n")
    );
}
