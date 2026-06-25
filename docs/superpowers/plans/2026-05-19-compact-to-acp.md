# Compact 下沉到 ACP 层 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将上下文压缩的触发判断和执行从 peri-tui 下沉到 peri-acp 的 execute_prompt，使 TUI 和 stdio/IDE 路径共享 compact 能力。

**Architecture:** 在 `peri-acp/src/session/` 新增 `compact_runner.rs` 模块封装 compact 执行逻辑（full_compact + re_inject + hooks + 事件发送）。修改 `executor.rs` 在 agent 执行结束后检查 token 阈值，需要时调用 compact_runner 并内部 resubmit（最多 3 次）。TUI 侧删除 ~400 行旧 compact 代码，新增 ~50 行纯 UI 处理。手动 `/compact` 通过新增 ACP `session/compact` 请求实现。

**Tech Stack:** Rust, tokio async, peri-agent compact/re_inject, peri-acp EventSink

---

## File Structure

### 新建文件
- `peri-acp/src/session/compact_runner.rs` — compact 公共执行函数（run_full_compact + run_micro_compact）

### 修改文件
- `peri-agent/src/agent/events.rs` — CompactCompleted 增加数据字段，新增 CompactFileInfo、CompactError
- `peri-acp/src/session/mod.rs` — 注册 compact_runner 模块
- `peri-acp/src/session/executor.rs` — execute_prompt 内部 compact 循环
- `peri-tui/src/app/events.rs` — 替换旧 CompactDone/CompactError 为新变体
- `peri-tui/src/app/agent.rs` — 删除 compact_task，修改 map_executor_event
- `peri-tui/src/app/agent_comm.rs` — 删除 8 个 compact 相关字段
- `peri-tui/src/app/agent_ops.rs` — 删除 compact 触发逻辑，新增 handle_compact_* UI 处理
- `peri-tui/src/app/agent_events_bg.rs` — 删除 deferred compact
- `peri-tui/src/app/agent_submit.rs` — 删除 compact 相关字段赋值
- `peri-tui/src/app/thread_ops.rs` — 删除 start_compact、start_micro_compact
- `peri-tui/src/command/session/compact.rs` — 改用 ACP 请求
- `peri-tui/src/acp_server.rs` — 新增 session/compact 请求处理

### 删除文件
- `peri-tui/src/app/agent_compact.rs` — 整个文件

---

## 前置验证

- [ ] **Step 0: 确认当前代码可编译**

```bash
cargo build --workspace 2>&1 | tail -5
```

Expected: 编译通过。

---

### Task 1: ExecutorEvent compact 变体改造 (peri-agent)

**Files:**
- Modify: `peri-agent/src/agent/events.rs`
- Modify: `peri-agent/src/agent/events_test.rs`

给 `CompactCompleted` 增加数据载荷，新增 `CompactError` 变体和 `CompactFileInfo` 结构体。

- [ ] **Step 1: 修改 `peri-agent/src/agent/events.rs`**

在 `BackgroundTaskResult` 结构体之前，添加 `CompactFileInfo`：

```rust
/// Compact 保留的文件信息摘要
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CompactFileInfo {
    pub path: String,
    pub lines: usize,
}
```

将现有的 `CompactStarted` / `CompactCompleted` 变体替换为：

```rust
    /// 上下文压缩开始
    CompactStarted,
    /// 上下文压缩完成
    CompactCompleted {
        /// 摘要文本（full compact 时非空，micro compact 时为空）
        summary: String,
        /// 保留的文件摘要列表
        files: Vec<CompactFileInfo>,
        /// 保留的 Skill 名称列表
        skills: Vec<String>,
        /// micro-compact 清除的工具结果数量（>0 表示 micro-compact）
        micro_cleared: usize,
    },
    /// 上下文压缩失败
    CompactError {
        message: String,
    },
```

- [ ] **Step 2: 编译验证**

```bash
cargo build -p peri-agent 2>&1
```

Expected: 编译通过（`CompactStarted` / `CompactCompleted` 只在本 crate 内使用，字段变更不影响外部，因为外部目前都是 match discard）。

- [ ] **Step 3: 更新测试**

在 `peri-agent/src/agent/events_test.rs` 中，找到 `test_context_warning_serde_roundtrip` 测试函数。检查是否有 `CompactCompleted` 的 serde 测试。如果有，更新其构造以匹配新字段。如果没有，不需要改动。

```bash
cargo test -p peri-agent --lib -- events 2>&1
```

Expected: 所有 events 测试通过。

- [ ] **Step 4: Commit**

```bash
git add peri-agent/src/agent/events.rs peri-agent/src/agent/events_test.rs
git commit -m "feat(agent): add data fields to CompactCompleted/CompactError ExecutorEvent variants

CompactCompleted now carries summary, files, skills, micro_cleared.
CompactError carries message. CompactFileInfo struct for file summaries.

Co-Authored-By: deepseek-v4-pro <deepseek-ai@claude-code-best.win>"
```

---

### Task 2: compact_runner 公共模块 (peri-acp)

**Files:**
- Create: `peri-acp/src/session/compact_runner.rs`
- Modify: `peri-acp/src/session/mod.rs`

提取 compact 执行逻辑为独立模块，供 executor auto-compact 和手动 `session/compact` 共用。

- [ ] **Step 1: 创建 `peri-acp/src/session/compact_runner.rs`**

```rust
//! Compact execution logic shared by auto-compact and manual session/compact.
//!
//! Wraps `peri_agent::agent::compact::{full_compact, micro_compact_enhanced, re_inject}`
//! with hook firing, event sending, and cancellation support.

use std::sync::Arc;

use peri_agent::agent::compact::config::CompactConfig;
use peri_agent::agent::compact::{full_compact, micro_compact_enhanced, re_inject};
use peri_agent::agent::events::{AgentEvent as ExecutorEvent, CompactFileInfo};
use peri_agent::agent::AgentCancellationToken;
use peri_agent::llm::BaseModel;
use peri_agent::messages::BaseMessage;
use tokio::sync::mpsc;
use tracing::{info, warn};

/// Compact 执行结果
pub struct CompactOutput {
    /// 压缩后的新消息列表（summary + re_inject messages）
    pub new_messages: Vec<BaseMessage>,
    /// 摘要文本
    pub summary: String,
    /// 保留的文件信息
    pub files: Vec<CompactFileInfo>,
    /// 保留的 Skill 名称
    pub skills: Vec<String>,
}

/// Hook 上下文信息
pub struct HookContext {
    pub cwd: String,
    pub session_id: String,
    pub transcript_path: String,
    pub provider_name: String,
    /// 可选的 compact 指令（手动 /compact 传入）
    pub instructions: String,
}

/// 通过 event_tx 发送事件
fn send_event(
    event_tx: &Arc<std::sync::Mutex<Option<mpsc::UnboundedSender<ExecutorEvent>>>>,
    event: ExecutorEvent,
) {
    if let Some(tx) = event_tx.lock().unwrap().as_ref() {
        let _ = tx.send(event);
    }
}

/// 执行 full compact：full_compact + re_inject + hooks + 事件通知
#[allow(clippy::too_many_arguments)]
pub async fn run_full_compact(
    messages: &[BaseMessage],
    model: &dyn BaseModel,
    config: &CompactConfig,
    cwd: &str,
    event_tx: &Arc<std::sync::Mutex<Option<mpsc::UnboundedSender<ExecutorEvent>>>>,
    cancel: &AgentCancellationToken,
    hooks: &[peri_middlewares::hooks::types::RegisteredHook],
    hook_ctx: &HookContext,
) -> Result<CompactOutput, String> {
    let msg_count = messages.len();
    info!(msg_count, "compact_runner: starting full compact");

    // Fire PreCompact hooks
    peri_middlewares::hooks::middleware::fire_standalone_lifecycle_hooks(
        hooks,
        peri_middlewares::hooks::types::HookEvent::PreCompact,
        &hook_ctx.cwd,
        &hook_ctx.session_id,
        &hook_ctx.transcript_path,
        &hook_ctx.provider_name,
        Some(msg_count),
    )
    .await;

    send_event(event_tx, ExecutorEvent::CompactStarted);

    // full_compact with cancellation
    let compact_result = tokio::select! {
        biased;
        _ = cancel.cancelled() => {
            send_event(event_tx, ExecutorEvent::CompactError {
                message: "已取消".to_string(),
            });
            fire_post_compact_hooks(hooks, hook_ctx, msg_count).await;
            return Err("已取消".to_string());
        }
        result = full_compact(messages, model, config, &hook_ctx.instructions) => {
            match result {
                Ok(r) => r,
                Err(e) => {
                    warn!(error = %e, "compact_runner: full_compact failed");
                    send_event(event_tx, ExecutorEvent::CompactError {
                        message: e.to_string(),
                    });
                    fire_post_compact_hooks(hooks, hook_ctx, msg_count).await;
                    return Err(e.to_string());
                }
            }
        }
    };

    // Cancel check before re_inject
    if cancel.is_cancelled() {
        send_event(event_tx, ExecutorEvent::CompactError {
            message: "已取消".to_string(),
        });
        fire_post_compact_hooks(hooks, hook_ctx, msg_count).await;
        return Err("已取消".to_string());
    }

    info!(
        summary_len = compact_result.summary.len(),
        messages_used = compact_result.messages_used,
        "compact_runner: full_compact completed"
    );

    // re_inject
    let re_inject_result = tokio::select! {
        biased;
        _ = cancel.cancelled() => {
            send_event(event_tx, ExecutorEvent::CompactError {
                message: "已取消".to_string(),
            });
            fire_post_compact_hooks(hooks, hook_ctx, msg_count).await;
            return Err("已取消".to_string());
        }
        result = re_inject(messages, config, cwd) => result,
    };

    info!(
        files_injected = re_inject_result.files_injected,
        skills_injected = re_inject_result.skills_injected,
        "compact_runner: re_inject completed"
    );

    // Build new messages
    let mut new_messages = vec![BaseMessage::system(compact_result.summary.clone())];
    new_messages.extend(re_inject_result.messages);

    // Extract file info from re_inject content
    let files = extract_file_info(messages, &re_inject_result);
    let skills = extract_skill_names(&re_inject_result);

    send_event(event_tx, ExecutorEvent::CompactCompleted {
        summary: compact_result.summary.clone(),
        files: files.clone(),
        skills: skills.clone(),
        micro_cleared: 0,
    });

    fire_post_compact_hooks(hooks, hook_ctx, msg_count).await;

    Ok(CompactOutput {
        new_messages,
        summary: compact_result.summary,
        files,
        skills,
    })
}

/// 执行 micro-compact：原地修改 messages，发送 CompactCompleted 事件
pub fn run_micro_compact(
    messages: &mut [BaseMessage],
    config: &CompactConfig,
    event_tx: &Arc<std::sync::Mutex<Option<mpsc::UnboundedSender<ExecutorEvent>>>>,
) -> usize {
    let cleared = micro_compact_enhanced(config, messages);
    if cleared > 0 {
        info!(cleared, "compact_runner: micro-compact completed");
        send_event(event_tx, ExecutorEvent::CompactCompleted {
            summary: String::new(),
            files: vec![],
            skills: vec![],
            micro_cleared: cleared,
        });
    }
    cleared
}

async fn fire_post_compact_hooks(
    hooks: &[peri_middlewares::hooks::types::RegisteredHook],
    ctx: &HookContext,
    msg_count: usize,
) {
    peri_middlewares::hooks::middleware::fire_standalone_lifecycle_hooks(
        hooks,
        peri_middlewares::hooks::types::HookEvent::PostCompact,
        &ctx.cwd,
        &ctx.session_id,
        &ctx.transcript_path,
        &ctx.provider_name,
        Some(msg_count),
    )
    .await
}

/// 从原始消息中提取 re_inject 涉及的文件路径和行数
fn extract_file_info(
    original_messages: &[BaseMessage],
    re_inject_result: &peri_agent::agent::compact::ReInjectResult,
) -> Vec<CompactFileInfo> {
    // re_inject 消息格式: "[最近读取的文件: path]\n{content}"
    let mut files = Vec::new();
    for msg in &re_inject_result.messages {
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

/// 从 re_inject 结果中提取 skill 名称
fn extract_skill_names(
    re_inject_result: &peri_agent::agent::compact::ReInjectResult,
) -> Vec<String> {
    let mut skills = Vec::new();
    for msg in &re_inject_result.messages {
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
```

- [ ] **Step 2: 注册模块 `peri-acp/src/session/mod.rs`**

在现有 `pub mod` 行之后添加：

```rust
pub mod compact_runner;
```

- [ ] **Step 3: 编译验证**

```bash
cargo build -p peri-acp 2>&1
```

Expected: 编译通过。

- [ ] **Step 4: Commit**

```bash
git add peri-acp/src/session/compact_runner.rs peri-acp/src/session/mod.rs
git commit -m "feat(acp): add compact_runner module for shared compact execution

Extracts full_compact + re_inject + hooks + event sending into
a reusable module for both auto-compact (executor loop) and
manual session/compact requests.

Co-Authored-By: deepseek-v4-pro <deepseek-ai@claude-code-best.win>"
```

---

### Task 3: executor.rs compact 循环 (peri-acp)

**Files:**
- Modify: `peri-acp/src/session/executor.rs`

在 `execute_prompt` 中增加 compact 循环：agent 执行结束后检查 token 阈值，需要时调用 compact_runner 并内部 resubmit。

- [ ] **Step 1: 修改 `peri-acp/src/session/executor.rs`**

在文件顶部添加 imports：

```rust
use crate::session::compact_runner::{self, HookContext};
use peri_agent::agent::compact::config::CompactConfig;
use peri_agent::agent::token::ContextBudget;
```

将 `execute_prompt` 函数体替换为以下结构。关键变化：

1. Event channel 和 pump 在循环外创建，整个 `execute_prompt` 生命周期内保持存活
2. 循环内部：build_agent → execute → compact check → resubmit or return
3. `PromptResult` 新增 `compacted: bool` 字段

新的 `PromptResult`：

```rust
/// Result of prompt execution.
pub struct PromptResult {
    /// Updated message history after execution.
    pub messages: Vec<BaseMessage>,
    /// Whether execution succeeded.
    pub ok: bool,
    /// Whether a compact occurred during execution.
    pub compacted: bool,
}
```

新的 `execute_prompt` 函数体（替换原函数体）：

```rust
#[allow(clippy::too_many_arguments)]
pub async fn execute_prompt(
    provider: &LlmProvider,
    peri_config: Arc<crate::provider::PeriConfig>,
    cwd: &str,
    content: String,
    history: Vec<BaseMessage>,
    is_empty_history: bool,
    permission_mode: Arc<peri_middlewares::prelude::SharedPermissionMode>,
    event_sink: Arc<dyn EventSink>,
    cancel: AgentCancellationToken,
    broker: Arc<dyn UserInteractionBroker>,
    plugin_skill_dirs: Vec<std::path::PathBuf>,
    plugin_agent_dirs: Vec<std::path::PathBuf>,
    hook_groups: Vec<Vec<peri_middlewares::hooks::RegisteredHook>>,
    cron_scheduler: Option<Arc<parking_lot::Mutex<peri_middlewares::cron::CronScheduler>>>,
    session_id: String,
    mcp_pool: Option<Arc<peri_middlewares::mcp::McpClientPool>>,
    tool_search_index: Arc<peri_middlewares::tool_search::ToolSearchIndex>,
    shared_tools: Arc<
        parking_lot::RwLock<
            std::collections::HashMap<String, Arc<dyn peri_agent::tools::BaseTool>>,
        >,
    >,
    lsp_servers: Vec<peri_lsp::config::LspServerConfig>,
) -> PromptResult {
    let agent_input = peri_agent::agent::react::AgentInput::text(content);

    // Compact config and context budget (computed once)
    let mut compact_config = peri_config.config.compact.clone().unwrap_or_default();
    compact_config.apply_env_overrides();
    let context_window = provider.context_window();
    let context_1m = peri_config.config.context_1m.unwrap_or(false);
    let effective_context_window = if context_1m { 1_000_000 } else { context_window };
    let budget = ContextBudget::new(effective_context_window)
        .with_auto_compact_threshold(compact_config.auto_compact_threshold)
        .with_warning_threshold(compact_config.micro_compact_threshold);

    let disable_compact = std::env::var("DISABLE_COMPACT").is_ok()
        || !compact_config.auto_compact_enabled;

    // Event channel (lives for entire execute_prompt lifetime)
    let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel::<ExecutorEvent>();
    let event_tx = Arc::new(std::sync::Mutex::new(Some(event_tx)));

    // Background event pump
    let sink = event_sink;
    let sid = session_id.clone();
    let (pump_done_tx, pump_done_rx) = oneshot::channel();
    let pump_cw = effective_context_window;
    tokio::spawn(async move {
        while let Some(exec_event) = event_rx.recv().await {
            sink.push_event(&sid, &exec_event, pump_cw).await;
        }
        sink.push_done(&sid).await;
        let _ = pump_done_tx.send(());
    });

    let mut current_history = history;
    let mut total_resubmits: u32 = 0;
    const MAX_RESUBMITS: u32 = 3;
    let mut compacted = false;

    loop {
        // Create event handler for this round
        let event_handler: Arc<dyn AgentEventHandler> =
            Arc::new(peri_agent::agent::events::FnEventHandler({
                let tx = event_tx.clone();
                move |event: ExecutorEvent| {
                    if let Some(tx) = tx.lock().unwrap().as_ref() {
                        let _ = tx.send(event);
                    }
                }
            }));

        let features = PromptFeatures::detect();
        let system_prompt = build_system_prompt(None, cwd, features, &plugin_agent_dirs);

        let agent_output = builder::build_agent(AcpAgentConfig {
            provider: provider.clone(),
            cwd: cwd.to_string(),
            system_prompt,
            event_handler,
            cancel: cancel.clone(),
            permission_mode: permission_mode.clone(),
            peri_config: Arc::new(peri_config.as_ref().clone()),
            cron_scheduler: cron_scheduler.clone(),
            agent_overrides: None,
            preload_skills: Vec::new(),
            session_id: Some(session_id.clone()),
            broker: broker.clone(),
            plugin_skill_dirs: plugin_skill_dirs.clone(),
            plugin_agent_dirs: plugin_agent_dirs.clone(),
            hook_groups: hook_groups.clone(),
            hook_session_start: is_empty_history && total_resubmits == 0,
            mcp_pool: mcp_pool.clone(),
            tool_search_index: tool_search_index.clone(),
            shared_tools: shared_tools.clone(),
            child_handler_factory: None,
            lsp_servers: lsp_servers.clone(),
        });

        // Execute agent
        let mut agent_state = AgentState::with_messages(cwd.to_string(), current_history);
        let result = agent_output
            .executor
            .execute(agent_input.clone(), &mut agent_state, Some(cancel.clone()))
            .await;
        drop(agent_output);

        let ok = result.is_ok();
        if let Err(e) = &result {
            error!(session_id = %session_id, error = %e, "Agent execution failed");
        }

        if !ok || cancel.is_cancelled() {
            // Close event channel and wait for pump
            close_channel(&event_tx);
            wait_for_pump(pump_done_rx, &session_id).await;
            return PromptResult {
                messages: agent_state.into_messages(),
                ok,
                compacted,
            };
        }

        let mut messages = agent_state.into_messages();

        // ── Compact check ──
        if !disable_compact && messages.len() > 1 {
            let tracker = agent_state.token_tracker();

            if budget.should_auto_compact(tracker) && total_resubmits < MAX_RESUBMITS {
                // Full compact + resubmit
                info!(session_id = %session_id, "auto-compact: threshold reached, triggering full compact");
                let all_hooks: Vec<_> = hook_groups.iter().flatten().cloned().collect();
                let hook_ctx = HookContext {
                    cwd: cwd.to_string(),
                    session_id: session_id.clone(),
                    transcript_path: String::new(),
                    provider_name: provider.display_name().to_string(),
                    instructions: String::new(),
                };

                match compact_runner::run_full_compact(
                    &messages,
                    &provider.clone().into_model(),
                    &compact_config,
                    cwd,
                    &event_tx,
                    &cancel,
                    &all_hooks,
                    &hook_ctx,
                )
                .await
                {
                    Ok(output) => {
                        compacted = true;
                        current_history = output.new_messages;
                        total_resubmits += 1;
                        info!(
                            session_id = %session_id,
                            resubmit = total_resubmits,
                            "auto-compact: resubmitting with compacted context"
                        );
                        continue;
                    }
                    Err(e) => {
                        warn!(session_id = %session_id, error = %e, "auto-compact: failed, returning original messages");
                    }
                }
            } else if budget.should_warn(tracker) {
                // Micro-compact only
                compact_runner::run_micro_compact(&mut messages, &compact_config, &event_tx);
            }
        }

        // Done — close event channel and wait for pump
        close_channel(&event_tx);
        wait_for_pump(pump_done_rx, &session_id).await;
        return PromptResult {
            messages,
            ok: true,
            compacted,
        };
    }
}

fn close_channel(event_tx: &Arc<std::sync::Mutex<Option<tokio::sync::mpsc::UnboundedSender<ExecutorEvent>>>>) {
    let mut tx_guard = event_tx.lock().unwrap();
    *tx_guard = None;
}

async fn wait_for_pump(
    pump_done_rx: oneshot::Receiver<()>,
    session_id: &str,
) {
    match pump_done_rx.await {
        Ok(()) => debug!(session_id, "Event pump done"),
        Err(_) => error!(session_id, "Event pump done channel closed unexpectedly"),
    }
}
```

- [ ] **Step 2: 编译验证**

```bash
cargo build -p peri-acp 2>&1
```

Expected: 编译通过。注意 `PromptResult` 增加了 `compacted: bool` 字段，下游使用处需要更新。

- [ ] **Step 3: 修复下游 PromptResult 使用处**

搜索所有使用 `PromptResult` 的地方，确保初始化时包含 `compacted: false`（或在 `acp_server.rs` 中使用 `..Default` 如果实现了 Default）。

在 `peri-tui/src/acp_server.rs` 的 `execute_prompt` 函数中，`result.messages` 和 `result.ok` 的使用不需要改动，`compacted` 字段暂时忽略。

```bash
cargo build -p peri-tui 2>&1
```

Expected: 编译通过。

- [ ] **Step 4: Commit**

```bash
git add peri-acp/src/session/executor.rs peri-tui/src/acp_server.rs
git commit -m "feat(acp): add compact loop to execute_prompt

After agent execution, checks token thresholds and auto-triggers
full compact + resubmit (up to 3x) or micro-compact as needed.
Event channel stays alive across the entire loop so compact events
flow through the same EventSink pipeline.

Co-Authored-By: deepseek-v4-pro <deepseek-ai@claude-code-best.win>"
```

---

### Task 4: TUI 事件类型更新 (peri-tui)

**Files:**
- Modify: `peri-tui/src/app/events.rs`
- Modify: `peri-tui/src/app/agent.rs`

更新 TUI AgentEvent 枚举和 map_executor_event 映射。

- [ ] **Step 1: 修改 `peri-tui/src/app/events.rs`**

删除旧的 `CompactDone` 和 `CompactError` 变体：

```rust
// 删除：
    /// 上下文压缩成功，携带摘要文本和新 Thread ID
    CompactDone {
        summary: String,
        new_thread_id: String,
    },
    /// 上下文压缩失败，携带错误信息
    CompactError(String),
```

添加新变体（在 `LspDiagnostics` 之前）：

```rust
    /// 上下文压缩开始
    CompactStarted,
    /// 上下文压缩完成
    CompactCompleted {
        summary: String,
        files: Vec<peri_agent::agent::events::CompactFileInfo>,
        skills: Vec<String>,
        micro_cleared: usize,
    },
    /// 上下文压缩失败
    CompactError {
        message: String,
    },
```

- [ ] **Step 2: 修改 `peri-tui/src/app/agent.rs` map_executor_event**

将现在的 discard 行：

```rust
        ExecutorEvent::SessionEnded
        | ExecutorEvent::CompactStarted
        | ExecutorEvent::CompactCompleted => return None,
```

替换为：

```rust
        ExecutorEvent::SessionEnded => return None,
        ExecutorEvent::CompactStarted => AgentEvent::CompactStarted,
        ExecutorEvent::CompactCompleted {
            summary,
            files,
            skills,
            micro_cleared,
        } => AgentEvent::CompactCompleted {
            summary,
            files,
            skills,
            micro_cleared,
        },
        ExecutorEvent::CompactError { message } => AgentEvent::CompactError { message },
```

- [ ] **Step 3: 编译验证**

此时期望编译失败，因为 `agent_compact.rs` 等旧代码引用了 `CompactDone` 和旧的 `CompactError(String)` 变体。这是预期的——将在 Task 5 中修复。

```bash
cargo build -p peri-tui 2>&1 | head -30
```

Expected: 编译错误指向 `agent_compact.rs`、`agent_ops.rs` 中对旧变体的引用。记录错误位置，下一步修复。

- [ ] **Step 4: Commit**

先不 commit，等 Task 5 一起修复编译。

---

### Task 5: TUI compact 清理 — 删除旧代码 + 新增 UI 处理 (peri-tui)

**Files:**
- Delete: `peri-tui/src/app/agent_compact.rs`
- Modify: `peri-tui/src/app/agent_comm.rs`
- Modify: `peri-tui/src/app/agent_ops.rs`
- Modify: `peri-tui/src/app/agent_events_bg.rs`
- Modify: `peri-tui/src/app/agent_submit.rs`
- Modify: `peri-tui/src/app/thread_ops.rs`
- Modify: `peri-tui/src/app/agent.rs`（删除 compact_task）
- Modify: `peri-tui/src/app/mod.rs`（移除 agent_compact 模块引用）

这是最大的任务。按顺序执行确保中间步骤可编译。

- [ ] **Step 1: 删除 `peri-tui/src/app/agent_compact.rs`**

```bash
rm peri-tui/src/app/agent_compact.rs
```

在 `peri-tui/src/app/mod.rs` 中删除对应的 `mod agent_compact;` 行（如果存在的话；可能通过 `mod.rs` 的 glob 引用）。搜索：

```bash
grep -n "agent_compact" peri-tui/src/app/mod.rs
```

如果找到，删除该行。

- [ ] **Step 2: 删除 `peri-tui/src/app/agent.rs` 中的 `compact_task` 函数**

在 `agent.rs` 中，删除整个 `compact_task` 函数（约 line 182-359，从 `pub async fn compact_task(` 到其结尾的 `}`）。同时删除文件顶部不再需要的 imports（`AgentCancellationToken` 如果仅被 `compact_task` 使用——检查 `map_executor_event` 是否也用，如果是则保留）。

- [ ] **Step 3: 删除 `peri-tui/src/app/thread_ops.rs` 中的 `start_compact` 和 `start_micro_compact`**

删除 `start_compact` 方法（约 line 335-451）和 `start_micro_compact` 方法（在 `agent_compact.rs` 中已被删除，但 `start_micro_compact` 可能在 `thread_ops.rs` 中也有引用——检查）。

实际上 `start_micro_compact` 定义在 `agent_compact.rs` 中（已被删除），但它的调用点在 `agent_ops.rs` 的 Done 处理器中。调用点将在下一步删除。

`start_compact` 定义在 `thread_ops.rs` line 335-451，删除它。

- [ ] **Step 4: 修改 `peri-tui/src/app/agent_comm.rs`**

从 `AgentComm` 结构体中删除以下字段：

```rust
    /// 是否需要 auto-compact（在 LlmCallEnd 时标记，Done 时执行）
    pub needs_auto_compact: bool,
    /// 连续 auto-compact 失败次数（circuit breaker，达到 3 次后停止自动触发）
    pub auto_compact_failures: u32,
    /// compact 前的 token tracker 快照（compact 失败时恢复，防止 tracker 失去对上下文大小的感知）
    pub pre_compact_token_snapshot: Option<peri_agent::agent::token::TokenTracker>,
    /// 本轮用户原始输入（compact 后自动 re-submit 用）
    pub last_user_input: Option<String>,
    /// compact 启动时保存的用户输入副本（防止 compact 过程中 last_user_input 被覆盖）
    pub pre_compact_user_input: Option<String>,
    /// 连续 auto-compact re-submit 次数（防止无限循环，上限 3 次）
    pub auto_compact_resubmit_count: u32,
    /// compact 完成后是否应自动 resubmit（仅 agent 执行中 auto-compact 为 true，
    /// 手动 /compact 和 Done 后 auto-compact 为 false）
    pub compact_should_resubmit: bool,
```

同时在 `Default` impl 中删除对应初始化。

- [ ] **Step 5: 修改 `peri-tui/src/app/agent_ops.rs`**

这是最复杂的修改。需要：

**a) 删除 `ContextWarning` 处理器中的 compact 触发逻辑**

在 `ContextWarning` match arm 中（约 line 355-372），删除 `needs_auto_compact = true` 的设置和相关检查。保留 `context_window` 更新。处理器简化为：

```rust
            AgentEvent::ContextWarning {
                used_tokens: _,
                total_tokens,
                percentage: _,
            } => {
                // 更新 context_window（模型可能切换导致变化）
                let cw = total_tokens as u32;
                if cw > 0
                    && self.session_mgr.sessions[self.session_mgr.active]
                        .agent
                        .context_window
                        != cw
                {
                    self.session_mgr.sessions[self.session_mgr.active]
                        .agent
                        .context_window = cw;
                }
                (true, false, false)
            }
```

**b) 删除 `TokenUsageUpdate` 处理器中的 compact 判断逻辑**

在 `TokenUsageUpdate` match arm 中（约 line 451-483），删除从 `// compact 被完全禁用` 开始到 `needs_auto_compact = true` 的整个 compact 检查块。保留 token 累积、cache 检查、spinner 更新。处理器尾部简化为直接返回 `(true, false, false)`。

**c) 删除 `Done` 处理器中的 auto-compact 执行逻辑**

在 `Done` match arm 中（约 line 682-726），删除从 `// Auto-compact 两级策略` 开始到 `start_micro_compact()` 调用的整个块。保留 `has_bg_tasks` 检查（后台任务仍然需要延迟 Done 处理）。

**d) 删除 `Interrupted` 处理器中的 `needs_auto_compact = false`**

在 `Interrupted` match arm 中（约 line 777-779），删除 `self.session_mgr.sessions[...].agent.needs_auto_compact = false;`。

**e) 新增 `CompactStarted`、`CompactCompleted`、`CompactError` 处理器**

在 `handle_agent_event` 的 match 中添加（在 `LspDiagnostics` 之后、`Done` 之前）：

```rust
            AgentEvent::CompactStarted => {
                self.session_mgr.sessions[self.session_mgr.active]
                    .spinner_state
                    .set_verb(Some(self.services.lc.tr("app-compact-compressing").leak()));
                self.request_rebuild();
                (true, false, false)
            }
            AgentEvent::CompactCompleted {
                summary,
                files,
                skills,
                micro_cleared,
            } => self.handle_compact_completed(summary, files, skills, micro_cleared),
            AgentEvent::CompactError { message } => self.handle_compact_error(message),
```

在 `impl App` 中（可以用一个新文件 `peri-tui/src/app/agent_compact_ui.rs` 或直接在 `agent_ops.rs` 末尾）添加处理方法：

```rust
    fn handle_compact_completed(
        &mut self,
        summary: String,
        files: Vec<peri_agent::agent::events::CompactFileInfo>,
        skills: Vec<String>,
        micro_cleared: usize,
    ) -> (bool, bool, bool) {
        if micro_cleared > 0 {
            // Micro-compact: 简单通知
            let vm = MessageViewModel::system(self.services.lc.tr_args(
                "app-compact-auto-cleared",
                &[("count".into(), (micro_cleared as i64).into())],
            ));
            self.apply_pipeline_action(PipelineAction::AddMessage(vm));
            return (true, false, false);
        }

        // Full compact: 显示摘要 + 文件列表
        let truncated: String = summary.chars().take(30).collect();
        let ellipsis = if summary.chars().count() > 30 { "…" } else { "" };
        let mut label_lines = vec![format!("✻ Compact: {}{}", truncated, ellipsis)];
        for fi in &files {
            label_lines.push(format!("  ⎿  Read {} ({} lines)", fi.path, fi.lines));
        }
        if !skills.is_empty() {
            label_lines.push(format!("  ⎿  Skill: {}", skills.join(", ")));
        }
        let compact_label = label_lines.join("\n");

        // 清除 ephemeral_notes（旧锚点失效）
        self.session_mgr.sessions[self.session_mgr.active]
            .messages
            .ephemeral_notes
            .clear();

        let view_msgs = vec![MessageViewModel::system(compact_label)];
        self.apply_pipeline_action(PipelineAction::RebuildAll {
            prefix_len: 0,
            tail_vms: view_msgs,
        });

        (true, false, false)
    }

    fn handle_compact_error(&mut self, message: String) -> (bool, bool, bool) {
        let vm = MessageViewModel::system(
            self.services
                .lc
                .tr_args("app-compact-failed", &[("error".into(), message.into())]),
        );
        self.apply_pipeline_action(PipelineAction::AddMessage(vm));
        (true, false, false)
    }
```

**f) 删除 `Done` 处理器中的 circuit breaker 渐进恢复逻辑**

在 `Done` match arm 中（约 line 738-746），删除 `auto_compact_failures /= 2` 的逻辑。

- [ ] **Step 6: 修改 `peri-tui/src/app/agent_events_bg.rs`**

在 `handle_background_task_completed` 中（约 line 216-234），删除 deferred compact 触发逻辑：

```rust
// 删除整个 if 块：
                if self.session_mgr.sessions[self.session_mgr.active]
                    .agent
                    .needs_auto_compact
                {
                    self.session_mgr.sessions[self.session_mgr.active]
                        .agent
                        .needs_auto_compact = false;
                    tracing::info!(...);
                    self.start_compact("auto".to_string());
                    self.session_mgr.sessions[self.session_mgr.active]
                        .agent
                        .compact_should_resubmit = false;
                    return (true, false, true);
                }
```

- [ ] **Step 7: 修改 `peri-tui/src/app/agent_submit.rs`**

删除以下字段的赋值（在 `submit_message` 中）：

```rust
// 删除：
        self.session_mgr.sessions[self.session_mgr.active]
            .agent
            .last_user_input = Some(input.clone());
        self.session_mgr.sessions[self.session_mgr.active]
            .agent
            .auto_compact_resubmit_count = 0;
```

- [ ] **Step 8: 修改 `peri-tui/src/app/thread_ops.rs`**

删除 `reset_agent_session` 中的 compact 相关重置：

```rust
// 删除：
        self.session_mgr.sessions[self.session_mgr.active]
            .agent
            .pre_compact_token_snapshot = None;
        self.session_mgr.sessions[self.session_mgr.active]
            .agent
            .needs_auto_compact = false;
        self.session_mgr.sessions[self.session_mgr.active]
            .agent
            .auto_compact_failures = 0;
        self.session_mgr.sessions[self.session_mgr.active]
            .agent
            .compact_should_resubmit = false;
```

- [ ] **Step 9: 编译验证**

```bash
cargo build -p peri-tui 2>&1
```

Expected: 编译通过。如果有残留引用旧字段的编译错误，逐个修复。

- [ ] **Step 10: 运行测试**

```bash
cargo test -p peri-tui --lib 2>&1 | tail -20
```

Expected: 所有测试通过。部分测试可能引用了旧的 compact 字段（如 `headless_test.rs` 中的 `needs_auto_compact`），需要更新这些测试以删除对已删字段的断言。

- [ ] **Step 11: Commit**

```bash
git add -A
git commit -m "refactor(tui): remove old compact code, add compact UI event handlers

- Delete agent_compact.rs (handle_compact_done/error + start_micro_compact)
- Delete compact_task from agent.rs
- Delete start_compact from thread_ops.rs
- Remove 8 compact-related fields from AgentComm
- Remove compact trigger logic from ContextWarning/TokenUsageUpdate/Done
- Remove deferred compact from agent_events_bg.rs
- Add handle_compact_completed/error UI handlers
- Map CompactStarted/Completed/Error in map_executor_event

Co-Authored-By: deepseek-v4-pro <deepseek-ai@claude-code-best.win>"
```

---

### Task 6: 手动 /compact 命令 — ACP session/compact (peri-tui + peri-acp)

**Files:**
- Modify: `peri-tui/src/acp_server.rs`
- Modify: `peri-tui/src/command/session/compact.rs`
- Modify: `peri-tui/src/acp_client/client.rs`（如果需要新增 send_compact 方法）

- [ ] **Step 1: 在 `peri-tui/src/acp_server.rs` 添加 `session/compact` 处理**

在 `run_acp_server` 的 main loop 中，在 `session/prompt` 分支之后添加 `session/compact` 分支：

```rust
                } else if method == "session/compact" {
                    let sessions = sessions.clone();
                    let transport = Arc::clone(&transport);
                    let provider = cfg.provider.clone();
                    let peri_config = cfg.peri_config.clone();
                    let hook_groups = cfg.hook_groups.clone();
                    let thread_store = cfg.thread_store.clone();
                    tokio::spawn(async move {
                        let result = execute_compact(
                            params,
                            &sessions,
                            &provider,
                            &peri_config,
                            &hook_groups,
                            &transport,
                            &thread_store,
                        )
                        .await;
                        let _ = transport.send_response(id, result).await;
                    });
```

添加 `execute_compact` 函数：

```rust
#[allow(clippy::too_many_arguments)]
async fn execute_compact(
    params: Value,
    sessions: &SharedSessions,
    provider: &Arc<RwLock<LlmProvider>>,
    peri_config: &Arc<RwLock<PeriConfig>>,
    hook_groups: &[Vec<peri_middlewares::hooks::RegisteredHook>],
    transport: &Arc<dyn peri_acp::transport::AcpTransport>,
    thread_store: &Arc<dyn peri_agent::thread::ThreadStore>,
) -> Result<Value, AcpError> {
    let session_id = params
        .get("sessionId")
        .or_else(|| params.get("session_id"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| AcpError::new(-32602, "missing sessionId"))?
        .to_string();
    let instructions = params
        .get("instructions")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let (cwd, history, thread_id) = {
        let sessions = sessions.lock().await;
        let state = sessions
            .get(&session_id)
            .ok_or_else(|| AcpError::new(-32602, "session not found"))?;
        (
            state.cwd.clone(),
            state.history.clone(),
            state.thread_id.clone(),
        )
    };

    let provider_snapshot = provider.read().clone();
    let peri_config_snapshot = Arc::new(peri_config.read().clone());
    let mut compact_config = peri_config_snapshot.config.compact.clone().unwrap_or_default();
    compact_config.apply_env_overrides();

    let cancel = AgentCancellationToken::new();

    // Use TransportEventSink for event routing
    let event_sink = Arc::new(TransportEventSink::new(Arc::clone(transport)));

    // Create event channel + pump (same pattern as execute_prompt)
    let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel::<ExecutorEvent>();
    let event_tx = Arc::new(std::sync::Mutex::new(Some(event_tx)));
    let sink = event_sink.clone();
    let sid = session_id.clone();
    let context_window = provider_snapshot.context_window();
    let (pump_done_tx, pump_done_rx) = oneshot::channel();
    tokio::spawn(async move {
        while let Some(event) = event_rx.recv().await {
            sink.push_event(&sid, &event, context_window).await;
        }
        sink.push_done(&sid).await;
        let _ = pump_done_tx.send(());
    });

    let all_hooks: Vec<_> = hook_groups.iter().flatten().cloned().collect();
    let hook_ctx = compact_runner::HookContext {
        cwd: cwd.clone(),
        session_id: session_id.clone(),
        transcript_path: String::new(),
        provider_name: provider_snapshot.display_name().to_string(),
        instructions,
    };

    let result = compact_runner::run_full_compact(
        &history,
        &provider_snapshot.clone().into_model(),
        &compact_config,
        &cwd,
        &event_tx,
        &cancel,
        &all_hooks,
        &hook_ctx,
    )
    .await;

    // Close event channel and wait for pump
    {
        let mut tx_guard = event_tx.lock().unwrap();
        *tx_guard = None;
    }
    let _ = pump_done_rx.await;

    match result {
        Ok(output) => {
            // 仅更新内存中的 session history（不立即持久化到 ThreadStore）。
            // ThreadStore 只有 append_messages，没有 replace_messages。
            // 下一次 session/prompt 执行完后，execute_prompt 返回的 messages
            // 会通过 append_messages 自然持久化。TUI 侧通过 CompactCompleted
            // 事件更新 agent_state_messages。
            {
                let mut sessions = sessions.lock().await;
                if let Some(state) = sessions.get_mut(&session_id) {
                    state.history = output.new_messages;
                }
            }
            let resp = serde_json::json!({
                "compacted": true,
                "summary": output.summary,
            });
            Ok(resp)
        }
        Err(e) => Err(AcpError::new(-32603, format!("Compact failed: {e}"))),
    }
}
```

- [ ] **Step 2: 在 `AcpTuiClient` 添加 `compact` 方法**

在 `peri-tui/src/acp_client/client.rs` 中添加：

```rust
    /// Send a session/compact request to the ACP server.
    pub async fn compact(&self, instructions: &str) -> Result<(), String> {
        let session_id = self
            .current_session_id
            .lock()
            .unwrap()
            .clone()
            .ok_or("no active session")?;
        self.transport
            .send_request(
                "session/compact",
                serde_json::json!({
                    "sessionId": session_id,
                    "instructions": instructions,
                }),
            )
            .await
            .map_err(|e| e.to_string())?;
        Ok(())
    }
```

- [ ] **Step 3: 修改 `/compact` 命令**

修改 `peri-tui/src/command/session/compact.rs`：

```rust
use crate::app::App;
use crate::command::Command;

pub struct CompactCommand;

impl Command for CompactCommand {
    fn name(&self) -> &str {
        "compact"
    }

    fn description(&self, _lc: &crate::i18n::LcRegistry) -> String {
        _lc.tr("command-compact-description")
    }

    fn execute(&self, app: &mut App, args: &str) {
        if app.session_mgr.sessions[app.session_mgr.active].ui.loading {
            app.push_system_note("Agent 运行中，无法执行压缩".to_string());
            app.render_rebuild();
            return;
        }
        if let Some(ref client) = app.acp_client {
            let client = client.clone();
            let args = args.to_string();
            tokio::spawn(async move {
                match client.compact(&args).await {
                    Ok(()) => tracing::info!("compact: request completed"),
                    Err(e) => tracing::error!(error = %e, "compact: request failed"),
                }
            });
        } else {
            app.push_system_note("ACP 客户端未初始化".to_string());
            app.render_rebuild();
        }
    }
}
```

注意：`push_system_note` 和 `render_rebuild` 需要确认是否是 App 的方法。如果不存在，使用现有的 `apply_pipeline_action(PipelineAction::AddMessage(...))` + `request_rebuild()`。

- [ ] **Step 4: 编译验证**

```bash
cargo build -p peri-tui 2>&1
```

Expected: 编译通过。

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat: add session/compact ACP request and rewrite /compact command

Manual compact now goes through ACP server instead of TUI-side direct
execution. This enables stdio/IDE clients to also trigger compact.

Co-Authored-By: deepseek-v4-pro <deepseek-ai@claude-code-best.win>"
```

---

### Task 7: 集成验证

- [ ] **Step 1: 全 workspace 编译**

```bash
cargo build --workspace 2>&1
```

Expected: 编译通过。

- [ ] **Step 2: 全量测试**

```bash
cargo test --workspace --lib 2>&1 | tail -30
```

Expected: 所有测试通过。失败的测试需要逐个检查是否引用了已删除的 compact 字段。

- [ ] **Step 3: Clippy**

```bash
cargo clippy --workspace --all-targets 2>&1 | grep "warning" | head -20
```

Expected: 无新增 warning。

- [ ] **Step 4: Pre-commit hooks**

```bash
lefthook run pre-commit 2>&1
```

Expected: 通过。

- [ ] **Step 5: Final commit**

```bash
git add -A
git commit -m "chore: final integration verification after compact-to-acp migration

Co-Authored-By: deepseek-v4-pro <deepseek-ai@claude-code.best.win>"
```
