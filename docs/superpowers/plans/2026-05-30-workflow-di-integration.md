# Workflow DI 集成：AcpAgentRunner + 命令挂载

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 将 `peri-workflow` crate 通过依赖注入集成到主项目。`peri-acp` 实现 `AgentRunner` trait（内部调用 `execute_prompt()`），`peri-tui` 挂载 CLI 子命令入口。

**Architecture:** 反向注入——`peri-workflow` 不知道 `peri-acp` 的存在。`peri-acp` 新增 `AcpAgentRunner`（impl `peri_workflow::AgentRunner`），封装 `execute_prompt()` 的全部参数。`peri-tui` 新增 `Commands::Workflow` 子命令（非交互模式，复用 `run_print` 的初始化模式）。

**依赖方向：**
```
peri-tui  →  peri-acp  →  peri-workflow  →  peri-agent
                ↓                  ↑
            实现 AgentRunner     定义 AgentRunner trait
```

**Tech Stack:** 沿用现有（tokio, async-trait, clap, tracing）

---

## File Structure

| 文件 | 变更 | 职责 |
|------|------|------|
| `peri-acp/Cargo.toml` | 修改 | 添加 `peri-workflow` 依赖 |
| `peri-acp/src/workflow.rs` | 创建 | `AcpAgentRunner` 实现 + `WorkflowService` 封装 |
| `peri-acp/src/lib.rs` | 修改 | 导出 `workflow` 模块 |
| `peri-acp/src/session/command/mod.rs` | 修改 | 为 `CommandContext` 添加 `agent_resources` 字段 |
| `peri-acp/src/session/executor.rs` | 修改 | 构造 `CommandContext` 时填充 `agent_resources` |
| `peri-acp/src/session/command/workflow_cmd.rs` | 创建 | `/workflow` Slash Command（`CommandKind::Immediate`） |
| `peri-tui/Cargo.toml` | 修改 | 添加 `peri-workflow` 依赖 |
| `peri-tui/src/main.rs` | 修改 | 新增 `Commands::Workflow` 子命令 |
| `peri-tui/src/cli_workflow.rs` | 创建 | `run_workflow()` 非交互模式执行逻辑 |

---

## Task 1: peri-acp 依赖 + AcpAgentRunner 实现

**Files:**
- Modify: `peri-acp/Cargo.toml`
- Create: `peri-acp/src/workflow.rs`
- Modify: `peri-acp/src/lib.rs`

- [ ] **Step 1: 添加依赖**

在 `peri-acp/Cargo.toml` 的 `[dependencies]` 中添加：
```toml
peri-workflow = { path = "../peri-workflow" }
```

- [ ] **Step 2: 创建 workflow.rs**

注意所有类型路径都已根据实际代码库验证：
- `LlmProvider` → `crate::provider::LlmProvider`（peri-acp 自身定义，非 `peri_agent::llm`）
- `ChannelState` → `peri_agent::interaction::ChannelState`
- `AgentPool` → `crate::session::agent_pool::AgentPool`
- `SessionManager` → `crate::session::SessionManager`
- `FrozenSessionData` → `crate::session::executor::FrozenSessionData`
- `ExecutorEvent` → `peri_agent::agent::events::AgentEvent`（event_sink.rs 中的别名）

```rust
//! Workflow 集成层 — 实现 `peri_workflow::AgentRunner` trait，
//! 桥接 workflow 执行器和 ACP agent 执行管线。

use std::sync::Arc;

use async_trait::async_trait;
use peri_agent::{
    agent::AgentCancellationToken,
    interaction::{ChannelState, UserInteractionBroker},
    messages::MessageContent,
};
use peri_workflow::{AgentOutput, AgentRunner};
use tracing::info;

use crate::{
    provider::PeriConfig,
    session::{
        agent_pool::AgentPool,
        event_sink::EventSink,
        executor::{execute_prompt, FrozenSessionData},
        SessionManager,
    },
};

/// ACP 层 AgentRunner 实现。
///
/// 封装 `execute_prompt()` 的全部参数，通过 DI 注入到
/// `peri_workflow::WorkflowExecutor`。每个 workflow 步骤的
/// agent 调用都会通过此结构体路由到真实的 Agent 执行管线。
pub struct AcpAgentRunner {
    pub provider: crate::provider::LlmProvider,
    pub peri_config: Arc<parking_lot::RwLock<PeriConfig>>,
    pub cwd: String,
    pub frozen: Option<FrozenSessionData>,
    pub permission_mode: Arc<peri_middlewares::prelude::SharedPermissionMode>,
    pub event_sink: Arc<dyn EventSink>,
    pub cancel: AgentCancellationToken,
    pub broker: Arc<dyn UserInteractionBroker>,
    pub plugin_skill_dirs: Vec<std::path::PathBuf>,
    pub plugin_agent_dirs: Vec<std::path::PathBuf>,
    pub hook_groups: Vec<Vec<peri_middlewares::hooks::RegisteredHook>>,
    pub cron_scheduler: Option<Arc<parking_lot::Mutex<peri_middlewares::cron::CronScheduler>>>,
    pub session_id: String,
    pub mcp_pool: Option<Arc<peri_middlewares::mcp::McpClientPool>>,
    pub channel_state: Option<Arc<ChannelState>>,
    pub tool_search_index: Arc<peri_middlewares::tool_search::ToolSearchIndex>,
    pub shared_tools: Arc<
        parking_lot::RwLock<std::collections::HashMap<String, Arc<dyn peri_agent::tools::BaseTool>>>,
    >,
    pub lsp_servers: Vec<peri_lsp::config::LspServerConfig>,
    pub langfuse_session: Option<Arc<crate::langfuse::LangfuseSession>>,
    pub pool: Arc<parking_lot::Mutex<AgentPool>>,
    pub thread_store: Option<Arc<dyn peri_agent::thread::ThreadStore>>,
    pub session_manager: Option<SessionManager>,
}

#[async_trait]
impl AgentRunner for AcpAgentRunner {
    async fn run_agent(
        &self,
        prompt: &str,
        label: &str,
        schema: Option<&serde_json::Value>,
        model: Option<&str>,
    ) -> peri_workflow::error::Result<AgentOutput> {
        info!(label = label, "Workflow agent 调用开始");

        let _ = schema; // 结构化输出暂不支持，保留接口

        // model 覆盖：通过 peri_config 解析别名
        let provider = match model {
            Some(m) => {
                let mut p = self.provider.clone();
                if let Some(resolved) = crate::provider::LlmProvider::from_config_for_alias(
                    &self.peri_config.read().config,
                    m,
                ) {
                    p = resolved;
                }
                p
            }
            None => self.provider.clone(),
        };

        let content = MessageContent::text(prompt.to_string());

        let result = execute_prompt(
            &provider,
            self.peri_config.clone(),
            &self.cwd,
            content,
            self.frozen.clone(),
            vec![],       // history — 空（每个步骤独立）
            vec![],       // incoming_recalls
            true,         // is_empty_history
            self.permission_mode.clone(),
            self.event_sink.clone(),
            self.cancel.clone(),
            self.broker.clone(),
            self.plugin_skill_dirs.clone(),
            self.plugin_agent_dirs.clone(),
            self.hook_groups.clone(),
            self.cron_scheduler.clone(),
            self.session_id.clone(),
            self.mcp_pool.clone(),
            self.channel_state.clone(),
            self.tool_search_index.clone(),
            self.shared_tools.clone(),
            self.lsp_servers.clone(),
            self.langfuse_session.clone(),
            self.pool.clone(),
            self.thread_store.clone(),
            None,   // thread_id
            self.session_manager.clone(),
            vec![], // bg_results
        )
        .await;

        let data = extract_agent_output(&result.messages);
        let tokens_used = None; // PromptResult 暂不暴露 token 用量

        info!(label = label, ok = result.ok, "Workflow agent 调用完成");

        Ok(AgentOutput {
            data,
            tokens_used,
        })
    }
}

/// 从 PromptResult.messages 中提取最后一条 AI 消息文本并尝试解析为 JSON
fn extract_agent_output(messages: &[peri_agent::messages::BaseMessage]) -> serde_json::Value {
    use peri_agent::messages::ContentBlock;

    let last_ai_text = messages.iter().rev().find_map(|msg| {
        if msg.is_ai() {
            msg.content.iter().find_map(|block| {
                if let ContentBlock::Text { text } = block {
                    Some(text.clone())
                } else {
                    None
                }
            })
        } else {
            None
        }
    });

    match last_ai_text {
        Some(text) => serde_json::from_str::<serde_json::Value>(&text)
            .unwrap_or(serde_json::Value::String(text)),
        None => serde_json::Value::Null,
    }
}
```

**关键注意事项：**

1. `execute_prompt` 有 28 个参数。`AcpAgentRunner` 持有全部这些参数的 Arc/Clone——这是资源容器模式。
2. `provider` 是 `crate::provider::LlmProvider`（peri-acp 自身定义），不是 `peri_agent::llm::LlmProvider`。
3. `channel_state` 类型是 `peri_agent::interaction::ChannelState`。
4. `pool` 类型是 `crate::session::agent_pool::AgentPool`。
5. `session_manager` 类型是 `crate::session::SessionManager`。
6. `extract_agent_output` 从 messages 中提取最后 AI 文本，尝试 JSON 解析，失败则包装为 String。
7. `tokens_used` 暂为 `None`——`PromptResult` 不暴露 token 用量，后续迭代补全。
8. `bg_results` 始终传空 vec——workflow 步骤不涉及后台任务结果。
9. `PeriConfig` 使用 `Arc<parking_lot::RwLock<PeriConfig>>` 包装（与 TUI 的 `SharedPeriConfig` 模式一致）。注意 `from_config_for_alias` 接收 `&PeriConfig` 而非 `&self.peri_config`（`PeriConfig` 的顶层是 `{ config: AppConfig }`），因此需要 `self.peri_config.read().config` 访问 `AppConfig`。但实际 `from_config_for_alias` 签名需要确认——如果它接收 `&PeriConfig` 则用 `&self.peri_config.read()`，如果接收 `&AppConfig` 则用 `&self.peri_config.read().config`。实现时以编译通过为准。

- [ ] **Step 3: 导出模块**

在 `peri-acp/src/lib.rs` 的模块声明区域添加：
```rust
pub mod workflow;
```

- [ ] **Step 4: 构建验证**

Run: `cargo build -p peri-acp`
Expected: 编译成功

- [ ] **Step 5: Commit**

```bash
git add peri-acp/Cargo.toml peri-acp/src/workflow.rs peri-acp/src/lib.rs
git commit -m "feat(acp): 实现 AcpAgentRunner 桥接 workflow 和 agent 执行管线

Co-Authored-By: glm-5.1 <zai-org@claude-code-best.win>"
```

---

## Task 2: `/workflow` ACP Slash Command + CommandContext 扩展

**Files:**
- Modify: `peri-acp/src/session/command/mod.rs`
- Modify: `peri-acp/src/session/executor.rs`
- Create: `peri-acp/src/session/command/workflow_cmd.rs`

- [ ] **Step 1: 扩展 CommandContext**

在 `peri-acp/src/session/command/mod.rs` 中，为 `CommandContext` 添加 `agent_resources` 字段。

在文件头部 imports 中添加 `peri_agent::interaction::ChannelState` 和其他需要的类型。

在 `CommandContext` 结构体定义之后，添加 `AcpAgentResources` 结构体和 `agent_resources` 字段：

```rust
use peri_agent::interaction::ChannelState;
use crate::session::agent_pool::AgentPool;
use crate::langfuse::LangfuseSession;
use crate::session::SessionManager;

/// ACP Agent 执行资源，用于需要构建 Agent 的命令（如 workflow）
pub struct AcpAgentResources {
    pub provider: crate::provider::LlmProvider,
    pub peri_config: Arc<crate::provider::PeriConfig>,
    pub permission_mode: Arc<peri_middlewares::prelude::SharedPermissionMode>,
    pub broker: Arc<dyn peri_agent::interaction::UserInteractionBroker>,
    pub plugin_skill_dirs: Vec<std::path::PathBuf>,
    pub plugin_agent_dirs: Vec<std::path::PathBuf>,
    pub hook_groups: Vec<Vec<peri_middlewares::hooks::RegisteredHook>>,
    pub cron_scheduler: Option<Arc<parking_lot::Mutex<peri_middlewares::cron::CronScheduler>>>,
    pub mcp_pool: Option<Arc<peri_middlewares::mcp::McpClientPool>>,
    pub channel_state: Option<Arc<ChannelState>>,
    pub tool_search_index: Arc<peri_middlewares::tool_search::ToolSearchIndex>,
    pub shared_tools: Arc<
        parking_lot::RwLock<
            std::collections::HashMap<String, Arc<dyn peri_agent::tools::BaseTool>>,
        >,
    >,
    pub lsp_servers: Vec<peri_lsp::config::LspServerConfig>,
    pub langfuse_session: Option<Arc<LangfuseSession>>,
    pub pool: Arc<parking_lot::Mutex<AgentPool>>,
    pub thread_store: Option<Arc<dyn peri_agent::thread::ThreadStore>>,
    pub session_manager: Option<SessionManager>,
}
```

然后在 `CommandContext` 中添加字段：

```rust
pub struct CommandContext {
    pub session_id: String,
    pub history: Vec<BaseMessage>,
    pub cwd: String,
    pub peri_config: Arc<PeriConfig>,
    /// 用于 compact 等需要 LLM 调用的命令。由 executor 从 provider 构造后传入。
    pub compact_model: Option<Arc<dyn BaseModel>>,
    pub event_sink: Arc<dyn EventSink>,
    /// 命令参数（命令名之后的文本）。
    pub args: String,
    /// Agent 执行资源（仅 workflow 等需要构建 Agent 的 Immediate 命令使用）
    pub agent_resources: Option<AcpAgentResources>,
}
```

- [ ] **Step 2: 在 executor 中填充 agent_resources**

在 `peri-acp/src/session/executor.rs` 中，找到构造 `CommandContext` 的代码块（约第 179 行），添加 `agent_resources` 字段。

当前代码：
```rust
let ctx = crate::session::command::CommandContext {
    session_id: session_id.clone(),
    history: history.clone(),
    cwd: cwd.to_string(),
    peri_config: Arc::new(peri_config.as_ref().clone()),
    compact_model: compact_model.clone(),
    event_sink: event_sink.clone(),
    args: args.to_string(),
};
```

修改为：
```rust
let ctx = crate::session::command::CommandContext {
    session_id: session_id.clone(),
    history: history.clone(),
    cwd: cwd.to_string(),
    peri_config: Arc::new(peri_config.as_ref().clone()),
    compact_model: compact_model.clone(),
    event_sink: event_sink.clone(),
    args: args.to_string(),
    agent_resources: Some(crate::session::command::AcpAgentResources {
        provider: provider.clone(),
        peri_config: Arc::new(peri_config.as_ref().clone()),
        permission_mode: permission_mode.clone(),
        broker: broker.clone(),
        plugin_skill_dirs: plugin_skill_dirs.clone(),
        plugin_agent_dirs: plugin_agent_dirs.clone(),
        hook_groups: hook_groups.clone(),
        cron_scheduler: cron_scheduler.clone(),
        mcp_pool: mcp_pool.clone(),
        channel_state: channel_state.clone(),
        tool_search_index: tool_search_index.clone(),
        shared_tools: shared_tools.clone(),
        lsp_servers: lsp_servers.clone(),
        langfuse_session: langfuse_session.clone(),
        pool: pool.clone(),
        thread_store: thread_store.clone(),
        session_manager: session_manager.clone(),
    }),
};
```

- [ ] **Step 3: 创建 workflow_cmd.rs**

```rust
//! `/workflow` 命令 — 在 TUI 中触发 workflow 执行。

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use tracing::info;

use super::{AgentCommand, AcpAgentResources, CommandContext, CommandKind, CommandResult};
use crate::session::executor::PromptStopReason;
use crate::workflow::AcpAgentRunner;
use peri_agent::agent::events::AgentEvent as ExecutorEvent;
use peri_workflow::{WorkflowEvent, WorkflowExecutor, WorkflowParser};

/// Workflow 命令。
///
/// 用法：`/workflow <workflow目录路径> [--param key=value ...]`
pub struct WorkflowCommand;

impl WorkflowCommand {
    pub const NAME: &'static str = "workflow";
}

#[async_trait]
impl AgentCommand for WorkflowCommand {
    fn name(&self) -> &str {
        Self::NAME
    }

    fn aliases(&self) -> Vec<&str> {
        vec!["wf"]
    }

    fn description(&self) -> &str {
        "执行 YAML workflow"
    }

    fn kind(&self) -> CommandKind {
        CommandKind::Immediate
    }

    async fn execute(&self, ctx: CommandContext) -> CommandResult {
        let args = ctx.args.trim();
        if args.is_empty() {
            push_text(&ctx, "用法: /workflow <目录路径> [--param key=value]").await;
            return CommandResult {
                messages: ctx.history,
                stop_reason: PromptStopReason::Completed,
            };
        }

        // 获取 agent 资源
        let resources = match ctx.agent_resources {
            Some(ref r) => r,
            None => {
                push_text(&ctx, "Workflow 需要 agent 资源，但当前不可用").await;
                return CommandResult {
                    messages: ctx.history,
                    stop_reason: PromptStopReason::Completed,
                };
            }
        };

        // 解析参数：第一个是目录路径，后续是 --param key=value
        let (dir_path, params) = parse_args(args);

        let workflow_dir = Path::new(&dir_path);
        if !workflow_dir.exists() {
            push_text(&ctx, &format!("Workflow 目录不存在: {}", dir_path)).await;
            return CommandResult {
                messages: ctx.history,
                stop_reason: PromptStopReason::Completed,
            };
        }

        // 解析 workflow
        let parsed = match WorkflowParser::parse_from_dir(workflow_dir) {
            Ok(p) => p,
            Err(e) => {
                push_text(&ctx, &format!("Workflow 解析失败: {}", e)).await;
                return CommandResult {
                    messages: ctx.history,
                    stop_reason: PromptStopReason::Completed,
                };
            }
        };

        info!(name = %parsed.def.name, "开始执行 workflow");
        push_text(&ctx, &format!("🚀 Workflow: {}", parsed.def.name)).await;

        // 构造 AcpAgentRunner（使用当前 session 的资源）
        let runner = Arc::new(AcpAgentRunner {
            provider: resources.provider.clone(),
            peri_config: {
                // peri_config 在 CommandContext 中是 Arc<PeriConfig>，
                // AcpAgentRunner 需要 Arc<RwLock<PeriConfig>>
                // 这里直接用 Arc::new 包装（workflow 步骤不跨轮次共享 config 变更）
                Arc::new(parking_lot::RwLock::new(
                    ctx.peri_config.as_ref().clone(),
                ))
            },
            cwd: ctx.cwd.clone(),
            frozen: None, // workflow 步骤不需要 frozen data
            permission_mode: resources.permission_mode.clone(),
            event_sink: ctx.event_sink.clone(),
            cancel: peri_agent::agent::AgentCancellationToken::new(),
            broker: resources.broker.clone(),
            plugin_skill_dirs: resources.plugin_skill_dirs.clone(),
            plugin_agent_dirs: resources.plugin_agent_dirs.clone(),
            hook_groups: resources.hook_groups.clone(),
            cron_scheduler: resources.cron_scheduler.clone(),
            session_id: ctx.session_id.clone(),
            mcp_pool: resources.mcp_pool.clone(),
            channel_state: resources.channel_state.clone(),
            tool_search_index: resources.tool_search_index.clone(),
            shared_tools: resources.shared_tools.clone(),
            lsp_servers: resources.lsp_servers.clone(),
            langfuse_session: resources.langfuse_session.clone(),
            pool: resources.pool.clone(),
            thread_store: resources.thread_store.clone(),
            session_manager: resources.session_manager.clone(),
        });

        // 创建 workflow 事件通道
        let (event_tx, mut event_rx) = tokio::sync::mpsc::channel(128);

        // 转发 workflow 事件到 ACP event_sink
        let sink = ctx.event_sink.clone();
        let sid = ctx.session_id.clone();
        let forwarder = tokio::spawn(async move {
            while let Some(event) = event_rx.recv().await {
                if let Some(exe_event) = map_workflow_event(&event) {
                    sink.push_event(&sid, &exe_event, 0).await;
                }
                if matches!(event, WorkflowEvent::WorkflowCompleted { .. }) {
                    break;
                }
            }
        });

        // 执行 workflow
        let executor = WorkflowExecutor::new(parsed, runner, event_tx)
            .expect("创建执行器失败");
        let result = executor.execute(params).await;

        // 等待事件转发完成
        let _ = forwarder.await;

        match result {
            Ok(wf_result) => {
                let summary = serde_json::to_string_pretty(&wf_result.returns)
                    .unwrap_or_default();
                push_text(&ctx, &format!("✅ Workflow 完成\n{}", summary)).await;
            }
            Err(e) => {
                push_text(&ctx, &format!("❌ Workflow 执行失败: {}", e)).await;
            }
        }

        CommandResult {
            messages: ctx.history,
            stop_reason: PromptStopReason::Completed,
        }
    }
}

/// 将 workflow 事件文本推送到 ACP event_sink
async fn push_text(ctx: &CommandContext, msg: &str) {
    let event = ExecutorEvent::TextChunk {
        message_id: uuid::Uuid::new_v4().to_string(),
        chunk: format!("{}\n", msg),
        source_agent_id: None,
    };
    ctx.event_sink.push_event(&ctx.session_id, &event, 0).await;
}

/// WorkflowEvent → ExecutorEvent 映射（仅转发用户可见事件）
fn map_workflow_event(event: &WorkflowEvent) -> Option<ExecutorEvent> {
    match event {
        WorkflowEvent::PhaseStarted { title } => Some(ExecutorEvent::TextChunk {
            message_id: uuid::Uuid::new_v4().to_string(),
            chunk: format!("\n━━━ {} ━━━\n", title),
            source_agent_id: None,
        }),
        WorkflowEvent::AgentStarted { label, .. } => Some(ExecutorEvent::TextChunk {
            message_id: uuid::Uuid::new_v4().to_string(),
            chunk: format!("⏳ Agent: {}\n", label),
            source_agent_id: None,
        }),
        WorkflowEvent::AgentCompleted { label, duration_ms } => {
            Some(ExecutorEvent::TextChunk {
                message_id: uuid::Uuid::new_v4().to_string(),
                chunk: format!(
                    "✔️ {} ({}ms)\n",
                    label,
                    duration_ms.unwrap_or(0)
                ),
                source_agent_id: None,
            })
        }
        WorkflowEvent::Log { message } => Some(ExecutorEvent::TextChunk {
            message_id: uuid::Uuid::new_v4().to_string(),
            chunk: format!("📋 {}\n", message),
            source_agent_id: None,
        }),
        WorkflowEvent::WorkflowCompleted { .. } => None, // 由 execute 返回后统一输出
        _ => None, // 其他事件不转发
    }
}

fn parse_args(args: &str) -> (String, HashMap<String, serde_json::Value>) {
    let mut parts = args.split_whitespace();
    let dir_path = parts.next().unwrap_or(".").to_string();
    let mut params = HashMap::new();

    while let Some(part) = parts.next() {
        if part == "--param" {
            if let Some(kv) = parts.next() {
                if let Some((k, v)) = kv.split_once('=') {
                    params.insert(
                        k.to_string(),
                        serde_json::from_str(v)
                            .unwrap_or(serde_json::Value::String(v.to_string())),
                    );
                }
            }
        }
    }

    (dir_path, params)
}
```

**关键注意事项：**

1. `push_text` 是 async 函数，直接调用 `event_sink.push_event().await`——在 `execute` 的 async 上下文中可以正常工作。

2. `peri_config` 类型转换：`CommandContext` 中 `peri_config` 是 `Arc<PeriConfig>`，但 `AcpAgentRunner` 需要 `Arc<RwLock<PeriConfig>>`。这里用 `Arc::new(RwLock::new(...))` 包装。这与 TUI 的 `SharedPeriConfig` 模式一致。如果 `AcpAgentRunner.peri_config` 的类型与实际不符，实现时调整。

3. `map_workflow_event` 只转发用户可见事件（Phase/Agent/Log），其他事件静默丢弃。

4. 事件转发使用 `tokio::spawn` 异步任务——workflow 执行和事件转发并行。

- [ ] **Step 4: 注册命令**

在 `peri-acp/src/session/command/mod.rs` 中：

添加模块声明：
```rust
pub mod workflow_cmd;
```

在 `default_command_registry()` 中注册：
```rust
pub fn default_command_registry() -> CommandRegistry {
    let mut reg = CommandRegistry::new();
    reg.register(Box::new(compact::CompactCommand));
    reg.register(Box::new(clear::ClearCommand));
    reg.register(Box::new(workflow_cmd::WorkflowCommand));
    reg
}
```

- [ ] **Step 5: 构建验证**

Run: `cargo build -p peri-acp`
Expected: 编译成功

- [ ] **Step 6: Commit**

```bash
git add peri-acp/src/
git commit -m "feat(acp): 添加 /workflow ACP Slash Command 和 CommandContext 资源扩展

Co-Authored-By: glm-5.1 <zai-org@claude-code-best.win>"
```

---

## Task 3: CLI 子命令 `peri workflow`

**Files:**
- Modify: `peri-tui/Cargo.toml`
- Modify: `peri-tui/src/main.rs`
- Create: `peri-tui/src/cli_workflow.rs`

- [ ] **Step 1: 添加依赖**

在 `peri-tui/Cargo.toml` 的 `[dependencies]` 中添加：
```toml
peri-workflow = { path = "../peri-workflow" }
```

- [ ] **Step 2: 创建 cli_workflow.rs**

复用 `cli_print.rs` 的初始化模式（`PrintBroker`、provider 初始化、`execute_prompt` 调用）：

```rust
//! `peri workflow` 子命令 — 非交互模式执行 YAML workflow

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use peri_agent::interaction::UserInteractionBroker;
use tracing::info;

/// `peri workflow` 执行入口
pub async fn run_workflow(
    dir: String,
    params: Vec<String>,
    model_override: Option<String>,
    permission_mode_str: Option<String>,
    skip_permissions: bool,
    settings_path: Option<String>,
) -> Result<()> {
    let _telemetry = peri_agent::telemetry::init_tracing("peri-workflow");

    // 加载配置（复用 run_print 的模式）
    let peri_config = match &settings_path {
        Some(path) => {
            let p = std::path::Path::new(path);
            if p.exists() {
                crate::config::load_from(p)?
            } else {
                let v: serde_json::Value = serde_json::from_str(path)
                    .map_err(|e| anyhow::anyhow!("--settings 不是有效文件路径或 JSON: {e}"))?;
                let tmp = std::env::temp_dir().join("peri-settings-override.json");
                std::fs::write(&tmp, serde_json::to_string_pretty(&v)?)?;
                crate::config::load_from(&tmp)?
            }
        }
        None => crate::config::load().unwrap_or_default(),
    };

    // 构建 provider（与 run_print 一致）
    let provider = crate::app::agent::LlmProvider::from_config(&peri_config)
        .or_else(crate::app::agent::LlmProvider::from_env)
        .ok_or_else(|| {
            anyhow::anyhow!("未配置 LLM provider。请设置 ANTHROPIC_API_KEY 或 OPENAI_API_KEY")
        })?;

    let provider = if let Some(ref model_str) = model_override {
        crate::app::agent::LlmProvider::from_config_for_alias(&peri_config, model_str)
            .unwrap_or(provider)
    } else {
        provider
    };

    // 解析 workflow
    let workflow_dir = std::path::Path::new(&dir);
    let parsed = peri_workflow::WorkflowParser::parse_from_dir(workflow_dir)
        .map_err(|e| anyhow::anyhow!("Workflow 解析失败: {}", e))?;

    println!("Workflow: {}", parsed.def.name);
    println!("描述: {}", parsed.def.description);

    // 解析参数
    let mut args = std::collections::HashMap::new();
    for kv in &params {
        if let Some((k, v)) = kv.split_once('=') {
            args.insert(
                k.to_string(),
                serde_json::from_str(v).unwrap_or(serde_json::Value::String(v.to_string())),
            );
        }
    }

    let cwd = std::env::current_dir()?
        .to_string_lossy()
        .to_string();

    // 权限模式
    let permission_mode = if skip_permissions {
        peri_middlewares::prelude::PermissionMode::Bypass
    } else if let Some(ref mode_str) = permission_mode_str {
        match mode_str.as_str() {
            "bypass" => peri_middlewares::prelude::PermissionMode::Bypass,
            "default" => peri_middlewares::prelude::PermissionMode::Default,
            "dont-ask" => peri_middlewares::prelude::PermissionMode::DontAsk,
            "accept-edit" => peri_middlewares::prelude::PermissionMode::AcceptEdit,
            "auto-mode" => peri_middlewares::prelude::PermissionMode::AutoMode,
            _ => peri_middlewares::prelude::PermissionMode::Bypass,
        }
    } else {
        peri_middlewares::prelude::PermissionMode::Bypass // 非交互默认 bypass
    };
    let shared_permission = peri_middlewares::prelude::SharedPermissionMode::new(permission_mode);

    // cron scheduler
    let cron_scheduler = {
        let scheduler =
            peri_middlewares::cron::CronScheduler::new(tokio::sync::mpsc::unbounded_channel().0);
        Arc::new(parking_lot::Mutex::new(scheduler))
    };

    // 基础资源（bare 模式，不初始化 MCP/插件/LSP）
    let tool_search_index = Arc::new(peri_middlewares::tool_search::ToolSearchIndex::new());
    let shared_tools = Arc::new(parking_lot::RwLock::new(std::collections::HashMap::new()));
    let pool = Arc::new(parking_lot::Mutex::new(
        peri_acp::session::agent_pool::AgentPool::new(),
    ));
    let broker: Arc<dyn UserInteractionBroker> = Arc::new(WorkflowBroker);

    // EventSink（丢弃 agent 事件，workflow 事件通过 mpsc channel 处理）
    let event_sink: Arc<dyn peri_acp::session::event_sink::EventSink> =
        Arc::new(WorkflowEventSink);

    let peri_config_arc = Arc::new(parking_lot::RwLock::new(peri_config));

    // 构造 AcpAgentRunner
    let runner = Arc::new(peri_acp::workflow::AcpAgentRunner {
        provider,
        peri_config: peri_config_arc,
        cwd,
        frozen: None,
        permission_mode: shared_permission,
        event_sink,
        cancel: peri_agent::agent::AgentCancellationToken::new(),
        broker,
        plugin_skill_dirs: vec![],
        plugin_agent_dirs: vec![],
        hook_groups: vec![],
        cron_scheduler: Some(cron_scheduler),
        session_id: String::new(),
        mcp_pool: None,
        channel_state: None,
        tool_search_index,
        shared_tools,
        lsp_servers: vec![],
        langfuse_session: None,
        pool,
        thread_store: None,
        session_manager: None,
    });

    // workflow 事件通道 + 打印
    let (event_tx, mut event_rx) = tokio::sync::mpsc::channel(128);
    let printer = tokio::spawn(async move {
        while let Some(event) = event_rx.recv().await {
            match event {
                peri_workflow::WorkflowEvent::WorkflowStarted { name } => {
                    println!("🚀 {}", name);
                }
                peri_workflow::WorkflowEvent::PhaseStarted { title } => {
                    println!("\n━━━ {} ━━━", title);
                }
                peri_workflow::WorkflowEvent::AgentStarted { label, .. } => {
                    println!("  ⏳ {}", label);
                }
                peri_workflow::WorkflowEvent::AgentCompleted {
                    label,
                    duration_ms,
                    ..
                } => {
                    println!("  ✔️  {} ({}ms)", label, duration_ms.unwrap_or(0));
                }
                peri_workflow::WorkflowEvent::Log { message } => {
                    println!("  📋 {}", message);
                }
                peri_workflow::WorkflowEvent::WorkflowCompleted { .. } => break,
                _ => {}
            }
        }
    });

    // 执行
    let executor = peri_workflow::WorkflowExecutor::new(parsed, runner, event_tx)
        .map_err(|e| anyhow::anyhow!("创建执行器失败: {}", e))?;
    let result = executor.execute(args).await
        .map_err(|e| anyhow::anyhow!("执行失败: {}", e))?;

    let _ = printer.await;

    // 输出结果
    println!("\n📦 返回值:");
    println!("{}", serde_json::to_string_pretty(&result.returns)?);

    Ok(())
}

/// 自动批准 broker（与 cli_print::PrintBroker 相同模式）
struct WorkflowBroker;

#[async_trait]
impl UserInteractionBroker for WorkflowBroker {
    async fn request(
        &self,
        context: peri_agent::interaction::InteractionContext,
    ) -> peri_agent::interaction::InteractionResponse {
        match context {
            peri_agent::interaction::InteractionContext::Approval { items } => {
                peri_agent::interaction::InteractionResponse::Decisions(
                    items
                        .into_iter()
                        .map(|_| peri_agent::interaction::ApprovalDecision::Approve {
                            source: None,
                        })
                        .collect(),
                )
            }
            peri_agent::interaction::InteractionContext::Questions { requests } => {
                peri_agent::interaction::InteractionResponse::Answers(
                    requests
                        .into_iter()
                        .map(|q| peri_agent::interaction::QuestionAnswer {
                            id: q.id,
                            selected: vec![],
                            text: Some(String::new()),
                        })
                        .collect(),
                )
            }
        }
    }
}

/// 丢弃 agent 事件的 EventSink（workflow 事件通过 mpsc channel 处理）
struct WorkflowEventSink;

#[async_trait]
impl peri_acp::session::event_sink::EventSink for WorkflowEventSink {
    async fn push_event(
        &self,
        _session_id: &str,
        _event: &peri_agent::agent::events::AgentEvent,
        _context_window: u32,
    ) {
        // workflow CLI 模式不关心 agent 的流式事件
    }

    async fn push_done(&self, _session_id: &str) {}
}
```

**关键注意事项：**

1. 完整复用 `cli_print.rs` 的初始化模式：`LlmProvider::from_config` + `from_env`、`PrintBroker`（此处为 `WorkflowBroker`）、bare 模式资源初始化。
2. `WorkflowBroker` 和 `WorkflowEventSink` 与 `cli_print.rs` 中的 `PrintBroker`/`PrintEventSink` 模式一致。
3. bare 模式初始化：不启动 MCP/插件/LSP。如需完整资源，可后续添加 `--no-bare` 选项。
4. `peri_config_arc` 使用 `Arc<RwLock<PeriConfig>>`，与 `AcpAgentRunner` 字段类型匹配。
5. `crate::app::agent::LlmProvider` → `peri_acp::provider::LlmProvider`（re-export chain）。

- [ ] **Step 3: 添加 CLI 子命令**

在 `peri-tui/src/main.rs` 的 `Commands` 枚举中添加：

```rust
#[derive(Subcommand)]
enum Commands {
    // ... 现有命令 (Acp, Update, Sync, Plugin) ...

    /// 执行 YAML workflow
    Workflow {
        /// Workflow 目录路径（包含 workflow.yaml）
        dir: String,

        /// 参数（格式：key=value，可多次使用）
        #[arg(long = "param", value_name = "KEY=VALUE")]
        params: Vec<String>,

        /// 模型覆盖
        #[arg(short, long)]
        model: Option<String>,

        /// 权限模式
        #[arg(long)]
        permission_mode: Option<String>,

        /// 跳过权限检查
        #[arg(long)]
        dangerously_skip_permissions: bool,

        /// 加载额外 settings
        #[arg(long)]
        settings: Option<String>,
    },
}
```

在 `match cli.command` 分支中添加：

```rust
Some(Commands::Workflow {
    dir,
    params,
    model,
    permission_mode,
    dangerously_skip_permissions,
    settings,
}) => {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .thread_stack_size(4 * 1024 * 1024)
        .enable_all()
        .build()?;
    rt.block_on(cli_workflow::run_workflow(
        dir,
        params,
        model,
        permission_mode,
        dangerously_skip_permissions,
        settings,
    ))
}
```

在文件头部添加模块声明（在已有的 `mod cli_print;` / `mod cli_plugin;` 旁）：
```rust
mod cli_workflow;
```

- [ ] **Step 4: 构建验证**

Run: `cargo build -p peri-tui`
Expected: 编译成功

- [ ] **Step 5: Commit**

```bash
git add peri-tui/Cargo.toml peri-tui/src/main.rs peri-tui/src/cli_workflow.rs
git commit -m "feat(tui): 添加 CLI 子命令 \`peri workflow\`

Co-Authored-By: glm-5.1 <zai-org@claude-code-best.win>"
```

---

## Task 4: 端到端验证

**Files:**
- 无新增

- [ ] **Step 1: CLI 模式验证**

```bash
cargo run -p peri-tui -- workflow peri/workflows/review-code --param range=HEAD~3
```

Expected: 打印 workflow 执行流程，Agent 实际调用 LLM，输出审查结果。
**注意**：需要有效的 API Key（`ANTHROPIC_API_KEY` 或 `OPENAI_API_KEY`）。

- [ ] **Step 2: TUI Slash Command 验证**

在 TUI 中输入 `/workflow peri/workflows/review-code`，验证：
- Workflow 事件正确显示在 TUI 消息区域
- Agent 执行事件正常流转
- 执行完成后显示结果

- [ ] **Step 3: 全 workspace 构建 + clippy**

Run: `cargo build && cargo clippy --workspace -- -D warnings 2>&1 | head -50`
Expected: 0 error，peri-workflow 和 peri-acp 相关 0 warning

- [ ] **Step 4: 最终 Commit（如有修复）**

```bash
git add -A
git commit -m "fix: 集成验证修复

Co-Authored-By: glm-5.1 <zai-org@claude-code-best.win>"
```

---

## 自审检查清单

### 1. 类型路径验证

| 计划中使用的类型 | 实际位置 | 验证 |
|------------------|----------|------|
| `crate::provider::LlmProvider` | `peri-acp/src/provider/mod.rs:14` | ✓ |
| `peri_agent::interaction::ChannelState` | `peri-agent/src/interaction/channel_state.rs` | ✓ |
| `crate::session::agent_pool::AgentPool` | `peri-acp/src/session/agent_pool.rs` | ✓ |
| `crate::session::SessionManager` | `peri-acp/src/session/` | ✓ |
| `peri_agent::agent::events::AgentEvent` as `ExecutorEvent` | event_sink.rs:8 的别名 | ✓ |
| `peri_agent::interaction::UserInteractionBroker` | peri-agent interaction trait | ✓ |

### 2. 依赖方向

| 依赖 | 方向 | 正确？ |
|------|------|--------|
| peri-workflow → peri-agent | ✅ 仅类型依赖 | ✓ |
| peri-acp → peri-workflow | ✅ 实现 trait + 调用 executor | ✓ |
| peri-tui → peri-acp | ✅ 已有依赖 | ✓ |
| peri-tui → peri-workflow | ✅ CLI 子命令直接调用 | ✓ |
| peri-workflow → peri-acp | ❌ **禁止** | ✓ 未违反 |

### 3. 占位符扫描

- 无 `..todo!()`、无 `TBD`、无 `TODO`（实现时待定）——所有步骤包含完整代码 ✓
- `tokens_used` 返回 `None` 有明确注释说明原因 ✓
- `schema` 参数 `let _ = schema` 有明确注释 ✓

### 4. CommandContext 扩展影响

- `agent_resources: Option<AcpAgentResources>` — 可选字段，仅 workflow 命令使用
- 现有命令（compact/clear）不需要此字段，executor 始终填充 `Some(...)`
- compact/clear 的 `execute` 实现不访问 `agent_resources`，无影响

### 5. PeriConfig 类型适配

- `CommandContext.peri_config`: `Arc<PeriConfig>`（ peri-acp 的 `PeriConfig`）
- `AcpAgentRunner.peri_config`: `Arc<RwLock<PeriConfig>>`（需要 RwLock 包装）
- Task 2 Step 3 使用 `Arc::new(RwLock::new(ctx.peri_config.as_ref().clone()))` 转换
- Task 3 使用 `Arc::new(RwLock::new(peri_config))` 包装
- 如果 `AcpAgentRunner.peri_config` 直接用 `Arc<PeriConfig>` 更简单（workflow 步骤不需要跨步骤 config 变更），实现时可考虑简化
