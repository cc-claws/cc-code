# Compact 中间件化重构 - 执行计划

**目标:** 彻底移除 `execute_prompt` 外层的 compact+resubmit 循环，将 compact 触发逻辑下沉为 `CompactMiddleware`，在 ReAct 循环内部的两个检查点（`before_model` / `after_agent`）原地压缩上下文，删除 resubmit 能力。

**架构:** 新增 `before_model` / `after_model` 钩子到 Middleware trait → 实现 `CompactMiddleware`（`peri-middlewares`）→ 在 ReAct 循环中插入调用点 → 简化 `execute_prompt` 为单次 `agent.execute()` → 清理 TUI 侧残留逻辑。

**设计决策:** 见 `/grill-me` 对话总结——只有**一个** ReAct 循环，compact 是该循环内的中间件行为；不再有外层重建 agent 并重新执行的能力。

**子计划:** Middleware trait 钩子扩展细节见 [spec-plan-model-hooks.md](./spec-plan-model-hooks.md)

**技术栈:** Rust 2021, tokio + async-trait, peri-agent + peri-middlewares + peri-acp + peri-tui

---

## 改动总览

本次改动涉及 4 个 crate：

| Crate | 改动 | 风险 |
|-------|------|------|
| `peri-agent` | Middleware trait 新增 `before_model` + `after_model`；ReAct 循环插入两个调用点 | **中** — 19 个中间件零改动（默认实现），详见 [spec-plan-model-hooks.md] |
| `peri-middlewares` | 新增 `CompactMiddleware` | **中** — 新模块，需正确接入 LLM/事件/Hooks 依赖 |
| `peri-acp` | `executor.rs` 删除外部循环，`builder.rs` 注入 CompactMiddleware | **高** — 核心执行路径重构 |
| `peri-tui` | 清理 `compact_task()` 死代码 | **低** — 删除而已 |

**Task 依赖:** Task 1（Middleware trait）→ Task 2（CompactMiddleware）→ Task 3（ReAct 循环）→ Task 4（executor 简化）→ Task 5（TUI 清理）→ Task 6（测试验证）。Task 2 和 3 可部分并行。

---

## 任务索引

| Task | 名称 | 概要 | 涉及文件 |
|------|------|------|----------|
| 1 | Middleware trait 扩展 | 新增 `before_model` + `after_model` 钩子，详见 [spec-plan-model-hooks.md] | trait.rs, chain.rs, chain_test.rs, executor/mod.rs |
| 2 | CompactMiddleware 实现 | 新建 `peri-middlewares/src/compact_middleware.rs` | compact_middleware.rs, mod.rs |
| 3 | ReAct 循环改造 | `execute()` 中插入 `before_model` / `after_model` 调用点 | executor/mod.rs（已在 Task 1 中完成） |
| 4 | executor.rs 去循环化 | 删除外层 `loop`，单次 `execute()` | executor.rs, builder.rs |
| 5 | TUI 清理 | 删除 `compact_task()` 死代码 | agent.rs, agent_compact.rs (适配) |
| 6 | 测试验证 | 全量测试 + 新增 CompactMiddleware 测试 | compact_middleware_test.rs |

---

## Task 1：Middleware trait 扩展

**目标:** 新增 `before_model` 钩子，Middleware trait + MiddlewareChain + 所有实现者。

### 1.1 Middleware trait（`peri-agent/src/middleware/trait.rs`）

在 `after_tool` 和 `after_agent` 之间插入新方法：

```rust
/// LLM 调用前调用（在每轮 ReAct 循环的 call_llm 之前）
/// 可用于上下文压缩、token 预算检查等
async fn before_model(&self, state: &mut S) -> AgentResult<()> {
    let _ = state;
    Ok(())
}
```

**影响范围:** 所有 `impl Middleware<S> for XxxMiddleware` 自动获得默认空实现。无需修改任何现有中间件。

### 1.2 MiddlewareChain（`peri-agent/src/middleware/chain.rs`）

新增 `run_before_model` 方法（按序执行所有中间件，遇错即停）：

```rust
/// 顺序执行 before_model 钩子
pub async fn run_before_model(&self, state: &mut S) -> AgentResult<()> {
    for middleware in &self.middlewares {
        middleware.before_model(state).await?;
    }
    Ok(())
}
```

### 1.3 测试更新（`chain_test.rs`）

新增 `test_run_before_model_basic` / `test_run_before_model_error_propagation` 测试。

**验证:** `cargo test -p peri-agent --lib middleware::chain::tests`

---

## Task 2：CompactMiddleware 实现

**目标:** 新建 `CompactMiddleware`，在 `before_model` 和 `after_agent` 中检查 token 阈值 + 执行 compact。

### 2.1 文件位置

新建 `peri-middlewares/src/compact_middleware.rs`，在 `peri-middlewares/src/lib.rs` 中声明模块并 re-export。

### 2.2 结构体设计

```rust
pub struct CompactMiddleware {
    /// LLM 模型（用于 full compact 摘要生成），None 则跳过 full compact
    model: Option<Arc<dyn peri_agent::llm::BaseModel>>,
    /// Compact 配置
    config: peri_agent::agent::compact::CompactConfig,
    /// 上下文窗口预算
    budget: peri_agent::agent::token::ContextBudget,
    /// 工作目录（re_inject 需要）
    cwd: String,
    /// 事件通道（发送 CompactStarted/Completed/Error）
    event_tx: Arc<std::sync::Mutex<Option<tokio::sync::mpsc::UnboundedSender<peri_agent::agent::events::AgentEvent>>>>,
    /// 取消令牌
    cancel: peri_agent::agent::AgentCancellationToken,
    /// Hooks（PreCompact/PostCompact）
    hooks: Vec<peri_middlewares::hooks::RegisteredHook>,
    /// Hook 上下文
    hook_ctx: peri_acp::session::compact_runner::HookContext,
}
```

**⚠️ 注意**: `HookContext` 在 `peri-acp` 中定义。若不想引入 `peri-acp` 依赖，可将其下沉到 `peri-agent` 或复制一份到 `peri-middlewares`。推荐方案：将 `HookContext` 提取到 `peri-agent::agent::compact` 模块（与 compact 逻辑共址）。

### 2.3 `before_model` 逻辑

```
before_model(state):
  1. 若 DISABLE_COMPACT / DISABLE_AUTO_COMPACT 环境变量或 config.auto_compact_enabled == false → 直接返回
  2. 检查 budget.should_auto_compact(tracker) 
     → true: 调用 do_full_compact(state)
     → false: 检查 budget.should_warn(tracker)
       → true: 调用 do_micro_compact(state)
  3. 返回 Ok(())
```

**关键:** compact 后**不改变**控制流——`before_model` 返回后，ReAct 循环自然继续执行 `call_llm`。

### 2.4 `after_agent` 逻辑

```
after_agent(state, output):
  1. 同 before_model 的阈值检查 + compact 逻辑
  2. 返回 Ok(output.clone())  // 输出内容不变，但 state 消息已原地替换
```

**关键:** 循环停止时 compact 后不 resubmit——`handle_final_answer` 已返回 `AgentOutput`，调用方 (`execute_prompt`) 读取 `state.into_messages()` 获得 compacted 历史。

### 2.5 `do_full_compact` 实现

复用现有的 `peri_agent::agent::compact::full_compact()` + `re_inject()`：

```
do_full_compact(state):
  1. 若 model 为 None → 跳过（事件通道发送 CompactError）
  2. 发送 CompactStarted 事件
  3. 执行 PreCompact hooks
  4. 调用 full_compact(messages, model, config, instructions)
  5. 取消检查
  6. 调用 re_inject(original_messages, config, cwd)
  7. 构建 new_messages = vec![system(summary)] + re_inject.messages
  8. 发送 CompactCompleted 事件（携带 messages/files/skills）
  9. 执行 PostCompact hooks
  10. 替换 state 消息: *state.messages_mut() = new_messages
  11. 重置 token_tracker: state.token_tracker_mut().reset()
```

**与现有 compact_runner 的关系:** `CompactMiddleware.do_full_compact` 将取代 `compact_runner::run_full_compact` 在 auto-compact 路径中的角色。手动 `/compact` 路径继续使用 `compact_runner`（不变）。

### 2.6 `do_micro_compact` 实现

```
do_micro_compact(state):
  1. 调用 micro_compact_enhanced(config, state.messages_mut())
  2. 若 cleared > 0，发送 CompactCompleted 事件（micro_cleared > 0）
```

### 2.7 借用安全

`CompactMiddleware` 的关键借用约束：
- 先读 `state.token_tracker()`（不可变借用）计算阈值 → 丢弃引用
- 再调用 `state.messages_mut()` / `state.token_tracker_mut()`（可变借用）
- 不可同时持有不可变和可变引用

```rust
/// 安全借用模式
async fn before_model(&self, state: &mut S) -> AgentResult<()> {
    if self.is_disabled() { return Ok(()); }
    
    // Step 1: 不可变借用（仅在块作用域内）
    let (should_full, should_micro) = {
        let tracker = state.token_tracker();
        let full = self.budget.should_auto_compact(tracker);
        let micro = !full && self.budget.should_warn(tracker);
        (full, micro)
    };
    
    // Step 2: 可变借用（tracker 引用已 drop）
    if should_full {
        self.do_full_compact(state).await?;
    } else if should_micro {
        self.do_micro_compact(state);
    }
    Ok(())
}
```

### 2.8 环境变量覆盖

`DISABLE_COMPACT` / `DISABLE_AUTO_COMPACT` 在 `before_model`/`after_agent` 入口检查，而非构造函数中——允许运行时通过环境变量动态控制。

**验证:** `cargo build -p peri-middlewares` + 新增单元测试

---

## Task 3：ReAct 循环改造

**目标:** 在 `ReActAgent::execute()` 的 `for step` 循环中，`call_llm` 之前插入 `chain.run_before_model(state)`。

### 3.1 executor/mod.rs 修改

在 `peri-agent/src/agent/executor/mod.rs` 第 245-246 行之间插入：

```rust
for step in 0..self.max_iterations {
    state.set_current_step(step);

    // CompactMiddleware: before_model 检查点（工具完成后 → LLM 调用前）
    self.chain.run_before_model(state).await?;

    // LLM 推理
    let reasoning =
        self::llm_step::call_llm(self, state, &tool_refs, step, &cancel).await?;
    // ... 后续不变
```

**语义:**
- **第 0 步**: `before_model` 在第一次 LLM 调用前执行（初始上下文通常不超阈值，无害）
- **第 1..N 步**: 工具调用完成后，下一轮 LLM 调用前执行 compact 检查
- **错误传播**: `before_model` 返回 `Err` 时通过 `?` 传播——compact 失败不应静默继续

### 3.2 微调 compact_config 字段

`ReActAgent.compact_config` 当前存在但仅传递给子 Agent。CompactMiddleware 持有自己的 config，`ReActAgent.compact_config` 可保留（子 Agent 仍需）或标记为 deprecated。

**不删除此字段**以避免破坏子 Agent 构建路径。

### 3.3 注释更新

删除 L266-267 的过时注释：
```rust
// micro-compact 由 TUI 侧在 ContextWarning (0.70 阈值) 时统一触发
// 此处不再重复执行，避免同一条消息被压缩两次
```
替换为简短注释说明 compact 已由 CompactMiddleware 处理。

**验证:** `cargo build -p peri-agent`

---

## Task 4：executor.rs 去循环化

**目标:** `execute_prompt()` 从 `loop { build_agent → execute → compact → resubmit }` 简化为 `build_agent → execute → return`。

### 4.1 executor.rs 简化（`peri-acp/src/session/executor.rs`）

**删除:**
- L131: `let mut current_history = history;`
- L131-133: `total_resubmits`, `MAX_RESUBMITS`, `compacted`
- L135-303: 整个 `loop { ... }` 块

**替换为:**

```rust
// 单次 Agent 执行（compact 由 CompactMiddleware 在循环内处理）
let event_handler: Arc<dyn AgentEventHandler> = Arc::new(FnEventHandler({
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
    hook_session_start: is_empty_history,
    mcp_pool: mcp_pool.clone(),
    tool_search_index: tool_search_index.clone(),
    shared_tools: shared_tools.clone(),
    child_handler_factory: None,
    lsp_servers: lsp_servers.clone(),
    compact_config: Some(compact_config.clone()),  // NEW: 传给 builder 用于构建 CompactMiddleware
    compact_budget: Some(budget.clone()),           // NEW
    compact_model: Some(provider.clone().into_model()),  // NEW
    compact_event_tx: Some(event_tx.clone()),       // NEW
});

// todo 转发（保持不变）
// ... todo_rx.spawn ...

let mut agent_state = AgentState::with_messages(cwd.to_string(), history);
let result = agent_output
    .executor
    .execute(agent_input, &mut agent_state, Some(cancel.clone()))
    .await;
drop(agent_output.executor);

let ok = result.is_ok();
if let Err(e) = &result {
    error!(session_id = %session_id, error = %e, "Agent execution failed");
}

let stop_reason = if cancel.is_cancelled() {
    PromptStopReason::Cancelled
} else if matches!(&result, Err(AgentError::MaxIterationsExceeded(_))) {
    PromptStopReason::MaxTurnRequests
} else if matches!(&result, Err(AgentError::Interrupted)) {
    PromptStopReason::Cancelled
} else {
    PromptStopReason::EndTurn
};

close_channel(&event_tx);
wait_for_pump(pump_done_rx, &session_id).await;

PromptResult {
    messages: agent_state.into_messages(),
    ok,
    compacted: false,  // compact 现在完全在中间件内处理；此字段可标记为 deprecated
    stop_reason,
}
```

### 4.2 AcpAgentConfig 扩展（`peri-acp/src/agent/builder.rs`）

新增字段（`Option` 类型，保持向后兼容）：

```rust
pub struct AcpAgentConfig {
    // ... 现有字段 ...
    /// Compact 中间件配置（None = 不启用自动 compact）
    pub compact_config: Option<CompactConfig>,
    /// 上下文窗口预算（CompactMiddleware 需要）
    pub compact_budget: Option<ContextBudget>,
    /// LLM 模型（CompactMiddleware 用于 full compact 摘要生成）
    pub compact_model: Option<Arc<dyn BaseModel>>,
    /// 事件通道（CompactMiddleware 发送 compact 事件）
    pub compact_event_tx: Option<Arc<std::sync::Mutex<Option<tokio::sync::mpsc::UnboundedSender<AgentEvent>>>>>,
}
```

### 4.3 build_agent 注入 CompactMiddleware

在 `builder.rs` 的 `build_agent()` 中，当 `compact_config` 等字段非 None 时，构建并注入 CompactMiddleware：

```rust
// 在所有中间件注册完毕后、agent 构建前
if let (Some(config), Some(budget), Some(model), Some(event_tx)) = (
    config.compact_config.clone(),
    config.compact_budget.clone(),
    config.compact_model.clone(),
    config.compact_event_tx.clone(),
) {
    let all_hooks: Vec<_> = config.hook_groups.iter().flatten().cloned().collect();
    let hook_ctx = HookContext {
        cwd: config.cwd.clone(),
        session_id: config.session_id.clone().unwrap_or_default(),
        transcript_path: String::new(),
        provider_name: config.provider.display_name().to_string(),
        instructions: String::new(),
    };
    let compact_mw = CompactMiddleware::new(
        model,
        config,
        budget,
        config.cwd.clone(),
        event_tx,
        config.cancel.clone(),
        all_hooks,
        hook_ctx,
    );
    agent.add_middleware(Box::new(compact_mw));
}
```

**中间件链顺序:** CompactMiddleware 在链中位置不重要——`before_model`/`after_agent` 都是顺序执行，不影响其他中间件。建议放在链尾附近（在 LspMiddleware 之后），便于理清顺序。

### 4.4 PromptResult.compacted 字段

标记为 deprecated。`compacted` 现在由 CompactMiddleware 内部通过事件通知 UI，不需要通过返回值传递。

### 4.5 删除 compact_runner 的 auto-compact 引用

`compact_runner` 模块保留（手动 `/compact` 路径仍需使用），但 `execute_prompt` 中不再引用它。`run_full_compact`/`run_micro_compact` 保留为公有 API。

**验证:** `cargo build -p peri-acp`

---

## Task 5：TUI 清理

### 5.1 删除 compact_task（`peri-tui/src/app/agent.rs`）

搜索并删除 `compact_task()` 函数（旧的手动 compact 残留）。

### 5.2 handle_compact_completed 适配

`peri-tui/src/app/agent_compact.rs` 中的 `handle_compact_completed` 逻辑保持不变——CompactMiddleware 仍通过事件通道发送 `CompactCompleted` 事件，TUI 映射和处理逻辑无需修改。

### 5.3 map_executor_event 不变

`CompactStarted` / `CompactCompleted` / `CompactError` 的映射保持不变（`peri-tui/src/app/agent.rs:154-168`）。

**验证:** `cargo build -p peri-tui`

---

## Task 6：测试验证

### 6.1 新增测试

- `peri-middlewares/src/compact_middleware_test.rs`: CompactMiddleware 单元测试
  - `test_before_model_noop_when_disabled` — 禁用时跳过
  - `test_before_model_micro_compact` — 低于 auto 阈值时触发 micro
  - `test_before_model_full_compact` — 超过 auto 阈值时触发 full
  - `test_after_agent_full_compact_no_resubmit` — 循环结束时的 compact
  - `test_compact_without_model_skips_full` — 无 model 时跳过 full compact
  - `test_borrow_safety_then_mut` — 验证先读 tracker 后改 messages 的借用模式

- `peri-agent/src/middleware/chain_test.rs`: `before_model` 钩子测试

- `peri-acp` 集成测试：验证 `execute_prompt` 单次执行不循环

### 6.2 全量测试

```bash
cargo test --workspace
cargo test -p peri-agent --lib
cargo test -p peri-middlewares --lib
cargo test -p peri-acp --lib
```

### 6.3 手动验证场景

1. **长对话自动 compact**: 启动 TUI → 发送多条消息触发工具调用 → 观察 compact 事件在工具执行后、下一轮 LLM 前触发
2. **循环结束时 compact**: 对话自然结束 → 观察 `after_agent` 中的 compact
3. **手动 /compact**: 验证手动路径仍正常工作
4. **取消打断**: compact 执行中被 Ctrl+C → 验证取消传播
5. **环境变量禁用**: `DISABLE_COMPACT=true` → 验证 compact 不触发
6. **stdout ACP 路径**: 通过 stdio transport 验证 compact 事件正确推送

**验证:** 全量测试 PASS + 手动 6 场景通过

---

## 风险与缓解

| 风险 | 缓解 |
|------|------|
| 所有 17 个中间件需兼容新 `before_model` 方法 | 默认空实现，零改动 |
| CompactMiddleware 借用模式在 async 中可能有问题 | 将 tracker 读取限制在同步块内，立即 drop 引用 |
| full_compact 中 LLM 调用在中间件内执行可能超时 | 复用现有 `cancel` token，`tokio::select!` 竞争取消 |
| HookContext 沉入 peri-agent 增加跨 crate 依赖 | 优先方案：在 peri-agent 中定义精简版 HookContext；备选：直接在 peri-middlewares 复制定义 |
| 手动 /compact 路径与 auto-compact 事件冲突 | 手动 compact 仍在 compact_runner 中独立发送事件，与 CompactMiddleware 互斥（不同 session 状态） |

---

## 与现有 compact_runner 的关系

| 路径 | 旧实现 | 新实现 |
|------|--------|--------|
| auto full compact | `executor.rs` loop → `compact_runner::run_full_compact` | `CompactMiddleware.before_model/after_agent` → 内联 |
| auto micro compact | `executor.rs` loop → `compact_runner::run_micro_compact` | `CompactMiddleware.before_model/after_agent` → `micro_compact_enhanced` |
| 手动 /compact | ACP `session/compact` → `compact_runner::run_full_compact` | **不变** |
| TUI ContextWarning | `llm_step.rs` emit → TUI `handle_context_warning` | **不变**（警告保留，但 micro-compact 不再由 TUI 触发） |

**compact_runner 模块保留**，仅删除 `execute_prompt` 中对它的 auto-compact 调用。

---

## 数据流（新架构）

```
execute_prompt()
  → build_agent()  [单次，注入 CompactMiddleware]
    → ReActAgent.execute()  [唯一 ReAct 循环]
      → for step in 0..max_iterations:
        → chain.run_before_model(state)     ← CompactMiddleware: 工具后 compact 检查点
        → call_llm()                        ← LLM 调用
        → [工具调用] → dispatch_tools() → emit_snapshot
        → [最终答案] → handle_final_answer()
          → chain.run_after_agent(state, output)  ← CompactMiddleware: 循环结束 compact 检查点
          → return AgentOutput
  → state.into_messages()  ← 获取 compacted 历史
  → return PromptResult
```

**删除的能力:**
- ❌ 外层 `loop { build_agent → execute → compact → resubmit }`
- ❌ `current_history` / `total_resubmits` / `MAX_RESUBMITS`
- ❌ `agent_output.executor` 被 drop 后重新构建
- ❌ `PromptResult.compacted` 字段（语义上废弃）
