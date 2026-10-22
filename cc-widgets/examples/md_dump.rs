//! 把 markdown 渲染结果打印到终端，用于与 Claude Code 并排目视对比。
//!
//! 用法（在仓库根目录）：
//! ```text
//! cargo run -p cc-widgets --features markdown-highlight --example md_dump -- <文件> [宽度] [--ansi]
//! ```
//!
//! - 默认把每个 `Text` 行拼成纯文本打印
//! - `--ansi` 额外输出 SGR 转义，便于直接看颜色/字形
//! - 通过 `CC_MD_WIDTH` 之外的一切都走 `cc_widgets::theme::DarkTheme`（对应 CC dark）

use cc_widgets::markdown::{parse_markdown_with_options, MarkdownTheme};
use cc_widgets::theme::{DarkTheme, Theme};
use ratatui::style::{Color, Modifier};

/// 与 CC dark 主题对齐的 markdown 调色板。
struct CcDark;
impl MarkdownTheme for CcDark {
    fn heading(&self) -> Color {
        // CC 标题无颜色，这里返回 Reset 表示「继承终端默认色」
        Color::Reset
    }
    fn text(&self) -> Color {
        DarkTheme.text()
    }
    fn muted(&self) -> Color {
        DarkTheme.muted()
    }
    fn code(&self) -> Color {
        // CC `permission`
        Color::Rgb(177, 185, 249)
    }
    fn link(&self) -> Color {
        DarkTheme.success()
    }
    fn code_prefix(&self) -> Color {
        DarkTheme.success()
    }
    fn quote_prefix(&self) -> Color {
        DarkTheme.muted()
    }
    fn list_bullet(&self) -> Color {
        DarkTheme.text()
    }
    fn separator(&self) -> Color {
        DarkTheme.muted()
    }
}

fn sgr(style: ratatui::style::Style) -> String {
    let mut codes: Vec<String> = Vec::new();
    let m = style.add_modifier;
    if m.contains(Modifier::BOLD) {
        codes.push("1".into());
    }
    if m.contains(Modifier::DIM) {
        codes.push("2".into());
    }
    if m.contains(Modifier::ITALIC) {
        codes.push("3".into());
    }
    if m.contains(Modifier::UNDERLINED) {
        codes.push("4".into());
    }
    if m.contains(Modifier::CROSSED_OUT) {
        codes.push("9".into());
    }
    if let Some(Color::Rgb(r, g, b)) = style.fg {
        codes.push(format!("38;2;{r};{g};{b}"));
    }
    if codes.is_empty() {
        String::new()
    } else {
        format!("\x1b[{}m", codes.join(";"))
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().unwrap_or_else(|| {
        eprintln!("用法: md_dump <markdown文件> [宽度] [--ansi]");
        std::process::exit(2);
    });
    let mut width = 80usize;
    let mut use_ansi = false;
    for a in args {
        if a == "--ansi" {
            use_ansi = true;
        } else if let Ok(w) = a.parse::<usize>() {
            width = w;
        }
    }

    let src = if path == "-" {
        // 从标准输入读（跨平台；Windows 上没有 /dev/stdin）
        let mut buf = String::new();
        if let Err(e) = std::io::Read::read_to_string(&mut std::io::stdin(), &mut buf) {
            eprintln!("读取标准输入失败: {e}");
            std::process::exit(1);
        }
        buf
    } else {
        std::fs::read_to_string(&path).unwrap_or_else(|e| {
            eprintln!("读取 {path} 失败: {e}");
            std::process::exit(1);
        })
    };

    // 关闭语法高亮 + 关闭 OSC8：与 CC 在「无 cli-highlight、无超链接终端」下一致，
    // 也是 cc_markdown_parity 对拍测试使用的同一配置。
    let doc = parse_markdown_with_options(&src, &CcDark, width, false, false);

    for line in &doc.text.lines {
        if use_ansi {
            let mut out = String::new();
            for span in &line.spans {
                out.push_str(&sgr(span.style));
                out.push_str(span.content.as_ref());
                if !sgr(span.style).is_empty() {
                    out.push_str("\x1b[0m");
                }
            }
            println!("{out}");
        } else {
            let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
            println!("{text}");
        }
    }
}
