# Compact 下沉到 ACP 层 — 设计文档

**日期**: 2026-05-19
**状态**: 设计完成，待实施
**目标**: 将上下文压缩（compact）的触发判断和执行从 TUI 下沉到 `peri-acp` 的 `execute_prompt`，使 TUI 和 stdio/IDE 路径共享 compact 能力。

---

## 背景与问题

当前 compact 的触发判断和执行完全在 TUI 层（`peri-tui`）：

- `agent_ops.rs` 中 `ContextWarning` 和 `TokenUsageUpdate` 事件处理器设置 `needs_auto_compact` 标志
- `Done` 事件处理器检查标志后调用 `start_compact()` 或 `start_micro_compact()`
- `thread_ops.rs` 中 `start_compact()` 独立 spawn compact 任务
- `agent_compact.rs` 中 `handle_compact_done/error` 处理结果、创建新 Thread、pipeline rebuild、resubmit

**问题**：

1. **Stdio/IDE 路径完全没有 auto-compact** — `StdioEventSink` 不做任何 compact 判断
2. **TUI 侧 token tracker 与 ACP 侧 context_budget 职责重复** — 两层都在做"该不该 compact"的决策
3. **compact 执行与 TUI UI 状态深度耦合** — Thread 创建、pipeline rebuild、ephemeral_notes 清理全混在一起

## 设计决策

| # | 问题 | 决策 |
|---|------|------|
| 1 | stdio/IDE 路径 compact 结果处理 | 替换 SessionState.history + 通过 session/update 通知客户端 |
| 2 | compact 触发位置 | 在 `peri-acp` 的 `execute_prompt` 内部 |
| 3 | auto-compact 后 resubmit | executor 内部循环，最多 3 次 |
| 4 | micro-compact | 也下沉到 executor |
| 5 | Thread 持久化 | 不创建新 Thread，只替换 messages，持久化由调用方处理 |
| 6 | 通知 TUI | 通过 EventSink 发送 CompactStarted/CompactCompleted/CompactError 事件 |
| 7 | Hooks | executor 直接调用 fire_standalone_lifecycle_hooks（TUI/stdio 共享） |
| 8 | CompactCompleted 载荷 | 携带完整摘要 + re_inject 信息（文件列表、skill 列表） |
| 9 | 手动 /compact | 新增 ACP 请求 `session/compact`，走 executor 层公共函数 |
| 10 | 事件流交互 | TUI 可见内部 resubmit 事件流（第一轮 → Compact* → 第二轮 → Done） |

## 架构

### 核心流程（改造后）

```
TUI submit → AcpTuiClient.prompt()
           → MpscTransport → ACP Server session/prompt
           → executor::execute_prompt()
               ↓
               build_agent() + execute()
               ↓ agent 事件通过 EventSink → TUI pump
               ↓ agent 结束
               ↓ 检查 TokenTracker → 是否需要 compact?
               ↓
               ├─ 不需要 → 返回 PromptResult { messages }
               │
               ├─ micro-compact → micro_compact_enhanced()
               │   → CompactCompleted { micro_cleared: N }
               │   → 返回 PromptResult { messages }
               │
               └─ full-compact → run_compact()
                   → CompactStarted
                   → full_compact() + re_inject()
                   → CompactCompleted { summary, files, skills }
                   → resubmit? → 内部循环（回到 build_agent + execute）
                   → 返回 PromptResult { messages }
               ↓
           TUI: 用 messages 替换 agent_state_messages
                pipeline.clear() + restore_completed
                用 CompactCompleted 数据生成 "Compact: xxx" MessageViewModel
```

### executor 内部 compact 循环伪代码

```rust
pub async fn execute_prompt(...) -> PromptResult {
    let mut current_history = history;
    let mut total_resubmits = 0;
    const MAX_RESUBMITS: u32 = 3;

    loop {
        // 构建 agent + 执行
        let agent_output = builder::build_agent(...);
        let mut agent_state = AgentState::with_messages(cwd, current_history);

        // 启动 event pump（event channel 在循环中保持存活）
        let (event_tx, event_rx) = unbounded_channel();
        spawn_event_pump(event_rx, &sink, session_id, context_window);

        let result = agent_output.executor.execute(input, &mut state, cancel).await;
        drop(event_tx);
        wait_for_pump_done().await;

        if !result.is_ok() || cancel.is_cancelled() {
            return PromptResult { messages: state.into_messages(), ok: result.is_ok() };
        }

        let messages = state.into_messages();
        let tracker = state.token_tracker();

        // Micro-compact
        if budget.should_warn(tracker) && !budget.should_auto_compact(tracker) {
            let cleared = micro_compact_enhanced(&compact_config, &mut messages);
            if cleared > 0 {
                send_event(&sink, session_id, CompactCompleted {
                    summary: String::new(),
                    files: vec![],
                    skills: vec![],
                    micro_cleared: cleared,
                });
            }
            return PromptResult { messages, ok: true };
        }

        // Full compact
        if !budget.should_auto_compact(tracker) || total_resubmits >= MAX_RESUBMITS {
            return PromptResult { messages, ok: true };
        }

        // 执行 compact
        match run_compact(&messages, model, &config, cwd, &sink, session_id,
                          context_window, cancel, hooks, hook_ctx).await {
            Ok(output) => {
                current_history = output.new_messages;
                total_resubmits += 1;
                continue; // resubmit
            }
            Err(e) => {
                send_event(&sink, session_id, CompactError { message: e.to_string() });
                return PromptResult { messages, ok: true };
            }
        }
    }
}
```

### ExecutorEvent 变体改造

```rust
// peri-agent/src/agent/events.rs

/// compact 文件信息摘要
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactFileInfo {
    pub path: String,
    pub lines: usize,
}

/// 上下文压缩完成
CompactCompleted {
    summary: String,
    files: Vec<CompactFileInfo>,
    skills: Vec<String>,
    micro_cleared: usize,  // >0 表示 micro-compact，=0 表示 full compact
},

/// 上下文压缩失败
CompactError {
    message: String,
},
```

### 手动 compact ACP 请求

```
Request:  session/compact
Params:   { sessionId: string, instructions?: string }
Response: { messages: BaseMessage[], compactInfo: { summary, files, skills } }
```

处理流程：
1. ACP Server 从 SessionState 取出 history
2. 调用 `compact_runner::run_compact()`（与 auto-compact 共用的公共函数）
3. CompactStarted/Completed 事件通过 EventSink 发送
4. 更新 SessionState.history
5. 返回压缩后的 messages

### compact_runner 公共函数

```rust
// peri-acp/src/session/compact_runner.rs（新文件）

pub struct CompactOutput {
    pub new_messages: Vec<BaseMessage>,
    pub summary: String,
    pub files: Vec<CompactFileInfo>,
    pub skills: Vec<String>,
}

pub async fn run_compact(
    messages: &[BaseMessage],
    model: &dyn BaseModel,
    config: &CompactConfig,
    cwd: &str,
    sink: &dyn EventSink,
    session_id: &str,
    context_window: u32,
    cancel: &AgentCancellationToken,
    hooks: &[RegisteredHook],
    hook_ctx: &HookContext,
) -> Result<CompactOutput, CompactError>
```

职责：fire PreCompact hooks → send CompactStarted → full_compact → re_inject → build new_messages → send CompactCompleted → fire PostCompact hooks

## TUI 侧变更

### 删除的代码

| 文件 | 内容 |
|------|------|
| `peri-tui/src/app/agent.rs` | `compact_task()` 整个函数（~180 行） |
| `peri-tui/src/app/agent_compact.rs` | 整个文件删除 |
| `peri-tui/src/app/thread_ops.rs` | `start_compact()`、`start_micro_compact()` 方法 |
| `peri-tui/src/app/agent_ops.rs` | ContextWarning 中的 compact 触发逻辑 |
| `peri-tui/src/app/agent_ops.rs` | TokenUsageUpdate 中的 compact 判断逻辑 |
| `peri-tui/src/app/agent_ops.rs` | Done 中的 auto-compact 执行逻辑 |
| `peri-tui/src/app/agent_events_bg.rs` | 后台任务完成后的 deferred compact 触发 |
| `peri-tui/src/app/events.rs` | 旧的 `CompactDone`/`CompactError` 变体 |

### AgentComm 删除的字段

```rust
needs_auto_compact: bool,
auto_compact_failures: u32,
pre_compact_token_snapshot: Option<TokenTracker>,
compact_should_resubmit: bool,
last_user_input: Option<String>,
pre_compact_user_input: Option<String>,
auto_compact_resubmit_count: u32,
```

### 新增/修改的代码

| 文件 | 内容 |
|------|------|
| `src/app/events.rs` | 新增 `CompactStarted`、`CompactCompleted { summary, files, skills, micro_cleared }`、`CompactError { message }` |
| `src/app/agent.rs` | `map_executor_event` 映射新 compact 事件变体 |
| `src/app/agent_ops.rs` | 新增 `handle_compact_started()`、`handle_compact_completed()`、`handle_compact_error()` — 纯 UI 处理 |
| `src/command/session/compact.rs` | 改为通过 ACP `session/compact` 请求触发 |
| `src/acp_server.rs` | 新增 `session/compact` 请求处理路由 |

### handle_compact_completed 职责（纯 UI）

1. 用 CompactCompleted 数据生成 "Compact: xxx" + 文件列表 + Skill 列表的 `MessageViewModel`
2. `pipeline.clear()` + 全量 rebuild
3. 清理 `ephemeral_notes`（旧消息锚点已失效）
4. `set_loading(false)`
5. 不做：Thread 创建、compact 执行、resubmit

## 错误处理

| 场景 | 处理 |
|------|------|
| Compact 过程中取消 | 发送 CompactError，返回原始 messages，PromptResult.ok = true |
| Full compact 失败（LLM 错误） | 发送 CompactError，返回原始 messages，不 resubmit |
| Resubmit 达到上限（3 次） | 发送 CompactError（limit reached），返回压缩后 messages，不继续 resubmit |
| 空 history | 跳过 compact |
| 后台任务运行时 | 不存在竞态（compact 在 agent 结束后执行，后台任务通知已通过事件流处理） |

## 变更文件汇总

**peri-agent**（1 文件）：
- `src/agent/events.rs` — CompactCompleted 增加数据字段，新增 CompactFileInfo、CompactError 变体

**peri-acp**（3 文件）：
- `src/session/executor.rs` — 核心改造：execute_prompt 内部 compact 循环
- `src/session/compact_runner.rs`（新建）— compact 公共函数
- `src/session/mod.rs` — 注册 compact_runner 模块

**peri-tui**（~8 文件）：
- `src/app/agent.rs` — 删除 compact_task，修改 map_executor_event
- `src/app/agent_compact.rs` — 删除整个文件
- `src/app/agent_comm.rs` — 删除 8 个 compact 相关字段
- `src/app/agent_ops.rs` — 删除 compact 触发逻辑，新增 UI 处理
- `src/app/agent_events_bg.rs` — 删除 deferred compact
- `src/app/thread_ops.rs` — 删除 start_compact/start_micro_compact
- `src/app/events.rs` — 替换旧 compact 变体
- `src/command/session/compact.rs` — 改用 ACP 请求
- `src/acp_server.rs` — 新增 session/compact 路由

**不变**：
- `peri-agent/src/agent/compact/` — full_compact、micro_compact、re_inject、config
- `peri-agent/src/agent/token.rs` — TokenTracker、ContextBudget
- `peri-acp/src/session/event_sink.rs` — EventSink trait
- `peri-middlewares/` — hooks
