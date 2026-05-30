//! 流式 Markdown 表格 Holdback 扫描器
//!
//! 在增量渲染过程中，检测 Markdown 表格行并判断是否应 holdback（暂不提交渲染）。
//! 表格行以 `|` 开头，列数由表头决定。流式过程中不完整的行（列数不足或行未以 `|` 或换行结束）
//! 保持 holdback 状态，直到行完整或流结束。

/// Holdback 决策
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HoldbackDecision {
    /// 全部提交渲染，无 holdback
    Commit,
    /// 从 `holdback_offset` 开始到文本末尾 holdback，只提交 offset 之前的内容
    Hold { holdback_offset: usize },
    /// 流结束，强制提交所有 holdback 内容（包括不完整的表格）
    FlushAll,
}

/// Markdown 表格行扫描器状态
///
/// 保留完整的有限状态机定义供后续增强使用（如跨多行增量追踪表格结构）。
#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
enum TableScanState {
    /// 不在表格模式中
    #[default]
    Idle,
    /// 已看到表头行，期望分隔行
    HeaderSeen {
        /// 表头列数
        col_count: usize,
    },
    /// 在表格数据区域中
    InTable {
        /// 表格列数（由表头决定）
        col_count: usize,
    },
}

/// 流式 Markdown 表格 holdback 扫描器
///
/// 维护跨多次 `scan()` 调用的表格检测状态。每次调用传入当前完整的 raw 文本，
/// scanner 判断尾部是否存在不完整的表格行需要 holdback。
#[derive(Debug, Clone, Default)]
pub struct TableHoldbackScanner {
    /// 当前扫描状态
    state: TableScanState,
    /// 上次 scan 的文本长度（用于增量扫描）
    last_scan_len: usize,
    /// 是否处于流式状态（streaming=true 时启用 holdback；false 时总是 FlushAll）
    streaming: bool,
}

impl TableHoldbackScanner {
    /// 创建新的扫描器
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置流式状态
    pub fn set_streaming(&mut self, streaming: bool) {
        self.streaming = streaming;
    }

    /// 扫描文本，决定 holdback 策略
    ///
    /// `text` 是当前 ContentBlockView::Text 的完整 raw 文本。
    /// 返回 HoldbackDecision 告知调用方如何处理。
    pub fn scan(&mut self, text: &str) -> HoldbackDecision {
        // 非流式模式总是全部提交
        if !self.streaming {
            self.last_scan_len = text.len();
            return HoldbackDecision::FlushAll;
        }

        // 增量扫描：只处理新增部分
        if text.len() <= self.last_scan_len {
            // 无新内容
            return HoldbackDecision::Commit;
        }

        let _new_text = &text[self.last_scan_len..];
        self.last_scan_len = text.len();

        // 逐行扫描新增文本
        self.process_text(text)
    }

    /// 重置扫描器状态（新消息或 finalize 时调用）
    pub fn reset(&mut self) {
        self.state = TableScanState::Idle;
        self.last_scan_len = 0;
    }
}

/// 辅助函数：计算 `|` 分隔的列数
///
/// 对于 `"| a | b | c |"` 返回 3。
/// 对于 `"| a | b | c"` 也返回 3（尾部 `|` 可选）。
/// 非 `|` 开头的行返回 0。
fn count_pipe_columns(line: &str) -> usize {
    let trimmed = line.trim();
    if !trimmed.starts_with('|') {
        return 0;
    }
    // 去掉首尾 `|` 后按 `|` 分隔
    let inner = trimmed.trim_start_matches('|').trim_end_matches('|');
    if inner.is_empty() {
        return 0;
    }
    inner.split('|').count()
}

/// 判断行是否为表格分隔行（如 `|---|---|`）
fn is_separator_row(line: &str) -> bool {
    let trimmed = line.trim();
    if !trimmed.starts_with('|') {
        return false;
    }
    // 去掉首尾 `|`
    let inner = trimmed.trim_start_matches('|').trim_end_matches('|');
    // 每个单元格只含 `-`、`:`、空格
    inner.split('|').all(|cell| {
        let c = cell.trim();
        !c.is_empty() && c.chars().all(|ch| ch == '-' || ch == ':' || ch == ' ') && c.contains('-')
    })
}

/// 判断行是否以 `|` 开头（可能是表格行）
fn is_table_line(line: &str) -> bool {
    line.trim().starts_with('|')
}

impl TableHoldbackScanner {
    /// 处理完整文本，检测末尾不完整的表格行
    fn process_text(&self, full_text: &str) -> HoldbackDecision {
        let lines: Vec<&str> = full_text.lines().collect();
        if lines.is_empty() {
            return HoldbackDecision::Commit;
        }

        // 从末尾向前找连续的表格行
        let mut table_start = lines.len();

        for i in (0..lines.len()).rev() {
            if is_table_line(lines[i]) || is_separator_row(lines[i]) {
                table_start = i;
            } else {
                break;
            }
        }

        // 末尾没有表格行
        if table_start == lines.len() {
            return HoldbackDecision::Commit;
        }

        // 检查文本末尾是否以换行结束（行是否完整）
        let last_line_complete = full_text.ends_with('\n');

        if !last_line_complete {
            // 最后一行未结束，检查它是否是表格行
            let last_line = lines.last().unwrap();
            if is_table_line(last_line) {
                // 不完整的表格行，需要 holdback
                let holdback_offset = find_line_start_offset(full_text, lines.len() - 1);
                return HoldbackDecision::Hold { holdback_offset };
            }
        }

        // 所有行都完整（以 `\n` 结尾或最后不是表格行）
        // 检查最后一个表格行是否列数完整
        let header_col_count = find_header_col_count(&lines[table_start..]);

        if header_col_count > 0 {
            // 检查每个数据行是否列数足够
            for (idx, line) in lines[table_start..].iter().enumerate() {
                if is_separator_row(line) {
                    continue;
                }
                if is_table_line(line) {
                    let cols = count_pipe_columns(line);
                    if cols > 0 && cols < header_col_count {
                        // 有不完整的行，holdback 从该行开始
                        let global_idx = table_start + idx;
                        let holdback_offset = find_line_start_offset(full_text, global_idx);
                        return HoldbackDecision::Hold { holdback_offset };
                    }
                }
            }
        }

        HoldbackDecision::Commit
    }
}

/// 在完整文本中找到第 line_idx 行的字节起始偏移
fn find_line_start_offset(text: &str, line_idx: usize) -> usize {
    let mut offset = 0;
    for (i, line) in text.lines().enumerate() {
        if i == line_idx {
            return offset;
        }
        offset += line.len();
        if offset < text.len() && text.as_bytes()[offset] == b'\n' {
            offset += 1;
        }
    }
    offset
}

/// 从表格行片段中找到表头的列数
fn find_header_col_count(table_lines: &[&str]) -> usize {
    for line in table_lines {
        if !is_separator_row(line) && is_table_line(line) {
            return count_pipe_columns(line);
        }
    }
    0
}

#[cfg(test)]
#[path = "table_holdback_test.rs"]
mod tests;
