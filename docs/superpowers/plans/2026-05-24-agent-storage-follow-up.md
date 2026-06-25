# Agent Storage Follow-up — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Three follow-up features for the unified agent storage system, in priority order: (1) cancel propagation, (2) compact per-thread, (3) /tasks panel.

**Spec:** `docs/superpowers/specs/2026-05-24-agent-storage-follow-up.md`

---

## File Structure

| File | Responsibility |
|------|---------------|
| `peri-middlewares/src/subagent/mod.rs` | 新增 register/deregister callback |
| `peri-middlewares/src/subagent/tool/define.rs` | 子 agent 注册 AgentRuntime |
| `peri-acp/src/session/executor.rs` | cancel 触发级联取消 |
| `peri-acp/src/session/mod.rs` | close_session 清理 |
| `peri-agent/src/agent/state.rs` | 新增 ancestor_len |
| `peri-agent/src/agent/compact/micro.rs` | micro-compact 限制范围 |
| `peri-middlewares/src/compact_middleware.rs` | full-compact 限制范围 + invalidate cache |
| `peri-tui/src/app/panel_manager.rs` | 新增 Tasks PanelKind/PanelState |
| `peri-tui/src/app/panels/tasks/` | 新增面板目录 |
| `peri-tui/src/app/commands/panel/tasks.rs` | /tasks 命令 |

---

### Task 1: AgentRuntime 注册/注销机制

**Files:**
- Modify: `peri-middlewares/src/subagent/mod.rs`
- Modify: `peri-middlewares/src/subagent/tool/define.rs`

- [ ] **Step 1: 在 SubAgentMiddleware 中添加 register/deregister callback**

在 `SubAgentMiddleware` struct 中新增：
```rust
register_runtime: Option<Arc<dyn Fn(String, CancellationToken, String) + Send + Sync>>,
// 参数: (thread_id, cancel_token, cancel_policy)
deregister_runtime: Option<Arc<dyn Fn(&str) + Send + Sync>>,
```

添加 builder 方法 `with_register_runtime()` / `with_deregister_runtime()`。

在 `build_tool()` 中传递给 SubAgentTool。

- [ ] **Step 2: 在 SubAgentTool 中使用 callback**

SubAgentTool 新增相同字段。在子 agent 执行前调用 register，执行后调用 deregister：

```rust
// 创建子 agent 后（thread 创建成功后）
if let Some(ref register) = self.register_runtime {
    let cancel_token = CancellationToken::new();
    register(child_thread_id.clone(), cancel_token.clone(), cancel_policy.clone());
    // 注意：cancel_token 需要传入 agent 执行，用于取消控制
}

// 子 agent 执行结束后（无论成功/失败/取消）
if let Some(ref deregister) = self.deregister_runtime {
    deregister(&child_thread_id);
}
```

- [ ] **Step 3: 确保 deregister 在所有退出路径调用**

用 `scopeguard` 或手动 ensure 模式保证 register 后必有 deregister（包括 panic 路径）。

- [ ] **Step 4: 编译验证**

Run: `cargo build`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git commit -m "feat(agent-storage): SubAgentMiddleware register/deregister AgentRuntime callbacks"
```

---

### Task 2: Executor Cancel 级联

**Files:**
- Modify: `peri-acp/src/session/executor.rs`
- Modify: `peri-acp/src/session/mod.rs`

- [ ] **Step 1: execute_prompt 新增 session 引用参数**

`execute_prompt()` 新增参数 `session_manager: Option<SessionManager>` 和 `session_id: String`（session_id 已有）。用于构建 register/deregister callback。

- [ ] **Step 2: 构建 register/deregister closure**

在 `execute_prompt()` 中，构建 SubAgentMiddleware 之前：

```rust
let sm_clone = session_manager.clone();
let sid_clone = session_id.clone();
let register = Arc::new(move |thread_id: String, cancel_token: CancellationToken, policy: String| {
    if let Some(sm) = &sm_clone {
        if let Some(session) = sm.get_session(&sid_clone) {
            let runtime = AgentRuntime::new(thread_id.clone(), CancelPolicy::from_str(&policy));
            // 覆盖 cancel_token
            let mut rt = runtime;
            // AgentRuntime 的 cancel_token 需要从外部注入
            session.active_agents.insert(thread_id, runtime);
        }
    }
});
let sm_clone2 = session_manager.clone();
let sid_clone2 = session_id.clone();
let deregister = Arc::new(move |thread_id: &str| {
    if let Some(sm) = &sm_clone2 {
        if let Some(mut session) = sm.get_session_mut(&sid_clone2) {
            session.active_agents.remove(thread_id);
        }
    }
});
```

注意：`DashMap::get_mut` 需要确认 API 可用性。如果 `get_session` 只返回不可变引用，需要新增 `get_session_mut` 方法。

- [ ] **Step 3: cancel 触发级联**

在 `execute_prompt()` 返回 `PromptStopReason::Cancelled` 时：

```rust
if stop_reason == PromptStopReason::Cancelled {
    if let Some(sm) = &session_manager {
        if let Some(session) = sm.get_session(&session_id) {
            session.cancel_cascade_children(&thread_id);
        }
    }
}
```

- [ ] **Step 4: cancel_session 适配**

`cancel_session()` 中，取消 session token 后，级联取消所有 agent：

```rust
pub fn cancel_session(&self, session_id: &str) {
    if let Some(mut session) = self.inner.sessions.get_mut(session_id) {
        session.cancel_token.cancel();
        session.cancel_token = CancellationToken::new();
        // 级联取消 cascade 子 agent
        session.cancel_cascade_children(/* 需要 parent thread_id — 取消所有 cascade 的 */);
        // ... 现有 pending_requests 清理
    }
}
```

注意：`cancel_cascade_children` 当前需要 `parent_thread_id` 参数。cancel_session 场景下应该取消所有 cascade 策略的 agent。可能需要改为 `cancel_cascade_all()` 无参数版本。

- [ ] **Step 5: 编译验证**

Run: `cargo build`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git commit -m "feat(agent-storage): executor cancel cascades to child agents"
```

---

### Task 3: AgentState ancestor_len

**Files:**
- Modify: `peri-agent/src/agent/state.rs`

- [ ] **Step 1: 添加 ancestor_len 字段**

```rust
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct AgentState {
    // 现有字段...
    #[serde(skip)]
    ancestor_len: usize,
}
```

- [ ] **Step 2: 添加 accessor 和 setter**

```rust
impl AgentState {
    pub fn ancestor_len(&self) -> usize { self.ancestor_len }
    
    pub fn with_ancestor_len(mut self, len: usize) -> Self {
        self.ancestor_len = len;
        self
    }
}
```

- [ ] **Step 3: 更新 with_thread_context**

```rust
pub async fn with_thread_context(
    thread_id: ThreadId,
    store: Arc<dyn ThreadStore>,
) -> Result<Self> {
    let meta = store.load_meta(&thread_id).await?;
    let all_messages = store.load_context(&thread_id).await?;
    let own_messages = store.load_messages(&thread_id).await?;
    let ancestor_len = all_messages.len().saturating_sub(own_messages.len());

    Ok(Self::new(&meta.cwd)
        .with_messages_from(all_messages)
        .with_ancestor_len(ancestor_len)
        .with_persistence(store, thread_id))
}
```

`new()` 和 `with_messages()` 路径：`ancestor_len` 默认 0（`Default` trait）。

- [ ] **Step 4: 在 State trait 中暴露**

```rust
pub trait State: Send + Sync + Clone + 'static {
    // 现有方法...
    fn ancestor_len(&self) -> usize { 0 }
}
```

默认实现返回 0，不破坏现有实现。`AgentState` 覆盖。

- [ ] **Step 5: 编译验证**

Run: `cargo build`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git commit -m "feat(agent-storage): AgentState.ancestor_len for compact boundary"
```

---

### Task 4: Compact per-thread 适配

**Files:**
- Modify: `peri-middlewares/src/compact_middleware.rs`（full compact）
- Modify: `peri-agent/src/agent/compact/micro.rs`（micro compact）

- [ ] **Step 1: full compact 限制范围**

在 `CompactMiddleware::before_model` 的 full compact 分支中：

```rust
// 替换 messages 前，保留祖先消息
let ancestor_len = state.ancestor_len();
let own_messages: Vec<BaseMessage> = state.messages_mut().drain(ancestor_len..).collect();

// 现有 compact 逻辑对 own_messages 执行
// ...

// 重建：祖先消息 + compacted
state.messages_mut().truncate(ancestor_len);
state.messages_mut().extend(compacted_messages);
```

- [ ] **Step 2: compact 后 invalidate cache**

在 compact 完成、messages 替换后：

```rust
// 失效物化缓存
if let (Some(store), Some(thread_id)) = (state.store(), state.thread_id()) {
    if let Err(e) = store.invalidate_context_cache(thread_id).await {
        tracing::warn!("failed to invalidate context cache after compact: {e}");
    }
}
```

注意：需要从 AgentState 暴露 `store()` 和 `thread_id()` 的 accessor。当前这些字段是私有的。需要在 State trait 中添加或在 AgentState 中添加公开 accessor。

- [ ] **Step 3: micro compact 限制范围**

在 `micro_compact` 函数中，迭代 `messages` 时跳过前 `ancestor_len` 条：

```rust
for i in (ancestor_len..messages.len()).rev() {
    // 只处理自身消息中的 tool_result
}
```

- [ ] **Step 4: AgentState 暴露 store/thread_id**

在 `state.rs` 中添加：

```rust
impl AgentState {
    pub fn store(&self) -> Option<&Arc<dyn ThreadStore>> { self.store.as_ref() }
    pub fn thread_id(&self) -> Option<&ThreadId> { self.thread_id.as_ref() }
}
```

- [ ] **Step 5: 编译验证**

Run: `cargo build`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git commit -m "feat(agent-storage): compact only operates on own messages, invalidates cache"
```

---

### Task 5: /tasks 面板 — PanelKind 注册

**Files:**
- Modify: `peri-tui/src/app/panel_manager.rs`

- [ ] **Step 1: 添加 Tasks variant**

在 `PanelKind` enum 中添加 `Tasks`。
在 `PanelState` enum 中添加 `Tasks(TasksPanel)`。
在 `PanelKind::scope()` 中添加 `Tasks => PanelScope::Session`。
在 `PanelKind::mutex_group()` 中添加 `Tasks => MutexGroup::Tools`。

- [ ] **Step 2: 编译验证**

Run: `cargo build -p peri-tui`
Expected: FAIL（TasksPanel 未定义，Task 6 定义）

- [ ] **Step 3: Commit（与 Task 6 一起）**

---

### Task 6: /tasks 面板 — TasksPanel 实现

**Files:**
- Create: `peri-tui/src/app/panels/tasks/mod.rs`
- Create: `peri-tui/src/app/panels/tasks/agent_list.rs`
- Create: `peri-tui/src/app/panels/tasks/agent_detail.rs`
- Create: `peri-tui/src/app/panels/tasks/cron_list.rs`
- Create: `peri-tui/src/app/commands/panel/tasks.rs`

- [ ] **Step 1: 定义 TasksPanel state**

```rust
// panels/tasks/mod.rs
use super::{PanelComponent, PanelKind, EventResult, panel_list::PanelList};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TasksTab {
    AgentThreads,
    CronTasks,
}

pub struct AgentThreadEntry {
    pub thread_id: String,
    pub title: String,
    pub status: String,  // "active" | "done" | "cancelled" | "error"
    pub is_active: bool,
}

pub struct TasksPanel {
    pub tab: TasksTab,
    pub agent_list: PanelList<AgentThreadEntry>,
    pub detail_thread_id: Option<String>,  // Enter 后进入详情视图
    pub detail_messages: Option<Vec<BaseMessage>>,
    pub cron_list: PanelList<CronTask>,
    pub confirm_delete: bool,
}
```

- [ ] **Step 2: 实现 PanelComponent trait**

```rust
impl PanelComponent for TasksPanel {
    fn kind(&self) -> PanelKind { PanelKind::Tasks }

    fn handle_key(&mut self, input: Input, ctx: &mut PanelContext) -> EventResult {
        match input {
            Input::Key(KeyEvent { code: Esc, .. }) => {
                if self.detail_thread_id.is_some() {
                    self.detail_thread_id = None;
                    self.detail_messages = None;
                    return EventResult::Consumed;
                }
                return EventResult::ClosePanel;
            }
            Input::Key(KeyEvent { code: Left, .. }) | Input::Key(KeyEvent { code: Char('h'), .. }) => {
                self.tab = match self.tab {
                    TasksTab::AgentThreads => TasksTab::CronTasks,
                    TasksTab::CronTasks => TasksTab::AgentThreads,
                };
                return EventResult::Consumed;
            }
            Input::Key(KeyEvent { code: Right, .. }) | Input::Key(KeyEvent { code: Char('l'), .. }) => {
                // 同上，方向相反
            }
            // Tab 切换
            Input::Key(KeyEvent { code: Tab, .. }) => { ... }
            // Enter 进入详情（Agent Threads tab）
            Input::Key(KeyEvent { code: Enter, .. }) if self.tab == TasksTab::AgentThreads => {
                if let Some(entry) = self.agent_list.selected() {
                    self.detail_thread_id = Some(entry.thread_id.clone());
                    // 懒加载在 render 时通过 store.load_messages()
                }
            }
            // 上下导航
            Input::Key(KeyEvent { code: Up, .. }) => { ... }
            Input::Key(KeyEvent { code: Down, .. }) => { ... }
            _ => EventResult::NotConsumed,
        }
    }

    fn desired_height(&self, _screen_height: u16, _screen_width: u16) -> u16 {
        14  // 与 Cron 面板类似
    }

    fn render(&mut self, f: &mut Frame, app: &mut App, area: Rect) {
        match self.tab {
            TasksTab::AgentThreads => {
                if self.detail_thread_id.is_some() {
                    render_agent_detail(f, self, app, area);
                } else {
                    render_agent_list(f, self, app, area);
                }
            }
            TasksTab::CronTasks => {
                render_cron_list(f, self, app, area);
            }
        }
    }
}
```

- [ ] **Step 3: Agent Threads 列表渲染**

```rust
// agent_list.rs
pub fn render_agent_list(f: &mut Frame, panel: &mut TasksPanel, app: &mut App, area: Rect) {
    // Tab header
    let tabs = Line::from(vec![
        Span::styled(" Agent Threads ", active_tab_style),
        Span::raw(" │ "),
        Span::styled(" Cron Tasks ", inactive_tab_style),
    ]);

    // 列表：活跃在前，非活跃在后
    for (i, entry) in panel.agent_list.items().iter().enumerate() {
        let status_icon = match entry.status.as_str() {
            "active" => "●",
            "done" => "✓",
            "cancelled" => "✗",
            "error" => "!",
            _ => "○",
        };
        let line = format!("{} {} [{}]", status_icon, entry.title, entry.status);
        // ... 渲染
    }
}
```

- [ ] **Step 4: 懒加载详情渲染**

```rust
// agent_detail.rs
pub fn render_agent_detail(f: &mut Frame, panel: &mut TasksPanel, app: &mut App, area: Rect) {
    // 首次渲染时懒加载
    if panel.detail_messages.is_none() {
        if let Some(ref thread_id) = panel.detail_thread_id {
            if let Some(store) = app.get_thread_store() {
                if let Ok(msgs) = store.load_messages(thread_id) {
                    panel.detail_messages = Some(msgs);
                }
            }
        }
    }
    // 渲染消息列表（只读模式）
    // 复用 messages_to_view_models 转换或简单文本渲染
}
```

- [ ] **Step 5: Cron Tasks tab（迁移自现有 cron 面板）**

将 `panels/cron.rs` 的渲染逻辑提取到 `tasks/cron_list.rs`。数据源不变。

- [ ] **Step 6: 注册 /tasks 命令**

```rust
// commands/panel/tasks.rs
pub struct TasksCommand;

impl Command for TasksCommand {
    fn name(&self) -> &str { "tasks" }
    fn description(&self, lc: &LcRegistry) -> String {
        lc.tr("command-tasks-description")
    }
    fn execute(&self, app: &mut App, _args: &str) {
        app.open_tasks_panel();
    }
}
```

在 `app/mod.rs` 中添加 `open_tasks_panel()` 方法：

```rust
pub fn open_tasks_panel(&mut self) {
    // 加载 agent threads 数据
    let entries = self.load_agent_thread_entries();
    // 加载 cron tasks
    let cron_tasks = self.services.cron.scheduler.lock().list_tasks()...;
    
    let panel = TasksPanel::new(entries, cron_tasks);
    self.open_panel(PanelState::Tasks(panel));
}
```

- [ ] **Step 7: i18n**

在 `locales/zh-CN/main.ftl` 和 `locales/en/main.ftl` 中添加：
```
command-tasks-description = 查看 agent 线程和定时任务
tasks-tab-agents = Agent 线程
tasks-tab-cron = 定时任务
```

- [ ] **Step 8: 编译验证**

Run: `cargo build -p peri-tui`
Expected: PASS

- [ ] **Step 9: Commit（与 Task 5 一起）**

```bash
git commit -m "feat(tui): /tasks panel with Agent Threads + Cron Tasks tabs"
```

---

### Task 7: 集成测试 + 全量验证

- [ ] **Step 1: Cancel propagation 测试**

验证：
- 子 agent 创建后 active_agents 包含该 thread_id
- 子 agent 完成后从 active_agents 移除
- 父 cancel 触发 cascade 子 agent 取消
- independent 子 agent 不受影响

- [ ] **Step 2: Compact per-thread 测试**

验证：
- ancestor_len 正确计算
- compact 后祖先消息不变
- cached_context 被清空

- [ ] **Step 3: 全量测试**

Run: `cargo test`
Expected: ALL PASS

- [ ] **Step 4: Clippy**

Run: `cargo clippy -- -D warnings`
Expected: 0 warnings

- [ ] **Step 5: Commit**

```bash
git commit -m "test(agent-storage): cancel propagation + compact per-thread integration tests"
```

---

## Self-Review

### Spec Coverage

| Spec Section | Task |
|-------------|------|
| 1. Cancel Propagation | Task 1 (callback) + Task 2 (executor) |
| 2. Compact per-thread | Task 3 (ancestor_len) + Task 4 (compact adaptation) |
| 3. /tasks Panel | Task 5 (PanelKind) + Task 6 (panel implementation) |

### Dependency Chain

```
Task 1 (register/deregister) → Task 2 (executor cancel)
Task 3 (ancestor_len) → Task 4 (compact adaptation)
Task 5 (PanelKind) + Task 6 (panel) → 可以并行但建议顺序
Task 7 (integration tests) → 依赖所有
```

Tasks 1-2 和 Tasks 3-4 是独立的，可以并行。
