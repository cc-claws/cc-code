use std::collections::HashMap;

use tokio_util::sync::CancellationToken;

use crate::agent::events::AgentEvent;
use crate::agent::react::{ReactLLM, Reasoning, ToolCall, ToolResult};
use crate::agent::state::State;
use crate::error::{AgentError, AgentResult};
use crate::messages::{BaseMessage, ToolCallRequest};
use crate::tools::BaseTool;

use super::ReActAgent;

/// 工具审批 → 并发执行 → 结果处理
pub(crate) async fn dispatch_tools<L: ReactLLM, S: State>(
    agent: &ReActAgent<L, S>,
    state: &mut S,
    reasoning: &Reasoning,
    all_tools: &HashMap<String, &dyn BaseTool>,
    cancel: &CancellationToken,
) -> AgentResult<Vec<(ToolCall, ToolResult)>> {
    let mut all_tool_calls: Vec<(ToolCall, ToolResult)> = Vec::new();

    let tc_reqs: Vec<ToolCallRequest> = reasoning
        .tool_calls
        .iter()
        .map(|tc| ToolCallRequest::new(tc.id.clone(), tc.name.clone(), tc.input.clone()))
        .collect();
    // 优先使用带 Reasoning block 的原始消息，保留 thinking 内容
    // source_message 的 tool_calls 字段在 LLM 解析阶段已填好
    let ai_msg = reasoning
        .source_message
        .clone()
        .unwrap_or_else(|| BaseMessage::ai_with_tool_calls(reasoning.thought.clone(), tc_reqs));
    let ai_msg_id = ai_msg.id(); // 捕获 message_id（Copy，供后续 ToolStart/ToolEnd 使用）
    let ai_msg_clone = ai_msg.clone();
    state.add_message(ai_msg);
    agent.emit(AgentEvent::MessageAdded(ai_msg_clone));
    // emit AI 推理内容
    agent.emit(AgentEvent::AiReasoning(reasoning.thought.clone()));

    // 阶段一：批量 before_tool（利用中间件的 batch 方法，如 HITL 批量审批）
    let original_calls: Vec<ToolCall> = reasoning.tool_calls.clone();
    let before_results = agent
        .chain
        .run_before_tools_batch(state, original_calls.clone())
        .await;
    let mut modified_calls: Vec<ToolCall> = Vec::with_capacity(original_calls.len());

    for (tool_call, before_result) in original_calls.iter().zip(before_results) {
        // before_tool 阶段也检查取消
        if cancel.is_cancelled() {
            return Err(AgentError::Interrupted);
        }
        let modified_call = match before_result {
            Ok(c) => c,
            Err(AgentError::ToolRejected { ref reason, .. }) => {
                // 拒绝不终止 Agent，将拒绝原因作为工具错误反馈给 LLM
                let rejection_result =
                    ToolResult::error(&tool_call.id, &tool_call.name, reason.clone());
                agent.emit(AgentEvent::ToolStart {
                    message_id: ai_msg_id,
                    tool_call_id: tool_call.id.clone(),
                    name: tool_call.name.clone(),
                    input: tool_call.input.clone(),
                });
                agent.emit(AgentEvent::ToolEnd {
                    message_id: ai_msg_id,
                    tool_call_id: tool_call.id.clone(),
                    name: tool_call.name.clone(),
                    output: rejection_result.output.clone(),
                    is_error: true,
                });
                let tool_msg = BaseMessage::tool_error(
                    &rejection_result.tool_call_id,
                    rejection_result.output.as_str(),
                );
                let tool_msg_clone = tool_msg.clone();
                state.add_message(tool_msg);
                agent.emit(AgentEvent::MessageAdded(tool_msg_clone));
                all_tool_calls.push((tool_call.clone(), rejection_result));
                continue;
            }
            Err(e) => {
                agent.chain.run_on_error(state, &e).await?;
                return Err(e);
            }
        };
        agent.emit(AgentEvent::ToolStart {
            message_id: ai_msg_id,
            tool_call_id: modified_call.id.clone(),
            name: modified_call.name.clone(),
            input: modified_call.input.clone(),
        });
        modified_calls.push(modified_call);
    }

    // 阶段二：并发执行所有工具；取消时每个工具以 error 收尾
    let tool_results: Vec<Result<String, AgentError>> = {
        let futures: Vec<_> = modified_calls
            .iter()
            .map(|call| {
                let tool_name = call.name.clone();
                let call_id = call.id.clone();
                let input = call.input.clone();
                let tool = all_tools.get(&call.name).copied();
                let cancel = cancel.clone();
                async move {
                    let span = tracing::info_span!(
                        "agent.tool_call",
                        tool.name = %tool_name,
                        tool.call_id = %call_id,
                    );
                    let _enter = span.enter();
                    let invoke_fut =
                        async {
                            match tool {
                                Some(t) => t.invoke(input).await.map_err(|e| {
                                    AgentError::ToolExecutionFailed {
                                        tool: tool_name.clone(),
                                        reason: e.to_string(),
                                    }
                                }),
                                None => Err(AgentError::ToolNotFound(tool_name.clone())),
                            }
                        };
                    tokio::select! {
                        biased;
                        _ = cancel.cancelled() => {
                            Err(AgentError::ToolExecutionFailed {
                                tool: tool_name,
                                reason: "interrupted by user".to_string(),
                            })
                        }
                        result = invoke_fut => result,
                    }
                }
            })
            .collect();
        futures::future::join_all(futures).await
    };

    // 检查是否已取消（工具全部结束后再决定是否继续）
    let was_cancelled = cancel.is_cancelled();

    // 阶段三：串行处理结果、after_tool、state 更新
    for (modified_call, tool_result) in modified_calls.into_iter().zip(tool_results) {
        let result = match tool_result {
            Ok(output) => ToolResult::success(&modified_call.id, &modified_call.name, output),
            Err(AgentError::ToolNotFound(ref name)) => {
                tracing::warn!(tool.name = %name, "工具未找到，作为错误结果返回");
                ToolResult::error(
                    &modified_call.id,
                    &modified_call.name,
                    format!("工具 '{}' 不存在", name),
                )
            }
            Err(ref e) => {
                agent.chain.run_on_error(state, e).await?;
                ToolResult::error(&modified_call.id, &modified_call.name, e.to_string())
            }
        };

        tracing::debug!(
            tool.name = %result.tool_name,
            tool.is_error = result.is_error,
            "tool call completed"
        );
        if result.is_error {
            tracing::warn!(
                tool.name = %result.tool_name,
                tool.is_error = true,
                error_len = result.output.len(),
                "tool call failed"
            );
        }
        agent.emit(AgentEvent::ToolEnd {
            message_id: ai_msg_id,
            tool_call_id: modified_call.id.clone(),
            name: modified_call.name.clone(),
            output: result.output.clone(),
            is_error: result.is_error,
        });

        if let Err(e) = agent
            .chain
            .run_after_tool(state, &modified_call, &result)
            .await
        {
            agent.chain.run_on_error(state, &e).await?;
            return Err(e);
        }

        let tool_msg = if result.is_error {
            BaseMessage::tool_error(&result.tool_call_id, result.output.as_str())
        } else {
            BaseMessage::tool_result(&result.tool_call_id, result.output.as_str())
        };
        let tool_msg_clone = tool_msg.clone();
        state.add_message(tool_msg);
        agent.emit(AgentEvent::MessageAdded(tool_msg_clone));

        all_tool_calls.push((modified_call, result));
    }

    // 工具结果全部写入状态后，若已取消则以 Interrupted 退出
    // （调用方可保存此刻的 state.messages 实现断点续跑）
    if was_cancelled {
        return Err(AgentError::Interrupted);
    }

    Ok(all_tool_calls)
}
