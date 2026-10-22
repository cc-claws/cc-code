//! OSC 8 终端超链接：把「可见文本 + 链接命中区」装饰成带 OSC 8 转义序列的 cell 符号。
//!
//! 设计要点：
//!
//! 1. **不改 buffer**：只把「某屏幕坐标是链接」登记进
//!    [`crate::terminal_backend::hyperlink`]，真正包裹转义序列发生在后端输出层。
//!    绝不能把序列塞进 `Cell` symbol——`ratatui` 的 `Buffer::diff` 按
//!    `symbol.width()` 跳过后续 cell，序列载荷是可见字符，会让链接后面的文字
//!    整段不落笔。
//! 2. **命中区坐标**：`LinkHit` 的 `line` 是输出 `Text.lines` 行下标，已在上游
//!    `render_thread` 中做过全局 dedup 行映射；`g_start`/`g_end` 是该行
//!    `plain_text` 的 grapheme 下标（不含 end）。本模块只负责把 grapheme 下标
//!    换算到**显示列**。
//! 3. **可测试**：坐标换算与 URL 识别都是纯函数。

use std::collections::HashMap;

use cc_widgets::markdown::LinkHit;

/// 是否输出 OSC 8 超链接。
///
/// 只保留 `DISABLE_HYPERLINKS` 作为逃生口，默认开启：原先用
/// `conpty::hover_available()` 当开关，但它衡量的是「宿主是否上报 ConPTY 鼠标悬停」，
/// 与「终端是否支持 OSC 8」并不等价——Windows Terminal 之外的常见宿主
/// （VS Code 集成终端、tmux、SSH 到远端）会被判 false，链接便既不可点、
/// 也没有终端侧的悬停提示。不认 OSC 8 的终端会直接丢弃该序列，不会显示乱码。
pub fn enabled() -> bool {
    enabled_from(std::env::var("DISABLE_HYPERLINKS").ok().as_deref())
}

/// [`enabled`] 的纯函数形式：便于断言各种取值而不污染进程级环境变量。
fn enabled_from(disable: Option<&str>) -> bool {
    match disable {
        Some(value) => {
            let value = value.trim().to_ascii_lowercase();
            !matches!(value.as_str(), "1" | "true" | "yes" | "on")
        }
        None => true,
    }
}

/// 从一行纯文本里找出裸 URL 的命中区（grapheme 下标）。
///
/// Markdown 层只为 `[文字](url)` 产出 [`LinkHit`]；工具结果、`!` 命令输出，
/// 以及「没有显示文字」而退化成纯 URL 的链接都只有文本，终端无从得知它是链接。
/// 这里按 [`crate::ui::message_render::find_url_ranges`] 的口径（与 CC
/// `OutputLine.tsx::URL_IN_JSON` 同源）逐行补齐命中区。
///
/// `line` 是调用方行空间（消息区即 `RenderCache` 的全局逻辑行号）里的下标，
/// 原样写回 `LinkHit.line`。
pub fn bare_url_hits(plain_text: &str, line: usize) -> Vec<LinkHit> {
    if !plain_text.contains("://") {
        return Vec::new();
    }
    crate::ui::message_render::find_url_ranges(plain_text)
        .into_iter()
        .filter_map(|(byte_start, byte_end)| {
            let g_start = grapheme_index(plain_text, byte_start)?;
            let g_end = grapheme_index(plain_text, byte_end)?;
            (g_end > g_start).then(|| LinkHit {
                line,
                g_start,
                g_end,
                url: plain_text[byte_start..byte_end].to_string(),
            })
        })
        .collect()
}

/// 字节偏移 → grapheme 下标（`LinkHit` 与 `char_widths` 都按 grapheme 计）。
fn grapheme_index(text: &str, byte: usize) -> Option<usize> {
    let prefix = text.get(..byte)?;
    Some(unicode_segmentation::UnicodeSegmentation::graphemes(prefix, true).count())
}

/// 单个链接在一行内的**显示列**区间（不含 end）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkColumns {
    pub columns: std::ops::Range<u16>,
    pub url: String,
}

/// 把一行内的 grapheme 命中区换算为显示列区间。
///
/// `plain_text` 是该行的纯文本（与 `LinkHit` 的 grapheme 坐标同基），
/// `hit` 是该行的一个命中区。超出范围或空区间返回 `None`。
///
/// 注意：`g_start`/`g_end` 是 grapheme 下标，而显示列需要按 `char_widths`
/// 累加得到；全角字符会占 2 列。
pub fn graphemes_to_columns(
    plain_text: &str,
    char_widths: &[u8],
    hit: &LinkHit,
) -> Option<LinkColumns> {
    if hit.g_end <= hit.g_start {
        return None;
    }
    // grapheme 下标 → 字节偏移，同时累加宽度。
    let mut grapheme_index = 0usize;
    let mut byte_offset = 0usize;
    let mut column = 0u16;
    let mut start_column: Option<u16> = None;
    let mut end_column: Option<u16> = None;

    for grapheme in unicode_segmentation::UnicodeSegmentation::graphemes(plain_text, true) {
        if grapheme_index == hit.g_start {
            start_column = Some(column);
        }
        if grapheme_index == hit.g_end {
            end_column = Some(column);
            break;
        }
        let width = char_widths.get(grapheme_index).copied().unwrap_or(1).max(1);
        column = column.saturating_add(u16::from(width));
        byte_offset += grapheme.len();
        grapheme_index += 1;
    }
    // g_end 落在行尾时上面的循环不会命中相等分支。
    if end_column.is_none() && grapheme_index == hit.g_end {
        end_column = Some(column);
    }
    let _ = byte_offset;
    let (start, end) = (start_column?, end_column?);
    (end > start).then(|| LinkColumns {
        columns: start..end,
        url: hit.url.clone(),
    })
}

/// 把若干 `LinkHit` 按行聚合为「行 → 该行的列区间列表」。
///
/// `line_offset` 是 `hits` 的行号相对调用方行空间的偏移（例如视口裁剪已把
/// 前 `first_idx` 行切掉时传 `-first_idx` 的等价形式）；此处用 `add` 表达，
/// 因为调用方总是把全局行号减去已裁剪前缀。
pub fn group_by_line(
    hits: &[LinkHit],
    wrap_map: &[crate::ui::render_thread::WrappedLineInfo],
    first_idx: usize,
) -> HashMap<usize, Vec<LinkColumns>> {
    let mut out: HashMap<usize, Vec<LinkColumns>> = HashMap::new();
    for hit in hits {
        let Some(local) = hit.line.checked_sub(first_idx) else {
            continue;
        };
        let Some(info) = wrap_map.get(first_idx + local) else {
            continue;
        };
        let Some(link) = graphemes_to_columns(&info.plain_text, &info.char_widths, hit) else {
            continue;
        };
        out.entry(local).or_default().push(link);
    }
    for links in out.values_mut() {
        links.sort_by_key(|link| link.columns.start);
    }
    out
}

/// 把一行内指定列区间的 cell 换算成超链接屏幕坐标。
///
/// `cells` 产出 `(行内列号, 屏幕坐标)`；返回 `(屏幕坐标, URL)`，由调用方登记到
/// [`crate::terminal_backend::hyperlink`]。**不修改 buffer**：转义序列在输出层包裹
/// （原因见模块文档）。纯函数，便于断言列换算。
pub fn link_cells(
    cells: impl Iterator<Item = (u16, (u16, u16))>,
    links: &[LinkColumns],
    area: ratatui::layout::Rect,
) -> Vec<((u16, u16), String)> {
    let mut out = Vec::new();
    for (column, (x, y)) in cells {
        if !area.contains(ratatui::layout::Position::new(x, y)) {
            continue;
        }
        let Some(link) = links.iter().find(|link| link.columns.contains(&column)) else {
            continue;
        };
        out.push(((x, y), link.url.clone()));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use unicode_segmentation::UnicodeSegmentation;

    fn hit(line: usize, g_start: usize, g_end: usize, url: &str) -> LinkHit {
        LinkHit {
            line,
            g_start,
            g_end,
            url: url.to_string(),
        }
    }

    #[test]
    fn ascii_hit_maps_to_same_columns() {
        // "see https://a.b here" —— URL 从 grapheme 4 到 18
        let text = "see https://a.b here";
        let widths = vec![1u8; text.len()];
        let link = graphemes_to_columns(text, &widths, &hit(0, 4, 18, "https://a.b")).unwrap();
        assert_eq!(link.columns, 4..18);
        assert_eq!(link.url, "https://a.b");
    }

    #[test]
    fn wide_chars_double_the_column_span() {
        // 4 个汉字，各占 2 列；grapheme 1..3 → 列 2..6
        let text = "查看链接";
        let widths = vec![2u8; 4];
        let link = graphemes_to_columns(text, &widths, &hit(0, 1, 3, "https://x.y")).unwrap();
        assert_eq!(link.columns, 2..6, "全角字符每个占两列");
    }

    #[test]
    fn hit_reaching_line_end_is_included() {
        let text = "ab https://a.b";
        let widths = vec![1u8; text.chars().count()];
        let count = text.chars().count();
        let link = graphemes_to_columns(text, &widths, &hit(0, 3, count, "https://a.b")).unwrap();
        assert_eq!(link.columns, 3..count as u16, "命中到行尾不应丢失末列");
    }

    #[test]
    fn empty_hit_is_ignored() {
        let text = "abc";
        let widths = vec![1u8; 3];
        assert!(graphemes_to_columns(text, &widths, &hit(0, 1, 1, "https://a.b")).is_none());
    }

    #[test]
    fn out_of_range_end_clamps_to_none() {
        let text = "abc";
        let widths = vec![1u8; 3];
        // g_end 超出实际 grapheme 数：起始可定位、结束定位不到 → None
        assert!(graphemes_to_columns(text, &widths, &hit(0, 1, 99, "https://a.b")).is_none());
    }

    #[test]
    fn link_cells_maps_columns_to_screen_coordinates() {
        use ratatui::layout::Rect;
        // 屏幕区域 x=5..15、y=3：链接位于行内列 4..8
        let area = Rect::new(5, 3, 10, 1);
        let links = vec![LinkColumns {
            columns: 4..8,
            url: "https://example.com".to_string(),
        }];
        let cells = link_cells(
            (0..10u16).map(|column| (column, (5 + column, 3))),
            &links,
            area,
        );
        assert_eq!(
            cells,
            (4..8u16)
                .map(|column| ((5 + column, 3), "https://example.com".to_string()))
                .collect::<Vec<_>>(),
            "只有命中区内的列才应登记，坐标按屏幕位置平移"
        );
    }

    #[test]
    fn link_cells_ignores_cells_outside_area() {
        use ratatui::layout::Rect;
        let area = Rect::new(0, 0, 2, 1);
        let links = vec![LinkColumns {
            columns: 0..2,
            url: "https://example.com".to_string(),
        }];
        let cells = link_cells(
            [(0u16, (0u16, 0u16)), (1, (1, 0)), (2, (9, 9))].into_iter(),
            &links,
            area,
        );
        assert_eq!(cells.len(), 2, "越界坐标不得登记，且不得 panic");
    }

    #[test]
    fn hyperlinks_default_on_and_only_explicit_disable_turns_off() {
        assert!(enabled_from(None), "默认必须开启，否则常见宿主点不动链接");
        for off in ["1", "true", "TRUE", "yes", " on "] {
            assert!(!enabled_from(Some(off)), "{off:?} 应视为关闭");
        }
        for keep in ["0", "false", ""] {
            assert!(enabled_from(Some(keep)), "{keep:?} 不应关闭超链接");
        }
    }

    #[test]
    fn bare_url_hits_finds_tool_output_url_with_ascii_prefix() {
        // Bash 工具结果行：`⎿ Build fail 2m30s <url>`
        let text = "⎿ Build (ubuntu-latest) fail 2m30s https://a.example/runs/1";
        let hits = bare_url_hits(text, 7);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].line, 7, "行号原样写回");
        assert_eq!(hits[0].url, "https://a.example/runs/1");
        let start = text.graphemes(true).take(hits[0].g_start).count();
        assert_eq!(
            text.graphemes(true).nth(start).unwrap(),
            "h",
            "g_start 必须落在 URL 首字符"
        );
    }

    #[test]
    fn bare_url_hits_uses_grapheme_offsets_across_cjk() {
        // 全角字符在 grapheme 下标里各占 1，在显示列上各占 2。
        let text = "构建失败 https://a.example/x";
        let hits = bare_url_hits(text, 0);
        assert_eq!(hits.len(), 1);
        assert_eq!(
            hits[0].g_start, 5,
            "「构建失败」4 字 + 空格 = 5 个 grapheme"
        );
        let widths: Vec<u8> = text
            .graphemes(true)
            .map(|g| unicode_width::UnicodeWidthStr::width(g) as u8)
            .collect();
        let columns = graphemes_to_columns(text, &widths, &hits[0]).unwrap();
        assert_eq!(columns.columns.start, 9, "全角 4 字占 8 列 + 空格 1 列");
    }

    #[test]
    fn bare_url_hits_ignores_text_without_scheme() {
        assert!(bare_url_hits("Build fail 2m30s, see example.com", 0).is_empty());
        assert!(bare_url_hits("https:// trailing", 0).is_empty());
    }
}
