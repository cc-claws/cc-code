//! `/compact` 命令 — 手动触发上下文压缩。
//!
//! 移植自 `peri-tui/src/acp_server/compact.rs`，
//! 改为接收 [`CommandContext`]、返回 [`CommandResult`]。

use std::sync::Arc;

use peri_agent::{
    agent::{
        compact::{full_compact, re_inject},
        events::{AgentEvent as ExecutorEvent, CompactFileInfo},
    },
    messages::BaseMessage,
};
use tracing::{info, warn};

use super::{AgentCommand, CommandContext, CommandKind, CommandResult};
use crate::session::executor::PromptStopReason;

/// 手动 compact 命令。
pub struct CompactCommand;

impl CompactCommand {
    pub const NAME: &'static str = "compact";
}

#[async_trait::async_trait]
impl AgentCommand for CompactCommand {
    fn name(&self) -> &str {
        Self::NAME
    }

    fn aliases(&self) -> Vec<&str> {
        vec!["compress"]
    }

    fn description(&self) -> &str {
        "压缩对话历史以释放上下文空间"
    }

    fn kind(&self) -> CommandKind {
        CommandKind::Immediate
    }

    async fn execute(&self, ctx: CommandContext) -> CommandResult {
        let CommandContext {
            session_id,
            history,
            cwd,
            peri_config,
            compact_model,
            event_sink,
            ..
        } = ctx;

        tracing::debug!(history_len = history.len(), "compact: execute called");

        if history.is_empty() {
            warn!("compact: 无历史消息可压缩");
            event_sink
                .push_event(
                    &session_id,
                    &ExecutorEvent::CompactError {
                        message: "no history to compact".into(),
                    },
                    0,
                )
                .await;
            return CommandResult {
                messages: history,
                stop_reason: PromptStopReason::EndTurn,
            };
        }

        // compact 配置
        let mut compact_config = peri_config.config.compact.clone().unwrap_or_default();
        compact_config.apply_env_overrides();

        // 获取 compact model
        let compact_model: Arc<dyn peri_agent::llm::BaseModel> = match compact_model {
            Some(m) => m,
            None => {
                warn!("compact: 无可用模型");
                event_sink
                    .push_event(
                        &session_id,
                        &ExecutorEvent::CompactError {
                            message: "no model available for compact".into(),
                        },
                        0,
                    )
                    .await;
                return CommandResult {
                    messages: history,
                    stop_reason: PromptStopReason::EndTurn,
                };
            }
        };

        // 发送 CompactStarted 事件
        event_sink
            .push_event(&session_id, &ExecutorEvent::CompactStarted, 0)
            .await;

        // 执行 full_compact
        let compact_result =
            match full_compact(&history, compact_model.as_ref(), &compact_config, "").await {
                Ok(r) => r,
                Err(e) => {
                    warn!(error = %e, "compact: full_compact 失败");
                    event_sink
                        .push_event(
                            &session_id,
                            &ExecutorEvent::CompactError {
                                message: e.to_string(),
                            },
                            0,
                        )
                        .await;
                    return CommandResult {
                        messages: history,
                        stop_reason: PromptStopReason::EndTurn,
                    };
                }
            };

        info!(
            summary_len = compact_result.summary.len(),
            "compact: full_compact 完成"
        );

        // 执行 re_inject
        let re_inject_result = re_inject(&history, &compact_config, &cwd).await;

        info!(
            files_injected = re_inject_result.files_injected,
            skills_injected = re_inject_result.skills_injected,
            "compact: re_inject 完成"
        );

        // 提取文件和 skill 信息
        let files = extract_file_info(&re_inject_result.messages);
        let skills = extract_skill_names(&re_inject_result.messages);

        // 摘要作为 Human 消息（与 auto-compact 路径和 Claude Code 实现对齐）
        let summary_content = format!(
            "{}\n\n[上下文已压缩，请根据摘要继续工作]",
            compact_result.summary
        );
        let mut new_messages = vec![BaseMessage::human(summary_content)];
        new_messages.extend(re_inject_result.messages.clone());

        // 发送 CompactCompleted 事件
        event_sink
            .push_event(
                &session_id,
                &ExecutorEvent::CompactCompleted {
                    summary: compact_result.summary,
                    files: files.clone(),
                    skills: skills.clone(),
                    micro_cleared: 0,
                    messages: new_messages.clone(),
                },
                0,
            )
            .await;

        info!("compact: 完成，session 已更新");

        CommandResult {
            messages: new_messages,
            stop_reason: PromptStopReason::EndTurn,
        }
    }
}

/// 从 re_inject 消息中提取文件信息。
fn extract_file_info(messages: &[BaseMessage]) -> Vec<CompactFileInfo> {
    let mut files = Vec::new();
    for msg in messages {
        let content = msg.content();
        if let Some(rest) = content.strip_prefix("[最近读取的文件: ") {
            let path = rest.lines().next().unwrap_or("");
            let line_count = rest.lines().count().saturating_sub(1);
            if !path.is_empty() {
                files.push(CompactFileInfo {
                    path: path.to_string(),
                    lines: line_count,
                });
            }
        }
    }
    files
}

/// 从 re_inject 消息中提取 skill 名称。
fn extract_skill_names(messages: &[BaseMessage]) -> Vec<String> {
    let mut skills = Vec::new();
    for msg in messages {
        let content = msg.content();
        if let Some(rest) = content.strip_prefix("[激活的 Skill 指令: ") {
            let name = rest.lines().next().unwrap_or("");
            if !name.is_empty() {
                skills.push(name.to_string());
            }
        }
    }
    skills
}
