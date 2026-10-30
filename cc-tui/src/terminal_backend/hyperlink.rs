//! 逐 cell 的 OSC 8 超链接登记表：渲染阶段按屏幕坐标登记，输出阶段再包裹转义序列。
//!
//! 为什么不在渲染阶段直接改 `Cell` symbol：`ratatui` 的 `Buffer::diff` 用
//! `symbol.width()` 判断一个 cell 占几列，并把后续 `width - 1` 个 cell 从差分里
//! **跳过不画**。OSC 8 序列的载荷（URL）是可打印字符，整个
//! `ESC]8;;URL BEL 文字 ESC]8;;BEL` 的 `width()` 等于 URL 长度，于是链接后面几十个
//! cell 被整段跳过——表现为「URL 只剩首字母，紧随其后的文字错位/残留旧内容」。
//!
//! 因此 buffer 里始终保持纯文本，链接色在差分前写入样式，只有 OSC 8 序列
//! 留到输出层注入。URL/锚点变化还需补画字符未变的格子，避免旧链接留在终端。
//!
//! 状态是**线程局部**的：一帧的登记（渲染闭包内）与读取（`Terminal::flush`）
//! 都发生在同一个 UI 线程上，线程局部既省掉锁，也避免多线程/并行测试互相串扰。

use std::cell::{Cell as FlagCell, RefCell};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use ratatui::buffer::{Buffer, Cell};
use ratatui::style::Color;
use unicode_width::UnicodeWidthStr;

/// 渲染命中区的逻辑行、起止 grapheme；同 URL 的独立命中区不能共用锚点。
pub type LinkId = (usize, usize, usize);
type Position = (u16, u16);
type LinkMap = HashMap<Position, Link>;

#[derive(Clone, Debug, PartialEq, Eq)]
struct Link {
    url: String,
    recolor: bool,
    id: LinkId,
}

thread_local! {
    /// 本线程本帧登记的超链接 cell：(屏幕列, 屏幕行) → (URL, 是否染成链接色)。
    static LINKS: RefCell<LinkMap> = RefCell::new(HashMap::new());
    /// 本线程本帧是否注入 OSC 8。
    static ENABLED: FlagCell<bool> = const { FlagCell::new(false) };
    /// 本帧最终 buffer 的链接属性和需要额外补画的格子，供后端消费一次。
    static PREPARED: RefCell<Option<PreparedFrame>> = const { RefCell::new(None) };
}

static NEXT_NAMESPACE: AtomicU64 = AtomicU64::new(1);

/// 后端独立持有已输出的链接基线；克隆只增加 Arc 引用计数。
#[derive(Clone)]
pub struct Frame {
    links: Arc<LinkMap>,
    namespace: u64,
    invalidated: Arc<HashSet<Position>>,
}

impl Default for Frame {
    fn default() -> Self {
        Self {
            links: Arc::new(HashMap::new()),
            namespace: NEXT_NAMESPACE.fetch_add(1, Ordering::Relaxed),
            invalidated: Arc::new(HashSet::new()),
        }
    }
}

pub(super) struct PreparedFrame {
    pub frame: Frame,
    pub redraws: Vec<(u16, u16, Cell)>,
}

impl Frame {
    /// 在所有组件和选区绘制后、ratatui 差分前完成链接样式与属性差分。
    /// 不改 symbol；补画只复制可落笔的 grapheme 首格，不能擦写宽字续格。
    pub fn prepare(&self, buffer: &mut Buffer) {
        let mut links = LINKS.with(|table| table.take());
        links.retain(|&(x, y), _| buffer.area.contains((x, y).into()));
        let mut redraws = Vec::new();
        if !links.is_empty() || !self.links.is_empty() || !self.invalidated.is_empty() {
            for y in buffer.area.top()..buffer.area.bottom() {
                let mut covered_until = buffer.area.left();
                for x in buffer.area.left()..buffer.area.right() {
                    let position = (x, y);
                    if x < covered_until {
                        links.remove(&position);
                        continue;
                    }
                    let cell = &mut buffer[position];
                    covered_until = x.saturating_add(cell.symbol().width().max(1) as u16);
                    if cell.skip {
                        links.remove(&position);
                        continue;
                    }
                    if links.get(&position).is_some_and(|link| link.recolor) {
                        cell.set_fg(LINK_COLOR);
                    }
                    // 旧链接可能位于当前宽字的续格；失败重试仍需重画覆盖它的首格。
                    let invalidated =
                        (x..covered_until).any(|column| self.invalidated.contains(&(column, y)));
                    if self.links.get(&position) != links.get(&position) || invalidated {
                        redraws.push((x, y, cell.clone()));
                    }
                }
            }
        }
        PREPARED.with(|prepared| {
            prepared.replace(Some(PreparedFrame {
                frame: Self {
                    links: Arc::new(links),
                    namespace: self.namespace,
                    invalidated: Arc::new(HashSet::new()),
                },
                redraws,
            }));
        });
    }

    /// 写入/刷新失败或清屏后，物理终端可能处于任一状态；下一帧重画所有涉及格子。
    pub(super) fn invalidate(&mut self, attempted: &Self) {
        let invalidated = Arc::make_mut(&mut self.invalidated);
        invalidated.extend(self.links.keys().copied());
        invalidated.extend(attempted.links.keys().copied());
        invalidated.extend(attempted.invalidated.iter().copied());
    }

    /// 只装饰真正写出的副本，原 buffer 与链接色均已由 prepare 确定。
    pub(super) fn decorate(&self, x: u16, y: u16, cell: &Cell) -> Option<Cell> {
        if cell.skip {
            return None;
        }
        let link = self.links.get(&(x, y))?;
        let mut decorated = cell.clone();
        decorated.set_symbol(&format!(
            "{}{}{}",
            open(&link.url, self.namespace, link.id),
            cell.symbol(),
            CLOSE
        ));
        Some(decorated)
    }
}

pub(super) fn take_prepared_frame(baseline: &Frame) -> Option<PreparedFrame> {
    PREPARED.with(|prepared| {
        let mut prepared = prepared.borrow_mut();
        if prepared
            .as_ref()
            .is_some_and(|prepared| prepared.frame.namespace == baseline.namespace)
        {
            prepared.take()
        } else {
            None
        }
    })
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
    LINKS.with(|table| table.borrow_mut().clear());
    PREPARED.with(|prepared| prepared.take());
}

/// 登记一个链接 cell。未启用时什么也不做，避免白白攒一张不会被读的表。
///
/// `recolor` 为 `false` 时只包 OSC 8、保留 cell 自身前景色（附件引用行等
/// 由正文色承载的链接）；颜色在 [`Frame::prepare`] 中处理。
pub fn register(x: u16, y: u16, url: &str, recolor: bool, id: LinkId) {
    if !ENABLED.with(FlagCell::get) {
        return;
    }
    LINKS.with(|table| {
        table.borrow_mut().insert(
            (x, y),
            Link {
                url: url.to_string(),
                recolor,
                id,
            },
        )
    });
}

/// OSC 8 开序列：`ESC ] 8 ; params ; URI BEL`。
///
/// 用 BEL 收尾与 CC `OSC8_END` 一致（其注释称兼容性更广）。
///
/// 安全：URI 中任何控制字符（含 `ESC`、`BEL`、换行）都会被剥离。否则一个
/// 含 `\x07` 或 `\x1b\\` 的 URL 能提前终止 OSC 8 序列并注入任意终端指令。
fn open(url: &str, namespace: u64, id: LinkId) -> String {
    let safe: String = url.chars().filter(|ch| !ch.is_control()).collect();
    format!(
        "\x1b]8;id=cc-code-{}-{namespace}-{}-{}-{};{safe}\x07",
        std::process::id(),
        id.0,
        id.1,
        id.2
    )
}

/// OSC 8 关序列。
const CLOSE: &str = "\x1b]8;;\x07";

#[cfg(test)]
#[path = "hyperlink_test.rs"]
mod tests;
