use std::{collections::HashSet, sync::Arc, time::Duration};

use crate::hooks::{
    output_parser::{parse_command_hook_output, parse_http_hook_response},
    ssrf_guard::check_url,
    types::{HookAction, HookInput, HookType, RegisteredHook},
    variables::resolve_hook_variables,
};
use cc_agent::{
    agent::{
        react::{AgentInput, ReactLLM},
        state::AgentState,
        ReActAgent, State,
    },
    encoding::decode_output_bytes,
    messages::BaseMessage,
};

/// 拼一段给用户看的 hook 错误原因：`<prefix>: <stderr 前 200 字符>`。
///
/// reason 最终落在 TUI 的单行 hook 结果行里，而 stderr 可能带换行、制表符与
/// ANSI 转义：先剥离转义，再把所有空白折叠成单个空格，保证「一行一条原因」
/// 且不会把终端控制序列带进渲染层。
fn hook_error_reason(prefix: &str, detail: &str) -> String {
    use crate::tools::output_filter::strip_ansi;

    let cleaned = strip_ansi(detail);
    let flattened = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    if flattened.is_empty() {
        return prefix.to_string();
    }
    let short: String = flattened.chars().take(200).collect();
    format!("{prefix}: {short}")
}

/// Execute a command hook (shell script).
///
/// - shell: platform default with Git Bash routing on Windows; timeout default 600s
/// - stdin: serialized HookInput JSON
/// - exit code 0 → parse stdout, 1/其他非零 → Error(不阻断), 2 → Block(reason)
/// - timeout / 信号 / 启动失败 → Error(不阻断)
pub async fn execute_command_hook(
    hook: &HookType,
    input: &HookInput,
    registered: &RegisteredHook,
) -> HookAction {
    let (command, shell, timeout_secs) = match hook {
        HookType::Command {
            command,
            shell,
            timeout,
            ..
        } => (command.clone(), shell.clone(), timeout.unwrap_or(600)),
        _ => {
            return HookAction::Allow;
        }
    };

    let input_json = match serde_json::to_string(input) {
        Ok(json) => json,
        Err(e) => {
            tracing::warn!("Failed to serialize HookInput: {}", e);
            return HookAction::Allow;
        }
    };

    // Resolve ${CLAUDE_PLUGIN_ROOT}, ${CLAUDE_PLUGIN_DATA}, ${ARGUMENTS} in command string
    let command = resolve_hook_variables(
        &command,
        &registered.plugin_root,
        &registered.plugin_data_dir,
        &input_json,
    );

    let plugin_root_str = registered.plugin_root.to_string_lossy().to_string();
    let plugin_data_str = registered.plugin_data_dir.to_string_lossy().to_string();
    let hook_event_str = format!("{:?}", input.hook_event_name);

    let mut cmd =
        match crate::process::try_shell_command_with_shell(&command, &[], shell.as_deref()) {
            Ok(command) => command,
            Err(error) => {
                tracing::warn!("Command hook execution failed: {}", error);
                return HookAction::Error {
                    reason: hook_error_reason("hook failed to run", &error.to_string()),
                };
            }
        };
    cmd.current_dir(&input.cwd)
        .env("CLAUDE_PROJECT_DIR", &input.cwd)
        .env("CLAUDE_PLUGIN_ROOT", &plugin_root_str)
        .env("CLAUDE_PLUGIN_DATA", &plugin_data_str)
        .env("CLAUDE_HOOK_EVENT_NAME", &hook_event_str);

    // Inject CLAUDE_PLUGIN_OPTION_* env vars
    for (key, value) in &registered.plugin_options {
        let env_key = format!("CLAUDE_PLUGIN_OPTION_{}", key.to_uppercase());
        cmd.env(env_key, value.to_string());
    }

    let result = crate::process::output_with_input_timeout(
        cmd,
        input_json.as_bytes(),
        Duration::from_secs(timeout_secs),
    )
    .await;

    match result {
        Ok(output) => {
            let stdout = decode_output_bytes(&output.stdout);
            let stderr = decode_output_bytes(&output.stderr);

            match output.status.code() {
                Some(0) => {
                    // Parse structured output
                    parse_command_hook_output(&stdout)
                }
                Some(1) => {
                    // 非阻断失败：CC 语义里 exit 1 表示 hook 执行出错，但不阻止工具调用。
                    // 保留为 Error 以便 UI 如实告知「hook 坏了」，流程上仍按 Allow 走。
                    if !stderr.is_empty() {
                        tracing::warn!("Command hook exited with code 1: {}", stderr);
                    }
                    HookAction::Error {
                        reason: hook_error_reason("hook exited with code 1", &stderr),
                    }
                }
                Some(2) => {
                    // Exit code 2 → Block
                    let reason = if !stdout.trim().is_empty() {
                        stdout.trim().to_string()
                    } else if !stderr.trim().is_empty() {
                        stderr.trim().to_string()
                    } else {
                        "Blocked by hook (exit code 2)".to_string()
                    };
                    HookAction::Block { reason }
                }
                Some(code) => {
                    tracing::warn!(
                        "Command hook exited with unexpected code {}: stderr={}",
                        code,
                        stderr
                    );
                    HookAction::Error {
                        reason: hook_error_reason(
                            &format!("hook exited with code {code}"),
                            &stderr,
                        ),
                    }
                }
                None => {
                    tracing::warn!("Command hook terminated by signal");
                    HookAction::Error {
                        reason: "hook terminated by signal".to_string(),
                    }
                }
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::TimedOut => {
            // Timeout
            tracing::warn!(
                "Command hook timed out after {}s: {}",
                timeout_secs,
                command
            );
            HookAction::Error {
                reason: format!("hook timed out after {timeout_secs}s"),
            }
        }
        Err(e) => {
            tracing::warn!("Command hook execution failed: {}", e);
            HookAction::Error {
                reason: hook_error_reason("hook failed to run", &e.to_string()),
            }
        }
    }
}

/// Execute a prompt hook (LLM evaluation).
///
/// - timeout default 30s
/// - Replace $ARGUMENTS in prompt with input JSON
/// - Call llm.generate_reasoning, parse result
pub async fn execute_prompt_hook(
    hook: &HookType,
    input: &HookInput,
    llm_factory: &Arc<dyn Fn() -> Box<dyn ReactLLM + Send + Sync> + Send + Sync>,
) -> HookAction {
    let (prompt_template, timeout_secs) = match hook {
        HookType::Prompt {
            prompt, timeout, ..
        } => (prompt.as_str(), timeout.unwrap_or(30)),
        _ => {
            return HookAction::Allow;
        }
    };

    let input_json = match serde_json::to_string(input) {
        Ok(json) => json,
        Err(e) => {
            tracing::warn!("Failed to serialize HookInput for prompt hook: {}", e);
            return HookAction::Allow;
        }
    };

    // Replace $ARGUMENTS with input JSON
    let prompt = prompt_template.replace("$ARGUMENTS", &input_json);
    let prompt = prompt.replace("${ARGUMENTS}", &input_json);

    let result = tokio::time::timeout(Duration::from_secs(timeout_secs), async {
        let llm = llm_factory();
        // Build a minimal message list with just the prompt as a system message
        let messages = vec![BaseMessage::system(prompt.clone())];
        let reasoning = llm.generate_reasoning(&messages, &[], None).await?;
        Ok::<_, anyhow::Error>(reasoning)
    })
    .await;

    match result {
        Ok(Ok(reasoning)) => {
            let answer = reasoning
                .final_answer
                .unwrap_or(reasoning.thought)
                .trim()
                .to_string();
            parse_command_hook_output(&answer)
        }
        Ok(Err(e)) => {
            // LLM 调用失败 = hook 自身跑不起来。不阻断调用，但要让用户看到
            tracing::warn!("Prompt hook LLM call failed: {}", e);
            HookAction::Error {
                reason: hook_error_reason("hook LLM call failed", &e.to_string()),
            }
        }
        Err(_) => {
            tracing::warn!("Prompt hook timed out after {}s", timeout_secs);
            HookAction::Error {
                reason: format!("hook timed out after {timeout_secs}s"),
            }
        }
    }
}

/// Execute an HTTP hook (POST request).
///
/// - SSRF guard check first
/// - timeout default 600s
/// - POST with JSON body, CRLF-injection-safe headers
pub async fn execute_http_hook(hook: &HookType, input: &HookInput) -> HookAction {
    let (url, timeout_secs, headers, allowed_env_vars) = match hook {
        HookType::Http {
            url,
            timeout,
            headers,
            allowed_env_vars,
            ..
        } => (
            url.as_str(),
            timeout.unwrap_or(600),
            headers,
            allowed_env_vars,
        ),
        _ => {
            return HookAction::Allow;
        }
    };

    // SSRF guard
    if let Err(reason) = check_url(url) {
        tracing::warn!("HTTP hook blocked by SSRF guard: {}", reason);
        return HookAction::Block {
            reason: format!("SSRF guard blocked URL: {}", reason),
        };
    }

    let health = crate::hooks::http_health::http_health_for(url);
    if health.is_open() {
        tracing::debug!(
            "HTTP hook skipped because endpoint circuit breaker is open (cooling down {}s): {}",
            crate::hooks::http_health::CIRCUIT_OPEN_SECS,
            url
        );
        return HookAction::Allow;
    }

    let input_json = match serde_json::to_string(input) {
        Ok(json) => json,
        Err(e) => {
            tracing::warn!("Failed to serialize HookInput for HTTP hook: {}", e);
            return HookAction::Allow;
        }
    };

    // Build allowed_env_vars set for header sanitization
    let allowed_set: HashSet<String> = allowed_env_vars.iter().cloned().collect();

    // Sanitize and build headers
    let mut req_headers = reqwest::header::HeaderMap::new();
    req_headers.insert(
        reqwest::header::CONTENT_TYPE,
        reqwest::header::HeaderValue::from_static("application/json"),
    );
    for (key, value) in headers {
        let sanitized = sanitize_header_value(value, &allowed_set);
        if let (Ok(name), Ok(val)) = (
            reqwest::header::HeaderName::from_bytes(key.as_bytes()),
            reqwest::header::HeaderValue::from_str(&sanitized),
        ) {
            req_headers.insert(name, val);
        }
    }

    let client = match reqwest::Client::builder()
        .connect_timeout(crate::hooks::http_health::connect_timeout())
        .timeout(Duration::from_secs(timeout_secs))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("Failed to build HTTP client: {}", e);
            return HookAction::Allow;
        }
    };

    match client
        .post(url)
        .headers(req_headers)
        .body(input_json)
        .send()
        .await
    {
        Ok(response) => {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();

            if !status.is_success() {
                // 5xx 视为端点性故障（服务不可用），触发端点熔断。
                // 4xx（含 404）是请求/配置层面的确定性错误，重试无益，
                // 但不代表端点不可达，也不该阻断同一端点上的其它 hook。
                if status.is_server_error() {
                    let tripped = health.trip();
                    if tripped {
                        tracing::warn!(
                            "HTTP hook returned server error {}, tripping circuit breaker for {}s: url={}",
                            status,
                            crate::hooks::http_health::CIRCUIT_OPEN_SECS,
                            url
                        );
                    }
                }
                tracing::warn!(
                    "HTTP hook returned non-success status {}: {}",
                    status,
                    if body.len() > 200 {
                        format!("{}...", &body[..body.floor_char_boundary(200)])
                    } else {
                        body.clone()
                    }
                );
                // 非 2xx = 端点自身出错（服务不可用 / 路径写错 / 鉴权失败）。
                // 不阻断调用，但要把状态码暴露给用户，否则「hook 没生效」无从排查。
                return HookAction::Error {
                    reason: hook_error_reason(&format!("hook returned status {status}"), &body),
                };
            }

            health.reset();
            parse_http_hook_response(&body)
        }
        Err(e) => {
            let tripped = health.trip();
            if tripped {
                tracing::warn!(
                    "HTTP hook request failed, tripping circuit breaker for {}s: url={}, error={}",
                    crate::hooks::http_health::CIRCUIT_OPEN_SECS,
                    url,
                    e
                );
            } else {
                tracing::debug!(
                    "HTTP hook request failed (already tripped): url={}, error={}",
                    url,
                    e
                );
            }
            // 连接失败 / 超时 / 请求构造失败：hook 端点没跑通，不阻断但上报
            HookAction::Error {
                reason: hook_error_reason("hook request failed", &e.to_string()),
            }
        }
    }
}

/// Execute an agent hook (full ReAct agent loop).
///
/// - timeout default 60s, max_turns 50
/// - No HookMiddleware, no SubAgentMiddleware (prevent recursion)
/// - After execution, look for structured output in messages
pub async fn execute_agent_hook(
    hook: &HookType,
    input: &HookInput,
    llm_factory: &Arc<dyn Fn() -> Box<dyn ReactLLM + Send + Sync> + Send + Sync>,
    cwd: &str,
) -> HookAction {
    let (prompt_template, timeout_secs) = match hook {
        HookType::Agent {
            prompt, timeout, ..
        } => (prompt.as_str(), timeout.unwrap_or(60)),
        _ => {
            return HookAction::Allow;
        }
    };

    let max_turns: usize = 50;

    let input_json = match serde_json::to_string_pretty(input) {
        Ok(json) => json,
        Err(e) => {
            tracing::warn!("Failed to serialize HookInput for agent hook: {}", e);
            return HookAction::Allow;
        }
    };

    let prompt = format!(
        "{}\n\nInput:\n```json\n{}\n```\n\nRespond with a JSON object describing the hook action.",
        prompt_template, input_json
    );

    let result = tokio::time::timeout(Duration::from_secs(timeout_secs), async {
        let llm = llm_factory();
        let mut state = AgentState::new(cwd);

        let agent = ReActAgent::new(llm).max_iterations(max_turns);

        let output = agent
            .execute(AgentInput::text(&prompt), &mut state, None)
            .await?;
        Ok::<_, cc_agent::error::AgentError>((output, state.messages().to_vec()))
    })
    .await;

    match result {
        Ok(Ok((_output, messages))) => extract_structured_output(&messages),
        Ok(Err(e)) => {
            // Agent hook 自身跑挂（LLM 报错 / 轮数耗尽）：不阻断调用但上报
            tracing::warn!("Agent hook execution failed: {}", e);
            HookAction::Error {
                reason: hook_error_reason("hook agent failed", &e.to_string()),
            }
        }
        Err(_) => {
            tracing::warn!("Agent hook timed out after {}s", timeout_secs);
            HookAction::Error {
                reason: format!("hook timed out after {timeout_secs}s"),
            }
        }
    }
}

/// Sanitize header value: remove CRLF sequences and expand whitelisted env vars.
///
/// CRLF injection protection: strips \r and \n from header values.
/// Env var expansion: only vars in `allowed_env_vars` set are expanded.
fn sanitize_header_value(value: &str, allowed_env_vars: &HashSet<String>) -> String {
    // First, strip CRLF to prevent injection
    let sanitized = value.replace(['\r', '\n'], "");

    // Expand whitelisted env vars (simple ${VAR} and $VAR patterns)
    let mut result = sanitized;
    for var_name in allowed_env_vars {
        let pattern1 = format!("${{{}}}", var_name);
        let pattern2 = format!("${}", var_name);
        if let Ok(val) = std::env::var(var_name) {
            result = result.replace(&pattern1, &val);
            result = result.replace(&pattern2, &val);
        }
    }

    result
}

/// Extract structured hook output from agent messages.
///
/// Looks through Tool messages for structured output and parses it.
/// Falls back to the last AI message text if no structured output is found.
fn extract_structured_output(messages: &[BaseMessage]) -> HookAction {
    // Look for Tool message results in reverse order (most recent first)
    for msg in messages.iter().rev() {
        if let BaseMessage::Tool { content, .. } = msg {
            let text = content.text_content();
            let action = parse_command_hook_output(&text);
            if !matches!(action, HookAction::Allow) {
                return action;
            }
        }
    }

    // Fallback: check last AI message for JSON
    for msg in messages.iter().rev() {
        if let BaseMessage::Ai { content, .. } = msg {
            let text = content.text_content();
            if text.trim().starts_with('{') {
                return parse_command_hook_output(&text);
            }
        }
    }

    HookAction::Allow
}

#[cfg(test)]
#[path = "executor_test.rs"]
mod tests;

#[cfg(test)]
#[path = "command_executor_test.rs"]
mod command_executor_test;
