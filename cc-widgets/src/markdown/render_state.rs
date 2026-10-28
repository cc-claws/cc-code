use pulldown_cmark::{Alignment, CodeBlockKind, Event, HeadingLevel, Tag, TagEnd};
use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};
use unicode_width::UnicodeWidthStr;

use super::{LinkHit, MarkdownTheme};
use unicode_segmentation::UnicodeSegmentation;

#[cfg(feature = "markdown-highlight")]
use super::highlight::highlight_code_block;

// ── 辅助类型 ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
enum ListType {
    Ordered(u64),
    Unordered,
}

#[derive(Debug, Clone)]
pub(crate) struct ListState {
    list_type: ListType,
}

// ── 表格类型 ──────────────────────────────────────────────────────────────────

type CellContent = Vec<Span<'static>>;

/// 单元格内已闭合的链接段，cell-local grapheme 坐标
///（`End(Link)` 在表格内按 `inline_g_len` / `frame.start` 计，与 `current_cell` 同坐标系）。
type CellLinkRange = (usize, usize, String);

#[derive(Debug, Default)]
struct CellData {
    spans: CellContent,
    links: Vec<CellLinkRange>,
}

/// 单元格折行后的一行：渲染 spans + 与 spans 的 grapheme 序列一一对应的
/// cell-local 输入下标。`src` 严格递增（词内 grapheme 与词间分隔空格都带真实
/// 输入下标；折行点丢弃的空格不出现在任何行里），命中区映射靠它把
/// cell-local 链接段切到折行后的行内偏移。
#[derive(Debug)]
struct CellLine {
    spans: Vec<Span<'static>>,
    src: Vec<usize>,
}

/// 单元格的折行结果：行 + 该格内全部链接段（cell-local 坐标）。
#[derive(Debug)]
struct WrappedCell {
    lines: Vec<CellLine>,
    links: Vec<CellLinkRange>,
}

#[derive(Debug, Default)]
struct TableBuilder {
    alignments: Vec<Alignment>,
    rows: Vec<Vec<CellData>>,
    current_row: Vec<CellData>,
    current_cell: CellContent,
    current_cell_links: Vec<CellLinkRange>,
    in_head: bool,
}

impl TableBuilder {
    fn new(alignments: Vec<Alignment>) -> Self {
        Self {
            alignments,
            ..Default::default()
        }
    }

    fn push_cell(&mut self) {
        let cell = std::mem::take(&mut self.current_cell);
        let links = std::mem::take(&mut self.current_cell_links);
        self.current_row.push(CellData { spans: cell, links });
    }

    fn push_row(&mut self) {
        if !self.current_row.is_empty() {
            let row = std::mem::take(&mut self.current_row);
            self.rows.push(row);
        }
    }

    /// 计算列宽并折行，返回 (各列分配宽度, 折行后的数据行)
    ///
    /// 列宽分配完全对齐 CC `MarkdownTable.tsx`：最小宽 = 该列所有单元格中最长
    /// **单词**的宽度（下限 3），理想宽 = 整格内容宽。空间不足时先按
    /// `ideal - min` 比例分配剩余空间，仍不够则按 `min` 等比压缩并允许硬断词。
    #[allow(clippy::type_complexity)]
    fn wrap_cells(
        &self,
        max_width: usize,
        _theme: &dyn MarkdownTheme,
    ) -> (Vec<usize>, Vec<Vec<WrappedCell>>) {
        let num_cols = self.rows.first().map(|r| r.len()).unwrap_or(0);
        if num_cols == 0 {
            return (vec![], vec![]);
        }

        // Step 1: 每列的最小（最长单词）与理想（整格）宽度
        let min_widths: Vec<usize> = (0..num_cols)
            .map(|c| {
                self.rows
                    .iter()
                    .filter_map(|row| row.get(c))
                    .map(|cell| cell_min_width(&cell.spans))
                    .max()
                    .unwrap_or(MIN_COLUMN_WIDTH)
            })
            .collect();
        let ideal_widths: Vec<usize> = (0..num_cols)
            .map(|c| {
                self.rows
                    .iter()
                    .filter_map(|row| row.get(c))
                    .map(|cell| cell_width(&cell.spans).max(MIN_COLUMN_WIDTH))
                    .max()
                    .unwrap_or(MIN_COLUMN_WIDTH)
                    .max(MIN_COLUMN_WIDTH)
            })
            .collect();

        // Step 2: 可用空间。边框开销 = 1 + 3*num_cols；
        // SAFETY_MARGIN 预留父级缩进与终端 resize 竞争的空间。
        let border_overhead = 1 + num_cols * 3;
        let available_width = max_width
            .saturating_sub(border_overhead)
            .saturating_sub(SAFETY_MARGIN)
            .max(num_cols * MIN_COLUMN_WIDTH);

        // Step 3: 分配列宽
        let total_min: usize = min_widths.iter().sum();
        let total_ideal: usize = ideal_widths.iter().sum();
        let mut needs_hard_wrap = false;
        let col_widths: Vec<usize> = if total_ideal <= available_width {
            ideal_widths.clone()
        } else if total_min <= available_width {
            let extra_space = available_width - total_min;
            let overflows: Vec<usize> = ideal_widths
                .iter()
                .zip(min_widths.iter())
                .map(|(&ideal, &min)| ideal.saturating_sub(min))
                .collect();
            let total_overflow: usize = overflows.iter().sum();
            min_widths
                .iter()
                .enumerate()
                .map(|(i, &min)| {
                    if total_overflow == 0 {
                        return min;
                    }
                    min + overflows[i] * extra_space / total_overflow
                })
                .collect()
        } else {
            // 最小宽都放不下：等比压缩，允许硬断词
            needs_hard_wrap = true;
            let scale = available_width as f64 / total_min as f64;
            min_widths
                .iter()
                .map(|&w| ((w as f64 * scale) as usize).max(MIN_COLUMN_WIDTH))
                .collect()
        };

        // Step 4: 折行每个单元格（折行结果附带 cell-local 链接段，供命中区映射）
        let wrapped_rows = self
            .rows
            .iter()
            .map(|row| {
                row.iter()
                    .enumerate()
                    .map(|(col, cell)| {
                        let width = col_widths.get(col).copied().unwrap_or(0);
                        WrappedCell {
                            lines: wrap_cell_spans(&cell.spans, width, needs_hard_wrap),
                            links: cell.links.clone(),
                        }
                    })
                    .collect()
            })
            .collect();

        (col_widths, wrapped_rows)
    }

    /// 渲染表格，支持自动换行；同时返回表格内链接的命中区。
    ///
    /// 整体对齐 CC `MarkdownTable.tsx`：表头强制居中、每行之间补分隔线、
    /// 折行过多或越界时退回纵向 key-value 格式。
    ///
    /// 命中区映射：返回的 `LinkHit.line` 是表格内的相对行号（调用方在
    /// `End(Table)` 处加上表格起始行）；`g_start`/`g_end` 是最终行 plain_text
    /// 的 grapheme 区间，含边框、空格与对齐 padding。纵向回退格式把值压成
    /// 纯文本（链接样式本身已丢弃），故不产出命中区。
    fn render_with_wrap(
        self,
        max_width: usize,
        theme: &dyn MarkdownTheme,
    ) -> (Vec<Line<'static>>, Vec<LinkHit>) {
        let (allocated_widths, wrapped_rows) = self.wrap_cells(max_width, theme);
        if wrapped_rows.is_empty() {
            return (Vec::new(), Vec::new());
        }

        // 任一单元格行数超过阈值 → 纵向 key-value 格式（对齐 CC `MAX_ROW_LINES`）
        let max_row_lines = wrapped_rows
            .iter()
            .map(|row| row.iter().map(|c| c.lines.len()).max().unwrap_or(1))
            .max()
            .unwrap_or(1);
        if max_row_lines > MAX_ROW_LINES {
            return (self.render_vertical(max_width), Vec::new());
        }

        // 列宽直接采用分配结果（对齐 CC：用 `columnWidths` 画边框与对齐，
        // 不再按折行后的内容宽度回缩，否则短内容会让列比分配宽度更窄）。
        let col_widths: Vec<usize> = allocated_widths;

        // 越界兜底：整表超出终端安全边距时退回纵向格式（对齐 CC 的二次校验）
        let total_width = 1 + col_widths.iter().map(|w| w + 3).sum::<usize>();
        if total_width > max_width.saturating_sub(SAFETY_MARGIN) {
            return (self.render_vertical(max_width), Vec::new());
        }

        let mut lines: Vec<Line<'static>> = Vec::new();
        let mut links: Vec<LinkHit> = Vec::new();
        lines.push(Line::from(make_border(&col_widths, '┌', '┬', '┐', '─')));

        for (row_idx, row) in wrapped_rows.iter().enumerate() {
            let max_lines = row.iter().map(|c| c.lines.len()).max().unwrap_or(1);
            // 单元格垂直居中（对齐 CC 的 verticalOffsets）
            let vertical_offsets: Vec<usize> = row
                .iter()
                .map(|cell| (max_lines.saturating_sub(cell.lines.len())) / 2)
                .collect();

            for line_idx in 0..max_lines {
                // 边框不带颜色：CC 的表格框线是纯文本，走终端默认色
                let mut spans = vec![Span::raw("│".to_string())];
                // 当前行已产出的 grapheme 数（行首 │ 计 1），用于命中区列换算
                let mut line_g = 1usize;

                for (col_idx, cell) in row.iter().enumerate() {
                    let col_w = col_widths.get(col_idx).copied().unwrap_or(0);
                    spans.push(Span::raw(" "));
                    line_g += 1;

                    let offset = vertical_offsets[col_idx];
                    let content_idx = line_idx as isize - offset as isize;
                    if content_idx >= 0 && (content_idx as usize) < cell.lines.len() {
                        let cell_line = &cell.lines[content_idx as usize];
                        let content_width: usize =
                            cell_line.spans.iter().map(|s| s.content.width()).sum();
                        let padding = col_w.saturating_sub(content_width);

                        // 表头强制居中；数据行用 markdown 声明的对齐方式
                        let align = if row_idx == 0 {
                            Alignment::Center
                        } else {
                            self.alignments
                                .get(col_idx)
                                .copied()
                                .unwrap_or(Alignment::None)
                        };
                        let left_pad = match align {
                            Alignment::Center => padding / 2,
                            Alignment::Right => padding,
                            Alignment::None | Alignment::Left => 0,
                        };
                        // 该格链接段 → 最终行命中区：内容区 grapheme 起点 =
                        // 列前空格 + 左 padding；段内偏移由 src 下标换算。
                        for (ls, le, url) in &cell.links {
                            let mut seg_start = None;
                            let mut seg_end = None;
                            for (i, &s) in cell_line.src.iter().enumerate() {
                                if s >= *ls && s < *le {
                                    if seg_start.is_none() {
                                        seg_start = Some(i);
                                    }
                                    seg_end = Some(i);
                                }
                            }
                            if let (Some(si), Some(ei)) = (seg_start, seg_end) {
                                links.push(LinkHit {
                                    // lines[0] 是顶边框，当前行下标 = 已有行数
                                    line: lines.len(),
                                    g_start: line_g + left_pad + si,
                                    g_end: line_g + left_pad + ei + 1,
                                    url: url.clone(),
                                    recolor: true,
                                });
                            }
                        }
                        push_aligned(&mut spans, &cell_line.spans, padding, align);
                        line_g += spans_g_len(&cell_line.spans) + padding;
                    } else {
                        spans.push(Span::raw(" ".repeat(col_w)));
                        line_g += col_w;
                    }

                    spans.push(Span::raw(" "));
                    spans.push(Span::raw("│".to_string()));
                    line_g += 2;
                }

                lines.push(Line::from(spans));
            }

            // 表头后与每个数据行之间都补分隔线（CC 逐行加 ├─┼─┤）
            if row_idx + 1 < wrapped_rows.len() {
                lines.push(Line::from(make_border(&col_widths, '├', '┼', '┤', '─')));
            }
        }

        lines.push(Line::from(make_border(&col_widths, '└', '┴', '┘', '─')));
        (lines, links)
    }

    /// 纵向 key-value 格式（对齐 CC `renderVerticalFormat`）。
    ///
    /// 列太窄导致折行过多时横向表格已不可读；改为每行 `表头: 值`，
    /// 数据行之间用横线分隔。值压成单行，不再折行。
    fn render_vertical(&self, max_width: usize) -> Vec<Line<'static>> {
        if self.rows.len() < 2 {
            return Vec::new();
        }
        let headers: Vec<String> = self.rows[0]
            .iter()
            .map(|cell| {
                cell.spans
                    .iter()
                    .map(|s| s.content.as_ref())
                    .collect::<String>()
            })
            .collect();

        let separator: String = "─".repeat(max_width.saturating_sub(1).min(40));
        let mut lines: Vec<Line<'static>> = Vec::new();

        for (row_idx, row) in self.rows.iter().enumerate().skip(1) {
            if row_idx > 1 {
                // 分隔线同样不带样式（CC 用纯 `─` 重复）
                lines.push(Line::from(Span::raw(separator.clone())));
            }
            for (col_idx, cell) in row.iter().enumerate() {
                let label = headers
                    .get(col_idx)
                    .filter(|h| !h.is_empty())
                    .cloned()
                    .unwrap_or_else(|| format!("Column {}", col_idx + 1));
                let value = cell
                    .spans
                    .iter()
                    .map(|s| s.content.as_ref())
                    .collect::<String>()
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ");

                // 对齐 CC：首行可用宽度要扣掉标签占位，续行用更宽的宽度再包一次，
                // 续行前缀固定 2 空格。标签本身加粗，值用正文色。
                let first_line_width = max_width.saturating_sub(label.width() + 3).max(10);
                // 续行有 2 列缩进前缀（"  "），可用宽度须扣掉 2 列
                let subsequent_width = max_width.saturating_sub(2).max(10);
                let first_pass = wrap_plain(&value, first_line_width);
                let first_line = first_pass.first().cloned().unwrap_or_default();
                let rest = if first_pass.is_empty() {
                    &first_pass[..]
                } else {
                    &first_pass[1..]
                };
                let rest_text = rest.iter().map(|l| l.trim()).collect::<Vec<_>>().join(" ");

                let mut spans = vec![
                    Span::styled(
                        format!("{label}:"),
                        Style::default().add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(" "),
                ];
                if !first_line.is_empty() {
                    // 值不带样式：CC 的纵向格式只加粗标签，值走终端默认色
                    spans.push(Span::raw(first_line.clone()));
                }
                lines.push(Line::from(spans));

                if rest_text.is_empty() {
                    continue;
                }
                let wrapped = if subsequent_width > first_line_width {
                    wrap_plain(&rest_text, subsequent_width)
                } else {
                    rest.to_vec()
                };
                for line in wrapped {
                    if line.trim().is_empty() {
                        continue;
                    }
                    lines.push(Line::from(vec![
                        Span::raw("  ".to_string()),
                        Span::raw(line),
                    ]));
                }
            }
        }
        lines
    }
}

/// 纯文本折行：按空格切词，超长单词（无空格长路径）按宽度硬断。
///
/// 保证纵向表格与长行绝对不溢出终端可视宽度（消除 Windows CMD 物理撕裂）。
fn wrap_plain(text: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![text.to_string()];
    }
    let mut rows: Vec<String> = vec![String::new()];
    let mut row_width = 0usize;
    let mut first = true;
    for word in text.split(' ') {
        let w = word.width();
        if !first && row_width > 0 {
            rows.last_mut().unwrap().push(' ');
            row_width += 1;
        }
        first = false;

        // 超长单词（如没有空格的长路径）超出总可用宽度：硬断开
        if w > width {
            for g in word.graphemes(true) {
                let gw = g.width();
                if row_width > 0 && row_width + gw > width {
                    rows.push(String::new());
                    row_width = 0;
                }
                rows.last_mut().unwrap().push_str(g);
                row_width += gw;
            }
            continue;
        }

        if row_width + w > width && row_width > 0 {
            rows.push(String::new());
            row_width = 0;
        }
        rows.last_mut().unwrap().push_str(word);
        row_width += w;
    }
    rows.into_iter().filter(|r| !r.is_empty()).collect()
}

/// 计算一行的断点（返回该行结束的 grapheme 下标，不含）。
///
/// 严格对齐 `wrap-ansi`（`hard:true, wordWrap:true, trim:false`）的贪心词装箱：
/// - 以 ASCII 空格切词，空格属于「前一个词的尾部」
/// - 当前行放不下该词且行内已有内容 → 换行（词整体翻到下一行）
/// - 词自身宽度就超过列宽 → 在列宽处硬切（CJK / 长 URL 走这条）
fn find_break(g: &[(&str, Style)], start: usize, end: usize, columns: usize) -> usize {
    if start >= end {
        return end;
    }
    let mut row_width = 0usize;
    let mut idx = start;

    while idx < end {
        // 取一个词（含其后紧跟的空格）
        let word_start = idx;
        while idx < end && g[idx].0 != " " && g[idx].0 != "\u{3000}" {
            idx += 1;
        }
        while idx < end && (g[idx].0 == " " || g[idx].0 == "\u{3000}") {
            idx += 1;
        }
        let word_end = idx;
        let word_width: usize = g[word_start..word_end].iter().map(|(t, _)| t.width()).sum();

        // 词比整行还宽：对齐 `wrap-ansi` 的 hard 分支——直接在本行
        // 剩余空间里按列宽硬切，不把词整体推到下一行。
        //
        // 实测（wrap-ansi 10.x）：`- 这是一段很长…` 在 columns=30 时首行
        // 是 `- 这是一段很长很长的列表项内容`，即 bullet 与长词同行。
        // 若在这里先换行，会产出只有 `- ` 的幽灵行。
        if word_width > columns {
            let mut acc = row_width;
            let mut cut = word_start;
            while cut < word_end {
                let w = g[cut].0.width();
                if acc + w > columns && cut > word_start {
                    break;
                }
                acc += w;
                cut += 1;
            }
            return cut.max(word_start + 1);
        }

        if row_width + word_width > columns && row_width > 0 {
            // 放不下：本行到上一个词结束为止
            return word_start;
        }
        row_width += word_width;
    }
    end
}

/// 把已折行的一行单元格内容按对齐方式写入 spans。
fn push_aligned(
    spans: &mut Vec<Span<'static>>,
    content: &[Span<'static>],
    padding: usize,
    align: Alignment,
) {
    match align {
        Alignment::Center => {
            let left = padding / 2;
            let right = padding - left;
            if left > 0 {
                spans.push(Span::raw(" ".repeat(left)));
            }
            spans.extend(content.iter().cloned());
            if right > 0 {
                spans.push(Span::raw(" ".repeat(right)));
            }
        }
        Alignment::Right => {
            if padding > 0 {
                spans.push(Span::raw(" ".repeat(padding)));
            }
            spans.extend(content.iter().cloned());
        }
        Alignment::None | Alignment::Left => {
            spans.extend(content.iter().cloned());
            if padding > 0 {
                spans.push(Span::raw(" ".repeat(padding)));
            }
        }
    }
}

/// 最小列宽（对齐 CC `MIN_COLUMN_WIDTH`）
const MIN_COLUMN_WIDTH: usize = 3;
/// 终端宽度安全边距（对齐 CC `SAFETY_MARGIN`）：预留父级缩进与 resize 竞争
const SAFETY_MARGIN: usize = 4;
/// 超过该行数时切换为纵向 key-value 格式（对齐 CC `MAX_ROW_LINES`）
const MAX_ROW_LINES: usize = 4;

/// 单元格内容显示宽度（不含换行）
fn cell_width(cell: &[Span]) -> usize {
    cell.iter().map(|s| s.content.width()).sum()
}

/// 单元格的最小宽度 = 最长“单词”的显示宽度（下限 `MIN_COLUMN_WIDTH`）。
///
/// 对齐 CC `getMinWidth`：以 ASCII 空格切词后取最宽者，保证折行时不会
/// 把普通单词从中间劈开。
fn cell_min_width(cell: &[Span]) -> usize {
    let text: String = cell.iter().map(|s| s.content.as_ref()).collect();
    text.split(' ')
        .filter(|w| !w.is_empty())
        .map(UnicodeWidthStr::width)
        .max()
        .unwrap_or(MIN_COLUMN_WIDTH)
        .max(MIN_COLUMN_WIDTH)
}

/// 把单元格按 `max_width` 折成多行，逐 grapheme 保留原样式。
///
/// `hard=true` 时超长单词按 grapheme 硬断（对应 `wrap-ansi` 的 hard 模式）；
/// 否则当单词放不下时整体换行，形成单行超宽（由调用方裁剪）。
/// 单元格折行：与旧实现逐 grapheme 等价（词切分、硬断词、去尾空格均不变），
/// 但每行额外返回与 spans 的 grapheme 序列一一对应的 cell-local 输入下标，
/// 供表格链接命中区映射使用。词间分隔空格取其真实输入下标；
/// 折行点丢弃的空格不出现在任何行里。
fn wrap_cell_spans(cell: &[Span], max_width: usize, hard: bool) -> Vec<CellLine> {
    if cell.is_empty() {
        return vec![CellLine {
            spans: Vec::new(),
            src: Vec::new(),
        }];
    }
    // 先按词切分，保留每个 grapheme 的样式与输入下标。
    // 词结构与旧实现（每 span 按 ' ' 切分）完全一致：空词（如连续空格产生）
    // 同样保留，折行时每个非首词前补的空格取该词 sep_before 处的真实空格。
    struct Word {
        items: Vec<(String, Style, usize)>,
        sep_before: Option<usize>,
    }
    let mut words: Vec<Word> = vec![Word {
        items: Vec::new(),
        sep_before: None,
    }];
    let mut idx = 0usize;
    for span in cell {
        let style = span.style;
        // 按 ASCII 空格切词（与 wrap-ansi 的 splitWords 一致）
        for (i, part) in span.content.split(' ').enumerate() {
            if i > 0 {
                // 切分点处的分隔空格，其输入下标就是当前 idx
                words.push(Word {
                    items: Vec::new(),
                    sep_before: Some(idx),
                });
                idx += 1; // 跳过分隔空格
            }
            for g in part.graphemes(true) {
                words
                    .last_mut()
                    .unwrap()
                    .items
                    .push((g.to_string(), style, idx));
                idx += 1;
            }
        }
    }

    fn word_width(w: &[(String, Style, usize)]) -> usize {
        w.iter().map(|(g, _, _)| g.width()).sum()
    }

    let mut rows: Vec<Vec<(String, Style, usize)>> = vec![Vec::new()];
    let mut row_width = 0usize;
    let mut is_first = true;

    for word in words {
        let w_width = word_width(&word.items);
        if !is_first {
            // wrap-ansi：词间补一个空格（trim:false 时行首也补）
            if row_width > 0 {
                // 非首词一定由分隔空格产生，sep_before 必有值；
                // 万一缺失用 usize::MAX 占位，使其不落入任何链接段。
                let sep_src = word.sep_before.unwrap_or(usize::MAX);
                rows.last_mut()
                    .unwrap()
                    .push((" ".to_string(), Style::default(), sep_src));
                row_width += 1;
            }
        }
        is_first = false;

        // hard 模式下超长单词逐 grapheme 断行
        if hard && w_width > max_width {
            for (g, style, src) in word.items {
                let gw = g.width();
                if row_width > 0 && row_width + gw > max_width {
                    rows.push(Vec::new());
                    row_width = 0;
                }
                rows.last_mut().unwrap().push((g, style, src));
                row_width += gw;
            }
            continue;
        }

        if row_width + w_width > max_width && row_width > 0 {
            rows.push(Vec::new());
            row_width = 0;
        }
        for (g, style, src) in word.items {
            rows.last_mut().unwrap().push((g, style, src));
        }
        row_width += w_width;
    }

    // 右侧去尾空格（对齐 wrap-ansi trim !== false 的可见空白裁剪），
    // 再合并相邻同 style 的 grapheme 减少 span 数量；src 与 grapheme 序列对齐。
    let mut out = Vec::with_capacity(rows.len());
    for mut row in rows {
        while matches!(row.last(), Some((g, _, _)) if g == " ") {
            row.pop();
        }
        let mut spans: Vec<Span<'static>> = Vec::new();
        let mut src: Vec<usize> = Vec::new();
        let mut buf = String::new();
        let mut buf_style: Option<Style> = None;
        for (g, style, s) in row {
            if buf_style.is_some() && buf_style != Some(style) {
                spans.push(Span::styled(std::mem::take(&mut buf), buf_style.unwrap()));
                buf_style = Some(style);
            } else if buf_style.is_none() {
                buf_style = Some(style);
            }
            buf.push_str(&g);
            src.push(s);
        }
        if !buf.is_empty() {
            spans.push(Span::styled(buf, buf_style.unwrap_or_default()));
        }
        out.push(CellLine { spans, src });
    }
    if out.is_empty() {
        out.push(CellLine {
            spans: Vec::new(),
            src: Vec::new(),
        });
    }
    out
}

/// 有序列表序号：深度 0/1 用阿拉伯数字，2 用字母，3 用罗马数字。
///
/// 对齐 CC `getListNumber`，让深层嵌套的序号风格一致。
fn format_ordered(n: u64, depth: usize) -> String {
    match depth {
        2 => number_to_letter(n),
        3 => number_to_roman(n),
        _ => n.to_string(),
    }
}

/// 1 → a，26 → z，27 → aa（对齐 CC `numberToLetter`）
fn number_to_letter(mut n: u64) -> String {
    let mut out = String::new();
    while n > 0 {
        n -= 1;
        out.insert(0, (b'a' + (n % 26) as u8) as char);
        n /= 26;
    }
    out
}

/// 罗马数字（小写，对齐 CC `numberToRoman`）
fn number_to_roman(mut n: u64) -> String {
    const VALUES: [(u64, &str); 13] = [
        (1000, "m"),
        (900, "cm"),
        (500, "d"),
        (400, "cd"),
        (100, "c"),
        (90, "xc"),
        (50, "l"),
        (40, "xl"),
        (10, "x"),
        (9, "ix"),
        (5, "v"),
        (4, "iv"),
        (1, "i"),
    ];
    let mut out = String::new();
    for (value, numeral) in VALUES {
        while n >= value {
            out.push_str(numeral);
            n -= value;
        }
    }
    out
}

fn make_border(
    col_widths: &[usize],
    left: char,
    mid: char,
    right: char,
    fill: char,
) -> Span<'static> {
    let mut s = String::new();
    s.push(left);
    for (i, &w) in col_widths.iter().enumerate() {
        for _ in 0..w + 2 {
            s.push(fill);
        }
        if i < col_widths.len() - 1 {
            s.push(mid);
        }
    }
    s.push(right);
    // 边框不带样式：对齐 CC MarkdownTable（纯文本框线，无 SGR）
    Span::raw(s)
}

// ── 渲染状态机 ────────────────────────────────────────────────────────────────

pub(super) struct RenderState<'a> {
    pub lines: Vec<Line<'static>>,
    pub current_spans: Vec<Span<'static>>,
    pub inline_style: Style,
    pub list_stack: Vec<ListState>,
    pub quote_depth: u32,
    pub in_code_block: bool,
    pub code_block_lang: String,
    /// 缓冲多行代码块的所有行，在 TagEnd::CodeBlock 时统一输出
    code_lines: Vec<String>,
    table: Option<TableBuilder>,
    theme: &'a dyn MarkdownTheme,
    max_width: usize,
    /// 当前列表项/引用块的悬挂缩进列数（续行前缀的显示宽度）
    hanging_indent: usize,
    /// 行内样式栈：进入 Strong/Emphasis 时压入外层样式，结束时恢复。
    ///
    /// 必须用「保存/恢复」而非「加/减修饰符」：CC 的样式是嵌套作用域，
    /// 例如标题(BOLD) 里再出现 `**粗**`，退出内层后标题的 BOLD 仍应保留。
    inline_style_stack: Vec<Style>,
    /// 打开中的链接栈（`Start` 已见、`End` 未见）；链接可嵌套（图片包在链接里）
    link_stack: Vec<LinkFrame>,
    /// 是否启用 OSC 8 超链接。关闭时对齐 CC：链接统一退化为纯 URL。
    hyperlinks: bool,
    /// 当前逻辑行（current_spans grapheme 空间）内已闭合的链接段 (g_start, g_end, url)
    line_link_ranges: Vec<(usize, usize, String)>,
    /// 正在跳过图片 alt 文本（图片只输出 URL）
    skip_image_alt: bool,
    /// 是否启用代码块语法高亮（对齐 CC `syntaxHighlightingDisabled` 的反向开关）。
    ///
    /// 关闭时多行代码块不调 syntect、也不套任何前景色，与 CC 的
    /// `highlight === null` 分支一致。
    syntax_highlighting: bool,
    /// 全部链接命中区（输出）
    pub links: Vec<LinkHit>,
    /// 下一个顶层块前是否有源文本空行（由解析器按字节偏移判定后写入）。
    /// 对齐 CC：块间空行只来自源文本，块级元素自身不再无条件补空行。
    pending_blank_before_block: bool,
}

/// 打开中的超链接：记录 URL、起始 grapheme 位置与被覆盖的样式。
struct LinkFrame {
    url: String,
    /// 在 current_spans grapheme 空间中的起点
    start: usize,
    /// 链接开始前的行内样式，End 时恢复
    saved_style: Style,
}

/// 统计 spans 的 grapheme 总数
fn spans_g_len(spans: &[Span<'static>]) -> usize {
    spans
        .iter()
        .map(|s| s.content.as_ref().graphemes(true).count())
        .sum()
}

/// 取 spans 中第 `start` 个 grapheme 起的全部文本。
fn collect_graphemes_from(spans: &[Span<'static>], start: usize) -> String {
    spans
        .iter()
        .flat_map(|s| s.content.as_ref().graphemes(true))
        .skip(start)
        .collect()
}

/// 把 spans 截断到前 `keep` 个 grapheme（按 grapheme 而非 span 边界）。
fn truncate_graphemes(spans: &mut Vec<Span<'static>>, keep: usize) {
    let mut remaining = keep;
    let mut out: Vec<Span<'static>> = Vec::with_capacity(spans.len());
    for span in spans.drain(..) {
        let count = span.content.as_ref().graphemes(true).count();
        if remaining == 0 {
            break;
        }
        if count <= remaining {
            remaining -= count;
            out.push(span);
        } else {
            let text: String = span
                .content
                .as_ref()
                .graphemes(true)
                .take(remaining)
                .collect();
            out.push(Span::styled(text, span.style));
            break;
        }
    }
    *spans = out;
}

/// 预折行输出段：line 为渲染行，其余字段用于链接命中区映射
struct WrappedSeg {
    line: Line<'static>,
    /// 输入 spans 的 grapheme 范围 [in_g_start, in_g_end)
    in_g_start: usize,
    in_g_end: usize,
    /// 输入 in_g_start 映射到输出行 plain_text 的 grapheme 起点（含续行缩进）
    out_g_offset: usize,
}

impl<'a> RenderState<'a> {
    pub fn new(theme: &'a dyn MarkdownTheme) -> Self {
        Self {
            lines: Vec::new(),
            current_spans: Vec::new(),
            inline_style: Style::default(),
            list_stack: Vec::new(),
            quote_depth: 0,
            in_code_block: false,
            code_block_lang: String::new(),
            code_lines: Vec::new(),
            table: None,
            theme,
            max_width: 80, // 默认宽度
            hanging_indent: 0,
            link_stack: Vec::new(),
            inline_style_stack: Vec::new(),
            hyperlinks: false,
            line_link_ranges: Vec::new(),
            skip_image_alt: false,
            syntax_highlighting: true,
            links: Vec::new(),
            pending_blank_before_block: false,
        }
    }

    pub fn with_max_width(mut self, width: usize) -> Self {
        self.max_width = width;
        self
    }

    /// 关闭代码块语法高亮，对齐 CC 的 `syntaxHighlightingDisabled`。
    pub fn with_syntax_highlighting(mut self, enabled: bool) -> Self {
        self.syntax_highlighting = enabled;
        self
    }

    /// 启用 OSC 8 超链接模式。
    ///
    /// 关闭（默认）时对齐 CC：链接显示为纯 URL。开启时保留链接文字并上色，
    /// 由渲染后端在终端输出层补 OSC 8 序列。
    pub fn with_hyperlinks(mut self, enabled: bool) -> Self {
        self.hyperlinks = enabled;
        self
    }

    /// 标记「下一个顶层块前有源文本空行」（由 `parse_markdown_with_links` 按偏移判定）
    pub fn set_pending_blank(&mut self, blank: bool) {
        self.pending_blank_before_block = blank;
    }

    /// 块开始时消费空行标记；列表项/表格内的块不消费。
    ///
    /// 引用块内的块**消费**——CC 在 `> 第一段\n>\n> 第二段` 里会输出一个无 `│`
    /// 前缀的空行。列表项内的空行不渲染（对齐 CC 的 `loose list`）。
    fn take_pending_blank_at_top_level(&mut self) -> bool {
        if self.list_stack.is_empty() && self.table.is_none() {
            std::mem::take(&mut self.pending_blank_before_block)
        } else {
            false
        }
    }

    /// 块级元素开始：消费源文本空行标记并按需插入空行。
    ///
    /// 调用点必须是「列表/表格之外」的块起点；列表、引用块、表格在自增
    /// 自身状态**之前**调用同名判定（见 [`take_pending_blank_at_top_level`]）。
    fn begin_block(&mut self) {
        if self.take_pending_blank_at_top_level() {
            self.ensure_blank_line();
        }
    }

    pub fn flush_line(&mut self) {
        let mut spans = std::mem::take(&mut self.current_spans);

        // 未闭合的链接（跨 SoftBreak）：把本行已产出的部分先登记为一段，
        // 下一行的起点归零，等 End(Link) 时再补齐后续行。
        // 表格内不走这条路：单元格内容不进 current_spans，frame.start 是
        // cell-local 坐标，归零会破坏表格链接段（见 #703）；单元格内的
        // HardBreak（`<br>`）不产生可登记的行，保留帧起点即可。
        if !self.link_stack.is_empty() && self.table.is_none() {
            let g_end = spans_g_len(&spans);
            for frame in self.link_stack.iter_mut() {
                if g_end > frame.start {
                    self.line_link_ranges
                        .push((frame.start, g_end, frame.url.clone()));
                }
                // 后续行从行首起算
                frame.start = 0;
            }
        }
        let mut link_ranges = std::mem::take(&mut self.line_link_ranges);

        // quote 前缀插入后整体 g 坐标右移。
        // 注意：必须先判断内容是否为空，否则空行也会被加上 `▎ ` 前缀，
        // 产生只含竖条的幽灵行。
        if self.quote_depth > 0 && !spans.is_empty() {
            let prefix = "▎ ".repeat(self.quote_depth as usize);
            let prefix_g = prefix.graphemes(true).count();
            for r in &mut link_ranges {
                r.0 += prefix_g;
                r.1 += prefix_g;
            }
            // 对齐 CC `formatToken` 的 blockquote 分支：竖条 `▎` 只加 DIM
            // （chalk.dim 只影响字形、不改前景色），正文整体 ITALIC。
            // 分隔的空格保持无样式。
            for span in spans.iter_mut() {
                span.style = span.style.add_modifier(Modifier::ITALIC);
            }
            let mut bar_spans: Vec<Span<'static>> = Vec::new();
            for _ in 0..self.quote_depth as usize {
                bar_spans.push(Span::styled(
                    "▎".to_string(),
                    Style::default().add_modifier(Modifier::DIM),
                ));
                bar_spans.push(Span::raw(" "));
            }
            let mut all = bar_spans;
            all.extend(spans);
            spans = all;
        }

        if spans.is_empty() {
            self.lines.push(Line::default());
            return;
        }

        // 计算实际的悬挂缩进（列表项或引用块内）
        let indent = if self.hanging_indent > 0 {
            self.hanging_indent
        } else if self.quote_depth > 0 {
            // 引用块：缩进 = 引用前缀的显示宽度
            "▎ ".repeat(self.quote_depth as usize).width()
        } else {
            0
        };

        let base_line = self.lines.len();
        // 对齐 CC：<Ansi> 文本节点走 Ink 的 textWrap='wrap'
        // （wrapAnsi with hard:true），即无论有无缩进都要按 max_width 折行。
        // 缩进 > 0 时续行补悬挂缩进；缩进 == 0 时续行顶格（与 CC 一致）。
        // 首行已含前缀（bullet / 引用条），其宽度从首行可用宽度中扣除。
        // 前缀宽度就是 `indent`：引用场景下它等于 "▎ " * depth 的宽度，
        // 列表场景下等于 bullet 的宽度，两者都在 flush_line 里算好了。
        let wrapped = self.wrap_spans_with_hanging_indent(&spans, indent);
        for (i, seg) in wrapped.into_iter().enumerate() {
            for (gs, ge, url) in &link_ranges {
                let is = (*gs).max(seg.in_g_start);
                let ie = (*ge).min(seg.in_g_end);
                if is < ie {
                    self.links.push(LinkHit {
                        line: base_line + i,
                        g_start: seg.out_g_offset + (is - seg.in_g_start),
                        g_end: seg.out_g_offset + (ie - seg.in_g_start),
                        url: url.clone(),
                        recolor: true,
                    });
                }
            }
            self.lines.push(seg.line);
        }
    }

    /// 将 spans 按 max_width 预折行，续行加悬挂缩进
    ///
    /// 首行直接使用原始 spans（bullet/引用前缀已在其中，且**计入可用宽度**，
    /// 因为 CC 的折行发生在整段 chunk 上）。续行前面插入与首行文字起始位置
    /// 等宽的空格。以 grapheme 为切分单位，返回段级 g 映射用于链接命中区。
    fn wrap_spans_with_hanging_indent(
        &self,
        spans: &[Span<'static>],
        indent: usize,
    ) -> Vec<WrappedSeg> {
        // 先计算首行总显示宽度
        let total_width: usize = spans.iter().map(|s| s.content.width()).sum();
        let total_g = spans_g_len(spans);
        if total_width <= self.max_width {
            // 不需要折行
            return vec![WrappedSeg {
                line: Line::from(spans.to_vec()),
                in_g_start: 0,
                in_g_end: total_g,
                out_g_offset: 0,
            }];
        }

        let max_w = self.max_width;

        // 将所有 spans 展平为 (grapheme, Style) 序列
        let mut g_with_style: Vec<(&str, Style)> = Vec::new();
        for span in spans {
            for g in span.content.graphemes(true) {
                g_with_style.push((g, span.style));
            }
        }

        let mut result: Vec<WrappedSeg> = Vec::new();
        let mut idx = 0;
        let total_gs = g_with_style.len();
        let mut is_first_line = true;

        while idx < total_gs {
            let line_start = idx;
            // 首行已含前缀（bullet/竖条）且前缀算在可用宽度里——
            // CC 的折行发生在整段 chunk 上，前缀同样占列宽。
            // 只有续行才需要为悬挂缩进预留空间。
            let content_max = if is_first_line {
                max_w
            } else {
                max_w.saturating_sub(indent)
            }
            .max(1);

            // 累加 grapheme 宽度，找到折行点。
            //
            // 语义严格对齐 `wrap-ansi`（CC 的折行实现，hard:true, wordWrap:true）：
            // - 以「词」（空格分隔）为单位贪心装箱
            // - 词本身宽度超过列宽时**在列宽处硬切**，不回头找空格
            //   （这是 CJK/长 URL 场景的关键：否则前缀空格会让首行只剩前缀）
            // - 空格留在行尾（trim:false）
            let mut break_idx = find_break(&g_with_style, line_start, total_gs, content_max);

            // 确保至少消费一个 grapheme（防止死循环）
            if break_idx == line_start && break_idx < total_gs {
                break_idx = line_start + 1;
            }

            // 构建本行的 spans：相邻同 style 的 grapheme 合并为一个 span
            let mut line_spans: Vec<Span<'static>> = Vec::new();

            // 续行加缩进 padding（记录到 out_g_offset）
            let mut out_g_offset = 0usize;
            if !is_first_line {
                if self.quote_depth > 0 && self.hanging_indent == 0 {
                    // 引用块续行：重新添加竖条前缀（只 DIM，不带前景色）
                    let prefix = "▎ ".repeat(self.quote_depth as usize);
                    out_g_offset = prefix.graphemes(true).count();
                    for _ in 0..self.quote_depth {
                        line_spans.push(Span::styled(
                            "▎".to_string(),
                            Style::default().add_modifier(Modifier::DIM),
                        ));
                        line_spans.push(Span::raw(" "));
                    }
                } else {
                    let padding = " ".repeat(indent);
                    out_g_offset = indent;
                    line_spans.push(Span::styled(padding, Style::default()));
                }
            }

            // 合并同 style grapheme 为 span
            let mut buf = String::new();
            let mut buf_style: Option<Style> = None;
            for &(g, style) in &g_with_style[line_start..break_idx] {
                if buf_style.is_some() && buf_style != Some(style) {
                    line_spans.push(Span::styled(std::mem::take(&mut buf), buf_style.unwrap()));
                    buf_style = Some(style);
                } else if buf_style.is_none() {
                    buf_style = Some(style);
                }
                buf.push_str(g);
            }
            if !buf.is_empty() {
                line_spans.push(Span::styled(buf, buf_style.unwrap_or_default()));
            }

            if !line_spans.is_empty() {
                result.push(WrappedSeg {
                    line: Line::from(line_spans),
                    in_g_start: line_start,
                    in_g_end: break_idx,
                    out_g_offset,
                });
            }

            idx = break_idx;
            is_first_line = false;
        }

        result
    }

    /// 确保与上一个输出行之间有一个空行（去重：如果上一行已是空行则跳过）
    fn ensure_blank_line(&mut self) {
        if !self.lines.is_empty() && !self.lines.last().unwrap().spans.is_empty() {
            self.lines.push(Line::default());
        }
    }

    /// 把 span 写到当前上下文：表格内写进当前单元格，否则写进当前行。
    ///
    /// 行内元素（链接、图片、行内代码）都必须走这里，否则表格内的内联内容
    /// 会被吐到表格外面，破坏表格结构。
    fn push_inline(&mut self, span: Span<'static>) {
        match self.table.as_mut() {
            Some(table) => table.current_cell.push(span),
            None => self.current_spans.push(span),
        }
    }

    /// 当前内联上下文的 grapheme 长度（表格内用单元格，否则用 current_spans）。
    fn inline_g_len(&self) -> usize {
        match self.table.as_ref() {
            Some(table) => spans_g_len(&table.current_cell),
            None => spans_g_len(&self.current_spans),
        }
    }

    /// 截断当前内联上下文到前 `keep` 个 grapheme。
    fn truncate_inline(&mut self, keep: usize) {
        match self.table.as_mut() {
            Some(table) => truncate_graphemes(&mut table.current_cell, keep),
            None => truncate_graphemes(&mut self.current_spans, keep),
        }
    }

    /// 取当前内联上下文从第 `start` 个 grapheme 起的文本。
    fn inline_text_from(&self, start: usize) -> String {
        match self.table.as_ref() {
            Some(table) => collect_graphemes_from(&table.current_cell, start),
            None => collect_graphemes_from(&self.current_spans, start),
        }
    }

    /// 处理事件，并携带该事件在源文本中的原始片段。
    ///
    /// 用于还原 pulldown 会改写的文本：HTML entity（`&amp;` → `&`）。
    /// CC（marked）不做 entity 解码，输出的是字面量，因此这里优先用源片段。
    pub fn handle_event_with_source(&mut self, event: Event<'_>, source: Option<&str>) {
        // 仅对 Text 事件做 entity 还原；其它事件直接委派
        if let Event::Text(ref text) = event {
            if let Some(raw) = source {
                // 只有当源片段含有 `&`（可能是 entity）且与解码结果不同时才替换
                if raw.contains('&') && raw != text.as_ref() && !self.in_code_block {
                    let raw = raw.to_string();
                    if self.skip_image_alt {
                        return;
                    }
                    let style = self.inline_style;
                    self.push_inline(Span::styled(raw, style));
                    return;
                }
            }
            // 表格内的纯文本同样要带 inline_style
            if self.table.is_some() && !self.in_code_block && !self.skip_image_alt {
                let style = self.inline_style;
                let s = text.to_string();
                self.push_inline(Span::styled(s, style));
                return;
            }
        }
        self.handle_event(event);
    }

    pub fn handle_event(&mut self, event: Event<'_>) {
        match event {
            // ── 标题 ─────────────────────────────────────────────────────────
            // 对齐 CC `formatToken` 的 `heading` 分支：标题只靠字形属性区分层级，
            // 不带主题色。H1 = bold+italic+underline，H2+ = bold（CC 里 h2 与 h3+
            // 是同一档，不按深度分色）。
            Event::Start(Tag::Heading { level, .. }) => {
                let mut style = Style::default().add_modifier(Modifier::BOLD);
                if level == HeadingLevel::H1 {
                    style = style.add_modifier(Modifier::ITALIC | Modifier::UNDERLINED);
                }
                self.inline_style = style;
                // 标题前添加空行分隔（去重）。行首不能因空 spans 调 flush_line——
                // 它在 spans 为空时会无条件插入一个空行，导致消息开头多一行留白
                if !self.current_spans.is_empty() {
                    self.flush_line();
                }
                // 对齐 CC：标题前是否留空行取决于源文本（CC 的 space token）
                self.begin_block();
            }
            Event::End(TagEnd::Heading(_)) => {
                self.inline_style = Style::default();
                self.flush_line();
                // 标题后添加空行分隔（去重）
                self.ensure_blank_line();
            }

            // ── 段落 ─────────────────────────────────────────────────────────
            Event::Start(Tag::Paragraph) => {
                // 段落前空行只来自源文本
                self.begin_block();
            }
            Event::End(TagEnd::Paragraph) => {
                // 对齐 CC：段落本身只结束当前行，块间空行由下一个块按源文本补
                self.flush_line();
            }

            // ── 代码块 ────────────────────────────────────────────────────────
            Event::Start(Tag::CodeBlock(kind)) => {
                // 先清理前一段落可能残留的 spans，避免首行粘黏
                if !self.current_spans.is_empty() {
                    self.flush_line();
                }
                // 代码块前的空行只来自源文本
                self.begin_block();
                self.in_code_block = true;
                self.code_block_lang = match kind {
                    CodeBlockKind::Fenced(lang) => lang.into_string(),
                    CodeBlockKind::Indented => String::new(),
                };
                self.code_lines.clear();
            }
            Event::End(TagEnd::CodeBlock) => {
                self.in_code_block = false;
                let lines = std::mem::take(&mut self.code_lines);

                // 过滤尾部空行
                let mut end = lines.len();
                while end > 0 && lines[end - 1].is_empty() {
                    end -= 1;
                }
                if end == 0 {
                    return;
                }

                // 语法高亮关闭时（`syntax_highlighting_disabled`）对齐 CC 的
                // `highlight === null` 分支：代码块原样输出、完全不带样式。
                if !self.syntax_highlighting {
                    for line_text in &lines[..end] {
                        self.current_spans.push(Span::raw(line_text.clone()));
                        self.flush_line();
                    }
                    return;
                }

                if end == 1 {
                    // 单行代码块：只上色，不要语言标签与 │ 前缀
                    self.current_spans.push(Span::styled(
                        lines[0].clone(),
                        Style::default().fg(self.theme.code()),
                    ));
                    self.flush_line();
                } else {
                    #[cfg(feature = "markdown-highlight")]
                    if let Some(highlighted) = highlight_code_block(
                        &self.code_block_lang,
                        &lines[..end],
                        self.theme.syntax_theme_name(),
                    ) {
                        self.lines.extend(highlighted);
                    } else {
                        // 语言未识别，回退到统一正文色
                        for line_text in &lines[..end] {
                            self.current_spans.push(Span::styled(
                                line_text.clone(),
                                Style::default().fg(self.theme.text()),
                            ));
                            self.flush_line();
                        }
                    }

                    #[cfg(not(feature = "markdown-highlight"))]
                    for line_text in &lines[..end] {
                        self.current_spans.push(Span::styled(
                            line_text.clone(),
                            Style::default().fg(self.theme.text()),
                        ));
                        self.flush_line();
                    }
                }
            }

            // ── 列表 ─────────────────────────────────────────────────────────
            Event::Start(Tag::List(start)) => {
                let list_type = match start {
                    Some(n) => ListType::Ordered(n),
                    None => ListType::Unordered,
                };
                // 顶层列表前的空行只来自源文本（必须在 push 之前判断顶层）
                let blank_before = self.take_pending_blank_at_top_level();
                self.list_stack.push(ListState { list_type });
                if self.list_stack.len() == 1 && blank_before {
                    self.ensure_blank_line();
                }
            }
            Event::End(TagEnd::List(_)) => {
                self.list_stack.pop();
                // 对齐 CC：列表后不补空行，由下一个块的源文本判断
            }
            Event::Start(Tag::Item) => {
                // 嵌套列表：外层项的内容尚未换行时先结束该行，
                // 否则内层 bullet 会与外层文字挤在同一行（CC 的每项以 EOL 结尾，不会挤行）
                if !self.current_spans.is_empty() {
                    self.flush_line();
                }
                let depth = self.list_stack.len().saturating_sub(1);
                let indent = "  ".repeat(depth);
                // 有序列表序号随深度换制：0/1 → 数字，2 → 字母，3 → 罗马（对齐 CC
                // `getListNumber`）。无序列表用 `- `（CC `formatToken` 的 text 分支）。
                let bullet = if let Some(state) = self.list_stack.last_mut() {
                    match &mut state.list_type {
                        ListType::Unordered => format!("{indent}- "),
                        ListType::Ordered(n) => {
                            let s = format!("{}{}. ", indent, format_ordered(*n, depth));
                            *n += 1;
                            s
                        }
                    }
                } else {
                    format!("{indent}- ")
                };
                // 计算悬挂缩进宽度 = bullet 前缀的显示宽度。
                // 符号本身不带色：对齐 CC `formatToken`，bullet 与 `- `/`n. `
                // 都拼在纯文本里，没有任何 SGR 包裹。
                self.hanging_indent = bullet.width();
                self.current_spans.push(Span::raw(bullet));
            }
            Event::End(TagEnd::Item) => {
                if !self.current_spans.is_empty() {
                    self.flush_line();
                }
                self.hanging_indent = 0;
            }

            // ── 引用块 ────────────────────────────────────────────────────────
            Event::Start(Tag::BlockQuote(_)) => {
                // 列表项内嵌引用块：bullet 已写入 current_spans 但尚未换行，
                // 必须先结束该行，否则会出现 `▎ - itemquote` 这种粘连。
                if !self.current_spans.is_empty() {
                    self.flush_line();
                }
                // 顶层引用块前的空行只来自源文本（须在 quote_depth 自增前判断）
                let blank_before = self.take_pending_blank_at_top_level();
                self.quote_depth += 1;
                if self.quote_depth == 1 && blank_before {
                    self.ensure_blank_line();
                }
            }
            Event::End(TagEnd::BlockQuote(_)) if self.quote_depth > 0 => {
                self.quote_depth -= 1;
                // 对齐 CC：引用块后不补空行，由下一个块的源文本判断
            }

            // ── 水平线 ────────────────────────────────────────────────────────
            Event::Rule => {
                // hr 前的空行来自源文本（CC 的 space token）
                self.begin_block();
                // 对齐 CC `formatToken` 的 `hr` 分支：固定输出 `---`，不带样式。
                self.current_spans.push(Span::raw("---"));
                self.flush_line();
                // CC 的 hr token 不带 EOL、下个块也不补空行，会把后文粘成
                // `---文字`。这是 CC 的缺陷（已在对拍测试中登记为偏离），
                // 这里保留 peri 的块分隔行为。
            }

            // ── 文本（含代码块内容） ───────────────────────────────────────────
            Event::Text(text) => {
                if self.skip_image_alt {
                    return;
                }
                let text_str = text.into_string();
                if self.in_code_block {
                    // 缓冲所有行，等 TagEnd::CodeBlock 时统一输出
                    for line in text_str.split('\n') {
                        self.code_lines.push(line.to_string());
                    }
                } else {
                    // 表格内走 push_inline，否则写 current_spans
                    let style = self.inline_style;
                    self.push_inline(Span::styled(text_str, style));
                }
            }

            // ── 内联 HTML ────────────────────────────────────────────────────
            // 对齐 CC：`formatToken` 的 `html` 分支返回 ''，标签整体丢弃，
            // 只保留标签之间的文字（`a <b>x</b> c` → `a x c`）。
            Event::InlineHtml(_) => {}

            // ── 块级 HTML ────────────────────────────────────────────────────
            // 对齐 CC：块级 HTML 同样整段丢弃。CC 对纯 HTML 文档仍输出
            // 一个空行（chunk trim 后 join 的结果），这里保持一致。
            Event::Html(_) => {
                if self.current_spans.is_empty() && self.lines.is_empty() {
                    self.lines.push(Line::default());
                }
            }

            // ── 行内代码 ──────────────────────────────────────────────────────
            // 对齐 CC：`codespan` 只覆盖前景色，外层字形（标题 BOLD、
            // 斜体 ITALIC 等）继续生效 —— CC 的 `color('permission')(text)`
            // 是嵌套在 `chalk.bold(...)` 内部输出的，属性会叠加。
            Event::Code(text) => {
                let style = self.inline_style.fg(self.theme.code());
                self.push_inline(Span::styled(text.into_string(), style));
            }

            // ── Strong / Emphasis ────────────────────────────────────────────
            // 注意：删除线不在这里渲染。CC 通过禁用 marked 的 `del` tokenizer
            // 主动关掉删除线（模型常把 `~100` 当“约等于”用），peri 侧在
            // `parse_markdown_with_links` 关闭 ENABLE_STRIKETHROUGH 达到同样效果，
            // 因此不会收到 Strikethrough 事件。
            Event::Start(Tag::Strong) => {
                // 压栈保存外层样式：标题是 BOLD，内层再套 BOLD 时，
                // End 只应撤掉内层这一层，不能把标题的 BOLD 一起删掉。
                self.inline_style_stack.push(self.inline_style);
                self.inline_style = self.inline_style.add_modifier(Modifier::BOLD);
            }
            Event::End(TagEnd::Strong) => {
                if let Some(prev) = self.inline_style_stack.pop() {
                    self.inline_style = prev;
                } else {
                    self.inline_style = self.inline_style.remove_modifier(Modifier::BOLD);
                }
            }
            Event::Start(Tag::Emphasis) => {
                self.inline_style_stack.push(self.inline_style);
                self.inline_style = self.inline_style.add_modifier(Modifier::ITALIC);
            }
            Event::End(TagEnd::Emphasis) => {
                if let Some(prev) = self.inline_style_stack.pop() {
                    self.inline_style = prev;
                } else {
                    self.inline_style = self.inline_style.remove_modifier(Modifier::ITALIC);
                }
            }

            // ── 图片 ─────────────────────────────────────────────────────────
            // 对齐 CC `formatToken` 的 `image` 分支：输出 URL 而不是 alt 文本
            // （终端无法显示图片，URL 至少可复制/可点）。
            Event::Start(Tag::Image { dest_url, .. }) => {
                let url = dest_url.into_string();
                let style = self.inline_style;
                self.push_inline(Span::styled(url, style));
                // alt 文本只是占位，跳过其内容
                self.skip_image_alt = true;
            }
            Event::End(TagEnd::Image) => {
                self.skip_image_alt = false;
            }

            // ── 链接 ─────────────────────────────────────────────────────────
            // 对齐 CC `formatToken` 的 `link` 分支：
            // - `mailto:` 只显示邮箱地址
            // - 有独立显示文本且不等于 URL → 显示文本（支持 OSC8 时）
            // - 否则显示 URL
            // CC 在不支持 OSC8 时统一退化为**纯 URL**（无颜色、无下划线），
            // 因此这里只在 `hyperlinks` 打开时才给链接文字上色。
            Event::Start(Tag::Link { dest_url, .. }) => {
                self.link_stack.push(LinkFrame {
                    url: dest_url.into_string(),
                    start: self.inline_g_len(),
                    saved_style: self.inline_style,
                });
                if self.hyperlinks {
                    self.inline_style = self
                        .inline_style
                        // 链接颜色由终端层统一设置为蓝色。
                        // .fg(self.theme.link())
                        .add_modifier(Modifier::UNDERLINED);
                }
            }
            Event::End(TagEnd::Link) => {
                let Some(frame) = self.link_stack.pop() else {
                    return;
                };
                self.inline_style = frame.saved_style;
                let url = frame.url;
                let g_end = self.inline_g_len();
                let has_text = g_end > frame.start;
                let in_table = self.table.is_some();

                // 取链接文字：frame.start 是当前内联上下文的 grapheme 偏移
                let link_text = self.inline_text_from(frame.start);

                // mailto: 直接显示邮箱，不加链接
                if let Some(email) = url.strip_prefix("mailto:") {
                    self.truncate_inline(frame.start);
                    self.push_inline(Span::raw(email.to_string()));
                    return;
                }

                // 表格内同样登记：frame.start / g_end 是 cell-local 坐标，
                // 随 current_cell 进 TableBuilder，折行映射见 render_with_wrap。
                if link_text == url || !self.hyperlinks || !has_text {
                    // 退化为纯 URL：截断已有链接文字，写入 URL 本身
                    self.truncate_inline(frame.start);
                    let start_g = frame.start;
                    let end_g = start_g + url.graphemes(true).count();
                    self.push_inline(Span::raw(url.clone()));
                    if has_text {
                        if in_table {
                            if let Some(tb) = self.table.as_mut() {
                                tb.current_cell_links.push((start_g, end_g, url));
                            }
                        } else {
                            self.line_link_ranges.push((start_g, end_g, url));
                        }
                    }
                    return;
                }
                // 有 OSC8：保留链接文字样式，并记录点击命中区。
                // 跨行时 flush_line 已按行登记过前几段，这里只补最后一段。
                if g_end > frame.start {
                    if in_table {
                        if let Some(tb) = self.table.as_mut() {
                            tb.current_cell_links.push((frame.start, g_end, url));
                        }
                    } else {
                        self.line_link_ranges.push((frame.start, g_end, url));
                    }
                }
            }

            // ── 表格 ─────────────────────────────────────────────────────────
            Event::Start(Tag::Table(alignments)) => {
                // 表格前的空行只来自源文本（table 尚未建立，可判断顶层）
                self.begin_block();
                self.table = Some(TableBuilder::new(alignments));
            }
            Event::End(TagEnd::Table) => {
                if let Some(tb) = self.table.take() {
                    // 表格内命中区是相对行号，换算成全文行号
                    let base = self.lines.len();
                    let (table_lines, mut table_links) =
                        tb.render_with_wrap(self.max_width, self.theme);
                    for link in &mut table_links {
                        link.line += base;
                    }
                    self.lines.extend(table_lines);
                    self.links.extend(table_links);
                }
            }
            Event::Start(Tag::TableHead) => {
                if let Some(tb) = self.table.as_mut() {
                    tb.in_head = true;
                }
            }
            Event::End(TagEnd::TableHead) => {
                if let Some(tb) = self.table.as_mut() {
                    // 每个 TableCell End 已 push_cell，这里不能再 push，
                    // 否则行尾会多出一个空 cell（表现为多余空列）
                    tb.push_row();
                    tb.in_head = false;
                }
            }
            Event::Start(Tag::TableRow) => {}
            Event::End(TagEnd::TableRow) => {
                if let Some(tb) = self.table.as_mut() {
                    // 同 TableHead：cell 已在 TableCell End 时 push，只 push_row
                    tb.push_row();
                }
            }
            Event::Start(Tag::TableCell) => {}
            Event::End(TagEnd::TableCell) => {
                if let Some(tb) = self.table.as_mut() {
                    tb.push_cell();
                }
            }

            // ── 换行 ─────────────────────────────────────────────────────────
            Event::SoftBreak => {
                // 对齐 CC：软换行保留为换行（CC 的 text token 含 `\n`，formatToken
                // 原样输出）。合并成空格会让多行段落塌成一行。
                if !self.current_spans.is_empty() {
                    self.flush_line();
                }
            }
            Event::HardBreak => {
                self.flush_line();
            }

            _ => {}
        }
    }
}
