use crate::agent::events::AgentEvent;
use crate::agent::react::{AgentOutput, ReactLLM, Reasoning, ToolCall, ToolResult};
use crate::agent::state::State;
use crate::error::AgentResult;
use crate::messages::BaseMessage;

use super::ReActAgent;

/// 消费后台任务完成通知，注入到 state 中
///
/// 仅注入 state 供 LLM 下一轮迭代可见，不发射 MessageAdded 或 BackgroundTaskCompleted
/// （后台任务自身已通过 event handler 发射 BackgroundTaskCompleted）
async fn drain_notifications<L: ReactLLM, S: State>(agent: &ReActAgent<L, S>, state: &mut S) {
    if let Some(ref rx) = agent.notification_rx {
        let mut rx_lock = rx.lock().await;
        while let Ok(result) = rx_lock.try_recv() {
            let notification = if result.success {
                format!(
                    "[后台任务 {} 已完成] Agent: {} | 工具调用: {} | 耗时: {}ms\n结果:\n{}",
                    result.task_id,
                    result.agent_name,
                    result.tool_calls_count,
                    result.duration_ms,
                    result.output,
                )
            } else {
                format!(
                    "[后台任务 {} 执行失败] Agent: {}\n错误:\n{}",
                    result.task_id, result.agent_name, result.output,
                )
            };
            let msg = BaseMessage::human(notification);
            state.add_message(msg);
        }
    }
}

/// 工具调用步骤后：发出 StateSnapshot + 消费后台通知 + 更新 last_message_count
pub(crate) async fn emit_snapshot_and_drain_notifications<L: ReactLLM, S: State>(
    agent: &ReActAgent<L, S>,
    state: &mut S,
    last_message_count: &mut usize,
) {
    // 发送状态快照（从用户消息开始的所有消息），便于增量持久化
    let msgs_since_human = state.messages()[*last_message_count..].to_vec();
    tracing::debug!(count = msgs_since_human.len(), "sending state snapshot");
    for msg in &msgs_since_human {
        match msg {
            BaseMessage::Ai {
                content: _,
                tool_calls,
                ..
            } => {
                tracing::debug!(
                    has_tc = !tool_calls.is_empty(),
                    tc_len = tool_calls.len(),
                    "ai message in snapshot"
                );
            }
            BaseMessage::Tool { tool_call_id, .. } => {
                tracing::debug!(tc_id = %tool_call_id, "tool message in snapshot");
            }
            _ => {}
        }
    }
    if !msgs_since_human.is_empty() {
        agent.emit(AgentEvent::StateSnapshot(msgs_since_human));
    }

    drain_notifications(agent, state).await;

    *last_message_count = state.messages().len();
}

/// 处理最终回答路径，返回 AgentOutput
pub(crate) async fn handle_final_answer<L: ReactLLM, S: State>(
    agent: &ReActAgent<L, S>,
    state: &mut S,
    reasoning: &Reasoning,
    all_tool_calls: Vec<(ToolCall, ToolResult)>,
    last_message_count: usize,
    step: usize,
) -> AgentResult<AgentOutput> {
    let answer = reasoning
        .final_answer
        .clone()
        .unwrap_or_else(|| reasoning.thought.clone());

    if answer.trim().is_empty() {
        tracing::warn!(
            step,
            "LLM 返回空最终回答（无 tool_calls 且 final_answer/thought 为空）"
        );
    }

    // 优先使用带 Reasoning block 的原始消息，保留 thinking 内容
    let ai_msg = reasoning
        .source_message
        .clone()
        .unwrap_or_else(|| BaseMessage::ai(answer.as_str()));
    let ai_msg_id = ai_msg.id(); // 捕获 message_id（Copy，供 TextChunk 使用）
    let ai_msg_clone = ai_msg.clone();
    state.add_message(ai_msg);
    agent.emit(AgentEvent::MessageAdded(ai_msg_clone));

    agent.emit(AgentEvent::TextChunk {
        message_id: ai_msg_id,
        chunk: answer.clone(),
    });

    // 发送包含最终回答的 StateSnapshot，确保 TUI 侧的 agent_state_messages
    // 包含完整对话历史（包括本次最终回答），否则下一轮对话上下文会丢失
    let msgs_since_last = state.messages()[last_message_count..].to_vec();
    if !msgs_since_last.is_empty() {
        agent.emit(AgentEvent::StateSnapshot(msgs_since_last));
    }

    // 消费后台任务完成通知（最终回答前）
    drain_notifications(agent, state).await;

    let output = AgentOutput {
        text: answer,
        steps: step + 1,
        tool_calls: all_tool_calls,
        stop_reason: None,
    };

    tracing::info!(
        steps = output.steps,
        tool_calls = output.tool_calls.len(),
        "agent finished"
    );

    match agent.chain.run_after_agent(state, output).await {
        Ok(o) => Ok(o),
        Err(e) => {
            agent.chain.run_on_error(state, &e).await?;
            Err(e)
        }
    }
}
