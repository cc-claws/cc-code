# Feature: 20260503_F003 - background-agent

## 需求背景

当前 SubAgentTool 的所有子 agent 调用都是同步的——主 agent 在 `invoke()` 中 await 子 agent 完成，拿到结果后才继续 ReAct 循环。对于耗时较长的子任务（如代码审查、批量重构、深度调研），主 agent 整段时间被阻塞，无法响应其他任务或用户交互。

需要支持**工具级后台运行**：LLM 通过 Agent 工具的 `run_in_background` 参数启动后台 agent，主 agent 立即继续执行，后台 agent 完成后通过事件推送通道主动通知主 agent。

参考实现：Claude Code 的 `LocalAgentTask` + `run_in_background` 参数 + `BackgroundTasksDialog` UI。

## 目标

- Agent 工具增加 `run_in_background` 参数，为 `true` 时主 agent 不等待子 agent 完成
- 最多 3 个并发后台 agent，超出返回错误提示 LLM 等待
- 后台 agent 完成后通过事件推送通道通知主 agent，主 agent 在当前工具调用完成后有序消费通知
- 共享 cwd，无文件系统隔离（MVP 阶段不做 worktree）
- TUI 显示后台任务状态和完成通知

## 方案设计

### 1. 整体架构

```
┌──────────────────────────────────────────────────────────┐
│                      ReAct 循环                           │
│  ┌──────────┐   ┌──────────┐   ┌──────────────────────┐  │
│  │ LLM 调用  │──→│ 工具执行  │──→│ 消费后台通知 ←───────────── notification_rx (mpsc)
│  └──────────┘   └──────────┘   └──────────────────────┘  │
│       ↑                              │                    │
│       └──────── 注入 Human 消息 ──────┘                    │
└──────────────────────────────────────────────────────────┘

后台 Agent Task:
  tokio::spawn → 运行子 agent → 完成 → notification_tx.send(result)
                                       → BackgroundTaskRegistry 标记完成
                                       → emit(AgentEvent::BackgroundTaskCompleted)
```

### 2. 核心数据结构

#### 2.1 后台任务注册中心（`rust-agent-middlewares`）

```rust
// 新文件: rust-agent-middlewares/src/subagent/background.rs

/// 后台任务结果
#[derive(Debug, Clone)]
pub struct BackgroundTaskResult {
    pub task_id: String,
    pub agent_name: String,
    pub prompt_summary: String,    // prompt 前 100 字符，用于 UI 显示
    pub success: bool,
    pub output: String,
    pub tool_calls_count: usize,
    pub duration_ms: u64,
}

/// 后台任务状态
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BackgroundTaskStatus {
    Running,
    Completed,
    Failed,
}

/// 后台任务信息
pub struct BackgroundTask {
    pub id: String,
    pub agent_name: String,
    pub prompt_summary: String,
    pub status: BackgroundTaskStatus,
    pub started_at: std::time::Instant,
    pub abort_handle: tokio::task::JoinHandle<()>,
}

/// 后台任务注册中心
pub struct BackgroundTaskRegistry {
    tasks: parking_lot::Mutex<HashMap<String, BackgroundTask>>,
    notification_tx: tokio::sync::mpsc::UnboundedSender<BackgroundTaskResult>,
    /// 并发上限
    max_concurrent: usize,
}

impl BackgroundTaskRegistry {
    pub fn new(notification_tx: tokio::sync::mpsc::UnboundedSender<BackgroundTaskResult>) -> Self {
        Self {
            tasks: parking_lot::Mutex::new(HashMap::new()),
            notification_tx,
            max_concurrent: 3,
        }
    }

    /// 当前运行中的任务数
    pub fn active_count(&self) -> usize { ... }

    /// 注册新任务，超出上限返回 Err
    pub fn register(&self, task: BackgroundTask) -> Result<(), String> { ... }

    /// 任务完成时调用：更新状态 + 推送通知
    pub fn complete(&self, task_id: &str, result: BackgroundTaskResult) { ... }

    /// 获取所有任务状态（UI 使用）
    pub fn list_tasks(&self) -> Vec<(String, BackgroundTaskStatus, String)> { ... }

    /// 取消指定任务
    pub fn cancel(&self, task_id: &str) -> Result<(), String> { ... }
}
```

#### 2.2 通知通道

使用 `tokio::sync::mpsc::unbounded_channel`，原因：

- 后台任务可能在任何时刻完成，UnboundedSender 保证 `send` 不阻塞
- ReAct 循环在每轮迭代末尾通过 `try_recv` 消费，不会无限堆积
- 通道创建在 `SubAgentMiddleware` 中，tx 传给 `BackgroundTaskRegistry`，rx 传给 `ReActAgent`

### 3. ReActAgent 变更（`rust-create-agent`）

#### 3.1 新增字段

```rust
// executor.rs — ReActAgent 结构体新增
pub struct ReActAgent<L, S> {
    // ... 现有字段 ...
    /// 后台任务通知接收端：后台 agent 完成时推送结果
    notification_rx: Option<tokio::sync::mpsc::UnboundedReceiver<BackgroundTaskResult>>,
}
```

Builder 方法：

```rust
pub fn with_notification_rx(
    mut self,
    rx: tokio::sync::mpsc::UnboundedReceiver<BackgroundTaskResult>,
) -> Self {
    self.notification_rx = Some(rx);
    self
}
```

#### 3.2 循环内消费点

在 ReAct 循环的**每轮迭代末尾**（StepDone + StateSnapshot 之后，回到循环顶部之前），消费所有已到达的后台通知：

```rust
// executor.rs — 循环末尾，约 line 494 之后
// 消费后台任务完成通知
if let Some(ref mut rx) = self.notification_rx {
    while let Ok(result) = rx.try_recv() {
        let notification = if result.success {
            format!(
                "[后台任务 {} 已完成]\n\
                 Agent: {}\n\
                 工具调用次数: {}\n\
                 耗时: {}ms\n\
                 结果:\n{}",
                result.task_id,
                result.agent_name,
                result.tool_calls_count,
                result.duration_ms,
                result.output,
            )
        } else {
            format!(
                "[后台任务 {} 执行失败]\n\
                 Agent: {}\n\
                 错误:\n{}",
                result.task_id,
                result.agent_name,
                result.output,
            )
        };
        state.add_message(BaseMessage::human(&notification));
        self.emit(AgentEvent::MessageAdded(BaseMessage::human(&notification)));
    }
}
```

**消费时序保证**：

| 主 agent 状态 | 后台任务完成时 |
|---|---|
| 正在执行工具调用 | 等待工具调用完成 → 进入迭代末尾消费 |
| 正在 LLM 调用 | LLM 调用返回后 → 工具执行 → 迭代末尾消费 |
| 空闲（无 ReAct 循环） | TUI 通过事件系统显示通知 |

### 4. SubAgentTool 变更（`rust-agent-middlewares`）

#### 4.1 新增字段

```rust
pub struct SubAgentTool {
    // ... 现有字段 ...
    /// 后台任务注册中心（run_in_background 模式使用）
    background_registry: Option<Arc<BackgroundTaskRegistry>>,
}
```

Builder：

```rust
pub fn with_background_registry(
    mut self,
    registry: Arc<BackgroundTaskRegistry>,
) -> Self {
    self.background_registry = Some(registry);
    self
}
```

#### 4.2 invoke() 分支

当前 `invoke()` 在 line 334 已解析 `_run_in_background` 但未使用。改为：

```rust
// tool.rs — invoke() 中，fork 检测之前
let run_in_background = input.get("run_in_background")
    .and_then(|v| v.as_bool())
    .unwrap_or(false);

if run_in_background {
    return self.invoke_background(prompt, subagent_type, cwd).await;
}

// ... 现有 fork / normal 逻辑 ...
```

#### 4.3 invoke_background() 方法

```rust
async fn invoke_background(
    &self,
    prompt: String,
    subagent_type: Option<String>,
    cwd: String,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let registry = self.background_registry.as_ref()
        .ok_or("Background tasks not available: no registry configured")?;

    // 检查并发上限
    if registry.active_count() >= 3 {
        return Ok(
            "Error: maximum 3 concurrent background tasks reached. \
             Wait for a running task to complete before starting a new one."
                .to_string()
        );
    }

    let task_id = format!("bg-{}", uuid::Uuid::new_v4());

    // 构建子 agent（与 normal 路径逻辑相同：解析 agent 定义、过滤工具、组装 ReActAgent）
    // 区别：不 await 执行，而是 tokio::spawn
    let agent_builder = self.build_child_agent(&subagent_type, &cwd, &prompt)?;
    let event_handler = self.event_handler.clone();
    let registry = Arc::clone(registry);
    let agent_name = subagent_type.unwrap_or_else(|| "fork".to_string());
    let prompt_summary = prompt.chars().take(100).collect::<String>();

    let handle = tokio::spawn(async move {
        let mut state = AgentState::new(&cwd);
        let start = std::time::Instant::now();
        let result = match agent_builder.execute(
            AgentInput::text(&prompt),
            &mut state,
            None,
        ).await {
            Ok(output) => BackgroundTaskResult {
                task_id: task_id.clone(),
                agent_name: agent_name.clone(),
                prompt_summary: prompt_summary.clone(),
                success: true,
                output: output.text,
                tool_calls_count: /* 从 state 统计 */,
                duration_ms: start.elapsed().as_millis() as u64,
            },
            Err(e) => BackgroundTaskResult {
                task_id: task_id.clone(),
                agent_name: agent_name.clone(),
                prompt_summary: prompt_summary.clone(),
                success: false,
                output: e.to_string(),
                tool_calls_count: 0,
                duration_ms: start.elapsed().as_millis() as u64,
            },
        };

        // 推送通知到通道 + 更新 registry 状态
        registry.complete(&task_id, result);

        // 发出事件通知 TUI
        if let Some(ref handler) = event_handler {
            handler.on_event(AgentEvent::BackgroundTaskCompleted { ... });
        }
    });

    registry.register(BackgroundTask {
        id: task_id.clone(),
        agent_name,
        prompt_summary,
        status: BackgroundTaskStatus::Running,
        started_at: std::time::Instant::now(),
        abort_handle: handle,
    });

    Ok(format!(
        "Background task {} started. You will be notified when it completes. \
         You can continue with other tasks in the meantime.",
        task_id
    ))
}
```

### 5. 通道创建与组装

通道的 tx/rx 分配在 `SubAgentMiddleware` 中完成：

```rust
impl SubAgentMiddleware {
    /// 创建通知通道，返回 (tx, rx)
    /// tx → BackgroundTaskRegistry → 传给 SubAgentTool
    /// rx → ReActAgent::with_notification_rx()
    pub fn create_notification_channel() -> (
        tokio::sync::mpsc::UnboundedSender<BackgroundTaskResult>,
        tokio::sync::mpsc::UnboundedReceiver<BackgroundTaskResult>,
    ) {
        tokio::sync::mpsc::unbounded_channel()
    }
}
```

在 `rust-agent-tui/src/app/agent_ops.rs` 组装主 agent 时：

```rust
let (notification_tx, notification_rx) = SubAgentMiddleware::create_notification_channel();
let registry = Arc::new(BackgroundTaskRegistry::new(notification_tx));

let subagent_middleware = SubAgentMiddleware::new(tools, event_handler, llm_factory)
    .with_system_builder(system_builder)
    .with_background_registry(registry);

let agent = ReActAgent::new(llm)
    .with_notification_rx(notification_rx)
    .add_middleware(Box::new(subagent_middleware))
    // ... 其他中间件 ...
```

### 6. 事件扩展（`rust-create-agent`）

新增 `AgentEvent` 变体：

```rust
// events.rs
pub enum AgentEvent {
    // ... 现有变体 ...

    /// 后台 agent 任务完成（TUI 使用，用于空闲时通知）
    BackgroundTaskCompleted {
        task_id: String,
        agent_name: String,
        success: bool,
        output: String,
    },
}
```

### 7. TUI 显示（`rust-agent-tui`）

#### 7.1 状态栏指示器

状态栏显示当前后台任务数：`[BG: 2]`。当有后台任务运行时显示，无任务时隐藏。

#### 7.2 完成通知

- `BackgroundTaskCompleted` 事件到达 TUI 时，在消息区显示通知气泡
- 后台任务结果同时作为 Human 消息注入到 `state.messages`（由 ReAct 循环消费点处理），主 agent 在下一轮迭代中看到

#### 7.3 后台任务面板（可选，非 MVP）

类似 `/agents` 面板，显示所有后台任务的状态。MVP 阶段通过状态栏指示器 + 完成通知即可，后续迭代可增加专用面板。

### 8. 工具描述更新

更新 `AGENT_DESCRIPTION` 常量，增加 `run_in_background` 参数说明：

```markdown
## Background execution (run_in_background: true)
- The sub-agent runs asynchronously in the background
- The main agent continues immediately without waiting
- Maximum 3 concurrent background tasks
- The main agent will be notified when the background task completes
- Use for long-running tasks that don't block the main workflow
- Background tasks share the same working directory as the main agent
```

## 实现要点

### 9.1 核心变更文件

| 文件 | 变更类型 | 说明 |
|------|---------|------|
| `rust-create-agent/src/agent/executor.rs` | 修改 | 新增 `notification_rx` 字段 + 循环内消费点 |
| `rust-create-agent/src/agent/events.rs` | 修改 | 新增 `BackgroundTaskCompleted` 变体 |
| `rust-agent-middlewares/src/subagent/background.rs` | **新增** | `BackgroundTaskRegistry` + `BackgroundTaskResult` + `BackgroundTaskStatus` |
| `rust-agent-middlewares/src/subagent/tool.rs` | 修改 | 新增 `background_registry` 字段 + `invoke_background()` 方法 |
| `rust-agent-middlewares/src/subagent/mod.rs` | 修改 | `build_tool()` 传递 registry，新增通道创建逻辑 |
| `rust-agent-tui/src/app/agent_ops.rs` | 修改 | 组装时创建通道和 registry |
| `rust-agent-tui/src/app/events.rs` | 修改 | 处理 `BackgroundTaskCompleted` 事件 |
| `rust-agent-tui/src/ui/main_ui/status_bar.rs` | 修改 | 显示后台任务计数 |

### 9.2 关键技术决策

- **Unbounded channel 而非 bounded**：后台任务完成时间不可预测，UnboundedSender 的 `send` 不阻塞，保证后台任务不会因通道满而卡住。ReAct 循环每轮 `try_recv` 全部消费，不会无限堆积。
- **消费点在循环末尾而非开头**：放在 StepDone/StateSnapshot 之后，保证当前轮次的状态快照已正确发出，不会混入下一轮的后台通知。
- **注入为 Human 消息**：后台任务结果作为 Human 消息注入，而非 ToolResult。因为原始工具调用早已返回了 "task started" 的 ToolResult，后续完成是独立事件，不是原始工具调用的延迟结果。
- **注册中心在 middlewares 层**：`BackgroundTaskRegistry` 放在 `rust-agent-middlewares` 而非 `rust-create-agent`，保持核心框架对后台任务概念的零依赖。核心层只提供泛用的 `notification_rx` 通道消费机制。
- **SubAgentTool 中提取 build_child_agent()**：将 normal 路径中的 agent 构建逻辑（解析定义、过滤工具、组装 ReActAgent）提取为独立方法，供 normal/background/fork 三条路径复用。

### 9.3 依赖

- 无新增外部 crate 依赖（`tokio::sync::mpsc` 和 `parking_lot::Mutex` 已在用）
- `uuid::Uuid::new_v4()` 已有依赖（UUID v7 用于消息 ID，v4 用于后台任务 ID 便于区分）

### 9.4 工具调用计数

后台任务结果中的 `tool_calls_count` 从子 agent 的 `AgentState` 统计：在 `execute()` 完成后，遍历 `state.messages()` 中 `BaseMessage::Tool` 的数量。

## 约束一致性

- **Middleware Chain 模式**：后台任务通过 `SubAgentMiddleware.build_tool()` 注入 registry，不引入新的中间件。消费点直接在 ReAct 循环中通过 `notification_rx` 实现，不需要新的 middleware hook。
- **工具系统**：`run_in_background` 作为工具参数扩展 `SubAgentTool`，不影响 `BaseTool` trait 接口。
- **消息不可变历史**：后台通知注入为追加的 Human 消息，不修改历史。
- **事件驱动 TUI 通信**：`BackgroundTaskCompleted` 事件通过现有 `AgentEventHandler` 推送到 TUI。
- **Workspace 依赖方向**：核心层只定义 `BackgroundTaskResult` 类型（通过 re-export 或泛型），不依赖 middlewares 层。`notification_rx` 的类型参数为 `BackgroundTaskResult`，该类型需在核心层定义或在 `State` trait 关联类型中声明。

  > **架构偏离说明**：`ReActAgent` 的 `notification_rx` 字段类型为 `UnboundedReceiver<BackgroundTaskResult>`。如果 `BackgroundTaskResult` 定义在 middlewares 层，会违反"核心不依赖上层"的约束。解决方案：将 `BackgroundTaskResult` 定义在 `rust-create-agent/src/agent/` 中（作为核心事件类型的一部分），middlewares 层使用该类型。

- **编码规范**：遵循 Rust 2021 edition + async-trait + tracing 日志。

## 验收标准

- [ ] `run_in_background: true` 参数触发后台执行，主 agent 立即收到 "task started" 响应
- [ ] 后台 agent 完成后，结果通过通知通道推送到主 agent
- [ ] 主 agent 在当前工具调用完成后的下一个消费点注入 Human 消息
- [ ] LLM 在下一轮迭代中看到后台任务结果
- [ ] 最多 3 个并发后台 agent，超出返回错误
- [ ] 后台任务完成时发出 `BackgroundTaskCompleted` 事件
- [ ] TUI 状态栏显示后台任务计数
- [ ] Normal 路径（`run_in_background` 缺省或 false）行为不变
- [ ] Fork 路径（`fork: true`）行为不变
- [ ] `AGENT_DESCRIPTION` 更新，包含 `run_in_background` 参数说明
- [ ] 新增单元测试：`BackgroundTaskRegistry` 注册/完成/并发上限
- [ ] 新增集成测试：headless 模式下验证后台任务完成通知注入
