pub mod cache;
mod render_state;

#[cfg(feature = "markdown-highlight")]
pub mod highlight;

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use ratatui::{style::Color, text::Text};

use cache::MarkdownCache;
use render_state::RenderState;

// ── MarkdownTheme trait ──────────────────────────────────────

/// Markdown 渲染颜色主题——将 render_state.rs 中的硬编码颜色参数化
pub trait MarkdownTheme {
    /// 标题颜色（H1-H3，对应原 theme::WARNING）
    fn heading(&self) -> Color;
    /// 主文字颜色（列表前缀、代码内容，对应原 theme::TEXT）
    fn text(&self) -> Color;
    /// 弱化文字颜色（边框、分隔线、代码标签，对应原 theme::MUTED）
    fn muted(&self) -> Color;
    /// 行内代码颜色（对应原 theme::WARNING，与 heading 共用）
    fn code(&self) -> Color;
    /// 链接颜色（对应原 theme::SAGE）
    fn link(&self) -> Color;
    /// 代码块行前缀颜色（`│`，对应原 theme::SAGE）
    fn code_prefix(&self) -> Color;
    /// 引用块前缀颜色（`▍`，对应原 theme::MUTED）
    fn quote_prefix(&self) -> Color;
    /// 列表项目符号颜色（`•`，对应原 theme::TEXT）
    fn list_bullet(&self) -> Color;
    /// 水平线颜色（`─`，对应原 theme::MUTED）
    fn separator(&self) -> Color;
    /// 代码块语法高亮的 syntect 主题名（随 UI 主题明暗族切换）
    fn syntax_theme_name(&self) -> &'static str {
        "base16-ocean.dark"
    }
}

/// 将 `Theme` trait 适配为 `MarkdownTheme`
///
/// 语义映射：
/// - heading → warning（标题用警告色，增强可读性）
/// - code → thinking（行内代码用思考色，蓝紫调）
/// - link → success（链接用成功色，绿色标识可点击）
/// - code_prefix → success
/// - quote_prefix → muted
/// - list_bullet → text
/// - separator → muted
pub struct ThemeMarkdownAdapter<'a>(pub &'a dyn crate::theme::Theme);

impl MarkdownTheme for ThemeMarkdownAdapter<'_> {
    fn heading(&self) -> Color {
        self.0.warning()
    }
    fn text(&self) -> Color {
        self.0.text()
    }
    fn muted(&self) -> Color {
        self.0.muted()
    }
    fn code(&self) -> Color {
        self.0.thinking()
    }
    fn link(&self) -> Color {
        self.0.success()
    }
    fn code_prefix(&self) -> Color {
        self.0.success()
    }
    fn quote_prefix(&self) -> Color {
        self.0.muted()
    }
    fn list_bullet(&self) -> Color {
        self.0.text()
    }
    fn separator(&self) -> Color {
        self.0.muted()
    }
    fn syntax_theme_name(&self) -> &'static str {
        self.0.syntax_theme_name()
    }
}

/// 默认 Markdown 主题——色值与 DarkTheme 一致
#[derive(Debug, Clone)]
pub struct DefaultMarkdownTheme;

impl MarkdownTheme for DefaultMarkdownTheme {
    fn heading(&self) -> Color {
        Color::Rgb(255, 193, 7)
    } // WARNING #FFC107
    fn text(&self) -> Color {
        Color::Rgb(255, 255, 255)
    } // TEXT #FFFFFF
    fn muted(&self) -> Color {
        Color::Rgb(153, 153, 153)
    } // MUTED #999999
    fn code(&self) -> Color {
        Color::Rgb(162, 169, 228)
    } // THINKING #A2A9E4（蓝紫色）
    fn link(&self) -> Color {
        Color::Rgb(78, 186, 101)
    } // SAGE #4EBA65
    fn code_prefix(&self) -> Color {
        Color::Rgb(78, 186, 101)
    } // SAGE #4EBA65
    fn quote_prefix(&self) -> Color {
        Color::Rgb(153, 153, 153)
    } // MUTED #999999
    fn list_bullet(&self) -> Color {
        Color::Rgb(255, 255, 255)
    } // TEXT #FFFFFF
    fn separator(&self) -> Color {
        Color::Rgb(153, 153, 153)
    } // MUTED #999999
}

/// Markdown 超链接命中区（grapheme 索引，与 TUI `visual_to_logical` 对齐）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkHit {
    /// 输出 `Text.lines` 中的行下标
    pub line: usize,
    /// 该行 plain_text 中的 grapheme 起点（含）
    pub g_start: usize,
    /// 该行 plain_text 中的 grapheme 终点（不含）
    pub g_end: usize,
    pub url: String,
}

/// Markdown 解析产物：渲染文本 + 链接命中区（两者行下标对齐）
#[derive(Debug, Clone)]
pub struct MarkdownDoc {
    pub text: Text<'static>,
    pub links: Vec<LinkHit>,
}

impl From<Text<'static>> for MarkdownDoc {
    fn from(text: Text<'static>) -> Self {
        Self {
            text,
            links: Vec::new(),
        }
    }
}

/// 解析 markdown 文本为 ratatui Text（带 LRU 缓存）
///
/// 缓存 key = (content_hash, max_width)，全局单例共享。
/// 命中时直接返回克隆的 Text<'static>，跳过完整解析。
/// 主题指纹：把 markdown 可见的全部颜色 + 语法主题名哈希成一个 u64，
/// 作为渲染缓存 key 的一部分——主题切换后指纹变化，旧配色缓存自然失效。
fn theme_fingerprint(theme: &dyn MarkdownTheme) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    theme.heading().hash(&mut h);
    theme.text().hash(&mut h);
    theme.muted().hash(&mut h);
    theme.code().hash(&mut h);
    theme.link().hash(&mut h);
    theme.code_prefix().hash(&mut h);
    theme.quote_prefix().hash(&mut h);
    theme.list_bullet().hash(&mut h);
    theme.separator().hash(&mut h);
    theme.syntax_theme_name().hash(&mut h);
    h.finish()
}

pub fn parse_markdown(input: &str, theme: &dyn MarkdownTheme, max_width: usize) -> Text<'static> {
    parse_markdown_with_links(input, theme, max_width).text
}

/// 判断两个块之间在源文本里是否有空行。
///
/// 从上一个块的起点扫到当前块起点，跳过当前块的起始行前缀后，从尾部向前看：
/// 遇到空行即有，遇到非空行即无。跳过 `>`（引用块前缀）与空白，因此
/// `> 第一段\n>\n> 第二段` 会被正确识别为有空行。
///
/// 不用两块 range 的间隙判断：pulldown-cmark 的块 range 会吞掉块尾的部分
/// 换行（`## T\n\n正文` 的间隙只有 1 个换行），不可靠。
fn has_blank_line_between(prev_top_start: Option<usize>, current_start: usize, input: &str) -> bool {
    let Some(prev) = prev_top_start else {
        return false;
    };
    if current_start <= prev || current_start > input.len() {
        return false;
    }
    let mut lines = input[prev..current_start].rsplit('\n');
    // 第一项是当前块的起始行前缀（或行终止符后的空串），不属于块间间隔
    lines.next();
    match lines.next() {
        Some(line) => line.trim().trim_start_matches('>').trim().is_empty(),
        None => false,
    }
}

/// 块间空行判定状态机。
///
/// 每个块级事件到来时调用 [`take`](Self::take)（水平线用 [`take_rule`](Self::take_rule)），
/// 返回「该块前是否要留空行」。规则集中在这里，新增块类型只需在解析循环里
/// 调一次，不必再复制 `last_start` / `last_was_rule` 的状态维护。
struct BlockGap {
    /// 上一个块的起始字节偏移
    last_start: Option<usize>,
    /// 上一个块是否为水平线：CC 的 hr tokenizer 吞掉后续换行，hr 之后恒无空行
    last_was_rule: bool,
}

impl BlockGap {
    fn new() -> Self {
        Self {
            last_start: None,
            last_was_rule: false,
        }
    }

    /// 记录一个普通块的起点，返回它与上一块之间是否有源文本空行
    fn take(&mut self, start: usize, input: &str) -> bool {
        let blank = !self.last_was_rule && has_blank_line_between(self.last_start, start, input);
        self.last_start = Some(start);
        self.last_was_rule = false;
        blank
    }

    /// 记录一个水平线块。hr 前按源文本留空，且不抑制自身前置判定
    fn take_rule(&mut self, start: usize, input: &str) -> bool {
        let blank = has_blank_line_between(self.last_start, start, input);
        self.last_start = Some(start);
        self.last_was_rule = true;
        blank
    }
}

/// 解析 markdown 文本，同时保留超链接命中区（供点击打开浏览器）
pub fn parse_markdown_with_links(
    input: &str,
    theme: &dyn MarkdownTheme,
    max_width: usize,
) -> MarkdownDoc {
    if input.is_empty() {
        return MarkdownDoc {
            text: Text::raw(""),
            links: Vec::new(),
        };
    }

    // 检查缓存（key 含主题指纹：切换主题后旧配色缓存自动失效）
    let width_u16 = max_width.min(u16::MAX as usize) as u16;
    let cache = MarkdownCache::global();
    if let Some(cached) = cache.get(input, width_u16, theme_fingerprint(theme)) {
        return cached;
    }

    // 缓存未命中，执行完整解析
    let options = Options::all() - Options::ENABLE_SMART_PUNCTUATION;
    let parser = Parser::new_ext(input, options).into_offset_iter();
    let mut state = RenderState::new(theme).with_max_width(max_width);
    // 列表嵌套深度：列表项内部的块不参与「块间空行」判定（对齐 CC 的 loose list）
    let mut list_depth = 0usize;
    let mut gap = BlockGap::new();
    for (event, range) in parser {
        match &event {
            Event::Rule => {
                state.set_pending_blank(gap.take_rule(range.start, input));
            }
            Event::Start(Tag::List(_)) => {
                if list_depth == 0 {
                    state.set_pending_blank(gap.take(range.start, input));
                }
                list_depth += 1;
            }
            Event::End(TagEnd::List(_)) => {
                list_depth = list_depth.saturating_sub(1);
            }
            Event::Start(Tag::BlockQuote(_)) => {
                state.set_pending_blank(gap.take(range.start, input));
            }
            Event::Start(Tag::Paragraph)
            | Event::Start(Tag::Heading { .. })
            | Event::Start(Tag::CodeBlock(_))
            | Event::Start(Tag::Table(_))
                if list_depth == 0 =>
            {
                state.set_pending_blank(gap.take(range.start, input));
            }
            _ => {}
        }
        state.handle_event(event);
    }
    if !state.current_spans.is_empty() {
        state.flush_line();
    }
    // 裁剪尾部空行，避免最后一个块级元素后多余留白
    while state.lines.last().is_some_and(|l| l.spans.is_empty()) {
        state.lines.pop();
    }
    let result = MarkdownDoc {
        text: Text::from(state.lines),
        links: state.links,
    };

    // 写入缓存
    cache.put(input, width_u16, theme_fingerprint(theme), result.clone());

    result
}

#[cfg(test)]
#[path = "mod_test.rs"]
mod tests;

#[cfg(test)]
#[path = "cache_test.rs"]
mod cache_tests;
