//! 高度只用于滚动定位；可见区域始终由真实 renderer 计算。
use super::super::message_view::{ContentBlockView, MessageViewModel, ToolPhase};

#[derive(Clone)]
pub(super) struct LayoutSummary {
    pub detail_rows: usize,
    pub detail_bytes: usize,
    compact_rows: usize,
    visible: bool,
}
impl LayoutSummary {
    pub fn visible(&self, _detail: bool) -> bool {
        self.visible
    }
    pub fn height(&self, width: u16, detail: bool, first: bool) -> usize {
        if !self.visible {
            return usize::from(first);
        }
        let rows = if detail {
            self.detail_rows
        } else {
            self.compact_rows
        };
        // 长行可能高估或低估，禁止用此值跳过实际可见消息。
        rows.saturating_mul(80)
            .div_ceil(usize::from(width).clamp(20, 80))
            .max(1)
    }
}
fn rows(text: &str) -> usize {
    text.as_bytes().iter().filter(|b| **b == b'\n').count()
        + usize::from(!text.is_empty() && !text.ends_with('\n'))
}

pub(super) fn summarize(vm: &MessageViewModel) -> LayoutSummary {
    let (detail, compact, bytes, visible) = match vm {
        MessageViewModel::UserBubble {
            content,
            expanded_content,
            ..
        } => {
            let expanded = expanded_content.as_deref().unwrap_or(content);
            (rows(expanded) + 2, rows(content) + 2, expanded.len(), true)
        }
        MessageViewModel::AssistantBubble { blocks, .. } => {
            let mut detail = 2;
            let mut compact = 2;
            let mut bytes = 0;
            for block in blocks {
                match block {
                    ContentBlockView::Text { raw, rendered, .. } => {
                        detail += rows(raw).max(rendered.lines.len());
                        compact += rendered.lines.len();
                        bytes += raw.len();
                    }
                    ContentBlockView::Reasoning { text, .. } => {
                        detail += rows(text) + 1;
                        compact += 1;
                        bytes += text.len();
                    }
                    ContentBlockView::ToolUse { .. } => {
                        detail += 1;
                        compact += 1;
                    }
                }
            }
            (detail, compact, bytes, true)
        }
        MessageViewModel::ToolBlock {
            tool_name,
            content,
            args_display,
            is_error,
            phase,
            source_agent_id,
            started_at,
            timeout_ms,
            shell_backgrounded,
            ..
        } => {
            let args = args_display.as_deref().unwrap_or("");
            let header = 2 + rows(args) + args.len() / 80;
            let output = if tool_name == "Read" && !is_error {
                1
            } else {
                rows(content)
            };
            let status_rows = if tool_name == "Bash" {
                if *shell_backgrounded {
                    1 + usize::from(timeout_ms.is_some())
                } else if matches!(*phase, ToolPhase::Queued | ToolPhase::AwaitingApproval) {
                    1 + usize::from(source_agent_id.is_none())
                } else if *phase == ToolPhase::Running
                    && started_at.is_some_and(|t| t.elapsed() >= std::time::Duration::from_secs(2))
                {
                    1 + usize::from(source_agent_id.is_none())
                } else if *phase == ToolPhase::Finished {
                    usize::from(content.is_empty()) + usize::from(timeout_ms.is_some())
                } else {
                    0
                }
            } else {
                usize::from(*phase == ToolPhase::Finished && content.is_empty())
            };
            (
                header + output + status_rows,
                header.min(3) + output.min(3) + status_rows,
                content.len() + args.len(),
                true,
            )
        }
        MessageViewModel::ShellCommand {
            command,
            stdin,
            stdout,
            stderr,
            ..
        } => {
            let count = rows(stdout) + rows(stderr) + stdin.iter().map(|s| rows(s)).sum::<usize>();
            (
                rows(command) + count + 4,
                count.min(3) + 3,
                command.len()
                    + stdout.len()
                    + stderr.len()
                    + stdin.iter().map(String::len).sum::<usize>(),
                true,
            )
        }
        MessageViewModel::SystemNote {
            content,
            detail_content,
            ..
        } => {
            let full = detail_content.as_deref().unwrap_or(content);
            (rows(full) + 1, rows(content) + 1, full.len(), true)
        }
        MessageViewModel::CacheWarning { content, .. } => {
            (rows(content) + 1, rows(content) + 1, content.len(), true)
        }
        MessageViewModel::ToolCallGroup { tools, .. } => {
            let count = tools
                .iter()
                .map(|tool| {
                    if tool.tool_name == "Read" {
                        3
                    } else {
                        rows(&tool.content) + 3
                    }
                })
                .sum::<usize>();
            (
                count + 1,
                4,
                tools.iter().map(|t| t.content.len()).sum(),
                true,
            )
        }
        MessageViewModel::SubAgentGroup {
            recent_messages,
            final_result,
            task_preview,
            batch_agents,
            ..
        } => {
            let children: Vec<_> = recent_messages.iter().map(summarize).collect();
            (
                4 + batch_agents.len()
                    + children.iter().map(|s| s.detail_rows).sum::<usize>()
                    + final_result.as_deref().map_or(0, rows),
                4 + children.len(),
                task_preview.len()
                    + final_result.as_ref().map_or(0, String::len)
                    + children.iter().map(|s| s.detail_bytes / 6).sum::<usize>(),
                true,
            )
        }
    };
    LayoutSummary {
        detail_rows: detail,
        compact_rows: compact,
        detail_bytes: bytes.saturating_mul(6),
        visible,
    }
}

pub(in crate::ui) fn is_live(vm: &MessageViewModel) -> bool {
    match vm {
        MessageViewModel::ToolBlock {
            phase,
            is_error,
            tool_name,
            ..
        } => *phase != ToolPhase::Finished && !*is_error && tool_name != "AskUserQuestion",
        MessageViewModel::ShellCommand { exit_code, .. } => exit_code.is_none(),
        MessageViewModel::SubAgentGroup { is_running, .. } => *is_running,
        _ => false,
    }
}
