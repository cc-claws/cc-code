use std::{borrow::Cow, collections::HashMap, io};

use ratatui::{
    backend::{Backend, ClearType, WindowSize},
    buffer::Cell,
    layout::{Position, Size},
};
use unicode_width::UnicodeWidthStr;

pub trait WidthProbe {
    fn width(&mut self, symbol: &str) -> Option<usize>;
    /// Native console fonts can lack spinner glyphs even when their width is correct.
    fn prefer_ascii_spinner(&self) -> bool {
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
}

impl<B, P: WidthProbe> WidthSafeBackend<B, P> {
    pub fn with_probe(inner: B, probe: P) -> Self {
        Self {
            inner,
            probe,
            symbols: HashMap::new(),
            clear_owned_lines_only: false,
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
            let spinner = self
                .probe
                .prefer_ascii_spinner()
                .then(|| spinner_fallback(symbol))
                .flatten();
            let replacement = spinner.map(str::to_owned).or_else(|| {
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

fn spinner_fallback(symbol: &str) -> Option<&'static str> {
    match symbol {
        "✵" | "✹" | "❃" => Some("|"),
        "✶" | "✺" | "❊" => Some("/"),
        "✷" | "✻" => Some("-"),
        "✸" | "✼" => Some("\\"),
        _ => None,
    }
}

/// 列宽分歧时的 1 列替代字形；`pub(super)` 供非 Windows 探针校验自己只量可降级的字形。
pub(super) fn fallback_symbol(symbol: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    if let Some(spinner) = spinner_fallback(symbol) {
        let mut result = spinner.to_owned();
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
        let adapted: Vec<_> = content
            .map(|(x, y, cell)| (x, y, self.output_cell(cell)))
            .collect();
        self.inner
            .draw(adapted.iter().map(|(x, y, cell)| (*x, *y, cell.as_ref())))
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
        self.inner.clear()
    }

    fn clear_region(&mut self, clear_type: ClearType) -> Result<(), Self::Error> {
        let clear_type = if self.clear_owned_lines_only && clear_type == ClearType::AfterCursor {
            ClearType::UntilNewLine
        } else {
            clear_type
        };
        self.inner.clear_region(clear_type)
    }

    fn size(&self) -> Result<Size, Self::Error> {
        self.inner.size()
    }

    fn window_size(&mut self) -> Result<WindowSize, Self::Error> {
        self.inner.window_size()
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        self.inner.flush()
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
