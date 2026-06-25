# Implementation Plan: define.rs + keyboard.rs 重构

**Goal**: 将 `define.rs`（1242 行）和 `keyboard.rs`（1222 行）拆分为可维护的子模块，消除重复代码，同时保持行为完全不变。

**Total scope**: 删除 ~56 行死代码 + 消除 ~250 行重复 + 拆出 6 个子模块，最终两个主文件各 ~200-350 行。

**Non-goals**: 不改 API、不改 cancel policy、不改 match arm 顺序、不改 DeregisterGuard 逻辑。

---

## Task 0: 删除 keyboard.rs 死代码（零风险热身）

**Why first**: 最小变更（-56 行），零风险，验证整个构建/测试流程。

### What

删除 `keyboard.rs` L233-288——Ctrl+T 模型切换和 Ctrl+Shift+T Provider 切换的第二次出现。L174-231 已处理这两个快捷键（包含 macOS Option 兼容路径），L233-288 的条件是 L174-231 的**真子集**，永远不会执行。

### Steps

1. 删除 L233-255（Ctrl+T 重复块，23 行）
2. 删除 L258-288（Ctrl+Shift+T 重复块，31 行）

### Verify

```bash
cargo build -p peri-tui
cargo test -p peri-tui --lib
# Ctrl+T / Ctrl+Shift+T / Alt+M / Alt+Shift+M 在 TUI 中仍正常切换模型和 Provider
```

### Risk

**Zero**. 删除的是不可达代码。条件完全相同但 L174-196 已被更早的 if 拦截。

---

## Task 1: define.rs 提取 `build_from_agent_def()` 公共函数

**Why next**: 消除 normal path 和 invoke_background 之间 15 步重复的 agent 构建逻辑（~250 行），是后续拆分的基础。

### Design decision: 用回调模式而非 struct 返回

**选择回调模式**（`with_built_agent<F, Fut>(self, agent_def, cwd, cancel_policy, f: F)`）而非返回 `BuiltAgent` struct。

**理由**:
- `DeregisterGuard` 的生命周期绑定在调用栈上——必须 `drop` 在 `agent_builder.execute()` 之后、在函数返回之前
- 返回 struct 需要 `ManuallyDrop` + 手动 drop，增加了错误表面
- 回调闭包自动保证 `DeregisterGuard` 在 f 完成后、`with_built_agent` 返回前 drop

**签名**:
```rust
impl SubAgentTool {
    pub(crate) async fn with_built_agent<F, Fut>(
        &self,
        agent_def: &ClaudeAgent,
        agent_id_for_events: &str,  // "agent_name" 用于事件/hook
        cwd: &str,
        prompt: &str,               // 仅用于 agent_builder.execute()
        instance_id: &str,          // child_thread_id
        cancel_policy: CancelPolicy, // Cascade | Independent
        f: F,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>>
    where
        F: FnOnce(ReActAgent, &mut AgentState, AgentCancellationToken) -> Fut,
        Fut: Future<Output = Result<AgentOutput, AgentError>>,
```

**CancelPolicy enum**:
```rust
pub(crate) enum CancelPolicy {
    /// parent cancel → child cancel (normal, fork)
    Cascade,
    /// only session-level cancel_all_agents (background)
    Independent,
}
```

### What to extract (合并 normal path L1032-1165 和 invoke_background L476-595)

提取步骤:
1. `uuid::Uuid::now_v7()` → `child_thread_id`
2. 如果 `self.thread_store.is_some()` → `ThreadMeta` + `create_thread`
3. `load_agent_def` → agent_def (调用方已完成)
4. `filter_tools`
5. `model_alias` → `self.llm_factory`
6. `max_iterations`
7. `ReActAgent::new(llm).max_iterations(...)`
8. `build_subagent_middlewares` → add_middleware
9. 如果 `self.system_builder` → `overrides_from_agent_def` → `with_system_prompt`
10. 注册 tools
11. 如果 `self.child_handler_factory` → `with_event_handler`
12. else if `self.event_handler` → `SourceAgentIdHandler`
13. `AgentState::new` → `with_persistence`
14. `SubagentStarted` event
15. `SubagentStart` hook
16. 根据 `cancel_policy` 做 `register_runtime` + `DeregisterGuard`
17. 调用 `f(agent_builder, &mut agent_state, cancel_token).await`
18. 默认的 result handling（`SubagentStopped` + `update_thread_status` + `format_subagent_result`）

### Steps

1. 在 `subagent/tool/` 下创建 `build_agent.rs`
2. 定义 `CancelPolicy` enum
3. 实现 `with_built_agent()` — 合并 L1032-1165 和 L476-595
4. 修改 `BaseTool::invoke()` 的 normal path 调用 `with_built_agent`（传入 `CancelPolicy::Cascade`，f = `agent_builder.execute`）
5. 修改 `invoke_background()` 的非 fork 分支调用 `with_built_agent`（传入 `CancelPolicy::Independent`，f 在 tokio::spawn 内部调用）

### Verify

```bash
cargo build -p peri-middlewares
cargo test -p peri-middlewares --lib -- subagent
```

**关键验证点**:
- normal subagent 仍能正常执行并返回结果
- background agent 仍能通过 `tokio::spawn` 独立运行
- `DeregisterGuard` 在所有路径（正常完成 / 错误 / panic）都正确触发
- cascade cancel 仍影响 normal subagent，independent cancel 不影响 background

### Risk

**Medium**. 这是最大的单次变更——合并两个执行路径的公共部分。需要仔细处理:
- normal path 的 `instance_id` 参数（`child_handler_factory(instance_id.clone())`）
- bg path 没有 `child_handler_factory` / `SourceAgentIdHandler`（缺少此项是 bg path 的特性，不是 bug——bg agents 没有事件处理管道）
- prompt 参数：normal path 用原始 prompt，bg path 也用原始 prompt（fork 路径用 `fork_directive`，不走 `with_built_agent`）

**缓解**: 分两个 commit——(a) 提取函数，(b) 修改调用方。每个 commit 独立通过构建和 `cargo check`。

---

## Task 2: 拆出 `invoke_fork()` → `execute_fork.rs`

**Why now**: 纯移动（无逻辑变更），是后续 bg 拆分的练手。`invoke_fork` 不依赖 `with_built_agent`（它是 fork 路径），所以可以和 Task 1 真正并行。

### Steps

1. 创建 `subagent/tool/execute_fork.rs`
2. 将 L270-448（整个 `invoke_fork` 方法）移动到新文件
3. 在 `define.rs` 中添加 `mod execute_fork;`
4. 确保 `use super::*` 导入覆盖 `DeregisterGuard`, `SourceAgentIdHandler`, `build_subagent_middlewares`, `SubAgentMiddlewareConfig` 等

### Verify

```bash
cargo build -p peri-middlewares
# invoke_fork 行为不变——纯移动
```

### Risk

**Low**. 纯函数移动。`invoke_fork` 使用 `self` 的所有字段，而 `impl SubAgentTool` 在同一个 crate 内，访问控制是 `pub(crate)` 级别，不受模块边界影响。

---

## Task 3: 拆出 `invoke_background()` → `execute_bg.rs`

### What moves

`invoke_background()` L450-718，其内部:
- L470-474: fork 分发 → 调用 `self.invoke_background_fork`（保留为方法调用）
- L476-595: agent 构建 + hook + 注册（→ 现在委托 `with_built_agent`）
- L597-681: tokio::spawn 块（→ 委托 `spawn_bg_task`，见 Task 3.5）
- L683-718: registry.register + SubagentStarted + 返回消息

### Steps

1. 创建 `subagent/tool/execute_bg.rs`
2. 移动 `invoke_background` 方法
3. 由于 `invoke_background` 调用 `self.invoke_background_fork`，两方法需要在同一个 `impl SubAgentTool` 块内。解决：在 `execute_bg.rs` 中创建 `impl SubAgentTool { fn invoke_background(...); fn invoke_background_fork(...); }`，或者让 `invoke_background` 通过 trait/super 调用 `invoke_background_fork`

**推荐**: 两个 bg 方法放在同一文件 `execute_bg.rs`（一个 `impl SubAgentTool` 块），因为它们共享上下文且 `invoke_background` 调用 `invoke_background_fork`。如果后续 `invoke_background_fork` 太大再拆。

### Verify

```bash
cargo build -p peri-middlewares
cargo test -p peri-middlewares --lib -- background
```

### Risk

**Medium**. `invoke_background` 内部有 `tokio::spawn`，移动时需要保持所有 `clone()` 正确——spawn 闭包是 `'static`，所有数据必须 move 进去。

---

## Task 3.5 (Optional): 提取 `spawn_bg_task()` 消除 bg 路径重复

`invoke_background` 和 `invoke_background_fork` 的 `tokio::spawn` 闭包有相同模式:
1. 构造 `BackgroundTaskResult`
2. `registry.complete`
3. `fire_subagent_lifecycle_hooks_static(SubagentStop)`
4. `bg_event_sender.send`
5. `deregister_runtime`

**如果不做**: 两个 spawn 闭包保留各自的重复（~80 行），属于"后续优化"范畴。

**如果做**: 提取为独立函数（不是方法，是自由函数，避免 borrow checker 问题）:

```rust
async fn run_bg_child_and_notify(
    agent_builder: ReActAgent,
    state: &mut AgentState,
    input: AgentInput,
    cancel_token: Option<AgentCancellationToken>,
    task_id: String, agent_name: String, prompt_summary: String,
    child_thread_id: String,
    registry: Arc<BackgroundTaskRegistry>,
    hooks: Arc<Vec<RegisteredHook>>,
    cwd: String,
    bg_sender: Option<UnboundedSender<AgentEvent>>,
    thread_store: Option<Arc<dyn ThreadStore>>,
    deregister: Option<Arc<dyn Fn(&str) + Send + Sync>>,
    has_thread_store: bool,
) { ... }
```

**Tradeoff**: 14 个参数 vs 消除 80 行重复。建议 Task 3-4 完成后评估，不在本次实现。

---

## Task 4: 拆出 keyboard.rs 短路阶段

**目标**: 从 `handle_key_event()` 中提取 1-13 阶段的 if-return 块到 `keyboard/` 子模块。

### Structure

```
event/keyboard/
├── mod.rs           → pub use handle_key_event, handle_bar_key_event
├── shortcuts.rs     → handle_backtab + handle_bg_bar + handle_cycle_model + handle_cycle_provider
├── setup_wizard.rs  → handle_setup_wizard
├── panels.rs        → handle_panels (session + global dispatch)
├── popups.rs        → handle_oauth + handle_askuser + handle_hitl
├── bar_focus.rs     → handle_bar_focus + handle_focused_mode + handle_bar_key_event (从 keyboard.rs 底部移入)
└── normal_keys.rs   → handle_normal_keys (主 match 的 arm body)
```

### Contract

每个模块暴露一个或多个函数，签名:

```rust
pub(super) fn handle_xxx(app: &mut App, key_event: &KeyEvent, input: &Input) -> Option<Action>
// 或不需要 input 的:
pub(super) fn handle_xxx(app: &mut App, key_event: &KeyEvent) -> Option<Action>
```

返回 `Some(Action)` 表示已处理（调用方应 return），`None` 表示 fallthrough。

### Steps

1. 创建 `event/keyboard/` 目录
2. 创建 `event/keyboard/mod.rs`（仅 re-export）
3. 逐个创建子模块，每次只移动一个阶段
4. 更新 `keyboard.rs` 的 `handle_key_event()` 调用这些 `handle_xxx` 函数
5. 每次移动后 `cargo build -p peri-tui` 确认构建

### Each extraction is self-contained and verifiable

```bash
# 每完成一个模块:
cargo build -p peri-tui && cargo test -p peri-tui --lib -- keyboard
```

### Risk

**Low**. 纯函数提取。每个阶段的函数签名一致，调用方代码变为:

```rust
if let Some(action) = keyboard::shortcuts::handle(app, &key_event) {
    return Ok(Some(action));
}
```

---

## Task 5: 主 match arm body 提取 → `normal_keys.rs`

### What

将 `keyboard.rs` L546-1055 的主 `match input { ... }` 块中每个 arm 的 body 移入 `normal_keys.rs` 的独立函数。

### 关键不变式

- **match 结构和 arm 顺序不变**（仍在 `handle_key_event()` 或 `normal_keys::handle()` 中）
- **每个 arm 的 guard 条件不变**
- **Ctrl+C 的状态机逻辑不变**（loading → interrupt, not loading → double-tap quit）
- **Enter 的 5 路分发优先级不变**（@mention > hint > newline > submit/buffer）

### Arm → function mapping

| Match arm | → function | Lines |
|-----------|-----------|-------|
| Ctrl+C | `fn handle_ctrl_c(app) -> Option<Action>` | ~20 |
| Esc (loading) | `fn handle_esc_loading(app)` | ~8 |
| Esc (@mention) | `fn handle_esc_at_mention(app)` | ~6 |
| Up | `fn handle_up(app)` | ~40 |
| Down | `fn handle_down(app)` | ~50 |
| Ctrl+V | `fn handle_ctrl_v(app)` | ~25 |
| Tab | `fn handle_tab(app)` | ~30 |
| Enter (5 arms) | `fn handle_enter(app, input) -> Option<Action>` | ~80 |
| PageUp | `fn handle_pageup_vscode(app)` | ~10 |
| Ctrl+U | `fn handle_ctrl_u(app)` | ~18 |
| Ctrl+D | `fn handle_ctrl_d(app)` | ~6 |
| Delete | `fn handle_delete_attachment(app)` | ~7 |
| Ctrl+N | `fn handle_ctrl_n(app)` | ~3 |
| Ctrl+P | `fn handle_ctrl_p(app)` | ~3 |
| Ctrl+W | `fn handle_ctrl_w(app, input)` | ~12 |
| Other text | `fn handle_text_input(app, input)` | ~15 |
| Default (_) | 内联（1 行） | 1 |

### Steps

1. 创建 `event/keyboard/normal_keys.rs`
2. 逐个提取 arm body 函数（每次一个）
3. 更新 match 块调用提取的函数
4. 每次提取后 `cargo build -p peri-tui`

### Verify

```bash
cargo build -p peri-tui
cargo test -p peri-tui --lib -- keyboard
cargo test -p peri-tui --lib -- message_pipeline  # 间接相关
```

### Risk

**Low**. 每个 arm body 转换为独立函数，行为零变化（纯代码移动）。

**唯一难点**: 部分 arm body 使用了 `return` 提前返回。提取到独立函数后，提前返回变为 `return Some(Action)`。这等价于原来在 match 内的提前 `return`。

---

## Task 6 (Cleanup): 验证全量测试 + 文件大小确认

```bash
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets

# 确认文件大小
wc -l peri-middlewares/src/subagent/tool/define.rs
wc -l peri-tui/src/event/keyboard.rs
```

**期望**:
- `define.rs` ≤ 350 行
- `keyboard.rs` ≤ 200 行
- 新增 6 个子模块文件（每个 60-200 行），总行数 ≈ 原文件
- 零 clippy warning

---

## Execution Order

```
Task 0 ──→ Task 1 ──→ Task 2 ──→ Task 3 ──→ Task 4 ──→ Task 5 ──→ Task 6
  │                                             │
  └── 可并行：Task 0 是 keyboard，Task 1-3 是 define，互不依赖
```

Task 0 和 Task 1 可以并行开始（不同 crate）。但 Task 1 是 Task 2-3 的前提（`with_built_agent` 简化了 bg 路径）。

建议顺序执行（避免上下文切换），每做完一个 Task 都 build + test。

---

## Verification Checklist

在提交前确认:

- [ ] `define.rs` 中 `DeregisterGuard` 的 drop 在所有路径正确触发
- [ ] `with_built_agent` 的回调在 cascade 和 independent 两种 cancel policy 下行为正确
- [ ] background agent 的 `tokio::spawn` 闭包中所有 clone 正确（`'static` 边界）
- [ ] `keyboard.rs` 的 match arm 顺序未改变
- [ ] `keyboard.rs` 中 Ctrl+T/Ctrl+Shift+T/Alt+M/Alt+Shift+M 仍能正常切换
- [ ] `keyboard.rs` 中 Enter 的 5 路分发的相对优先级不变
- [ ] `cargo clippy --workspace --all-targets` 零 warning
- [ ] `cargo test --workspace` 全绿
