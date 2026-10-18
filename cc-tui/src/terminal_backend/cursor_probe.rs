//! 非 Windows 侧的字宽探针：Unix 不提供字形度量 API，但 VT 兼容终端都支持
//! 光标位置回报（CPR，`ESC[6n`）。启动时在 alternate screen 里量一次真实列宽，
//! 供 `WidthSafeBackend` 决定是否降级字形。
//!
//! 为什么要量：终端按自己的宽度表推进光标（CJK 字体、iTerm2 的「歧义字符按双宽」
//! 设置等），与 `unicode-width` 的分歧不会报错，只会让同一行后续每一列整体右移，
//! 表现为状态栏/表格错列与行尾残影。Windows 侧可以建不显示的第二个控制台缓冲区来
//! 测量，Unix 没有第二条屏幕缓冲区，只能用 CPR。
//!
//! 代价与保护：
//! - 只在 stdin 与 stdout 都是 TTY、且 `TERM` 不是 `dumb` 时才测。
//! - 每条字形是一次 `ESC[6n` 往返，整体限时 [`PROBE_BUDGET`]；连续 [`MAX_MISSES`]
//!   条不回包（终端忽略 CPR、被复用器吞掉等）立即放弃，并排空迟到回包，避免它们
//!   之后被输入线程当成按键。
//! - 任何一步不成立就返回空探针，输出与裸 `CrosstermBackend` 完全一致。
//! - 只探测 `unicode-width` 判定为 1 列的图标字形：反向分歧（程序以为 2 列、终端画 1 列）
//!   的降级结果是 `?` 加空格，只能用于图标，绝不能套到 CJK 正文与 emoji 上。
//!
//! 探测借用当前行的第一列（此时 alt screen 首帧尚未绘制、屏幕空白），测完擦除并还原光标。

use std::{
    collections::HashMap,
    io::{self, IsTerminal, Write},
    time::{Duration, Instant},
};

use unicode_width::UnicodeWidthStr;

use super::compatible::WidthProbe;

/// 待测字形，按「用户最先看到」的顺序排列：预算耗尽时先保证状态栏与消息区图标。
const PROBE_GLYPHS: &[&str] = &[
    // 状态栏与消息区图标
    "✓", "✗", "×", "·", "∴", "…", "●", "•", "▪", "○", "⏱", "⎿", "⚠",
    // 思考/运行动画帧
    "✵", "✹", "❃", "✶", "✺", "❊", "✷", "✻", "✸", "✼", "⠋", "⠙", "⠹",
    // 进度条与制表符
    "█", "░", "▒", "▓", "▌", "─", "│", "┌", "┐", "└", "┘", "┼", "╭", "╮", "╰", "╯",
    // 引号、破折号与箭头
    "‘", "’", "“", "”", "—", "←", "→", "↑", "↓",
];

/// 单条 `ESC[6n` 回包的等待上限。
const RESPONSE_TIMEOUT: Duration = Duration::from_millis(60);
/// 整轮探测的时间预算：高延迟链路（SSH、慢复用器）不拖住启动。
const PROBE_BUDGET: Duration = Duration::from_millis(150);
/// 连续不回包达到这个次数就认定终端不支持 CPR。
const MAX_MISSES: usize = 2;
/// 放弃探测后丢弃迟到回包的时间窗口。只有「不回包但回得慢」的链路会走这条路径，
/// 正常情况下第一次 5ms 空轮询就退出，不额外拖慢启动。
const DRAIN_WINDOW: Duration = Duration::from_millis(100);

const SAVE_CURSOR: &[u8] = b"\x1b7";
const ERASE_LINE: &[u8] = b"\r\x1b[2K";
const QUERY_CURSOR: &[u8] = b"\x1b[6n";
const ERASE_LINE_AND_RESTORE: &[u8] = b"\r\x1b[2K\x1b8";

/// 宿主终端实测列宽与 `unicode-width` 的差异表；只存不一致的字形。
pub struct CursorWidthProbe {
    wide: HashMap<String, usize>,
}

impl CursorWidthProbe {
    /// 量一次宿主终端。任何前置条件不成立都返回空探针（不修正任何字形）。
    pub fn measure<W: Write>(out: &mut W) -> Self {
        if !probe_allowed() {
            tracing::debug!("字宽探针跳过：非 TTY 或 TERM=dumb");
            return Self {
                wide: HashMap::new(),
            };
        }
        let mut source = StdinSource {
            fd: libc::STDIN_FILENO,
        };
        let wide = measure_with_source(out, &mut source);
        if wide.is_empty() {
            tracing::debug!("字宽探针：宿主终端列宽与 unicode-width 一致，不降级字形");
        } else {
            tracing::debug!(glyphs = ?wide, "字宽探针：以下字形宿主终端更宽，输出降级为 1 列替代");
        }
        Self { wide }
    }

    #[cfg(test)]
    pub(super) fn with_widths(wide: HashMap<String, usize>) -> Self {
        Self { wide }
    }
}

impl WidthProbe for CursorWidthProbe {
    fn width(&mut self, symbol: &str) -> Option<usize> {
        self.wide.get(symbol).copied()
    }

    /// 列宽策略在会话内固定：重新测量要独占 stdin 做 CPR 往返，而输入线程已持有读端。
    fn refresh(&mut self) -> bool {
        false
    }
}

fn probe_allowed() -> bool {
    io::stdin().is_terminal()
        && io::stdout().is_terminal()
        && !std::env::var("TERM").is_ok_and(|term| term == "dumb")
}

fn measure_with_source<W: Write, S: ByteSource + ?Sized>(
    out: &mut W,
    source: &mut S,
) -> HashMap<String, usize> {
    let mut wide = HashMap::new();
    if out
        .write_all(SAVE_CURSOR)
        .and_then(|()| out.flush())
        .is_err()
    {
        return wide;
    }
    let started = Instant::now();
    let mut misses = 0;
    for glyph in PROBE_GLYPHS {
        // 只量本该 1 列的字形，见模块注释末段。
        if glyph.width() != 1 {
            continue;
        }
        if started.elapsed() >= PROBE_BUDGET {
            break;
        }
        let asked = out
            .write_all(ERASE_LINE)
            .and_then(|()| out.write_all(glyph.as_bytes()))
            .and_then(|()| out.write_all(QUERY_CURSOR))
            .and_then(|()| out.flush());
        if asked.is_err() {
            break;
        }
        match parse_position(source) {
            Position::Column(column) => {
                misses = 0;
                // 擦行后光标在 0 列，回包列号从 1 起算：列号减去 1 就是字形实际列宽。
                let width = column.saturating_sub(1);
                if width != 1 {
                    wide.insert((*glyph).to_owned(), width);
                }
            }
            Position::NoReply => {
                misses += 1;
                if misses >= MAX_MISSES {
                    drain_late_replies(source);
                    break;
                }
            }
            Position::Malformed => break,
        }
    }
    let _ = out.write_all(ERASE_LINE_AND_RESTORE);
    let _ = out.flush();
    wide
}

/// 丢弃迟到的 CPR 回包，避免它们流入输入线程被解析成按键。
fn drain_late_replies<S: ByteSource + ?Sized>(source: &mut S) {
    let deadline = Instant::now() + DRAIN_WINDOW;
    while Instant::now() < deadline {
        if matches!(source.next_byte(Duration::from_millis(5)), Byte::Value(_)) {
            continue;
        }
        break;
    }
}

enum Byte {
    Value(u8),
    /// 在超时时间内没有任何字节。
    Timeout,
    /// 读端已关闭或读失败。
    Closed,
}

trait ByteSource {
    fn next_byte(&mut self, timeout: Duration) -> Byte;
}

enum Position {
    /// `ESC[<row>;<column>R` 中的列号（从 1 起）。
    Column(usize),
    NoReply,
    Malformed,
}

fn parse_position<S: ByteSource + ?Sized>(source: &mut S) -> Position {
    match source.next_byte(RESPONSE_TIMEOUT) {
        Byte::Value(0x1b) => {}
        // 没有回包：可能是超时，也可能是读端已关闭（终端不支持 CPR）。
        Byte::Timeout | Byte::Closed => return Position::NoReply,
        // 首个字节不是转义序列，说明用户按了键：立即放弃，不猜测。
        Byte::Value(_) => return Position::Malformed,
    }
    if !matches!(source.next_byte(RESPONSE_TIMEOUT), Byte::Value(b'[')) {
        return Position::Malformed;
    }
    let mut digits: Vec<u8> = Vec::with_capacity(6);
    let mut after_separator = false;
    loop {
        match source.next_byte(RESPONSE_TIMEOUT) {
            Byte::Value(digit @ b'0'..=b'9') => {
                // 回包只有行列两个短数字；超长说明字节流已经错位。
                if digits.len() >= 6 {
                    return Position::Malformed;
                }
                digits.push(digit);
            }
            Byte::Value(b';') if !after_separator && !digits.is_empty() => {
                digits.clear();
                after_separator = true;
            }
            Byte::Value(b'R') if after_separator && !digits.is_empty() => {
                let column = digits
                    .iter()
                    .fold(0usize, |acc, digit| acc * 10 + usize::from(digit - b'0'));
                return Position::Column(column);
            }
            Byte::Timeout => return Position::NoReply,
            _ => return Position::Malformed,
        }
    }
}

struct StdinSource {
    fd: libc::c_int,
}

impl ByteSource for StdinSource {
    fn next_byte(&mut self, timeout: Duration) -> Byte {
        let deadline = Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            let mut poll_fd = libc::pollfd {
                fd: self.fd,
                events: libc::POLLIN,
                revents: 0,
            };
            // SAFETY: 单个已初始化的 pollfd；fd 0 由进程持有，这里只读不改所有权。
            let ready = unsafe { libc::poll(&mut poll_fd, 1, remaining.as_millis() as i32) };
            if ready == 0 {
                return Byte::Timeout;
            }
            if ready < 0 {
                if io::Error::last_os_error().kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                return Byte::Closed;
            }
            let mut byte = 0u8;
            // SAFETY: byte 是一字节可写缓冲，fd 0 的所有权不变。
            let read = unsafe { libc::read(self.fd, (&mut byte as *mut u8).cast(), 1) };
            match read {
                1 => return Byte::Value(byte),
                0 => return Byte::Closed,
                _ => {
                    if io::Error::last_os_error().kind() == io::ErrorKind::Interrupted {
                        continue;
                    }
                    return Byte::Closed;
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "cursor_probe_test.rs"]
mod tests;
