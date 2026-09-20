use std::{borrow::Cow, collections::HashMap, io};

use ratatui::{
    backend::{Backend, ClearType, WindowSize},
    buffer::Cell,
    layout::{Position, Size},
};
use unicode_width::UnicodeWidthStr;

pub trait WidthProbe {
    fn width(&mut self, symbol: &str) -> Option<usize>;
    /// Return true when the terminal's width policy may have changed.
    fn refresh(&mut self) -> bool;
}

/// Adapt output only: the original frame, selection and stored messages stay intact.
pub struct WidthSafeBackend<B, P> {
    inner: B,
    probe: P,
    symbols: HashMap<String, Option<String>>,
}

impl<B, P: WidthProbe> WidthSafeBackend<B, P> {
    pub fn with_probe(inner: B, probe: P) -> Self {
        Self {
            inner,
            probe,
            symbols: HashMap::new(),
        }
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
        if !self.symbols.contains_key(symbol) {
            // Bound memory for long sessions containing many different graphemes.
            if self.symbols.len() >= 4096 {
                self.symbols.clear();
            }
            let expected = symbol.width();
            let replacement = self
                .probe
                .width(symbol)
                .filter(|&actual| actual != expected)
                .map(|_| fallback_symbol(symbol, expected));
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

fn fallback_symbol(symbol: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    let first = match symbol {
        "‘" | "’" | "‚" | "‛" => '\'',
        "“" | "”" | "„" | "‟" => '"',
        "—" | "–" | "−" | "─" | "━" | "═" => '-',
        "│" | "┃" | "║" => '|',
        "┌" | "┐" | "└" | "┘" | "┬" | "┴" | "├" | "┤" | "┼"
        | "╭" | "╮" | "╯" | "╰"
        | "╔" | "╗" | "╚" | "╝" | "╦" | "╩" | "╠" | "╣" | "╬" => '+',
        "●" | "•" | "▪" | "✻" | "✦" | "✧" => '*',
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
        "⠋" | "⠙" | "⠹" | "⠸" | "⠼" | "⠴" | "⠦" | "⠧" | "⠇" | "⠏"
        | "✵" | "✶" | "✷" | "✸" | "✹" | "✺" | "✼" | "❃" | "❊" => '*',
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
