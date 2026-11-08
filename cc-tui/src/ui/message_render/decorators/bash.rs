use super::output_line::try_json_format_lines;
use super::ToolDisplayLine;
use crate::ui::branch_prefix::BranchPrefix;

pub const BRANCH_PREFIX: &str = BranchPrefix::PREFIX;
pub const INDENT_PREFIX: &str = BranchPrefix::HANG;

/// 正文源行窗口：折叠态只对可见部分做 JSON 美化（issue #645）。
///
/// JSON 美化是逐行独立、保序的变换，窗口外的源行不在显示区，跳过
/// 它们的美化对可见输出前缀零影响；折叠态由此省掉 90%+ 的无效解析。
#[derive(Debug, Clone, Copy)]
pub enum BodyWindow {
    /// 全量（详细模式 / 全量展开渲染）。
    All,
    /// 头部源行窗口，行数为上限。N 个源行至少展开 N 个显示行，
    /// 足够覆盖折叠态前 N 个视觉行。
    HeadLines(usize),
    /// 头部源行窗口，按累计字符数取到 ≥ n（多取到恰好满足的那行），
    /// 足够覆盖按字符截断的显示前缀（错误摘要 400 字符）。
    HeadChars(usize),
    /// 尾部源行窗口，行数为上限。覆盖流式实时输出「只显示最后几行」。
    TailLines(usize),
}

impl BodyWindow {
    /// 在源行序列上取窗口 `[start, end)`。
    fn slice(self, lines: &[&str]) -> (usize, usize) {
        let total = lines.len();
        match self {
            BodyWindow::All => (0, total),
            BodyWindow::HeadLines(n) => (0, n.min(total)),
            BodyWindow::HeadChars(n) => {
                let mut chars = 0usize;
                let mut end = 0usize;
                while end < total {
                    chars += lines[end].chars().count();
                    end += 1;
                    if chars >= n {
                        break;
                    }
                }
                (0, end)
            }
            BodyWindow::TailLines(n) => (total.saturating_sub(n), total),
        }
    }
}

/// [`transform_bash_output_lines`] 的产物：渲染行 + 源行统计。
pub struct BashOutputLines<'a> {
    pub lines: Vec<ToolDisplayLine<'a>>,
    /// 正文源行总数（用户口径，`lines()` 计数）。
    pub source_total: usize,
    /// 进入美化的源行数（[`BodyWindow`] 窗口内）。
    pub source_taken: usize,
}

/// 从 Bash 工具输出中解析 exit code。
///
/// 匹配格式：`[Exit code: N]` 或 `[Command completed with exit code N]`
/// 解析失败返回 None。
pub fn parse_exit_code(content: &str) -> Option<i32> {
    if let Some(pos) = content.rfind("[Exit code: ") {
        let rest = &content[pos + "[Exit code: ".len()..];
        if let Some(end) = rest.find(']') {
            if let Ok(code) = rest[..end].trim().parse::<i32>() {
                return Some(code);
            }
        }
    }
    if let Some(pos) = content.rfind("exit code ") {
        let rest = &content[pos + "exit code ".len()..];
        if let Some(end) = rest.find(']') {
            if let Ok(code) = rest[..end].trim().parse::<i32>() {
                return Some(code);
            }
        }
    }
    None
}

/// 剥离命令输出末尾由底层追加的退出码标记及紧邻的空行。
pub fn strip_exit_code_suffix(content: &str) -> &str {
    let trimmed = content.trim_end();
    if let Some(pos) = trimmed.rfind("[Exit code: ") {
        if trimmed[pos..].ends_with(']') {
            return trimmed[..pos].trim_end();
        }
    }
    if let Some(pos) = trimmed.rfind("[Command completed with exit code ") {
        if trimmed[pos..].ends_with(']') {
            return trimmed[..pos].trim_end();
        }
    }
    trimmed
}

/// 将 Bash 命令输出转换为结构化渲染行。
///
/// 1. 失败时（非零退出码）：
///    - 首行提升为 `Error: Exit code {code}` 并挂在树形折角 [`BranchPrefix::PREFIX`]
///    - 剥离末尾冗余的 `[Exit code: {code}]` 标记
///    - 正文（stdout/stderr）作为子内容统一以 [`BranchPrefix::HANG`] 缩进
/// 2. 成功时（0 或无退出码）：
///    - 首行挂 [`BranchPrefix::PREFIX`]，后续行 [`BranchPrefix::HANG`] 缩进
///
/// 正文先经 [`try_json_format_lines`] 尝试 JSON 美化（对齐 CC
/// `OutputLine` → `tryJsonFormatContent`）：单行 JSON 会展开成 2 空格缩进的多行。
/// `window` 限定参与美化的源行（折叠态先截断再美化，见 issue #645）。
pub fn transform_bash_output_lines<'a>(
    content: &'a str,
    exit_code: Option<i32>,
    error_header_buf: &'a mut String,
    window: BodyWindow,
) -> BashOutputLines<'a> {
    let mut lines = Vec::new();
    let mut source_total = 0usize;
    let mut source_taken = 0usize;

    if let Some(code) = exit_code.filter(|&c| c != 0) {
        *error_header_buf = format!("Error: Exit code {code}");
        lines.push(ToolDisplayLine {
            prefix: BRANCH_PREFIX,
            text: std::borrow::Cow::Borrowed(error_header_buf.as_str()),
            is_error_header: true,
        });

        let clean_body = strip_exit_code_suffix(content);
        if !clean_body.is_empty() {
            let (total, taken) = push_body_lines(&mut lines, clean_body, true, window);
            source_total = total;
            source_taken = taken;
        }
    } else if !content.is_empty() {
        let (total, taken) = push_body_lines(&mut lines, content, false, window);
        source_total = total;
        source_taken = taken;
    }

    BashOutputLines {
        lines,
        source_total,
        source_taken,
    }
}

/// 把正文行追加成 [`ToolDisplayLine`]，返回（源行总数，窗口内源行数）。
///
/// `reserve_first` 为真时把第一行留给已有的错误头（`Error: Exit code N`），
/// 正文首行改用缩进前缀。JSON 美化产生的行是 owned（原输出里并不存在），
/// 未美化时继续借用 `body`，零拷贝。
fn push_body_lines<'a>(
    lines: &mut Vec<ToolDisplayLine<'a>>,
    body: &'a str,
    reserve_first: bool,
    window: BodyWindow,
) -> (usize, usize) {
    // 美化口径用 split('\n')（与 try_json_format_content 内部一致），
    // 未美化口径用 lines()（丢尾空行），与旧全量行为逐字节一致。
    let split_src: Vec<&str> = body.split('\n').collect();
    let (start, end) = window.slice(&split_src);
    // 长度守卫按全文口径：超上限整段跳过美化，防止折叠态把原本
    // 不美化的内容漂移成美化版。
    let pretty = try_json_format_lines(&split_src[start..end], body.chars().count());
    let taken = end - start;
    match pretty {
        Some(text) => {
            for (i, line) in text.lines().enumerate() {
                lines.push(ToolDisplayLine {
                    prefix: body_prefix(i, reserve_first),
                    text: std::borrow::Cow::Owned(line.to_string()),
                    is_error_header: false,
                });
            }
        }
        None => {
            let line_src: Vec<&str> = body.lines().collect();
            let (start, end) = window.slice(&line_src);
            for (i, line) in line_src[start..end].iter().enumerate() {
                lines.push(ToolDisplayLine {
                    prefix: body_prefix(i, reserve_first),
                    text: std::borrow::Cow::Borrowed(line),
                    is_error_header: false,
                });
            }
        }
    }
    (body.lines().count(), taken)
}

/// 正文行的树形前缀：只有「整段结果的第一行」用分支折角 [`BranchPrefix::PREFIX`]，
/// 其余一律缩进。`reserve_first` 为真表示首行已被错误头占用，正文全用缩进。
fn body_prefix(index: usize, reserve_first: bool) -> &'static str {
    if index == 0 && !reserve_first {
        BRANCH_PREFIX
    } else {
        INDENT_PREFIX
    }
}

#[cfg(test)]
#[path = "bash_test.rs"]
mod bash_test;
