//! #629 S2：detail 模式下单条消息的视觉行数估算器。
//!
//! 用途：虚拟化策略的 Estimated 段高度，以及 `detail_render_strategy = auto`
//! 的档位判定（估算总行数 vs 阈值）。估算只允许读 VM 的原始文本，成本
//! O（字符数），不得触发任何渲染。
//!
//! 校准口径（逐类型，单测对 `render_one` 的 detail 实测锁定；
//! 渲染器行为变更必须同步复核本表。2026-10-20 生产形状校准，
//! 与 #629 研究阶段 bench fixture 的常数有出入时以本表为准）：
//! - Bash / ShellCommand 输出：按内容行逐行折算（宽度感知），近乎精确；
//! - Read 工具：detail 下只呈现约 3 行（常数）；
//! - Write 新建文件：新内容折行 + 2（代码预览逐行渲染）；
//! - Edit 类 diff：新旧片段总行数 ×1.05 + 6（输入本就是变更片段，
//!   diff 按 hunk 渲染，膨胀只来自折行与 hunk 头/上下文）；
//! - Assistant 文本：markdown 折行 ×1.15（展开膨胀与空行折叠相抵后的净值）；
//! - Assistant Reasoning：按显示宽度折行（CJK 双宽，字数口径不成立；
//!   无全文时回退字数 / 55）；
//! - User：内容折行 + 1。

use super::message_view::{ContentBlockView, MessageViewModel};
use unicode_width::UnicodeWidthStr;

/// 文本在给定列宽下的视觉行数（逐逻辑行按显示宽度折行，空行计 1）。
fn wrap_rows(text: &str, width: usize) -> usize {
    let w = width.max(1);
    text.split('\n')
        .map(|line| {
            // ASCII 快速路径：纯 ASCII 行宽 = 字节数（\t 按 1 计，估算可接受），
            // 避免对日志/代码这类主体内容逐字查 unicode 宽度表（冷开估算的热点）。
            let dw = if line.is_ascii() {
                line.len()
            } else {
                UnicodeWidthStr::width(line)
            };
            dw.div_ceil(w).max(1)
        })
        .sum()
}

/// 非空文本的逻辑行数（空串为 0）。
fn text_lines(s: &str) -> usize {
    if s.is_empty() {
        0
    } else {
        s.lines().count()
    }
}

/// 估算一条消息在 detail 模式、给定终端宽度下的视觉行数。
pub fn estimate_detail_visual_rows(vm: &MessageViewModel, width: usize) -> usize {
    let bubble_w = width.saturating_sub(2).max(20);
    let tool_w = width.saturating_sub(4).max(20);
    match vm {
        MessageViewModel::UserBubble {
            content,
            expanded_content,
            ..
        } => {
            let mut rows = wrap_rows(content, bubble_w) + 1;
            if let Some(expanded) = expanded_content {
                rows += wrap_rows(expanded, bubble_w);
            }
            rows
        }
        MessageViewModel::AssistantBubble { blocks, .. } => {
            let body: usize = blocks
                .iter()
                .map(|b| match b {
                    ContentBlockView::Text { raw, .. } => {
                        (wrap_rows(raw, bubble_w) as f64 * 1.15).ceil() as usize
                    }
                    ContentBlockView::Reasoning {
                        char_count, text, ..
                    } => {
                        if !text.is_empty() {
                            wrap_rows(text, bubble_w)
                        } else {
                            char_count.div_ceil(55).max(1)
                        }
                    }
                    ContentBlockView::ToolUse { .. } => 1,
                })
                .sum();
            body + 1
        }
        MessageViewModel::ToolBlock {
            tool_name,
            content,
            diff_input,
            ..
        } => {
            if let Some(diff) = diff_input {
                if diff.is_new_file {
                    return wrap_rows(&diff.new_content, tool_w) + 2;
                }
                let lines = text_lines(&diff.old_content) + text_lines(&diff.new_content);
                return (lines as f64 * 1.05).ceil() as usize + 6;
            }
            match tool_name.as_str() {
                // Read 在 detail 下只呈现约 3 行（文件体不展开）
                "Read" => 4,
                _ => 1 + wrap_rows(content, tool_w) + 1,
            }
        }
        MessageViewModel::ShellCommand {
            command,
            stdout,
            stderr,
            ..
        } => {
            1 + wrap_rows(command, tool_w)
                + wrap_rows(stdout, tool_w)
                + wrap_rows(stderr, tool_w)
                + 1
        }
        MessageViewModel::SystemNote {
            content,
            detail_content,
            ..
        } => wrap_rows(detail_content.as_deref().unwrap_or(content), bubble_w) + 1,
        MessageViewModel::CacheWarning { content, .. } => wrap_rows(content, bubble_w) + 1,
        MessageViewModel::ToolCallGroup { tools, .. } => {
            tools
                .iter()
                .map(|t| 2 + wrap_rows(&t.content, tool_w))
                .sum::<usize>()
                + 1
        }
        MessageViewModel::SubAgentGroup {
            recent_messages, ..
        } => {
            2 + recent_messages
                .iter()
                .map(|m| estimate_detail_visual_rows(m, width))
                .sum::<usize>()
        }
    }
}

/// 估算整段历史在 detail 模式下的总视觉行数（auto 档位判定用）。
pub fn estimate_total_detail_rows(messages: &[MessageViewModel], width: usize) -> usize {
    messages
        .iter()
        .map(|m| estimate_detail_visual_rows(m, width))
        .sum()
}

/// `detail_render_strategy = auto` 的 Virtual 阈值（估算 detail 总行数）。
/// 约对应本机实测构成下 ~1,800 条消息总量的历史规模（#629 研究数据）。
pub const AUTO_VIRTUAL_THRESHOLD_ROWS: usize = 300_000;

/// 解析 detail 渲染策略为「是否 Virtual」：
/// - `virtual` 恒 true，`cache` 恒 false；
/// - `auto` / 未知值：估算总行数达阈值则 true；会话内单调——
///   已经 Virtual 的不因估算波动退回（历史只增不减，判定不抖动）。
pub fn resolve_detail_virtual(
    setting: Option<&str>,
    estimated_total_rows: usize,
    already_virtual: bool,
) -> bool {
    match setting {
        Some("virtual") => true,
        Some("cache") => false,
        _ => already_virtual || estimated_total_rows >= AUTO_VIRTUAL_THRESHOLD_ROWS,
    }
}
