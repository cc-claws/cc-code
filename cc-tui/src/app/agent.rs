// ── Live code retained after unified event mapping ──
// map_executor_event: handles categories ②③ only
// Category ① events now come through session/update → handle_session_update_peri()

pub use super::provider::LlmProvider;
use super::AgentEvent;
use cc_agent::agent::events::AgentEvent as ExecutorEvent;

/// 将 ExecutorEvent 映射为 TUI AgentEvent。
///
/// 仅处理 session/update 无法映射的事件（类别②③）。
/// 类别①事件（TextChunk, AiReasoning, ToolPending, ToolStart, ToolOutput, ToolEnd, TodoUpdate）
/// 已通过 session/update → handle_session_update_peri() 处理，此处返回 None。
pub(crate) fn map_executor_event(event: ExecutorEvent, _cwd: &str) -> Option<AgentEvent> {
    Some(match event {
        ExecutorEvent::SubagentEvent {
            source_agent_id,
            event,
        } => AgentEvent::SubagentEvent {
            source_agent_id,
            event: Box::new(map_executor_event(*event, _cwd)?),
        },
        // ── 类别③：无 SessionUpdate 映射，仍通过 peri/agent_event ──
        ExecutorEvent::StateSnapshot(msgs) => AgentEvent::StateSnapshot(msgs),
        ExecutorEvent::SubagentStarted {
            agent_name,
            instance_id,
            is_background,
            task_preview,
            prompt,
            ..
        } => AgentEvent::SubAgentStart {
            agent_id: agent_name.clone(),
            instance_id,
            task_preview,
            is_background,
            prompt,
        },
        ExecutorEvent::SubagentStopped {
            agent_name,
            result,
            is_error,
            instance_id,
            ..
        } => AgentEvent::SubAgentEnd {
            agent_id: Some(agent_name),
            instance_id: Some(instance_id),
            result,
            is_error,
        },
        ExecutorEvent::SubagentProgress {
            instance_id,
            total_steps,
        } => AgentEvent::SubagentProgress {
            instance_id,
            total_steps,
        },
        ExecutorEvent::CompactStarted { manual } => AgentEvent::CompactStarted { manual },
        ExecutorEvent::CompactCompleted {
            summary,
            files,
            skills,
            micro_cleared,
            messages,
        } => AgentEvent::CompactCompleted {
            summary,
            files,
            skills,
            micro_cleared,
            messages,
        },
        ExecutorEvent::CompactError { message } => AgentEvent::CompactError(message),
        ExecutorEvent::HooksStarted {
            event,
            tool_call_id,
        } => AgentEvent::HooksStarted {
            event,
            tool_call_id,
        },
        ExecutorEvent::HooksCompleted {
            event,
            ran,
            duration_ms,
            failures,
            is_async,
            tool_call_id,
        } => AgentEvent::HooksCompleted {
            event,
            ran,
            duration_ms,
            failures,
            is_async,
            tool_call_id,
        },
        ExecutorEvent::RewindCompleted { summary, messages } => {
            AgentEvent::RewindCompleted { summary, messages }
        }
        ExecutorEvent::RecapCompleted { text } => AgentEvent::RecapCompleted { text },
        ExecutorEvent::RecapError { message } => AgentEvent::RecapError(message),
        ExecutorEvent::BackgroundTaskCompleted(result) => AgentEvent::BackgroundTaskCompleted {
            task_id: result.task_id,
            agent_name: result.agent_name,
            success: result.success,
            output: result.output,
            tool_calls_count: result.tool_calls_count,
            duration_ms: result.duration_ms,
            child_thread_id: result.child_thread_id,
        },
        ExecutorEvent::LspDiagnostics {
            errors,
            warnings,
            files_with_errors,
        } => AgentEvent::LspDiagnostics {
            errors,
            warnings,
            files_with_errors,
        },
        ExecutorEvent::AgentExecutionFailed { message } => {
            if message == "Interrupted by user" {
                AgentEvent::Interrupted
            } else {
                AgentEvent::Error(message)
            }
        }

        // ── 类别②：SessionUpdate 丢失信息的增强事件 ──
        ExecutorEvent::ContextWarning {
            used_tokens,
            total_tokens,
            percentage,
        } => AgentEvent::ContextWarning {
            used_tokens,
            total_tokens,
            percentage,
        },
        ExecutorEvent::LlmRetrying {
            attempt,
            max_attempts,
            delay_ms,
            error,
        } => AgentEvent::LlmRetrying {
            attempt,
            max_attempts,
            delay_ms,
            error,
        },
        ExecutorEvent::SystemNotice { level, message } => {
            AgentEvent::SystemNotice { level, message }
        }

        // ── 类别① + 已过滤：已由 session/update → handle_session_update_peri() 处理或无需转发 ──
        ExecutorEvent::TextChunk { .. }
        | ExecutorEvent::AiReasoning(_)
        | ExecutorEvent::ToolPending { .. }
        | ExecutorEvent::ToolStart { .. }
        | ExecutorEvent::ToolOutput { .. }
        | ExecutorEvent::ToolEnd { .. }
        | ExecutorEvent::TodoUpdate(_)
        | ExecutorEvent::LlmCallEnd { .. }
        | ExecutorEvent::MessageAdded(_)
        | ExecutorEvent::LlmCallStart { .. } => return None,
    })
}

#[cfg(test)]
#[path = "agent_test.rs"]
mod tests;
