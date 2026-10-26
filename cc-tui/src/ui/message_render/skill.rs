use ratatui::{
    style::Style,
    text::{Line, Span},
};
use serde::Deserialize;
use unicode_width::UnicodeWidthStr;

use super::{error_summary_lines, truncate_to_display_width, ERROR_SUMMARY_MAX_LINES};
use crate::{
    app::tool_display::sanitize_display_text,
    ui::{branch_prefix::BranchPrefix, message_view::ToolPhase, theme},
};

/// 只反序列化展示字段，技能指令仍完整保留在原始 tool_result 中。
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SkillResultSummary {
    success: bool,
    #[serde(default)]
    allowed_tools: Option<Vec<String>>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    status: Option<String>,
}

/// 详细视图展示原始 Markdown，避免 JSON 转义将整篇指令挤在一行中。
pub(super) fn instruction_for_display(content: &str) -> Option<String> {
    #[derive(Deserialize)]
    struct SkillInstruction {
        instruction: String,
    }
    serde_json::from_str::<SkillInstruction>(content)
        .ok()
        .map(|result| result.instruction)
}

/// 对齐 CC：工具头下只显示一行结果；显式展开及详细模式仍展示完整指令。
pub(super) fn render_collapsed_skill(
    args: Option<&str>,
    content: &str,
    is_error: bool,
    phase: ToolPhase,
    width: usize,
    tick: u64,
) -> Vec<Line<'static>> {
    let result = serde_json::from_str::<SkillResultSummary>(content).ok();
    let failed = is_error || result.as_ref().is_some_and(|result| !result.success);
    let (status, summary) = if failed {
        (
            cc_widgets::ToolCallStatus::Failed,
            "Skill failed".to_string(),
        )
    } else {
        match phase {
            ToolPhase::Queued | ToolPhase::AwaitingApproval => (
                cc_widgets::ToolCallStatus::Pending,
                "Initializing…".to_string(),
            ),
            ToolPhase::Running => (
                cc_widgets::ToolCallStatus::Running,
                "Initializing…".to_string(),
            ),
            ToolPhase::Finished => {
                let summary = match result {
                    Some(result) if result.status.as_deref() == Some("forked") => {
                        "Done".to_string()
                    }
                    Some(result) => {
                        let mut parts = vec!["Successfully loaded skill".to_string()];
                        if let Some(tools) = result.allowed_tools.filter(|tools| !tools.is_empty())
                        {
                            let count = tools.len();
                            let unit = if count == 1 { "tool" } else { "tools" };
                            parts.push(format!("{count} {unit} allowed"));
                        }
                        if let Some(model) = result.model.filter(|model| !model.is_empty()) {
                            parts.push(sanitize_display_text(&model));
                        }
                        parts.join(" · ")
                    }
                    None => "Skill completed".to_string(),
                };
                (cc_widgets::ToolCallStatus::Completed, summary)
            }
        }
    };
    let (indicator, color) = cc_widgets::tool_call::display::format_indicator(
        status,
        tick,
        theme::cur().inactive,
        theme::cur().success,
        theme::cur().error,
    );
    let args = sanitize_display_text(args.unwrap_or_default());
    let prefix = format!("{indicator} Skill");
    let prefix_width = UnicodeWidthStr::width(prefix.as_str());
    let has_args = !args.is_empty() && width > prefix_width + 2;
    let mut header = vec![
        Span::styled(indicator.to_string(), Style::default().fg(color)),
        Span::styled(" Skill", Style::default().fg(theme::cur().text_soft)),
    ];
    if has_args {
        let args_width = width.saturating_sub(prefix_width + 2);
        header.push(Span::styled(
            format!("({})", truncate_to_display_width(&args, args_width)),
            Style::default().fg(theme::cur().text_soft),
        ));
    }
    // 极窄视口只绘制能容纳的前缀，不让测量结果超过宿主宽度。
    let mut lines = if width < prefix_width {
        vec![Line::from(Span::styled(
            truncate_to_display_width(&prefix, width),
            Style::default().fg(color),
        ))]
    } else {
        vec![Line::from(header)]
    };
    lines.push(Line::from(vec![
        Span::styled(
            truncate_to_display_width(BranchPrefix::PREFIX, width),
            Style::default().fg(if failed {
                theme::cur().error
            } else {
                theme::cur().inactive
            }),
        ),
        // 对齐 CC SkillTool 的 `<Text>`（无 dimColor，只有 Byline 的 ` · ` 分隔符是 dim）：
        // 结果文案不压暗、也不用纯白 text，取 text_soft 贴近 CC 继承的终端默认前景。
        Span::styled(
            truncate_to_display_width(&summary, width.saturating_sub(BranchPrefix::WIDTH)),
            Style::default().fg(if failed {
                theme::cur().error
            } else {
                theme::cur().text_soft
            }),
        ),
    ]));
    if failed && !content.trim().is_empty() {
        lines.extend(error_summary_lines(content, width, ERROR_SUMMARY_MAX_LINES));
    }
    lines
}

#[cfg(test)]
#[path = "skill_test.rs"]
mod tests;
