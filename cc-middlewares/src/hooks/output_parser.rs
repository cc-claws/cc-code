use crate::hooks::types::{HookAction, HookDecision, HookSpecificOutput, SyncHookResponse};

/// 解析 command hook stdout 输出
///
/// 对齐 CC parseHookOutput + processHookJSONOutput:
/// - 不以 `{` 开头 → 纯文本输出，视为 Allow
/// - 以 `{` 开头 → 尝试解析为 SyncHookResponse JSON
pub fn parse_command_hook_output(stdout: &str) -> HookAction {
    let trimmed = stdout.trim();

    // 不以 { 开头 → 纯文本输出，视为 Allow
    if !trimmed.starts_with('{') {
        return HookAction::Allow;
    }

    // 尝试解析为 SyncHookResponse JSON
    match serde_json::from_str::<SyncHookResponse>(trimmed) {
        Ok(response) => sync_response_to_action(&response),
        Err(e) => {
            // 以 `{` 开头却不是合法 JSON → 声明了结构化输出却没写对，
            // 是「hook 坏了」而非「hook 无意见」：上报 Error（不阻断调用）。
            tracing::warn!("Hook stdout JSON parse failed: {}", e);
            HookAction::Error {
                reason: format!("invalid hook output: {e}"),
            }
        }
    }
}

/// 解析 HTTP hook 响应
///
/// 对齐 CC parseHttpHookOutput：
/// - 空 body → 视为 {}（有效 JSON）
/// - 不以 `{` 开头 → 非法（HTTP hook 必须返回 JSON）
pub fn parse_http_hook_response(body: &str) -> HookAction {
    let trimmed = body.trim();

    // 空 body → 视为 {}（有效 JSON）
    if trimmed.is_empty() {
        return HookAction::Allow;
    }

    // 不以 { 开头 → 非法（HTTP hook 必须返回 JSON）
    if !trimmed.starts_with('{') {
        // reason 会渲染成单行：把正文里的换行/制表符折叠成空格再截断
        let flattened = trimmed.split_whitespace().collect::<Vec<_>>().join(" ");
        let snippet: String = flattened.chars().take(200).collect();
        tracing::warn!("HTTP hook must return JSON, got non-JSON body: {}", snippet);
        // 端点返回了非 JSON 正文 → hook 配置/服务有问题，上报 Error（不阻断调用）
        return HookAction::Error {
            reason: format!("invalid hook output: response is not JSON: {snippet}"),
        };
    }

    match serde_json::from_str::<SyncHookResponse>(trimmed) {
        Ok(response) => sync_response_to_action(&response),
        Err(e) => {
            tracing::warn!("HTTP hook JSON parse failed: {}", e);
            HookAction::Error {
                reason: format!("invalid hook output: {e}"),
            }
        }
    }
}

/// 将 SyncHookResponse 转换为内部 HookAction
///
/// 优先级（严格按顺序）：
/// 1. continue=false → PreventContinuation
/// 2. decision=block → Block
/// 3. systemMessage → SystemMessage
/// 4. hookSpecificOutput → 事件特定处理
/// 5. 以上都不满足 → Allow
fn sync_response_to_action(response: &SyncHookResponse) -> HookAction {
    // 1. continue=false → 阻止继续
    if response.continue_run == Some(false) {
        return HookAction::PreventContinuation {
            stop_reason: response.stop_reason.clone(),
        };
    }

    // 2. decision=block → 阻止操作
    if response.decision == Some(HookDecision::Block) {
        return HookAction::Block {
            reason: response
                .reason
                .clone()
                .unwrap_or_else(|| "Blocked by hook".into()),
        };
    }

    // 3. systemMessage → 注入系统消息
    if let Some(ref msg) = response.system_message {
        return HookAction::SystemMessage {
            message: msg.clone(),
        };
    }

    // 4. hookSpecificOutput → 事件特定处理
    if let Some(ref specific) = response.hook_specific_output {
        return hook_specific_to_action(specific);
    }

    HookAction::Allow
}

/// 将 HookSpecificOutput 转换为内部 HookAction
fn hook_specific_to_action(specific: &HookSpecificOutput) -> HookAction {
    match specific {
        HookSpecificOutput::PreToolUse {
            updated_input: Some(input),
            ..
        } => HookAction::ModifyInput {
            new_input: input.clone(),
        },
        HookSpecificOutput::PreToolUse {
            permission_decision: Some(decision),
            permission_decision_reason,
            ..
        } => HookAction::PermissionOverride {
            decision: decision.clone(),
            reason: permission_decision_reason.clone(),
        },
        HookSpecificOutput::UserPromptSubmit {
            additional_context: Some(ctx),
            ..
        } => HookAction::AdditionalContext {
            context: ctx.clone(),
        },
        HookSpecificOutput::SessionStart {
            initial_user_message: Some(msg),
            ..
        } => HookAction::InitialUserMessage {
            message: msg.clone(),
        },
        HookSpecificOutput::SessionStart {
            additional_context: Some(ctx),
            ..
        } => HookAction::AdditionalContext {
            context: ctx.clone(),
        },
        _ => HookAction::Allow,
    }
}

#[cfg(test)]
#[path = "output_parser_test.rs"]
mod tests;
