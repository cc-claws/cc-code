//! OSC 8 终端超链接：把「可见文本 + 链接命中区」装饰成带 OSC 8 转义序列的 cell 符号。
//!
//! 设计要点：
//!
//! 1. **布局零影响**：OSC 8 序列只注入 `Cell` 的 symbol 文本，不改
//!    `symbol.width()` 之外的任何度量。注入发生在宽度适配后端（`WidthSafeBackend`）
//!    之后，避免转义序列污染字形列宽探测。
//! 2. **命中区坐标**：`LinkHit` 的 `line` 是输出 `Text.lines` 行下标，已在上游
//!    `render_thread` 中做过全局 dedup 行映射；`g_start`/`g_end` 是该行
//!    `plain_text` 的 grapheme 下标（不含 end）。本模块只负责把 grapheme 下标
//!    换算到**显示列**。
//! 3. **可测试**：装饰逻辑与终端无关，纯函数，可在 `TestBackend` 上断言。

use std::collections::HashMap;

use cc_widgets::markdown::LinkHit;

/// OSC 8 开序列前缀：`ESC ] 8 ; params ; URI ST`。
///
/// 安全：URI 中任何控制字符（含 `ESC`、`BEL`、换行）都会被剥离。否则一个
/// 含 `\x07` 或 `\x1b\\` 的 URL 能提前终止 OSC 8 序列并注入任意终端指令。
fn osc8_open(url: &str) -> String {
    let safe: String = url.chars().filter(|ch| !ch.is_control()).collect();
    format!("\x1b]8;;{safe}\x1b\\")
}

/// OSC 8 关序列。
const OSC8_CLOSE: &str = "\x1b]8;;\x1b\\";

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

/// 给一行内指定列区间的 cell 包裹 OSC 8 序列。
///
/// `cells` 产出 `(行内列号, 屏幕坐标)`，`buffer` 为目标帧缓冲。只装饰
/// **非空且非降级跳过**的 cell。同一链接的连续 cell 各自独立包裹，因为
/// ratatui 会按 cell 独立 diff 与输出，跨 cell 的连续序列无法保证。
pub fn decorate_cells(
    cells: impl Iterator<Item = (u16, (u16, u16))>,
    links: &[LinkColumns],
    buffer: &mut ratatui::buffer::Buffer,
) {
    if links.is_empty() {
        return;
    }
    for (column, (x, y)) in cells {
        if !buffer.area.contains(ratatui::layout::Position::new(x, y)) {
            continue;
        }
        let Some(link) = links.iter().find(|link| link.columns.contains(&column)) else {
            continue;
        };
        let cell = &mut buffer[(x, y)];
        if cell.skip || cell.symbol().trim().is_empty() {
            continue;
        }
        let decorated = format!("{}{}{}", osc8_open(&link.url), cell.symbol(), OSC8_CLOSE);
        cell.set_symbol(&decorated);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn control_characters_are_stripped_from_url() {
        // 防注入：URL 中的控制字符必须被移除，避免提前终止 OSC8 序列。
        let decorated = format!(
            "{}{}{}",
            osc8_open("https://a.b/\u{7}evil"),
            "x",
            OSC8_CLOSE
        );
        assert!(
            !decorated.contains('\u{7}'),
            "控制字符必须被剥离，否则可注入任意终端序列"
        );
    }

    #[test]
    fn decorate_cells_wraps_only_cells_inside_link_columns() {
        use ratatui::{buffer::Buffer, layout::Rect};

        // 一行 10 列："See https…" 的链接位于列 4..8
        let area = Rect::new(0, 0, 10, 1);
        let mut buffer = Buffer::empty(area);
        for x in 0..10u16 {
            buffer[(x, 0)].set_symbol("x");
        }
        let links = vec![LinkColumns {
            columns: 4..8,
            url: "https://example.com".to_string(),
        }];
        decorate_cells((0..10u16).map(|x| (x, (x, 0))), &links, &mut buffer);

        for x in 0..10u16 {
            let symbol = buffer[(x, 0)].symbol().to_string();
            let linked = symbol.starts_with("\x1b]8;;https://example.com\x1b\\");
            assert_eq!(
                linked,
                (4..8).contains(&x),
                "只有命中区内的 cell 才应被包裹，列 {x} 实际为 {symbol:?}"
            );
            if linked {
                assert!(symbol.ends_with(OSC8_CLOSE), "包裹必须成对闭合");
            }
        }
    }

    #[test]
    fn decorate_cells_skips_blank_cells() {
        use ratatui::{buffer::Buffer, layout::Rect};

        let area = Rect::new(0, 0, 4, 1);
        let mut buffer = Buffer::empty(area); // 默认全为空格
        let links = vec![LinkColumns {
            columns: 0..4,
            url: "https://example.com".to_string(),
        }];
        decorate_cells((0..4u16).map(|x| (x, (x, 0))), &links, &mut buffer);
        for x in 0..4u16 {
            assert_eq!(
                buffer[(x, 0)].symbol(),
                " ",
                "空白 cell 不应被包裹，否则会把无内容的空格变成热点"
            );
        }
    }

    #[test]
    fn decorated_symbol_never_reaches_width_probe() {
        // 不变式：注入 OSC8 后，宽度适配后端（WidthSafeBackend::output_cell）
        // 要么因 `symbol.is_ascii()` 直接返回，要么因含控制字符而让探针返回
        // None（见 windows.rs 的 `symbol.chars().any(char::is_control)` 守卫）。
        // 若这条不成立，转义序列会被当成字形去量列宽，导致整行错位。
        let decorated = format!("{}中{}\x1b]8;;\x1b\\", osc8_open("https://a.b"), "");
        // 1) 含控制字符 → 探针必然放弃测量
        assert!(
            decorated.chars().any(char::is_control),
            "注入后必须含控制字符，探针才能安全放弃"
        );
        // 2) 纯 ASCII 装饰（如链接文字是英文）时 is_ascii 为真，同样跳过探测
        let ascii_only = format!("{}x{}", osc8_open("https://a.b"), OSC8_CLOSE);
        assert!(
            ascii_only.is_ascii(),
            "OSC8 转义序列本身是 ASCII，含它时 is_ascii() 必须为真以跳过探测"
        );
    }
    #[test]
    fn decorate_cells_ignores_out_of_area_positions() {
        use ratatui::{buffer::Buffer, layout::Rect};

        let area = Rect::new(0, 0, 2, 1);
        let mut buffer = Buffer::empty(area);
        buffer[(0, 0)].set_symbol("a");
        let links = vec![LinkColumns {
            columns: 0..2,
            url: "https://example.com".to_string(),
        }];
        // 故意给一个越界坐标，不得 panic。
        decorate_cells(
            [(0u16, (0u16, 0u16)), (1u16, (1u16, 0u16)), (2, (9, 9))].into_iter(),
            &links,
            &mut buffer,
        );
        assert!(buffer[(0, 0)].symbol().starts_with('\x1b'));
    }
}
