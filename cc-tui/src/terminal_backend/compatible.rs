use std::{borrow::Cow, collections::HashMap, io};

use ratatui::{
    backend::{Backend, ClearType, WindowSize},
    buffer::Cell,
    layout::{Position, Size},
};
use unicode_width::UnicodeWidthStr;

pub trait WidthProbe {
    fn width(&mut self, symbol: &str) -> Option<usize>;
    /// Native console fonts can lack glyphs even when their width is correct.
    ///
    /// 缺字形不会改变列宽（宿主照样按 1 列推进，只是画成方框），宽度测量看不见，
    /// 所以这类宿主必须在测量之前就用 [`ascii_fallback`] 的定向表直接替换。
    fn prefer_ascii_fallback(&self) -> bool {
        false
    }
    /// Return true when the terminal's width policy may have changed.
    fn refresh(&mut self) -> bool;
}

/// Adapt output only: the original frame, selection and stored messages stay intact.
pub struct WidthSafeBackend<B, P> {
    /// 输出侧后端；`pub(super)` 便于其他探针的测试直接检查实际输出的 cell。
    pub(super) inner: B,
    probe: P,
    symbols: HashMap<String, Option<String>>,
    clear_owned_lines_only: bool,
    hyperlinks: super::hyperlink::Frame,
    pending_hyperlinks: Option<super::hyperlink::Frame>,
}

impl<B, P: WidthProbe> WidthSafeBackend<B, P> {
    pub fn with_probe(inner: B, probe: P) -> Self {
        Self {
            inner,
            probe,
            symbols: HashMap::new(),
            clear_owned_lines_only: false,
            hyperlinks: super::hyperlink::Frame::default(),
            pending_hyperlinks: None,
        }
    }

    /// draw 借用终端前取得准备器，在闭包末尾处理最终 buffer。
    pub fn hyperlink_frame(&self) -> super::hyperlink::Frame {
        let mut frame = self.hyperlinks.clone();
        if let Some(pending) = &self.pending_hyperlinks {
            frame.invalidate(pending);
        }
        frame
    }

    fn clear_hyperlinks(&mut self) {
        let previous = self.hyperlinks.clone();
        self.hyperlinks.invalidate(&previous);
        if let Some(pending) = self.pending_hyperlinks.take() {
            self.hyperlinks.invalidate(&pending);
        }
    }

    /// Fixed 视口的 ratatui clear 会按行发送 AfterCursor；限制为当前行，
    /// 避免清除增长视口下方不属于应用的正常屏区域。
    pub fn clear_owned_lines_only(&mut self) {
        self.clear_owned_lines_only = true;
    }

    /// The caller must clear/redraw the terminal if previously drawn glyphs changed width.
    pub fn refresh_widths(&mut self) -> bool {
        if self.probe.refresh() {
            self.symbols.clear();
            true
        } else {
            false
        }
    }

    fn output_cell<'a>(&mut self, cell: &'a Cell) -> Cow<'a, Cell> {
        let symbol = cell.symbol();
        if symbol.is_ascii() {
            return Cow::Borrowed(cell);
        }
        // 含控制字符的 symbol 一定是终端转义序列（如 OSC 8 超链接的
        // `ESC ]8;;…`），不是可测量的字形。直接原样输出，否则会被当作
        // 宽字符去量列宽并替换成降级字形，链接失效且整行错位。
        if symbol.chars().any(char::is_control) {
            return Cow::Borrowed(cell);
        }
        if !self.symbols.contains_key(symbol) {
            // Bound memory for long sessions containing many different graphemes.
            if self.symbols.len() >= 4096 {
                self.symbols.clear();
            }
            let expected = symbol.width();
            let degraded = self
                .probe
                .prefer_ascii_fallback()
                .then(|| ascii_fallback(symbol))
                .flatten();
            let replacement = degraded.map(str::to_owned).or_else(|| {
                self.probe
                    .width(symbol)
                    .filter(|&actual| actual != expected)
                    .map(|_| fallback_symbol(symbol, expected))
            });
            self.symbols.insert(symbol.to_owned(), replacement);
        }
        match self.symbols.get(symbol).and_then(Option::as_deref) {
            Some(replacement) => {
                let mut adapted = cell.clone();
                adapted.set_symbol(replacement);
                Cow::Owned(adapted)
            }
            None => Cow::Borrowed(cell),
        }
    }
}

/// 定向 ASCII 降级表：宽度正常但字形可能缺失的字形。
///
/// 表内字形分两类用途：
/// 1. 动画帧——列宽分歧时也要保留旋转语义（见 [`fallback_symbol`]）；
/// 2. 传统 conhost 原生窗口的缺字字形——缺字不改变列宽，探针量不出差异，
///    必须由 [`WidthProbe::prefer_ascii_fallback`] 在测量之前直接替换。
fn ascii_fallback(symbol: &str) -> Option<&'static str> {
    match symbol {
        // 上游参考实现 2.1.307 起菊花字形改为 `· ✢ * ✶ ✻ ✽`（Ghostty 用 `✳`）。
        // `✳`(U+2733) 会被 emoji-regex 判成双宽，`✢`/`✽` 在旧控制台也是缺字，
        // 这里统一给 ASCII 旋转替代。
        "✳" => Some("*"),
        "✢" => Some("+"),
        "✽" => Some("\\"),
        // 历史盲文字形（v0.6 之前的帧集）保留映射，避免老快照/兼容路径失效
        "✵" | "✹" | "❃" => Some("|"),
        "✶" | "✺" | "❊" => Some("/"),
        "✷" | "✻" => Some("-"),
        "✸" | "✼" => Some("\\"),
        // `❯`(U+276F) 是输入框提示符与用户消息气泡前缀（`SYMBOL_CURSOR`）。
        // 它落在 Dingbats 区间，传统 conhost 的字体覆盖很差：缺字时画成方框，
        // 光标仍只推进 1 列，宽度探针判定「一致」而放行，故只能定向替换为 `>`。
        "❯" => Some(">"),
        // `⏵`(U+23F5) 是状态栏权限 pill 的前缀（`PERMISSION_MODE_SYMBOL` = `⏵⏵`）。
        // Misc Technical Symbols 同属控制台字体覆盖盲区，两个 `⏵` 会画成两个方框。
        "⏵" => Some(">"),
        // `※`(U+203B) 是 recap 行前缀，`◐`(U+25D0) 是状态栏工具 pill 与 SubAgent 卡片头；
        // 两者在控制台字体里既可能缺字（画方框），也可能被判成歧义宽度（画 2 列），
        // 两种情况都要落到 ASCII 星号。
        "※" | "◐" => Some("*"),
        // 面板折叠箭头与排队消息前缀：`▸`/`›` 向右、`▾` 向下、`△` 向上。
        "▸" | "›" => Some(">"),
        "▾" => Some("v"),
        "△" => Some("^"),
        // `‹`(U+2039) 是排队消息上翻按钮（`[‹]`），`↩`(U+21A9) 是 rewind/compact
        // 通知行的前缀，两者都是回退语义，统一落到 `<`。
        "‹" | "↩" => Some("<"),
        // todo 状态图标：`◼`(进行中) 与 `◻`(待办)；`☐` 是 ask_user 未确认 tab，
        // `⌕` 是会话浏览器搜索图标。都是方框类字形，控制台字体常缺字，
        // 统一落到 `#` / `o`（与 `█`→`#`、`○`→`o` 保持一致）。
        "◼" => Some("#"),
        "◻" | "☐" | "⌕" => Some("o"),
        // MCP 面板的「已禁用」标记。
        "⊘" => Some("x"),
        // cc-widgets 的列表光标 `▶` 与滚动条上下按钮 `▲`/`▼`。
        "▶" => Some(">"),
        "▲" => Some("^"),
        "▼" => Some("v"),
        _ => None,
    }
}

/// 列宽分歧时的 1 列替代字形；`pub(super)` 供非 Windows 探针校验自己只量可降级的字形。
pub(super) fn fallback_symbol(symbol: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    if let Some(ascii) = ascii_fallback(symbol) {
        let mut result = ascii.to_owned();
        result.extend(std::iter::repeat_n(' ', width - 1));
        return result;
    }
    let first = match symbol {
        "‘" | "’" | "‚" | "‛" => '\'',
        "“" | "”" | "„" | "‟" => '"',
        "—" | "–" | "−" | "─" | "━" | "═" => '-',
        "│" | "┃" | "║" => '|',
        "┌" | "┐" | "└" | "┘" | "┬" | "┴" | "├" | "┤" | "┼" | "╭" | "╮" | "╯" | "╰" | "╔" | "╗"
        | "╚" | "╝" | "╦" | "╩" | "╠" | "╣" | "╬" => '+',
        // `⎿`（U+23BF）为工具结果行前缀（Bash/Read/Edit/Grep/Glob/AskUserQuestion），
        // 归入制表符组降级为 `+`，避免落入 `_ => '?'` 显示成无语义问号（#383）。
        "⎿" => '+',
        "●" | "•" | "▪" | "✦" | "✧" => '*',
        "○" | "◯" => 'o',
        "∴" | "·" | "…" => '.',
        "←" => '<',
        "→" => '>',
        "↑" => '^',
        "↓" => 'v',
        "✓" | "✔" => 'v',
        "✗" | "✘" | "×" => 'x',
        "█" | "▉" | "▊" | "▋" | "▌" | "▍" | "▎" | "▏" => '#',
        "░" | "▒" | "▓" => '-',
        "⏱" | "⏳" | "⏰" | "⌛" => 't',
        "⚠️" | "⚠" => '!',
        "⠋" | "⠙" | "⠹" | "⠸" | "⠼" | "⠴" | "⠦" | "⠧" | "⠇" | "⠏" => '*',
        s if s.chars().all(char::is_whitespace) => ' ',
        _ => '?',
    };
    let mut result = first.to_string();
    result.extend(std::iter::repeat_n(' ', width - 1));
    result
}

impl<B: Backend, P: WidthProbe> Backend for WidthSafeBackend<B, P> {
    type Error = B::Error;

    fn draw<'a, I>(&mut self, content: I) -> Result<(), Self::Error>
    where
        I: Iterator<Item = (u16, u16, &'a Cell)>,
    {
        if let Some(pending) = self.pending_hyperlinks.take() {
            self.hyperlinks.invalidate(&pending);
        }
        let prepared = super::hyperlink::take_prepared_frame(&self.hyperlinks);
        let frame = prepared
            .as_ref()
            .map(|prepared| prepared.frame.clone())
            .unwrap_or_else(|| self.hyperlinks.clone());
        let mut cells: Vec<_> = content
            .map(|(x, y, cell)| (x, y, Cow::Borrowed(cell)))
            .collect();
        if let Some(prepared) = prepared.filter(|prepared| !prepared.redraws.is_empty()) {
            cells.extend(
                prepared
                    .redraws
                    .into_iter()
                    .map(|(x, y, cell)| (x, y, Cow::Owned(cell))),
            );
            // ratatui 差分和链接属性补画可能命中同一格；保持行列顺序且只写一次。
            cells.sort_by_key(|(x, y, _)| (*y, *x));
            cells.dedup_by_key(|(x, y, _)| (*y, *x));
        }
        let adapted: Vec<_> = cells
            .iter()
            .map(|(x, y, cell)| {
                let adapted = self.output_cell(cell.as_ref());
                // OSC 8 只能在这里包裹：`Buffer::diff` 会用 `symbol.width()` 决定
                // 跳过后续多少 cell，把转义序列（其载荷是可见字符）写进 buffer
                // 会让链接后面的整段文字不落笔（见 `hyperlink` 模块文档）。
                let adapted = match frame.decorate(*x, *y, adapted.as_ref()) {
                    Some(decorated) => Cow::Owned(decorated),
                    None => adapted,
                };
                (*x, *y, adapted)
            })
            .collect();
        if let Err(error) = self
            .inner
            .draw(adapted.iter().map(|(x, y, cell)| (*x, *y, cell.as_ref())))
        {
            self.hyperlinks.invalidate(&frame);
            return Err(error);
        }
        self.pending_hyperlinks = Some(frame);
        Ok(())
    }

    fn append_lines(&mut self, n: u16) -> Result<(), Self::Error> {
        self.inner.append_lines(n)
    }

    fn hide_cursor(&mut self) -> Result<(), Self::Error> {
        self.inner.hide_cursor()
    }

    fn show_cursor(&mut self) -> Result<(), Self::Error> {
        self.inner.show_cursor()
    }

    fn get_cursor_position(&mut self) -> Result<Position, Self::Error> {
        self.inner.get_cursor_position()
    }

    fn set_cursor_position<Q: Into<Position>>(&mut self, position: Q) -> Result<(), Self::Error> {
        self.inner.set_cursor_position(position)
    }

    fn clear(&mut self) -> Result<(), Self::Error> {
        self.clear_hyperlinks();
        self.inner.clear()
    }

    fn clear_region(&mut self, clear_type: ClearType) -> Result<(), Self::Error> {
        let clear_type = if self.clear_owned_lines_only && clear_type == ClearType::AfterCursor {
            ClearType::UntilNewLine
        } else {
            clear_type
        };
        self.clear_hyperlinks();
        self.inner.clear_region(clear_type)
    }

    fn size(&self) -> Result<Size, Self::Error> {
        self.inner.size()
    }

    fn window_size(&mut self) -> Result<WindowSize, Self::Error> {
        self.inner.window_size()
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        if let Err(error) = self.inner.flush() {
            if let Some(pending) = &self.pending_hyperlinks {
                self.hyperlinks.invalidate(pending);
            }
            return Err(error);
        }
        if let Some(frame) = self.pending_hyperlinks.take() {
            self.hyperlinks = frame;
        }
        Ok(())
    }
}

impl<B: io::Write, P> io::Write for WidthSafeBackend<B, P> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.inner.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

#[cfg(test)]
#[path = "compatible_test.rs"]
mod tests;

#[cfg(test)]
#[path = "hyperlink_frame_test.rs"]
mod hyperlink_frame_tests;
