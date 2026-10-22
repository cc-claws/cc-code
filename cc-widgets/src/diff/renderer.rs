use std::{
    hash::{Hash, Hasher},
    sync::Mutex,
};

use once_cell::sync::Lazy;
use ratatui::{
    style::{Color, Style},
    text::{Line, Span},
};

use crate::byte_cache::{estimate_lines_bytes, ByteBudgetCache};
use crate::theme::Theme;

use super::{compute_diff, CacheStats, DiffInput, DiffLine, DiffWordType};

#[cfg(feature = "markdown-highlight")]
use crate::markdown::highlight::{SYNTAX_SET, THEME_SET};

/// 渲染缓存条数上限
const RENDER_CACHE_CAPACITY: usize = 64;

/// 渲染缓存字节预算：8 MB（在条数上限之外追加，防单条大 diff 撑爆）
const RENDER_CACHE_BUDGET_BYTES: usize = 8 * 1024 * 1024;

/// 全局渲染结果缓存（单例，条数 + 字节双重上限）
static RENDER_CACHE: Lazy<Mutex<ByteBudgetCache<RenderCacheKey, Vec<Line<'static>>>>> =
    Lazy::new(|| {
        Mutex::new(ByteBudgetCache::new(
            RENDER_CACHE_CAPACITY,
            RENDER_CACHE_BUDGET_BYTES,
            |lines: &Vec<Line<'static>>| estimate_lines_bytes(lines),
        ))
    });

/// 渲染缓存统计快照（供 `/gc` 诊断）：条数 / 容量 / 估算字节 / 预算。
pub fn render_cache_stats() -> CacheStats {
    RENDER_CACHE
        .lock()
        .map(|c| c.stats())
        .unwrap_or(CacheStats {
            entries: 0,
            capacity: RENDER_CACHE_CAPACITY,
            bytes: 0,
            budget: RENDER_CACHE_BUDGET_BYTES,
        })
}

/// 渲染缓存 key
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
struct RenderCacheKey {
    old_hash: u64,
    new_hash: u64,
    flags: u8,
    width: usize,
    /// 语法高亮主题指纹（名称哈希），主题切换时缓存失效
    syntax_theme_hash: u64,
    /// 语言指纹（由 file_path 扩展名推导），不同扩展名的相同内容不得共用缓存
    lang_hash: u64,
}

impl RenderCacheKey {
    fn from_input(input: &DiffInput, width: usize, theme: &dyn Theme) -> Self {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        input.old_content.hash(&mut hasher);
        let old_hash = hasher.finish();

        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        input.new_content.hash(&mut hasher);
        let new_hash = hasher.finish();

        let mut flags = 0u8;
        if input.is_new_file {
            flags |= 1;
        }
        if input.is_deleted_file {
            flags |= 2;
        }
        if input.is_binary {
            flags |= 4;
        }

        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        theme.syntax_theme_name().hash(&mut hasher);
        let syntax_theme_hash = hasher.finish();

        // 渲染结果中的语法着色完全由 file_path 推导的语言决定，
        // 内容哈希相同的两个不同扩展名文件必须区分缓存。
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        #[cfg(feature = "markdown-highlight")]
        detect_language(&input.file_path, first_line_of(input)).hash(&mut hasher);
        #[cfg(not(feature = "markdown-highlight"))]
        input.file_path.hash(&mut hasher);
        let lang_hash = hasher.finish();

        RenderCacheKey {
            old_hash,
            new_hash,
            flags,
            width,
            syntax_theme_hash,
            lang_hash,
        }
    }
}

/// 返回 `s` 在 `max_width` 显示列内能放下的字节长度（CJK 安全）。
///
/// 只扫描到放不下为止，是 O(min(len, max_width))——折行时每段都调用它，
/// 必须避免在剩余全串上做宽度统计（那会让折行退化为 O(n²)）。
fn take_width_len(s: &str, max_width: usize) -> usize {
    if max_width == 0 {
        return 0;
    }
    let mut width = 0usize;
    let mut end = 0usize;
    for (i, ch) in s.char_indices() {
        let cw = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
        if width + cw > max_width {
            break;
        }
        width += cw;
        end = i + ch.len_utf8();
    }
    end
}

/// 计算 max_line_number 的十进制位数（至少 1）
fn digit_count(max_num: u32) -> usize {
    if max_num == 0 {
        return 1;
    }
    let mut n = max_num;
    let mut digits = 0;
    while n > 0 {
        n /= 10;
        digits += 1;
    }
    digits
}

/// 主渲染入口（带缓存）
pub fn render_diff_impl(input: &DiffInput, width: usize, theme: &dyn Theme) -> Vec<Line<'static>> {
    let key = RenderCacheKey::from_input(input, width, theme);

    // 查缓存
    if let Ok(mut cache) = RENDER_CACHE.lock() {
        if let Some(cached) = cache.get(&key) {
            return cached.clone();
        }
    }

    // 未命中，执行渲染
    let lines = render_diff_uncached(input, width, theme);

    // 写入缓存
    if let Ok(mut cache) = RENDER_CACHE.lock() {
        cache.put(key, lines.clone());
    }

    lines
}

/// CC `FILENAME_LANGS`：按文件名或「首个点号前的 stem」识别语言
/// （对齐 `color-diff-napi/src/index.ts:415`）。
#[cfg(feature = "markdown-highlight")]
const FILENAME_LANGS: &[(&str, &str)] = &[
    ("Dockerfile", "dockerfile"),
    ("Makefile", "makefile"),
    ("Rakefile", "ruby"),
    ("Gemfile", "ruby"),
    ("CMakeLists", "cmake"),
];

/// CC 注册语言的扩展名别名 → syntect token。
///
/// 对照 `packages/color-diff-napi/src/__tests__/language-registration.test.ts`
/// 中的 26 种语言：CC 用 `hljs.getLanguage(ext)` 判断扩展名是否受支持，这里用
/// 显式白名单达到同样效果。映射到 syntect token 时修正歧义项
/// （`h` → `c`，否则会命中 Objective-C）。
///
/// 其中 dockerfile / cmake / kotlin / graphql / typescript 在 syntect 默认语法集里
/// 没有对应语法，`find_syntax_by_token` 返回 None，调用方回退纯文本。
#[cfg(feature = "markdown-highlight")]
const EXTENSION_LANGS: &[(&str, &str)] = &[
    // bash / shell
    ("sh", "sh"),
    ("bash", "bash"),
    ("zsh", "bash"),
    // c / cpp / csharp
    ("c", "c"),
    ("h", "c"),
    ("cc", "cpp"),
    ("cpp", "cpp"),
    ("cxx", "cpp"),
    ("hpp", "cpp"),
    ("hh", "cpp"),
    ("hxx", "cpp"),
    ("cs", "cs"),
    // cmake
    ("cmake", "cmake"),
    // css
    ("css", "css"),
    // diff
    ("diff", "diff"),
    ("patch", "diff"),
    // dockerfile
    ("dockerfile", "dockerfile"),
    // go
    ("go", "go"),
    // graphql
    ("graphql", "graphql"),
    ("gql", "graphql"),
    // java
    ("java", "java"),
    // javascript
    ("js", "js"),
    ("jsx", "js"),
    ("mjs", "js"),
    ("cjs", "js"),
    // json
    ("json", "json"),
    ("jsonc", "json"),
    // kotlin
    ("kt", "kt"),
    ("kts", "kt"),
    // makefile
    ("makefile", "makefile"),
    ("mk", "makefile"),
    ("mak", "makefile"),
    // markdown 不做语法高亮：CC 的 hljs markdown 只产出 section/strong scope，
    // 其主题 scope 表没有对应颜色，输出与纯文本完全一致；这里直接按未识别语言回退。
    // perl
    ("pl", "pl"),
    ("pm", "pl"),
    // php
    ("php", "php"),
    // python
    ("py", "py"),
    // ruby
    ("rb", "rb"),
    // rust
    ("rs", "rs"),
    // sql
    ("sql", "sql"),
    // typescript
    ("ts", "ts"),
    ("tsx", "tsx"),
    // xml
    ("xml", "xml"),
    ("html", "html"),
    ("xhtml", "html"),
    ("svg", "xml"),
    // yaml
    ("yaml", "yaml"),
    ("yml", "yml"),
];

/// 对齐 CC `detectLanguage(filePath, firstLine)`（`color-diff-napi/src/index.ts:423`）：
/// 文件名表 → 扩展名白名单 → shebang / `<?php` / `<?xml` 首行检测。
///
/// 未命中返回 None，调用方回退纯文本。与 CC 的唯一差别是语法集：
/// syntect 默认集缺少部分 CC 支持的语言，对应文件会回退纯文本。
#[cfg(feature = "markdown-highlight")]
fn detect_language(file_path: &str, first_line: Option<&str>) -> Option<String> {
    let path = std::path::Path::new(file_path);
    let base = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let stem = base.split('.').next().unwrap_or(base);
    if let Some((_, lang)) = FILENAME_LANGS
        .iter()
        .find(|(name, _)| base == *name || stem == *name)
    {
        return Some((*lang).to_string());
    }

    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        let ext = ext.to_ascii_lowercase();
        if let Some((_, token)) = EXTENSION_LANGS.iter().find(|(alias, _)| *alias == ext) {
            return Some((*token).to_string());
        }
    }

    // 去掉 UTF-8 BOM 后按首行识别
    let line = first_line.map(|l| l.strip_prefix('\u{feff}').unwrap_or(l))?;
    if let Some(rest) = line.strip_prefix("#!") {
        if rest.contains("bash") || rest.contains("/sh") {
            return Some("bash".to_string());
        }
        if rest.contains("python") {
            return Some("python".to_string());
        }
        if rest.contains("node") {
            return Some("js".to_string());
        }
        if rest.contains("ruby") {
            return Some("ruby".to_string());
        }
        if rest.contains("perl") {
            return Some("perl".to_string());
        }
    }
    if line.starts_with("<?php") {
        return Some("php".to_string());
    }
    if line.starts_with("<?xml") {
        return Some("xml".to_string());
    }
    None
}

/// diff 输入的「文件首行」：新建/修改取新内容，纯删除取旧内容，供 shebang 检测。
#[cfg(feature = "markdown-highlight")]
fn first_line_of(input: &DiffInput) -> Option<&str> {
    let text = if input.new_content.is_empty() {
        &input.old_content
    } else {
        &input.new_content
    };
    text.lines().next()
}

/// 对单行代码做语法高亮，生成带 fg（syntect 语法色）+ bg（diff 行背景色）的 Span 列表。
///
/// - `line_text`：去缩进后的代码行（不含换行符）
/// - `bg`：diff 行的整行背景色（绿底/无底）
/// - `content_width`：内容区最大显示宽度
/// - `theme_name`：syntect 主题名
/// - `lang`：语言 token（如 "rs"）
///
/// 返回 None 时调用方应回退到纯文本渲染。
/// 单行着色 span（仅前景色，不含背景与宽度处理）。
///
/// 逐行无状态高亮，与 CC `color-diff` 对齐：`ColorDiff.render` 与 `ColorFile.render`
/// 共用同一个 `highlightLine`，按单行 code 独立高亮，不跨行维护 grammar 状态。
/// 因此块注释/多行字符串的第二行起在两侧都不会被识别为注释，口径保持一致。
#[cfg(feature = "markdown-highlight")]
fn highlight_line_spans(
    line_text: &str,
    theme_name: &str,
    lang: &str,
) -> Option<Vec<Span<'static>>> {
    let ss = &*SYNTAX_SET;
    let syntax = ss.find_syntax_by_token(lang)?;
    let theme = THEME_SET
        .themes
        .get(theme_name)
        .unwrap_or(&THEME_SET.themes["base16-ocean.dark"]);
    let mut highlighter = syntect::easy::HighlightLines::new(syntax, theme);
    // 追加 \n 让行注释等 pattern 正确终止
    let code = format!("{}\n", line_text);
    let ranges = highlighter.highlight_line(&code, ss).ok()?;

    let mut spans = Vec::new();
    for (style, text) in ranges {
        // 去掉 highlight_line 追加的尾部换行
        let text = text.trim_end_matches('\n');
        if text.is_empty() {
            continue;
        }
        let fg = Color::Rgb(style.foreground.r, style.foreground.g, style.foreground.b);
        spans.push(Span::styled(text.to_string(), Style::default().fg(fg)));
    }
    Some(spans)
}

/// 给语法高亮的每个 span 叠加整行背景色（不截断、不 padding，交给折行处理）
#[cfg(feature = "markdown-highlight")]
fn highlight_line_spans_on_bg(
    line_text: &str,
    bg: Color,
    theme_name: &str,
    lang: &str,
) -> Option<Vec<Span<'static>>> {
    Some(
        highlight_line_spans(line_text, theme_name, lang)?
            .into_iter()
            .map(|s| Span::styled(s.content, s.style.bg(bg)))
            .collect(),
    )
}

/// 按显示宽度折行（对齐 CC `wrapText`）：把一行的 span 序列切成多行。
///
/// 保证前进：单字符宽度超过 `width` 时强制取一个字符（会溢出，但避免死循环）。
/// 复杂度 O(n)：每段只测量到放不下为止，剩余部分用切片推进而非重新复制。
fn wrap_spans(spans: Vec<Span<'static>>, width: usize) -> Vec<Vec<Span<'static>>> {
    let width = width.max(1);
    let mut out: Vec<Vec<Span<'static>>> = Vec::new();
    let mut cur: Vec<Span<'static>> = Vec::new();
    let mut cur_w = 0usize;

    for span in spans {
        let style = span.style;
        let text = span.content.into_owned();
        let mut rest = text.as_str();
        while !rest.is_empty() {
            let remaining = width - cur_w;
            let take = take_width_len(rest, remaining);
            if take == rest.len() {
                // 剩余内容整体放得下
                cur.push(Span::styled(rest.to_string(), style));
                cur_w += unicode_width::UnicodeWidthStr::width(rest);
                break;
            }
            if take == 0 {
                if cur_w == 0 {
                    // 首个字符就放不下：强制取一个字符，保证前进
                    let first_len = rest.chars().next().map_or(rest.len(), |c| c.len_utf8());
                    let (first, tail) = rest.split_at(first_len);
                    cur.push(Span::styled(first.to_string(), style));
                    out.push(std::mem::take(&mut cur));
                    rest = tail;
                } else {
                    // 本行已无空间：换行后重试同一块
                    out.push(std::mem::take(&mut cur));
                }
                cur_w = 0;
                continue;
            }
            let (head, tail) = rest.split_at(take);
            cur.push(Span::styled(head.to_string(), style));
            out.push(std::mem::take(&mut cur));
            cur_w = 0;
            rest = tail;
        }
    }
    // 空输入仍需产出一行（空行），但强制换行后不得留下尾部空白段
    if !cur.is_empty() || out.is_empty() {
        out.push(cur);
    }
    out
}

/// diff 行的 gutter 布局：首行显示行号，续行用 marker + 空格占位保持列对齐
struct GutterLayout {
    first: String,
    continuation: String,
    style: Style,
}

impl GutterLayout {
    /// `n` 为行号最大位数；续行宽度与首行一致（CC `addLineNumber` 的 i > 0 分支）
    fn new(first: String, marker: &str, n: usize, style: Style) -> Self {
        Self {
            first,
            continuation: format!("{}{}", marker, " ".repeat(n + 2)),
            style,
        }
    }
}

/// 把一条 diff 行按显示宽度折行后推入 `lines`（对齐 CC `wrapText` + `addLineNumber`）。
///
/// - 首段用完整 gutter，续段的 gutter 位置用 marker + 空格占位，保持列对齐
/// - `pad_bg` 为 `Some(bg)`（改动行）时每段 padding 到 `content_width`，
///   让底色延伸到右边；上下文行不 padding，与 CC 一致
fn push_wrapped_line(
    lines: &mut Vec<Line<'static>>,
    gutter: &GutterLayout,
    spans: Vec<Span<'static>>,
    content_width: usize,
    pad_bg: Option<Color>,
) {
    for (i, mut seg) in wrap_spans(spans, content_width).into_iter().enumerate() {
        let gutter_text = if i == 0 {
            gutter.first.clone()
        } else {
            gutter.continuation.clone()
        };
        if let Some(bg) = pad_bg {
            let used: usize = seg
                .iter()
                .map(|s| unicode_width::UnicodeWidthStr::width(s.content.as_ref()))
                .sum();
            if used < content_width {
                seg.push(Span::styled(
                    " ".repeat(content_width - used),
                    Style::default().bg(bg),
                ));
            }
        }
        let mut line_spans = Vec::with_capacity(seg.len() + 1);
        line_spans.push(Span::styled(gutter_text, gutter.style));
        line_spans.extend(seg);
        lines.push(Line::from(line_spans));
    }
}

/// 实际渲染逻辑（无缓存）
fn render_diff_uncached(input: &DiffInput, width: usize, theme: &dyn Theme) -> Vec<Line<'static>> {
    let result = compute_diff(input);

    // 特殊情况：二进制文件
    if result.is_binary {
        return vec![Line::from(vec![Span::styled(
            format!("  Binary file {} - cannot display diff", input.file_path),
            Style::default().fg(theme.dim()),
        )])];
    }

    // 特殊情况：截断
    if result.is_truncated {
        return vec![Line::from(vec![Span::styled(
            format!(
                "  Diff too large for {} - changes not displayed",
                input.file_path
            ),
            Style::default().fg(theme.dim()),
        )])];
    }

    // 空结果 + 非新建/删除 → 返回空
    if result.hunks.is_empty() && !result.is_new_file && !result.is_deleted_file {
        return Vec::new();
    }

    let mut lines = Vec::new();

    // 标题行
    let (prefix, fg, bg) = if result.is_new_file {
        ("+", theme.diff_add(), Some(theme.diff_add_bg()))
    } else if result.is_deleted_file {
        ("-", theme.diff_remove(), Some(theme.diff_remove_bg()))
    } else {
        (" ", theme.muted(), None)
    };
    let title_style = match bg {
        Some(bg) => Style::default().fg(fg).bg(bg),
        None => Style::default().fg(fg),
    };
    lines.push(Line::from(vec![Span::styled(
        format!("{} {}", prefix, input.file_path),
        title_style,
    )]));

    // 摘要行：Added N lines, removed M lines（对齐 CC，替代 @@ hunk header）
    // 新建文件不输出：Write 工具结果已显示 "Wrote N lines to <path>"，重复显示与 CC 不符
    let num_additions: usize = result
        .hunks
        .iter()
        .flat_map(|h| &h.lines)
        .filter(|dl| matches!(dl, DiffLine::Add { .. }))
        .count();
    let num_removals: usize = result
        .hunks
        .iter()
        .flat_map(|h| &h.lines)
        .filter(|dl| matches!(dl, DiffLine::Remove { .. }))
        .count();
    if !result.is_new_file && (num_additions > 0 || num_removals > 0) {
        // 对齐 CC FileEditToolUpdatedMessage：普通 <Text>，无前景色，仅数字加粗
        let plain = Style::default();
        let bold = Style::default().add_modifier(ratatui::style::Modifier::BOLD);
        let mut parts: Vec<Span<'static>> = Vec::new();
        if num_additions > 0 {
            let unit = if num_additions > 1 { "lines" } else { "line" };
            parts.push(Span::styled("Added ", plain));
            parts.push(Span::styled(format!("{num_additions}"), bold));
            parts.push(Span::styled(format!(" {unit}"), plain));
        }
        if num_additions > 0 && num_removals > 0 {
            parts.push(Span::styled(", ", plain));
        }
        if num_removals > 0 {
            let unit = if num_removals > 1 { "lines" } else { "line" };
            let lead = if num_additions == 0 {
                "Removed "
            } else {
                "removed "
            };
            parts.push(Span::styled(lead, plain));
            parts.push(Span::styled(format!("{num_removals}"), bold));
            parts.push(Span::styled(format!(" {unit}"), plain));
        }
        lines.push(Line::from(parts));
    }

    // 计算最大行号
    let mut max_line_num: u32 = 0;
    for hunk in &result.hunks {
        for dl in &hunk.lines {
            match dl {
                DiffLine::Context {
                    old_line_num,
                    new_line_num,
                    ..
                } => {
                    max_line_num = max_line_num.max(*old_line_num).max(*new_line_num);
                }
                DiffLine::Add { line_num, .. } | DiffLine::Remove { line_num, .. } => {
                    max_line_num = max_line_num.max(*line_num);
                }
                DiffLine::HunkHeader { .. } => {}
            }
        }
    }

    let n = digit_count(max_line_num).max(1);
    // 对齐 CC color-diff：单行号，格式 marker + ' ' + 右对齐行号 + ' '，无竖线分隔符
    let gutter_width = 1 + 1 + n + 1;

    // 语法高亮：从文件扩展名推导语言
    #[cfg(feature = "markdown-highlight")]
    let syntax_lang = detect_language(&input.file_path, first_line_of(input));
    #[cfg(feature = "markdown-highlight")]
    let syntax_theme_name = theme.syntax_theme_name();

    for (hunk_idx, hunk) in result.hunks.iter().enumerate() {
        // 对齐 CC：多 hunk 之间用 "..." 分隔
        if hunk_idx > 0 {
            lines.push(Line::from(Span::styled(
                "...",
                Style::default().fg(theme.dim()),
            )));
        }
        // 对齐 CC：保留原始缩进（不去公共前导空格），上下文行与改动行列对齐
        for dl in &hunk.lines {
            match dl {
                DiffLine::HunkHeader { .. } => {
                    // 对齐 CC：不渲染 @@ hunk header（摘要行已替代其信息量）
                }
                DiffLine::Context {
                    text,
                    old_line_num: _,
                    new_line_num,
                } => {
                    // CC 单行号：context 行用新文件行号，marker 位为空格
                    let gutter_style = Style::default().fg(theme.dim());
                    let gutter = GutterLayout::new(
                        format!("  {:>n$} ", new_line_num, n = n),
                        " ",
                        n,
                        gutter_style,
                    );
                    let dedented = text.trim_end_matches('\n').to_string();
                    let content_width = width.saturating_sub(gutter_width);

                    // 尝试语法高亮（Context 行无背景色）
                    #[cfg(feature = "markdown-highlight")]
                    let spans = syntax_lang
                        .as_deref()
                        .and_then(|lang| highlight_line_spans(&dedented, syntax_theme_name, lang))
                        .unwrap_or_else(|| vec![Span::styled(dedented, Style::default())]);
                    #[cfg(not(feature = "markdown-highlight"))]
                    let spans = vec![Span::styled(dedented, Style::default())];

                    push_wrapped_line(&mut lines, &gutter, spans, content_width, None);
                }
                DiffLine::Add {
                    text,
                    line_num,
                    word_diff,
                } => {
                    let marker = "+";
                    // CC 单行号：marker + ' ' + 右对齐行号 + ' '
                    // 分层（CC color-diff）：marker 与行号用 decoration 色，
                    // 代码正文用语法高亮/正文色，不再三处共用一个前景色。
                    let deco = theme.diff_add_decoration();
                    let fg = theme.diff_add();
                    let bg = theme.diff_add_bg();
                    let gutter = GutterLayout::new(
                        format!("{} {:>n$} ", marker, line_num, n = n),
                        marker,
                        n,
                        Style::default().fg(deco).bg(bg),
                    );
                    let content_width = width.saturating_sub(gutter_width);
                    let line_text = text.trim_end_matches('\n').to_string();

                    // 对齐 CC color-diff：+ 行先做语法高亮，再把词级变更区间
                    // 叠加为更深底色（前景保留语法色），两者并存而非二选一。
                    // 高亮不可用（语言未识别 / feature 关闭）时回退纯色词级渲染。
                    let spans = if let Some(wd) = word_diff {
                        let word_bg = theme.diff_add_word_bg();
                        #[cfg(feature = "markdown-highlight")]
                        let highlighted = syntax_lang.as_deref().and_then(|lang| {
                            let spans = highlight_line_spans_on_bg(
                                &line_text,
                                bg,
                                syntax_theme_name,
                                lang,
                            )?;
                            Some(apply_word_diff_bg(
                                spans,
                                &word_diff_change_ranges(wd),
                                word_bg,
                            ))
                        });
                        #[cfg(not(feature = "markdown-highlight"))]
                        let highlighted: Option<Vec<Span<'static>>> = None;

                        highlighted.unwrap_or_else(|| {
                            render_word_diff_spans(wd, &line_text, fg, bg, word_bg, true)
                        })
                    } else {
                        #[cfg(feature = "markdown-highlight")]
                        let highlighted = syntax_lang.as_deref().and_then(|lang| {
                            highlight_line_spans_on_bg(&line_text, bg, syntax_theme_name, lang)
                        });
                        #[cfg(not(feature = "markdown-highlight"))]
                        let highlighted: Option<Vec<Span<'static>>> = None;

                        highlighted.unwrap_or_else(|| {
                            vec![Span::styled(
                                line_text.clone(),
                                Style::default().fg(fg).bg(bg),
                            )]
                        })
                    };

                    push_wrapped_line(&mut lines, &gutter, spans, content_width, Some(bg));
                }
                DiffLine::Remove {
                    text,
                    line_num,
                    word_diff,
                } => {
                    let marker = "-";
                    // CC 单行号：marker + ' ' + 右对齐行号 + ' '
                    // 分层（CC color-diff `marker === '-'` 分支）：marker 与行号用
                    // deleteDecoration 暗红，**代码正文用正文色（近白）**——之前三处
                    // 共用 diff_remove 亮红，删除行代码整行发红，与 CC 不符。
                    let deco = theme.diff_remove_decoration();
                    let bg = theme.diff_remove_bg();
                    let fg = theme.text();
                    let gutter = GutterLayout::new(
                        format!("{} {:>n$} ", marker, line_num, n = n),
                        marker,
                        n,
                        Style::default().fg(deco).bg(bg),
                    );
                    let content_width = width.saturating_sub(gutter_width);
                    let line_text = text.trim_end_matches('\n').to_string();

                    // 删除行不做语法高亮，与 CC 一致
                    let spans = match word_diff {
                        Some(wd) => render_word_diff_spans(
                            wd,
                            &line_text,
                            fg,
                            bg,
                            theme.diff_remove_word_bg(),
                            false,
                        ),
                        None => vec![Span::styled(
                            line_text.clone(),
                            Style::default().fg(fg).bg(bg),
                        )],
                    };

                    push_wrapped_line(&mut lines, &gutter, spans, content_width, Some(bg));
                }
            }
        }
    }

    lines
}

/// 词级变更区间（相对显示文本的字节偏移）。
///
/// Add 行渲染时跳过 `Removed` 段，因此偏移只累计 `Unchanged` + `Added` 段，
/// 与 [`render_word_diff_spans`] 的取舍保持一致。
#[cfg(feature = "markdown-highlight")]
fn word_diff_change_ranges(wd: &super::WordDiff) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut offset = 0usize;
    for (text, word_type) in &wd.segments {
        match word_type {
            DiffWordType::Added => {
                if !text.is_empty() {
                    ranges.push((offset, offset + text.len()));
                }
                offset += text.len();
            }
            DiffWordType::Unchanged => offset += text.len(),
            // Removed 段在 Add 行不显示，不占显示文本的偏移
            DiffWordType::Removed => {}
        }
    }
    ranges
}

/// 把落在词级变更区间内的片段背景换成 `word_bg`，前景色保留语法高亮结果。
///
/// 对齐 CC color-diff 的 `applyBackground`：语法前景色与词级底色是两个正交维度，
/// 先在已着色的 span 上换背景，而不是用词级切分结果覆盖整行样式。
#[cfg(feature = "markdown-highlight")]
fn apply_word_diff_bg(
    spans: Vec<Span<'static>>,
    ranges: &[(usize, usize)],
    word_bg: Color,
) -> Vec<Span<'static>> {
    if ranges.is_empty() {
        return spans;
    }
    let mut out = Vec::with_capacity(spans.len());
    let mut offset = 0usize;
    for span in spans {
        let text = span.content.into_owned();
        let style = span.style;
        let span_start = offset;
        let span_end = span_start + text.len();
        offset = span_end;
        let mut cursor = span_start;
        while cursor < span_end {
            let in_range = ranges
                .iter()
                .any(|&(start, end)| start <= cursor && cursor < end);
            // 下一个切点取最近的区间端点；区间与 span 的边界都落在 UTF-8 字符边界上
            let mut next = span_end;
            for &(start, end) in ranges {
                if end > cursor && end < next {
                    next = end;
                }
                if start > cursor && start < next {
                    next = start;
                }
            }
            if next <= cursor {
                next = span_end;
            }
            let piece = text[cursor - span_start..next - span_start].to_string();
            let piece_style = if in_range { style.bg(word_bg) } else { style };
            out.push(Span::styled(piece, piece_style));
            cursor = next;
        }
    }
    out
}

/// 渲染 word diff 段：未变部分用 base bg，变更部分用 word_bg（更深背景）。
///
/// 不截断、不 padding——折行与底色延伸由 [`push_wrapped_line`] 统一处理，
/// 否则超宽行会先被截掉，折行就失去意义。
fn render_word_diff_spans(
    wd: &super::WordDiff,
    full_text: &str,
    fg: Color,
    bg: Color,
    word_bg: Color,
    is_add: bool,
) -> Vec<Span<'static>> {
    // 先判断是否所有段都是 Unchanged
    let has_change = wd
        .segments
        .iter()
        .any(|(_, t)| !matches!(t, DiffWordType::Unchanged));
    if !has_change {
        return vec![Span::styled(
            full_text.trim_end_matches('\n').to_string(),
            Style::default().fg(fg).bg(bg),
        )];
    }

    let mut spans = Vec::new();

    for (text, word_type) in &wd.segments {
        // Add 行跳过 Removed 段，Remove 行跳过 Added 段
        let should_skip = match word_type {
            DiffWordType::Added => !is_add,
            DiffWordType::Removed => is_add,
            DiffWordType::Unchanged => false,
        };
        if should_skip || text.is_empty() {
            continue;
        }

        let is_changed = !matches!(word_type, DiffWordType::Unchanged);
        let span_bg = if is_changed { word_bg } else { bg };
        spans.push(Span::styled(
            text.clone(),
            Style::default().fg(fg).bg(span_bg),
        ));
    }

    spans
}

/// 渲染新建文件的代码预览（对齐 CC `FileWriteTool` 的 create 分支 / `ColorFile`）：
/// 不画 diff 底色，只做语法高亮，并带行号 gutter（宽度 `总行数位数 + 2`）；
/// 超过 `max_lines` 源行数时截断并追加 `… +N lines (ctrl+o to expand)` 提示
/// （`max_lines == usize::MAX` 表示全量）。
///
/// 超宽行按显示宽度折行（与 diff 行口径一致，续行行号位补空格），
/// 语言无法识别时回退纯文本。
pub fn render_new_file_code(
    file_path: &str,
    content: &str,
    width: usize,
    theme: &dyn Theme,
    max_lines: usize,
) -> Vec<Line<'static>> {
    // 对齐 CC `FileWriteTool`：空内容显示 "(No content)" 占位
    let body = if content.is_empty() {
        "(No content)"
    } else {
        content.strip_suffix('\n').unwrap_or(content)
    };
    // 对齐 CC `HighlightedCode/Fallback.tsx`：预览内容同样行首 tab → 2 空格
    let body = super::convert_leading_tabs_to_spaces(body);
    let raw_lines: Vec<&str> = body.split('\n').collect();
    let total = raw_lines.len();
    let shown = total.min(max_lines);

    // 对齐 CC ColorFile：行号宽度取总行数位数，gutter = ' ' + 行号 + ' '
    let n = digit_count(total as u32).max(1);
    let gutter_width = 1 + n + 1;
    let content_width = width.saturating_sub(gutter_width);
    let gutter_style = Style::default().fg(theme.dim());
    let cont_gutter = " ".repeat(gutter_width);

    #[cfg(feature = "markdown-highlight")]
    let syntax_lang = detect_language(file_path, raw_lines.first().copied());
    #[cfg(feature = "markdown-highlight")]
    let syntax_theme_name = theme.syntax_theme_name();
    #[cfg(not(feature = "markdown-highlight"))]
    let _ = (file_path, theme);

    let mut out: Vec<Line<'static>> = Vec::with_capacity(shown);
    for (i, line_text) in raw_lines[..shown].iter().enumerate() {
        #[cfg(feature = "markdown-highlight")]
        let spans = syntax_lang
            .as_deref()
            .and_then(|lang| highlight_line_spans(line_text, syntax_theme_name, lang))
            .unwrap_or_else(|| vec![Span::raw(line_text.to_string())]);
        #[cfg(not(feature = "markdown-highlight"))]
        let spans = vec![Span::raw(line_text.to_string())];

        let first_gutter = format!(" {:>n$} ", i + 1, n = n);
        for (seg_idx, seg) in wrap_spans(spans, content_width).into_iter().enumerate() {
            let gutter = if seg_idx == 0 {
                first_gutter.clone()
            } else {
                cont_gutter.clone()
            };
            let mut line_spans = Vec::with_capacity(seg.len() + 1);
            line_spans.push(Span::styled(gutter, gutter_style));
            line_spans.extend(seg);
            out.push(Line::from(line_spans));
        }
    }

    if shown < total {
        let hidden = total - shown;
        out.push(Line::from(Span::styled(
            format!(
                "… +{hidden} line{} (ctrl+o to expand)",
                if hidden == 1 { "" } else { "s" }
            ),
            Style::default().fg(theme.dim()),
        )));
    }

    out
}

#[cfg(test)]
#[path = "renderer_test.rs"]
mod tests;
