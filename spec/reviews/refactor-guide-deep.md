# define.rs + keyboard.rs 深度重构分析

> 上次指南提出拆分方案后，本次深入两文件完整源码，对每个执行路径做了逐行追踪，补充了共享模式、依赖拓扑、风险边界等细节。

---

## 1. `define.rs` — 执行路径拓扑

### 1.1 调用图

```
BaseTool::invoke() @ L995-1242
├── [dispatch] run_in_background && background_registry.is_some()
│   └── invoke_background() @ L450-719
│       ├── [dispatch] is_fork
│       │   └── invoke_background_fork() @ L721-941
│       └── [else] 内联 agent_def 构建 + tokio::spawn
├── [dispatch] is_fork
│   └── invoke_fork() @ L270-448
└── [else] 内联 normal sync path @ L1032-1241
```

### 1.2 四条路径的共享代码矩阵

| 操作 | normal | invoke_fork | invoke_bg | invoke_bg_fork |
|------|--------|-------------|-----------|----------------|
| UUID child_thread_id | L1044 | L284 | L551 | L741 |
| ThreadMeta + create_thread | L1046-1058 | L285-298 | L552-563 | L742-755 |
| load_agent_def | L1060 | — | L485 | — |
| filter_tools | L1065 | — | L490 | — |
| model_alias → llm_factory | L1070-1075 | L310 | L508-513 | L757 |
| max_iterations | L1076-1081 | L310(fixed 200) | L514-519 | L758(fixed 200) |
| build_subagent_middlewares | L1085 | L315 | L522 | L759 |
| with_system_prompt | L1092-1100 | L319-322 | L529-537 | L763-766 |
| register_tools | L1102-1104 | L324-327 | L539-541 | L768-770 |
| child_handler_factory / SourceAgentIdHandler | L1106-1114 | L329-337 | — | — |
| parent_messages 注入 | — | L275-281, L306-309 | — | L732-738, L818-821 |
| register_runtime + DeregisterGuard | L1143-1165 (cascade) | L354-381 (cascade) | L569-587 (independent) | L783-801 (independent) |
| SubagentStarted event | L1123-1129 | L339-345 | L700-706 | L922-928 |
| SubagentStart hook | L1130-1136 | L346-352 | L589-595 | L803-809 |
| agent_builder.execute() | L1173-1175 | L383-389 | L606-608 | L824-830 |
| SubagentStopped event | L1193-1200 | L401-408 | — | — |
| SubagentStop hook | L1201-1207 | L409-415 | L649-656 | L871-878 |
| update_thread_status(done/cancelled/error) | L1211-1239 | L419-446 | L640-645 | L861-867 |
| format_subagent_result | L1214 | L422 | — | — |
| registry.register | — | — | L683-690 | L905-912 |
| registry.complete | — | — | L647 | L869 |
| BackgroundTaskResult 构造 | — | — | L616-636 | L838-858 |
| bg_event_sender.send | — | — | L666 | L888 |
| deregister_runtime | — | — | L676-680 | L898-902 |

### 1.3 关键发现

**A. normal path 和 invoke_background 的 agent 构建是高度重复的**

```rust
// 以下 15 个步骤在 invoke_background(L485-541) 和 invoke(L1060-1104) 中完全一致:
1. load_agent_def(&agent_id, &cwd)
2. filter_tools(allowed, disallowed)
3. model_alias 解析（filter !is_empty && != "inherit"）
4. self.llm_factory(model_alias)
5. max_turns → max_iterations (0 → 200)
6. ReActAgent::new(llm).max_iterations(...)
7. build_subagent_middlewares(SubAgentMiddlewareConfig::for_agent_def(...))
8. for mw → agent_builder.add_middleware(mw)
9. if system_builder → overrides_from_agent_def → builder(...) → with_system_prompt
10. for tool → agent_builder.register_tool(tool)
11. child_handler_factory / SourceAgentIdHandler (bg path 缺少此项!)
12. AgentState::new → with_persistence
13. SubagentStarted event
14. SubagentStart hook
15. register_runtime + DeregisterGuard
```

**这意味着可以提取一个 `build_agent_from_def()` → `(ReActAgent, AgentState)` 的公共函数**，同时为 bg 和 sync 路径共享。

**B. invoke_background 和 invoke_background_fork 共享整个 `tokio::spawn` 模式**

```rust
// 两个函数从 "注册 registry" 到 "deregister" 的模式完全相同：
1. registry.register(BackgroundTask { id, name, summary, Running, started_at, abort_handle })
2. event_handler.on_event(SubagentStarted { ... is_background: true })
3. Ok(format!("Background task {} started ..."), task_id)
// spawn 内部:
4. background_registry.complete(&task_id, result)
5. fire_subagent_lifecycle_hooks_static(SubagentStop, ...)
6. bg_event_sender.send(BackgroundTaskCompleted(result))
7. deregister_runtime(&child_thread_id)
```

**可以提取 `spawn_background_child()` 消除 ~120 行重复**。

**C. cancel policy 分化是故意的**

| 路径 | cancel policy | cancel token 来源 |
|------|--------------|-------------------|
| normal sync | `"cascade"` | `self.cancel.as_ref().map(\|t\| t.child_token())` |
| invoke_fork | `"cascade"` | 同上 |
| invoke_background | `"independent"` | `AgentCancellationToken::new()` |
| invoke_background_fork | `"independent"` | 同上 |

Fork/background_fork 的 fork_directive 也不同：fork 路径用 `build_fork_directive(prompt)`，background fork 复用了这一行。

### 1.4 拆分方案（修正版）

```
subagent/tool/
├── mod.rs                    → 已有，保持
├── define.rs                 → ~350 行（struct + builder + BaseTool impl + 公共辅助）
├── build_agent.rs            → ~200 行（从 agent_def 构建 ReActAgent + AgentState）
├── execute_fork.rs           → ~180 行（invoke_fork）
├── execute_bg_spawn.rs       → ~130 行（tokio::spawn 公共模版）
├── execute_bg.rs             → ~180 行（invoke_background，委托 build_agent + execute_bg_spawn）
└── execute_bg_fork.rs        → ~220 行（invoke_background_fork，委托 execute_bg_spawn）
```

**新增 `build_agent.rs` 是关键**：从 normal path 和 invoke_background 中提取重复的 15 步构建序列。

```rust
// build_agent.rs
pub(crate) struct BuiltAgent {
    pub builder: ReActAgent,
    pub state: AgentState,
    pub child_thread_id: String,
}

impl SubAgentTool {
    pub(crate) async fn build_from_agent_def(
        &self,
        agent_def: &ClaudeAgent,
        cwd: &str,
        cancel_policy: &str,  // "cascade" | "independent"
    ) -> Result<BuiltAgent, String> {
        // 合并原 L1060-1104 和 L485-537
        let child_thread_id = uuid::Uuid::now_v7().to_string();
        // ... ThreadMeta, with_persistence ...
        // ... model_alias, llm_factory, max_iterations ...
        // ... middlewares, system_builder, tools ...
        // ... register_runtime + DeregisterGuard ...
        // ... SubagentStarted + SubagentStart hook ...
        Ok(BuiltAgent { builder, state, child_thread_id })
    }
}
```

**代价**：`DeregisterGuard` 需要在 `BuiltAgent` 中携带（或用 `ManuallyDrop` 模式），因为 AgentState 和 DeregisterGuard 生命周期绑定。

**替代方案（更安全）**：不返回 BuiltAgent struct，改为回调模式：

```rust
pub(crate) async fn with_built_agent<F, Fut>(
    &self,
    agent_def: &ClaudeAgent,
    cwd: &str,
    cancel_policy: CancelPolicy,
    f: F,
) -> Result<String, ...>
where
    F: FnOnce(ReActAgent, &mut AgentState, AgentCancellationToken) -> Fut,
    Fut: Future<Output = Result<AgentOutput, AgentError>>,
```

**建议**：回调模式虽然函数签名复杂，但避免了 DeregisterGuard 跨边界传递的问题，更符合当前 panic-safe RAII 模式。

---

## 2. `keyboard.rs` — 优先级链分析

### 2.1 完整的优先级拓扑

`handle_key_event()` 的执行顺序决定了优先级：

```
1. Release filter (L119-121)        → 丢弃
2. Bar focus mode (L123-131)        → delegate to handle_bar_key_event
3. Focused-only mode (L133-148)     → 仅 Esc 退出
4. BackTab = perm cycle (L153-158)
5. Ctrl+B = bg bar (L161-171)
6. Model cycling (L174-196)         Ctrl+T / Alt+M
7. Provider cycling (L199-231)      Ctrl+Shift+T / Alt+Shift+M
   ↑ L233-288: DUPLICATE blocks of 6-7 with same logic!
8. Setup wizard (L293-361)
9. PanelManager session panels (L365-409)
10. PanelManager global panels (L411-446)
11. OAuth prompt (L448-452)
12. AskUser popup (L455-507)
13. HITL popup (L509-544)
14. main match {} (L546-1055):
    ├── Ctrl+C (quit/interrupt state machine)
    ├── Esc (loading / @mention)
    ├── Up/Down (4-way: @mention → hint → history → textarea)
    ├── Ctrl+V (clipboard)
    ├── Tab (@mention/hints)
    ├── Enter (5-way dispatch)
    ├── PageUp (VS Code workaround)
    ├── Ctrl+U/D (scroll/delete_line)
    ├── Delete (attachment)
    ├── Ctrl+N/P (session switch)
    ├── Ctrl+W (close session)
    └── General text input
```

### 2.2 关键发现

**A. 快捷键重复定义（L174-196 vs L233-255 vs L198-231 vs L258-288）**

Ctrl+T 和 Ctrl+Shift+T 的模型/Provider 切换逻辑各出现了**两次**，且第二次（L233-288）是第一次的完整副本。这是历史残留——Alt+M/Alt+Shift+M 的 macOS 兼容路径（L174-196, L199-231）和 Ctrl+T/Ctrl+Shift+T 的跨平台路径（L233-288）合并后没有清理重复。

```rust
// L174-196: Ctrl+T / Alt+M → cycle model (第一次)
// L233-255: Ctrl+T → cycle model (第二次，重复代码)
// L199-231: Ctrl+Shift+T / Alt+Shift+M → cycle provider (第一次)
// L258-288: Ctrl+Shift+T → cycle provider (第二次，重复代码)
```

**修复**：删除 L233-288 的重复块。Ctrl+T/Ctrl+Shift+T 在 L174-231 已覆盖。

**B. 优先级链 1-13 是严格的 if-return 模式**

每个阶段的函数签名应该是：

```rust
fn handle_xxx(app: &mut App, key_event: &KeyEvent, input: &Input) -> Option<Action>
```

返回 `Some(Action)` 表示已处理（调用方应 return），`None` 表示 fallthrough 到下一阶段。

**C. 14 号区块（主 match）是真正的挑战**

这个 match 块的 14 个 arm 有复杂的 guard 条件和隐式优先级。但仔细观察，每个 arm 处理的是一组语义相关的按键，且 arm 之间互斥（Rust match 保证一次只匹配一个 arm）。

**可以按按键分组提取，但保持 match 结构不变**：

```rust
match input {
    Input { key: Key::Char('c'), ctrl: true, .. } => handle_ctrl_c(app),
    Input { key: Key::Esc, .. } if loading => handle_esc_loading(app),
    Input { key: Key::Esc, .. } if at_mention_active => handle_esc_at_mention(app),
    Input { key: Key::Up, .. } => handle_up(app),
    Input { key: Key::Down, .. } => handle_down(app),
    Input { key: Key::Char('v'), ctrl: true, .. } if !loading => handle_ctrl_v(app),
    // ... etc
}
```

**关键**：这种重构不改变优先级拓扑。Rust 的 match 语义保证 arm 顺序 = 优先级。我们只是在每个 arm 的 body 处调用一个独立函数，不改变匹配顺序。

**D. Enter 的 5 路分发是目前最复杂的单 arm**

```rust
Input { key: Key::Enter, .. } 有 5 个 guard 分支：
1. !loading && at_mention.active && !candidates.is_empty() → inject_at_mention_path
2. !loading && hint_count > 0 → hint_complete
3. shift || alt → newline
4. 无 guard → submit/buffer
```

这里不能简单拆成独立函数，因为 5 个分支的优先级由 Rust 的 match arm 顺序决定。但如果把整个 Enter 处理提取为一个函数：

```rust
fn handle_enter(app: &mut App, input: Input) -> Option<Action> {
    if !loading && at_mention.active && !candidates.is_empty() {
        inject_at_mention_path(app);
        return Some(Action::Redraw);
    }
    if !loading && hint_count > 0 { ... }
    if input.shift || input.alt { ... }
    // fallthrough: submit/buffer
    Some(submit_or_buffer(app))
}
```

### 2.3 拆分方案（修正版）

```
event/
├── mod.rs                     → 已有
├── keyboard.rs                → ~200 行（入口 + KeyBinding + 标签函数）
└── keyboard/
    ├── mod.rs                 → re-export handle_key_event
    ├── shortcuts.rs           → ~80 行（模型/Provider 切换）
    ├── setup_wizard.rs        → ~70 行
    ├── panels.rs              → ~85 行（PanelManager dispatch）
    ├── popups.rs              → ~120 行（OAuth/AskUser/HITL）
    ├── normal_keys.rs         → ~500 行（主 match 的各 arm 处理函数）
    ├── textarea.rs            → ~250 行（Up/Down dispatch + Enter dispatch + text input）
    └── bar_focus.rs           → ~60 行（handle_bar_key_event + Bar focus 拦截）
```

**Stage 0（预热，不是拆分）**：删除 L233-288 重复的 model/provider 切换代码。预计 -56 行，零风险。

**Stage 1：短路阶段提取（L123-544）**

难度最低，因为每个阶段都是 `if condition { handle... return }` 模式：

```
keyboard.rs::handle_key_event()
├── L123-131: → bar_focus::check(app, key_event)   → Option<Action>
├── L133-148: → focused_mode::check(app, key_event)  → Option<Action>
├── L150-158: → 内联（BackTab 只有 5 行）
├── L160-171: → shortcuts::handle_bg_bar(...)
├── L173-288: → shortcuts::handle_cycle(...)  [修复重复后]
├── L293-361: → setup_wizard::handle(...)
├── L365-446: → panels::handle(...)
├── L448-452: → popups::handle_oauth(...)
├── L455-507: → popups::handle_askuser(...)
├── L509-544: → popups::handle_hitl(...)
└── L546-1055: → normal_keys::handle(app, input)
```

**Stage 2：主 match 提取（L546-1055）**

不是拆 match 本身，而是把每个 arm 的 body 移入 `normal_keys.rs` 中的独立函数。这样可以保持 match 结构和 arm 顺序不变。

```rust
// normal_keys.rs 导出：
pub(super) fn handle(app: &mut App, input: Input) -> Result<Option<Action>> {
    match input {
        Input { key: Key::Char('c'), ctrl: true, .. } => ctrl_c(app),
        Input { key: Key::Esc, .. } if loading        => esc_loading(app),
        Input { key: Key::Esc, .. } if at_mention     => esc_at_mention(app),
        Input { key: Key::Up, .. }                    => key_up(app),
        Input { key: Key::Down, .. }                  => key_down(app),
        // ...
        Input { key: Key::Enter, .. } if cond1        => enter_at_mention(app),
        Input { key: Key::Enter, .. } if cond2        => enter_hint(app),
        Input { key: Key::Enter, .. } if shift_or_alt => enter_newline(app),
        Input { key: Key::Enter, .. }                 => enter_submit(app),
        // ...
        input if input.key != Key::Enter              => text_input(app, input),
        _ => { app.quit_pending_since = None; }
    }
    Ok(Some(Action::Redraw))
}
```

### 2.4 风险矩阵

| 操作 | 风险 | 原因 |
|------|------|------|
| 删除 L233-288 重复代码 | **零** | 逻辑与 L173-231 完全相同，两个 if 块各自执行，上一个未命中才到下一个。但实际 L174-196 和 L233-255 的 match 条件**完全相同**！也就是说 L233-255 是 dead code。L174-196 已经拦截了 Ctrl+T 和 Alt+M |
| 提取 1-13 的短路阶段 | **低** | 纯函数提取，签名 `fn handle(app, key) -> Option<Action>` |
| 主 match arm body 提取 | **低** | 不改变 match 结构和 arm 顺序 |
| Enter 5 路分发提取 | **中** | guard 条件有复杂依赖，但提取为独立函数后顺序仍由调用方 match 控制 |
| Up/Down 4 路分发提取 | **低** | 已清晰分离（@mention → hint → history → textarea） |

### 2.5 重复代码确认

**L174-196 和 L233-255 是完全重复**：

```
L174: if SHORTCUT_CTRL_CYCLE_MODE.matches(&key_event) || SHORTCUT_CYCLE_MODE.matches(&key_event)
L233: if SHORTCUT_CTRL_CYCLE_MODE.matches(&key_event)
```

L174 的条件是 L233 的**超集**（多了一个 `|| SHORTCUT_CYCLE_MODE`）。所以 L233-255 **永远不会被执行**（Ctrl+T 在 L174 就被拦截了，Alt+M 也在 L174 被拦截了）。

同样 L199-231 和 L258-288：L199 的条件也包含 `|| SHORTCUT_CYCLE_PROVIDER`，所以 L258 的 `SHORTCUT_CTRL_CYCLE_PROVIDER` 在 L199 已被拦截。

**结论**：L233-288 四块（共 56 行）是纯死代码，可以直接删除。这是 126 次提交中合并了两个快捷键方案但没删旧代码的残留。

---

## 3. 执行顺序建议（修正）

| 优先级 | 文件 | 操作 | 难度 | 风险 | 收益 |
|--------|------|------|------|------|------|
| **0** | `keyboard.rs` | **删除 L233-288 重复快捷键块** | 无 | 零 | -56 行 |
| 1 | `define.rs` | 提取 `build_from_agent_def()` 公共函数 | 中 | 中 | -250 行重复 |
| 2 | `define.rs` | 拆出 `invoke_fork()` → `execute_fork.rs` | 低 | 低 | -179 行 |
| 3 | `define.rs` | 拆出 `invoke_background()` → `execute_bg.rs` | 中 | 中 | -270 行 |
| 4 | `define.rs` | 拆出 `invoke_background_fork()` → `execute_bg_fork.rs` | 中 | 中 | -222 行 |
| 5 | `keyboard.rs` | 提取 1-13 短路阶段 → 独立文件 | 低 | 低 | -400 行 |
| 6 | `keyboard.rs` | 主 match arm body 提取 → `normal_keys.rs` | 低 | 低 | -400 行 |

**总预估**：define.rs 1242 → ~350 行，keyboard.rs 1222 → ~200 行，总计消除 ~1900 行冗余，新增 6 个模块文件（每个 60-200 行）。

---

## 4. 不应做的事

- **不要动 DeregisterGuard** — 它是 panic-safe 的关键守卫，拆分时必须在每个路径中保留
- **不要改变 cancel policy** — `"cascade"` vs `"independent"` 的分化是故意的，与 parent-child 生命周期相关
- **不要动 keyboard.rs 的 match arm 顺序** — 这是隐式优先级约定，任何重排都是行为变更
- **不要合并 invoke_fork 和 invoke_background_fork** — 共享模式只有注册部分（Stage 3-4 已处理），执行部分（sync await vs tokio::spawn）不能合并
- **不要在 define.rs 提取公共函数时用 &self 的 async 回调** — `AgentState` 和 `DeregisterGuard` 的生命周期绑定在调用栈上，回调模式需要仔细设计所有权传递
