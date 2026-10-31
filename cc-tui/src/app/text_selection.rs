use unicode_segmentation::UnicodeSegmentation;

/// 仅保留本帧可见行的命中数据，不保留渲染成品；后台发布新窗口不能改变已显示内容的坐标。
pub struct PresentedSelectionSnapshot {
    pub layout: std::sync::Arc<crate::ui::render_thread::VirtualLayoutSnapshot>,
    pub wrap_map: Vec<crate::ui::render_thread::WrappedLineInfo>,
    pub links: Vec<cc_widgets::markdown::LinkHit>,
    pub width: u16,
}

impl PresentedSelectionSnapshot {
    pub fn selection_anchor_at(
        &self,
        row: usize,
        column: u16,
    ) -> Option<crate::ui::render_thread::SelectionAnchor> {
        selection_anchor_from_parts(&self.layout, &self.wrap_map, row, column, self.width)
    }

    pub fn link_at_selection_anchor(
        &self,
        anchor: crate::ui::render_thread::SelectionAnchor,
    ) -> Option<String> {
        link_from_parts(&self.layout, &self.links, anchor)
    }
}

pub(crate) fn selection_anchor_from_parts(
    layout: &crate::ui::render_thread::VirtualLayoutSnapshot,
    wrap_map: &[crate::ui::render_thread::WrappedLineInfo],
    row: usize,
    column: u16,
    width: u16,
) -> Option<crate::ui::render_thread::SelectionAnchor> {
    let (line_index, grapheme_offset) = visual_to_logical(row, column, wrap_map, width)?;
    let &(message_index, logical_line) = layout.line_origins.get(line_index)?;
    Some(crate::ui::render_thread::SelectionAnchor {
        history_epoch: layout.history_epoch,
        message_index,
        message_version: *layout.message_versions.get(message_index)?,
        logical_line,
        grapheme_offset,
    })
}

pub(crate) fn link_from_parts(
    layout: &crate::ui::render_thread::VirtualLayoutSnapshot,
    links: &[cc_widgets::markdown::LinkHit],
    anchor: crate::ui::render_thread::SelectionAnchor,
) -> Option<String> {
    if layout.history_epoch != anchor.history_epoch
        || layout.message_versions.get(anchor.message_index) != Some(&anchor.message_version)
    {
        return None;
    }
    let line = layout
        .line_origins
        .iter()
        .position(|&origin| origin == (anchor.message_index, anchor.logical_line))?;
    links
        .iter()
        .find(|hit| {
            hit.line == line
                && anchor.grapheme_offset >= hit.g_start
                && anchor.grapheme_offset < hit.g_end
        })
        .map(|hit| hit.url.clone())
}

/// 返回指定原始消息行中被选中的字素范围。
pub fn anchored_selection_columns(
    start: crate::ui::render_thread::SelectionAnchor,
    end: crate::ui::render_thread::SelectionAnchor,
    message_index: usize,
    logical_line: usize,
) -> Option<(usize, usize)> {
    if start.history_epoch != end.history_epoch {
        return None;
    }
    let key = |anchor: crate::ui::render_thread::SelectionAnchor| {
        (
            anchor.message_index,
            anchor.logical_line,
            anchor.grapheme_offset,
        )
    };
    let (start, end) = if key(start) <= key(end) {
        (start, end)
    } else {
        (end, start)
    };
    let line = (message_index, logical_line);
    if line < (start.message_index, start.logical_line)
        || line > (end.message_index, end.logical_line)
    {
        return None;
    }
    let from = if line == (start.message_index, start.logical_line) {
        start.grapheme_offset
    } else {
        0
    };
    let to = if line == (end.message_index, end.logical_line) {
        end.grapheme_offset
    } else {
        usize::MAX
    };
    (from < to).then_some((from, to))
}

/// 文本选区状态
#[derive(Debug, Clone)]
pub struct TextSelection {
    /// 选区起始视觉坐标（相对于消息区域左上角）
    pub start: Option<(usize, u16)>, // (visual_row, visual_col)
    /// 选区结束视觉坐标
    pub end: Option<(usize, u16)>,
    /// 是否正在拖拽中
    pub dragging: bool,
    /// 选区对应的纯文本内容（松开鼠标后计算）
    pub selected_text: Option<String>,
    /// 虚拟布局中的内容坐标，不受前方估算高度校正影响。
    pub anchor_start: Option<crate::ui::render_thread::SelectionAnchor>,
    pub anchor_end: Option<crate::ui::render_thread::SelectionAnchor>,
    /// 点击新位置或清空时递增，避免异步复制覆盖后来的选区。
    pub generation: u64,
}

impl Default for TextSelection {
    fn default() -> Self {
        Self::new()
    }
}

impl TextSelection {
    pub fn new() -> Self {
        Self {
            start: None,
            end: None,
            dragging: false,
            selected_text: None,
            anchor_start: None,
            anchor_end: None,
            generation: 0,
        }
    }

    /// 开始拖拽：记录起始坐标，清除旧选区
    pub fn start_drag(&mut self, row: usize, col: u16) {
        self.generation = self.generation.wrapping_add(1);
        self.anchor_start = None;
        self.anchor_end = None;
        self.start = Some((row, col));
        self.end = Some((row, col));
        self.dragging = true;
        self.selected_text = None;
    }

    /// 更新拖拽：更新结束坐标
    pub fn update_drag(&mut self, row: usize, col: u16) {
        if self.dragging {
            self.end = Some((row, col));
        }
    }

    /// 结束拖拽：标记拖拽结束，selected_text 由外部计算后通过 set_selected_text 设置
    pub fn end_drag(&mut self) {
        self.dragging = false;
    }

    /// 设置提取后的选区文本
    pub fn set_selected_text(&mut self, text: Option<String>) {
        self.selected_text = text;
    }

    /// 清除选区（鼠标点击非拖拽、复制后、resize 后调用）
    pub fn clear(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.anchor_start = None;
        self.anchor_end = None;
        self.start = None;
        self.end = None;
        self.dragging = false;
        self.selected_text = None;
    }

    /// 是否有活跃的选区（正在拖拽或已选中文字）
    pub fn is_active(&self) -> bool {
        self.dragging
            || self.selected_text.is_some()
            || self
                .anchor_start
                .zip(self.anchor_end)
                .is_some_and(|(a, b)| a != b)
    }
}

/// 面板文字选区状态（用于 thread_browser / agent / cron 等列表面板）
#[derive(Debug, Clone)]
pub struct PanelTextSelection {
    /// 选区起始坐标（内容空间：row 已包含 scroll offset）
    pub start: Option<(u16, u16)>, // (content_row, col)
    /// 选区结束坐标
    pub end: Option<(u16, u16)>,
    /// 是否正在拖拽中
    pub dragging: bool,
    /// 选区对应的纯文本内容
    pub selected_text: Option<String>,
}

impl Default for PanelTextSelection {
    fn default() -> Self {
        Self::new()
    }
}

impl PanelTextSelection {
    pub fn new() -> Self {
        Self {
            start: None,
            end: None,
            dragging: false,
            selected_text: None,
        }
    }

    pub fn start_drag(&mut self, row: u16, col: u16) {
        self.start = Some((row, col));
        self.end = Some((row, col));
        self.dragging = true;
        self.selected_text = None;
    }

    pub fn update_drag(&mut self, row: u16, col: u16) {
        if self.dragging {
            self.end = Some((row, col));
        }
    }

    pub fn end_drag(&mut self) {
        self.dragging = false;
    }

    pub fn set_selected_text(&mut self, text: Option<String>) {
        self.selected_text = text;
    }

    pub fn clear(&mut self) {
        self.start = None;
        self.end = None;
        self.dragging = false;
        self.selected_text = None;
    }

    pub fn is_active(&self) -> bool {
        self.dragging || self.selected_text.is_some()
    }
}

/// 从面板纯文本行中提取选区文本（字符级精度）。
/// start/end 为内容空间坐标 (content_row, col)。
/// 自动处理 start > end 的情况。
pub fn extract_panel_text(
    start: (u16, u16),
    end: (u16, u16),
    plain_lines: &[String],
) -> Option<String> {
    let ((sr, sc), (er, ec)) = if start <= end {
        (start, end)
    } else {
        (end, start)
    };
    let sr = sr as usize;
    let er = er as usize;
    if sr >= plain_lines.len() {
        return None;
    }
    let er = er.min(plain_lines.len() - 1);

    let mut parts: Vec<String> = Vec::new();
    for (i, text) in plain_lines.iter().enumerate().take(er + 1).skip(sr) {
        if sr == er {
            // 同一行
            let b_start = grapheme_to_byte_idx(text, sc as usize);
            let b_end = grapheme_to_byte_idx(text, ec as usize);
            if b_start >= b_end {
                return None;
            }
            parts.push(text[b_start..b_end].to_string());
        } else if i == sr {
            // 首行：从 sc 到行尾
            let b_start = grapheme_to_byte_idx(text, sc as usize);
            parts.push(text[b_start..].to_string());
        } else if i == er {
            // 末行：从行首到 ec
            let b_end = grapheme_to_byte_idx(text, ec as usize);
            parts.push(text[..b_end].to_string());
        } else {
            // 中间行：整行
            parts.push(text.clone());
        }
    }

    if parts.is_empty() {
        None
    } else {
        Some(parts.join("\n"))
    }
}

/// 在 char_widths（grapheme 级别）中，模拟 ratatui WordWrapper 的 word-break 算法，
/// 定位到第 row_in_line 个视觉行的起始 grapheme 偏移，然后在该视觉行内累积宽度到 visual_col，
/// 返回 grapheme 偏移量。
///
/// word-break 规则（与 ratatui WordWrapper 一致，trim=false）：
/// - 空白跟随前一个 word 到同一行
/// - 当 line_width + whitespace_width + word_width > usable_width 时换行
/// - 当一个 word 本身超过行宽时，在行宽处硬断
fn char_col_to_offset(
    char_widths: &[u8],
    visual_col: u16,
    row_in_line: usize,
    usable_width: u16,
) -> usize {
    let uw = usable_width as usize;
    if uw == 0 || char_widths.is_empty() {
        return 0;
    }

    let target = visual_col as usize;
    let mut current_row: usize = 0;
    let mut line_width: usize = 0;
    let mut word_width: usize = 0;
    let mut whitespace_width: usize = 0;
    let mut non_ws_previous = false;
    let mut in_target_row = row_in_line == 0;

    for (i, &w) in char_widths.iter().enumerate() {
        let w = w as usize;
        let is_whitespace = w == 0; // 零宽字符视为空白

        // 跳过比行宽还宽的 grapheme
        if w > uw {
            non_ws_previous = false;
            continue;
        }

        let word_found = non_ws_previous && is_whitespace;
        let untrimmed_overflow = line_width == 0 && word_width + whitespace_width + w > uw;

        if word_found || untrimmed_overflow {
            line_width += whitespace_width + word_width;
            whitespace_width = 0;
            word_width = 0;
        }

        let line_full = line_width >= uw;
        let pending_overflow = w > 0 && line_width + whitespace_width + word_width >= uw;

        if line_full || pending_overflow {
            if in_target_row {
                break;
            }
            current_row += 1;
            line_width = 0;
            whitespace_width = 0;
            if current_row == row_in_line {
                in_target_row = true;
            }
            if is_whitespace {
                continue;
            }
        }

        if is_whitespace {
            whitespace_width += w;
        } else {
            word_width += w;
        }

        if in_target_row {
            let effective = line_width + whitespace_width + word_width;
            if effective > target {
                return i;
            }
        }

        non_ws_previous = !is_whitespace;
    }

    char_widths.len()
}

/// 将视觉坐标 (visual_row, visual_col) 通过 wrap_map 映射为 (line_idx, grapheme_offset)。
/// `usable_width` 为消息区域可用宽度（text_area.width）。
pub fn visual_to_logical(
    visual_row: usize,
    visual_col: u16,
    wrap_map: &[crate::ui::render_thread::WrappedLineInfo],
    usable_width: u16,
) -> Option<(usize, usize)> {
    let idx = wrap_map.partition_point(|info| info.visual_row_end <= visual_row);
    if idx >= wrap_map.len() {
        return None;
    }
    let info = &wrap_map[idx];
    if visual_row < info.visual_row_start {
        return None;
    }
    let row_in_line = visual_row - info.visual_row_start;
    let char_offset = char_col_to_offset(&info.char_widths, visual_col, row_in_line, usable_width);
    Some((info.line_idx, char_offset))
}

/// 将 grapheme 索引转换为字节索引，用于安全切割 String。
/// `grapheme_idx` 是 text 中的 grapheme 位置（从 0 开始）。
/// 返回对应的 byte 偏移量。如果 grapheme_idx 超出 grapheme 数，返回 text.len()。
fn grapheme_to_byte_idx(text: &str, grapheme_idx: usize) -> usize {
    text.grapheme_indices(true)
        .nth(grapheme_idx)
        .map(|(i, _)| i)
        .unwrap_or(text.len())
}

/// 根据选区起止坐标从 wrap_map 的 plain_text 提取文本（字符级精度）。
/// 自动处理 start > end 的情况（swap）。
/// 首行从 start_col 对应的字符位置截取，末行到 end_col 对应的字符位置截取，中间行整行。
/// 所有 char offset 通过 grapheme_to_byte_idx 转为 byte 索引后切割，保证 unicode 安全。
pub fn extract_selected_text(
    start: (usize, u16),
    end: (usize, u16),
    wrap_map: &[crate::ui::render_thread::WrappedLineInfo],
    usable_width: u16,
) -> Option<String> {
    let ((start_row, start_col), (end_row, end_col)) = if start <= end {
        (start, end)
    } else {
        (end, start)
    };

    let start_idx = wrap_map.partition_point(|info| info.visual_row_end <= start_row);
    let end_idx = wrap_map.partition_point(|info| info.visual_row_end <= end_row);

    if start_idx >= wrap_map.len() {
        return None;
    }
    let end_idx = end_idx.min(wrap_map.len() - 1);

    let mut parts: Vec<String> = Vec::new();

    for (i, info) in wrap_map
        .iter()
        .enumerate()
        .take(end_idx + 1)
        .skip(start_idx)
    {
        let text = &info.plain_text;

        if start_idx == end_idx {
            // 同一逻辑行：截取 [start_char, end_char)
            let row_in_start = start_row - info.visual_row_start;
            let row_in_end = end_row - info.visual_row_start;
            let c_start =
                char_col_to_offset(&info.char_widths, start_col, row_in_start, usable_width);
            let c_end = char_col_to_offset(&info.char_widths, end_col, row_in_end, usable_width);
            let b_start = grapheme_to_byte_idx(text, c_start.max(info.copy_prefix_graphemes));
            let b_end = grapheme_to_byte_idx(text, c_end.max(info.copy_prefix_graphemes));
            if b_start >= b_end {
                return None;
            }
            parts.push(text[b_start..b_end].to_string());
        } else if i == start_idx {
            // 首行：从 start_col 对应的字符位置到行尾
            let row_in_line = start_row - info.visual_row_start;
            let c_start =
                char_col_to_offset(&info.char_widths, start_col, row_in_line, usable_width);
            let b_start = grapheme_to_byte_idx(text, c_start.max(info.copy_prefix_graphemes));
            parts.push(text[b_start..].to_string());
        } else if i == end_idx {
            // 末行：从行首到 end_col 对应的字符位置
            let row_in_line = end_row - info.visual_row_start;
            let c_end = char_col_to_offset(&info.char_widths, end_col, row_in_line, usable_width);
            let b_start = grapheme_to_byte_idx(text, info.copy_prefix_graphemes);
            let b_end = grapheme_to_byte_idx(text, c_end.max(info.copy_prefix_graphemes));
            parts.push(text[b_start..b_end].to_string());
        } else {
            // 中间行：整行
            let b_start = grapheme_to_byte_idx(text, info.copy_prefix_graphemes);
            parts.push(text[b_start..].to_string());
        }
    }

    if parts.is_empty() {
        None
    } else {
        Some(parts.join("\n"))
    }
}

// ── ScreenSnapshot / ScreenSelection：基于渲染 Buffer 的全局选区 ──────────────
//
// 覆盖消息区域以外的所有区域（面板/状态栏/sticky header/bg agent bar/空白）。
// MouseUp 时从 ScreenSnapshot 提取选区文本复制到剪贴板，所见即所得。
// 消息区域仍由 TextSelection（wrap_map 字符级精度）处理，两套系统互斥衔接。

/// terminal.draw() 后克隆的 Buffer 文本快照，作为非消息区域选区的文本源。
/// 仅存储每个 cell 的 symbol（轻量），不存 style。宽字符占位 cell 的 symbol 为 ""。
#[derive(Debug, Clone)]
pub struct ScreenSnapshot {
    symbols: Vec<String>, // row-major：symbols[row * width + col]
    width: usize,
    height: usize,
    origin_row: usize,
    origin_col: usize,
}

impl ScreenSnapshot {
    /// 从 ratatui Buffer 克隆出文本快照。
    pub fn from_buffer(buffer: &ratatui::buffer::Buffer) -> Self {
        let area = buffer.area;
        let width = area.width as usize;
        let height = area.height as usize;
        let symbols = buffer
            .content
            .iter()
            .map(|cell| cell.symbol().to_string())
            .collect();
        Self {
            symbols,
            width,
            height,
            origin_row: area.y as usize,
            origin_col: area.x as usize,
        }
    }

    /// 取 (row, col) 处 cell 的 symbol；越界返回空串。
    pub fn cell_symbol(&self, row: usize, col: usize) -> &str {
        let Some(row) = row.checked_sub(self.origin_row) else {
            return "";
        };
        let Some(col) = col.checked_sub(self.origin_col) else {
            return "";
        };
        if row < self.height && col < self.width {
            self.symbols[row * self.width + col].as_str()
        } else {
            ""
        }
    }
}

/// 屏幕选区状态，使用绝对屏幕坐标 (row, col)。
#[derive(Debug, Clone, Default)]
pub struct ScreenSelection {
    /// 选区起始绝对坐标
    pub start: Option<(u16, u16)>, // (row, col)
    /// 选区结束绝对坐标
    pub end: Option<(u16, u16)>,
    /// 是否正在拖拽中
    pub dragging: bool,
    /// 选区对应的纯文本内容（松开鼠标后计算）
    pub selected_text: Option<String>,
}

impl ScreenSelection {
    /// 开始拖拽：记录起始绝对坐标，清除旧选区文本。
    pub fn start_drag(&mut self, row: u16, col: u16) {
        self.start = Some((row, col));
        self.end = Some((row, col));
        self.dragging = true;
        self.selected_text = None;
    }

    /// 更新拖拽：更新结束绝对坐标。
    pub fn update_drag(&mut self, row: u16, col: u16) {
        if self.dragging {
            self.end = Some((row, col));
        }
    }

    /// 结束拖拽：标记拖拽结束，selected_text 由外部计算后通过 set_selected_text 设置。
    pub fn end_drag(&mut self) {
        self.dragging = false;
    }

    /// 设置提取后的选区文本。
    pub fn set_selected_text(&mut self, text: Option<String>) {
        self.selected_text = text;
    }

    /// 清除选区。
    pub fn clear(&mut self) {
        self.start = None;
        self.end = None;
        self.dragging = false;
        self.selected_text = None;
    }

    /// 是否有活跃的选区（正在拖拽，或 start 仍在→复制后高亮持续到点击他处）。
    pub fn is_active(&self) -> bool {
        self.dragging || self.start.is_some()
    }

    /// 返回归一化后的矩形范围 (start_row, start_col, end_row, end_col)，自动 swap。
    pub fn normalized_range(&self) -> Option<(u16, u16, u16, u16)> {
        match (self.start, self.end) {
            (Some(s), Some(e)) => {
                if s <= e {
                    Some((s.0, s.1, e.0, e.1))
                } else {
                    Some((e.0, e.1, s.0, s.1))
                }
            }
            _ => None,
        }
    }
}

/// 拼接一行内 `[col_start, col_end)` 列的文本，按 symbol 显示宽度推进列游标。
///
/// 宽字符（如 CJK）占 2 列，其后的占位 cell 由 ratatui 置为 `Cell::EMPTY`，
/// 而 `Cell::symbol()` 会把它读成 `" "`。逐列拼接会把占位格当成真实空格，
/// 导致「触发」被复制成「触 发」。
///
/// 解法是从**行首单元格边界**开始按宽度推进：读到宽度为 w 的 symbol 就前进 w 列，
/// 于是占位列永远不会被单独访问，也就不会产出多余空格。起点取 `origin_col` 而非
/// `col_start`，是为了保证推进始终从单元格边界出发——若直接从 `col_start` 起步而该列
/// 恰是某宽字符的占位列，整行后续列的字符对齐都会错位。
fn extract_row_text(
    snapshot: &ScreenSnapshot,
    row: usize,
    col_start: usize,
    col_end: usize,
) -> String {
    let mut out = String::new();
    let mut col = snapshot.origin_col;
    while col < col_end {
        let symbol = snapshot.cell_symbol(row, col);
        // 空白/组合符宽度可能为 0，至少按 1 列推进，避免死循环。
        let width = unicode_width::UnicodeWidthStr::width(symbol).max(1);
        // 只产出完全落入选区的字符：宽字符被选区边界截断时整体丢弃，不复制半个字。
        if col >= col_start && col + width <= col_end {
            out.push_str(symbol);
        }
        col += width;
    }
    out
}

/// 从 ScreenSnapshot 提取选区文本（CJK 友好）。
/// start/end 为绝对屏幕坐标 (row, col)，自动处理 start > end。
/// 首行从 start_col 起、末行到 end_col 止，中间行整行；每行 trim_end 去尾部空白，
/// 剔除首尾空行（保留中间空行以维持视觉结构）。
///
/// 宽字符占位 cell 必须跳过：ratatui 的 `Cell::symbol()` 对无 symbol 的 cell 返回
/// `" "`（而非 `""`），而宽字符的后继列恰是这种 `Cell::EMPTY`。若逐列直接拼接，
/// 每读一个汉字就会多出一个空格（「触发」→「触 发」）。因此这里按 symbol 的显示宽度
/// 推进列游标：宽字符消费掉其占位列，占位列不再单独产出字符。
pub fn extract_snapshot_text(
    snapshot: &ScreenSnapshot,
    start: (u16, u16),
    end: (u16, u16),
) -> Option<String> {
    let ((sr, sc), (er, ec)) = if start <= end {
        (start, end)
    } else {
        (end, start)
    };
    let sr = sr as usize;
    let er = er as usize;
    let sc = (sc as usize).max(snapshot.origin_col);
    let ec = ec as usize;
    if sr > er || snapshot.height == 0 || snapshot.width == 0 {
        return None;
    }
    let mut lines: Vec<String> = Vec::new();
    for row in sr.max(snapshot.origin_row)
        ..=er.min(snapshot.origin_row + snapshot.height.saturating_sub(1))
    {
        let col_start = if row == sr { sc } else { snapshot.origin_col };
        let col_end = if row == er {
            (ec + 1).min(snapshot.origin_col + snapshot.width)
        } else {
            snapshot.origin_col + snapshot.width
        };
        let line: String = extract_row_text(snapshot, row, col_start, col_end);
        lines.push(line.trim_end().to_string());
    }
    // 剔除首尾空行
    while lines.first().map(|s| s.is_empty()).unwrap_or(false) {
        lines.remove(0);
    }
    while lines.last().map(|s| s.is_empty()).unwrap_or(false) {
        lines.pop();
    }
    if lines.is_empty() {
        None
    } else {
        Some(lines.join("\n"))
    }
}

#[cfg(test)]
#[path = "text_selection_test.rs"]
mod tests;
