use once_cell::sync::Lazy;
use ratatui::{
    style::{Color, Style},
    text::{Line, Span},
};
use syntect::{easy::HighlightLines, highlighting::ThemeSet, parsing::SyntaxSet};

pub static SYNTAX_SET: Lazy<SyntaxSet> = Lazy::new(two_face::syntax::extra_newlines);
pub static THEME_SET: Lazy<ThemeSet> = Lazy::new(load_theme_set);

/// 内嵌的 Monokai Extended 主题（dark / light），对齐 CC 的 diff 配色。
/// 来源：https://github.com/jonschlinkert/sublime-monokai-extended（bat 同款）
const MONOKAI_EXTENDED_DARK: &str = include_str!("themes/monokai-extended.tmTheme");
const MONOKAI_EXTENDED_LIGHT: &str = include_str!("themes/monokai-extended-light.tmTheme");

/// 加载 syntect 默认主题集 + 内嵌的 Monokai Extended 主题 + CC 用于 light/ansi 的主题
fn load_theme_set() -> ThemeSet {
    let mut set = ThemeSet::load_defaults();
    let dark = syntect::highlighting::ThemeSet::load_from_reader(
        &mut std::io::Cursor::new(MONOKAI_EXTENDED_DARK.as_bytes()),
    )
    .ok();
    let light = syntect::highlighting::ThemeSet::load_from_reader(
        &mut std::io::Cursor::new(MONOKAI_EXTENDED_LIGHT.as_bytes()),
    )
    .ok();
    if let Some(theme) = dark {
        set.themes.insert("Monokai Extended".to_string(), theme);
    }
    if let Some(theme) = light {
        set.themes
            .insert("Monokai Extended Light".to_string(), theme);
    }
    // 对齐 CC `defaultSyntaxThemeName`：light 系列用 GitHub、ansi 系列用 ansi。
    // 两份主题取自 bat 主题集（与语法集同源），不额外引入资源文件。
    let extra = two_face::theme::extra();
    for name in [
        two_face::theme::EmbeddedThemeName::Github,
        two_face::theme::EmbeddedThemeName::Ansi,
    ] {
        set.themes
            .insert(name.as_name().to_string(), extra.get(name).clone());
    }
    set
}

/// 对多行代码块进行语法高亮，返回着色后的 Line 列表。
/// 当语言标签未识别时返回 None，调用方应回退到统一颜色渲染。
pub fn highlight_code_block(
    lang: &str,
    lines: &[String],
    theme_name: &str,
) -> Option<Vec<Line<'static>>> {
    let ss = &*SYNTAX_SET;
    let syntax = ss.find_syntax_by_token(lang)?;
    let theme = THEME_SET
        .themes
        .get(theme_name)
        .unwrap_or(&THEME_SET.themes["base16-ocean.dark"]);
    let mut highlighter = HighlightLines::new(syntax, theme);

    let mut result = Vec::with_capacity(lines.len());
    for line_text in lines {
        let mut spans = Vec::new();

        let ranges = highlighter.highlight_line(line_text, ss).ok()?;
        for (style, text) in ranges {
            let color = Color::Rgb(style.foreground.r, style.foreground.g, style.foreground.b);
            spans.push(Span::styled(text.to_string(), Style::default().fg(color)));
        }
        result.push(Line::from(spans));
    }
    Some(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    include!("highlight_test.rs");
}
