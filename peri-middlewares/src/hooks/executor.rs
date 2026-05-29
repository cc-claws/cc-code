use std::{collections::HashSet, process::Stdio, sync::Arc, time::Duration};

use tokio::io::AsyncWriteExt;

use crate::hooks::{
    output_parser::{parse_command_hook_output, parse_http_hook_response},
    ssrf_guard::check_url,
    types::{HookAction, HookInput, HookType, RegisteredHook},
    variables::resolve_hook_variables,
};
use peri_agent::{
    agent::{
        react::{AgentInput, ReactLLM},
        state::AgentState,
        ReActAgent, State,
    },
    messages::BaseMessage,
};

/// Execute a command hook (shell script).
///
/// - shell default "bash", timeout default 600s
/// - stdin: serialized HookInput JSON
/// - exit code 0 → parse stdout, 1 → Allow(warn), 2 → Block(reason)
/// - timeout → Allow(warn)
pub async fn execute_command_hook(
    hook: &HookType,
    input: &HookInput,
    registered: &RegisteredHook,
) -> HookAction {
    let (command, _shell, timeout_secs) = match hook {
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

    let result = tokio::time::timeout(Duration::from_secs(timeout_secs), async {
        let mut cmd = crate::process::shell_command(&command, &[]);
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .env("CLAUDE_PROJECT_DIR", &input.cwd)
            .env("CLAUDE_PLUGIN_ROOT", &plugin_root_str)
            .env("CLAUDE_PLUGIN_DATA", &plugin_data_str)
            .env("CLAUDE_HOOK_EVENT_NAME", &hook_event_str)
            .kill_on_drop(true);

        // Inject CLAUDE_PLUGIN_OPTION_* env vars
        for (key, value) in &registered.plugin_options {
            let env_key = format!("CLAUDE_PLUGIN_OPTION_{}", key.to_uppercase());
            cmd.env(env_key, value.to_string());
        }

        let mut child = cmd.spawn()?;

        // Write input JSON to stdin
        if let Some(mut stdin) = child.stdin.take() {
            if let Err(e) = stdin.write_all(input_json.as_bytes()).await {
                tracing::warn!("Failed to write to hook stdin: {}", e);
            }
            drop(stdin);
        }

        let output = child.wait_with_output().await?;
        Ok::<_, std::io::Error>(output)
    })
    .await;

    match result {
        Ok(Ok(output)) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);

            match output.status.code() {
                Some(0) => {
                    // Parse structured output
                    parse_command_hook_output(&stdout)
                }
                Some(1) => {
                    // Exit code 1 → Allow with warning
                    if !stderr.is_empty() {
                        tracing::warn!("Command hook exited with code 1: {}", stderr);
                    }
                    HookAction::Allow
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
                    HookAction::Allow
                }
                None => {
                    tracing::warn!("Command hook terminated by signal");
                    HookAction::Allow
                }
            }
        }
        Ok(Err(e)) => {
            tracing::warn!("Command hook execution failed: {}", e);
            HookAction::Allow
        }
        Err(_) => {
            // Timeout
            tracing::warn!(
                "Command hook timed out after {}s: {}",
                timeout_secs,
                command
            );
            HookAction::Allow
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
            tracing::warn!("Prompt hook LLM call failed: {}", e);
            HookAction::Allow
        }
        Err(_) => {
            tracing::warn!("Prompt hook timed out after {}s", timeout_secs);
            HookAction::Allow
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
                tracing::warn!(
                    "HTTP hook returned non-success status {}: {}",
                    status,
                    if body.len() > 200 {
                        format!("{}...", &body[..body.floor_char_boundary(200)])
                    } else {
                        body
                    }
                );
                return HookAction::Allow;
            }

            parse_http_hook_response(&body)
        }
        Err(e) => {
            tracing::warn!("HTTP hook request failed: {}", e);
            HookAction::Allow
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
        Ok::<_, peri_agent::error::AgentError>((output, state.messages().to_vec()))
    })
    .await;

    match result {
        Ok(Ok((_output, messages))) => extract_structured_output(&messages),
        Ok(Err(e)) => {
            tracing::warn!("Agent hook execution failed: {}", e);
            HookAction::Allow
        }
        Err(_) => {
            tracing::warn!("Agent hook timed out after {}s", timeout_secs);
            HookAction::Allow
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
