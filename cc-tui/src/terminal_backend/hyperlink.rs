//! 逐 cell 的 OSC 8 超链接登记表：渲染阶段按屏幕坐标登记，输出阶段再包裹转义序列。
//!
//! 为什么不在渲染阶段直接改 `Cell` symbol：`ratatui` 的 `Buffer::diff` 用
//! `symbol.width()` 判断一个 cell 占几列，并把后续 `width - 1` 个 cell 从差分里
//! **跳过不画**。OSC 8 序列的载荷（URL）是可打印字符，整个
//! `ESC]8;;URL BEL 文字 ESC]8;;BEL` 的 `width()` 等于 URL 长度，于是链接后面几十个
//! cell 被整段跳过——表现为「URL 只剩首字母，紧随其后的文字错位/残留旧内容」。
//!
//! 因此 buffer 里始终保持纯文本（宽度语义 1 列），只在 [`crate::terminal_backend`]
//! 真正写字节时按坐标查表包裹序列。这里是那张表。
//!
//! 状态是**线程局部**的：一帧的登记（渲染闭包内）与读取（`Terminal::flush`）
//! 都发生在同一个 UI 线程上，线程局部既省掉锁，也避免多线程/并行测试互相串扰。

use std::cell::{Cell as FlagCell, RefCell};
use std::collections::HashMap;

use ratatui::buffer::Cell;
use ratatui::style::Color;

thread_local! {
    /// 本线程本帧登记的超链接 cell：(屏幕列, 屏幕行) → (URL, 是否染成链接色)。
    static LINKS: RefCell<HashMap<(u16, u16), (String, bool)>> = RefCell::new(HashMap::new());
    /// 本线程本帧是否注入 OSC 8。
    static ENABLED: FlagCell<bool> = const { FlagCell::new(false) };
}

/// 链接前景色。
///
/// CC 源码是 `chalk.blue`（SGR 34），但实测该终端里 `34` / `38;5;4` 都比 CC 的
/// 链接观感更深；`38;5;12`（ratatui `LightBlue` → crossterm `Blue`）才是并排比对后
/// 视觉一致的那档，故取 12 号槽。
const LINK_COLOR: Color = Color::LightBlue;

/// 开始新的一帧：清空上一帧登记，并锁定本帧是否注入。
pub fn begin_frame(enabled: bool) {
    ENABLED.with(|flag| flag.set(enabled));
    if enabled {
        LINKS.with(|table| table.borrow_mut().clear());
    }
}

/// 登记一个链接 cell。未启用时什么也不做，避免白白攒一张不会被读的表。
///
/// `recolor` 为 `false` 时只包 OSC 8、保留 cell 自身前景色（附件引用行等
/// 由正文色承载的链接），见 [`decorate`]。
pub fn register(x: u16, y: u16, url: &str, recolor: bool) {
    if !ENABLED.with(FlagCell::get) {
        return;
    }
    LINKS.with(|table| table.borrow_mut().insert((x, y), (url.to_string(), recolor)));
}

/// 若 `(x, y)` 是登记的链接 cell，返回「包好 OSC 8 + 链接色」的 cell 副本。
///
/// 空白 cell 不处理：把无内容的空格变成热点只会误触。
pub fn decorate(x: u16, y: u16, cell: &Cell) -> Option<Cell> {
    if !ENABLED.with(FlagCell::get) || cell.symbol().trim().is_empty() {
        return None;
    }
    let (url, recolor) = LINKS.with(|table| table.borrow().get(&(x, y)).cloned())?;
    let mut decorated = cell.clone();
    decorated.set_symbol(&format!("{}{}{}", open(&url), cell.symbol(), CLOSE));
    if recolor {
        // CC 只给链接文字上蓝色，不加下划线（下划线由终端在悬停时给出）。
        decorated.set_style(cell.style().fg(LINK_COLOR));
    }
    Some(decorated)
}

/// OSC 8 开序列：`ESC ] 8 ; params ; URI BEL`。
///
/// 用 BEL 收尾与 CC `OSC8_END` 一致（其注释称兼容性更广）。
///
/// 安全：URI 中任何控制字符（含 `ESC`、`BEL`、换行）都会被剥离。否则一个
/// 含 `\x07` 或 `\x1b\\` 的 URL 能提前终止 OSC 8 序列并注入任意终端指令。
fn open(url: &str) -> String {
    let safe: String = url.chars().filter(|ch| !ch.is_control()).collect();
    format!("\x1b]8;;{safe}\x07")
}

/// OSC 8 关序列。
const CLOSE: &str = "\x1b]8;;\x07";

#[cfg(test)]
#[path = "hyperlink_test.rs"]
mod tests;
