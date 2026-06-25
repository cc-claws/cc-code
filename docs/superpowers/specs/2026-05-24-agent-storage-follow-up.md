# Agent 统一存储 — 后续实现

> 日期：2026-05-24
> 状态：Draft
> 前置 spec：`2026-05-24-unified-agent-storage-design.md`

## 概述

基于已完成的 agent 统一存储核心机制（ThreadMeta 扩展、schema migration、SubAgentTool 创建子 thread），三个待做项按优先级排列：

1. **Cancel Propagation** — 接上取消链，防止子 agent 资源泄漏
2. **Compact per-thread** — 区分祖先消息与自身消息，compact 只压缩自身
3. **/tasks 面板** — TUI 新增面板，含 Agent Threads tab + Cron Tasks tab

---

## 1. Cancel Propagation

### 1.1 问题

`AcpSession` 已有 `active_agents: HashMap<ThreadId, AgentRuntime>` 和 `cancel_all_agents()` / `cancel_cascade_children()` 方法，但从未被调用。当前 SubAgentTool 创建子 agent 后，AgentRuntime 未注册到 session，取消时不传播。

### 1.2 设计

#### AgentRuntime 注册/注销

SubAgentTool 不持有 AcpSession 引用。通过 callback 机制桥接：

```rust
// SubAgentMiddleware 新增字段
register_runtime: Option<Arc<dyn Fn(ThreadId, AgentRuntime) + Send + Sync>>,
deregister_runtime: Option<Arc<dyn Fn(&ThreadId) + Send + Sync>>,
```

SubAgentTool 在创建子 agent 后调用 `register_runtime(child_thread_id, runtime)`；子 agent 执行结束后调用 `deregister_runtime(&child_thread_id)`。

在 `execute_prompt()` 构建 SubAgentMiddleware 时，传入 closure 捕获 AcpSession 引用：

```rust
let session = session_clone;
let register = Arc::new(move |thread_id: ThreadId, runtime: AgentRuntime| {
    if let Some(s) = session.get_session(&session_id) {
        s.active_agents.insert(thread_id, runtime);
    }
});
let deregister = Arc::new(move |thread_id: &ThreadId| {
    if let Some(s) = session.get_session(&session_id) {
        s.active_agents.remove(thread_id);
    }
});
```

#### executor.rs 触发点

```rust
// execute_prompt() 循环内，agent.execute() 返回后
match stop_reason {
    PromptStopReason::Cancelled => {
        // 级联取消所有 cascade 子 agent
        if let Some(session) = session_ref {
            session.cancel_cascade_children(&thread_id);
        }
        // 清理所有已完成的 AgentRuntime
        cleanup_finished_agents(session, &session_id);
    }
    PromptStopReason::EndTurn | PromptStopReason::MaxTurnRequests => {
        cleanup_finished_agents(session, &session_id);
    }
}
```

#### close_session 适配

```rust
// SessionManager::close_session()
pub async fn close_session(&self, session_id: &str) -> anyhow::Result<()> {
    if let Some(session) = self.sessions.get(session_id) {
        // 取消所有 agent（包括 background）
        session.cancel_all_agents();
        // 更新所有 active thread 状态
        for (thread_id, runtime) in &session.active_agents {
            if runtime.status.is_active() {
                self.thread_store.update_thread_status(thread_id, "cancelled").await?;
            }
        }
        session.active_agents.clear();
        // 现有取消逻辑
        session.cancel_token.cancel();
    }
    // ... 现有清理
}
```

### 1.3 受影响文件

| 文件 | 变更 |
|------|------|
| `peri-middlewares/src/subagent/mod.rs` | 新增 register/deregister callback 字段 |
| `peri-middlewares/src/subagent/tool/define.rs` | 子 agent 创建后注册，完成后注销 |
| `peri-acp/src/session/executor.rs` | cancel 时调用 cancel_cascade_children |
| `peri-acp/src/session/mod.rs` | close_session 调用 cancel_all_agents + 清理 |

### 1.4 测试

- 子 agent 创建后 active_agents 包含该 thread_id
- 子 agent 完成后从 active_agents 移除
- 父 agent 取消时 cascade 子 agent 的 cancel_token 被触发
- 父 agent 取消时 independent 子 agent 不受影响
- close_session 时所有 agent 被取消

---

## 2. Compact per-thread

### 2.1 问题

`load_context()` 返回 `[ancestor msgs] + [own msgs]`，但 `CompactMiddleware::do_full_compact()` 对 `state.messages()` 全量操作，会触碰祖先消息。`invalidate_context_cache()` 从未被调用。

### 2.2 设计

#### AgentState 新增 ancestor_len

```rust
pub struct AgentState {
    // 现有字段
    pub cwd: String,
    pub messages: Vec<BaseMessage>,
    // ...
    thread_id: Option<ThreadId>,
    store: Option<Arc<dyn ThreadStore>>,
    // 新增
    ancestor_len: usize,  // messages[..ancestor_len] = 只读祖先消息
}
```

在 `with_thread_context()` 中设置：

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

`with_messages(cwd, messages)` 路径：`ancestor_len = 0`（无祖先）。

`new(cwd)` 路径：`ancestor_len = 0`。

#### CompactMiddleware 适配

```rust
fn do_full_compact(&self, state: &mut AgentState) -> Result<()> {
    let ancestor_len = state.ancestor_len();

    // 只 compact 自身消息
    let own_messages: Vec<BaseMessage> = state.messages_mut().drain(ancestor_len..).collect();
    if own_messages.is_empty() {
        return Ok(());
    }

    // 现有 compact 逻辑（摘要生成 + re_inject）
    let compacted = self.generate_summary(&own_messages)?;

    // 重建：祖先消息（不变） + compacted
    state.messages_mut().truncate(ancestor_len);
    state.messages_mut().extend(compacted);

    // 失效缓存
    if let (Some(store), Some(thread_id)) = (state.store(), state.thread_id()) {
        store.invalidate_context_cache(thread_id)?;
    }

    Ok(())
}
```

Micro-compact 同理：只清除 `messages[ancestor_len..]` 中 ≥5 步前的 tool_result。

#### ContextBudget

保持现有逻辑：`TokenTracker::estimated_context_tokens()` 反映最近一次 LLM 调用的 input_tokens。祖先消息已包含在这个值中，超预算时 compact 自身消息即可。不需要额外计算。

### 2.3 受影响文件

| 文件 | 变更 |
|------|------|
| `peri-agent/src/agent/state.rs` | 新增 `ancestor_len` 字段 + `with_ancestor_len()` + `ancestor_len()` |
| `peri-agent/src/agent/compact/` | `do_full_compact` / `do_micro_compact` 区分 ancestor_len |
| `peri-acp/src/session/compact_runner.rs`（如存在） | 确认 compact 后调用 invalidate_context_cache |

### 2.4 测试

- `with_thread_context` 正确计算 ancestor_len
- compact 后祖先消息不变
- compact 后 cached_context 被清空
- micro-compact 只清除自身消息中的旧 tool_result
- 无祖先的 agent（主 agent）：ancestor_len=0，行为与现有一致

---

## 3. /tasks 面板

### 3.1 结构

新增 `/tasks` 面板，两个 tab：

```
/tasks 面板
  ├── Tab 1: Agent Threads
  │     列表视图:
  │       ● Main Agent [active]              thread_1
  │       ○ Code Reviewer [done]             thread_2
  │       ○ Explorer [cancelled]             thread_3
  │       ○ Background Task [done]           thread_4
  │     排序: active 在前，非活跃在后
  │     Enter → 懒加载该 thread 自身消息，显示详情
  │     Esc → 返回列表
  └── Tab 2: Cron Tasks
        └── 现有 cron 面板内容迁移
```

### 3.2 数据源

#### Agent Threads tab

```rust
fn load_agent_threads(session: &AcpSession, store: &dyn ThreadStore) -> Vec<AgentThreadEntry> {
    let root_id = &session.thread_id;

    // 活跃 agent（从 active_agents 获取实时状态）
    let active: HashMap<ThreadId, &AgentRuntime> = &session.active_agents;

    // 全部 agent thread（含历史）
    let all_threads = store.list_session_threads(root_id)?;

    let mut entries: Vec<AgentThreadEntry> = all_threads.into_iter().map(|t| {
        let runtime = active.get(&t.id);
        AgentThreadEntry {
            thread_id: t.id.clone(),
            title: t.title.unwrap_or_else(|| t.id.clone()),
            status: runtime.map(|r| r.status).unwrap_or_else(|| {
                AgentStatus::from_str(&t.agent_status)
            }),
            is_active: runtime.is_some(),
            hidden: t.hidden,
        }
    }).collect();

    // 排序：活跃在前
    entries.sort_by(|a, b| b.is_active.cmp(&a.is_active));
    entries
}
```

#### 懒加载详情

```rust
// 用户选中某个 thread 按 Enter
fn load_thread_detail(thread_id: &ThreadId, store: &dyn ThreadStore) -> Vec<BaseMessage> {
    // 只加载自身消息（不需要祖先快照，人类浏览用）
    store.load_messages(thread_id)
}
```

渲染：复用 MessagePipeline 的 `messages_to_view_models()` 转换，在只读模式下展示。

#### Cron Tasks tab

现有 cron 面板（`panels/cron.rs`）内容迁移到 tab 2。数据源不变（`CronScheduler`）。

### 3.3 面板系统集成

```rust
// panels/ 目录新增
panels/tasks/
    mod.rs          // TasksPanel: PanelComponent trait 实现
    agent_list.rs   // Agent Threads tab 列表渲染
    agent_detail.rs // Agent Threads tab 详情渲染（懒加载）
    cron_list.rs    // Cron Tasks tab（迁移自 panels/cron.rs）

// mod.rs 中的 PanelComponent 实现
impl PanelComponent for TasksPanel {
    fn handle_key(&mut self, key: KeyEvent, app: &mut App) -> PanelAction { ... }
    fn render(&mut self, f: &mut Frame, area: Rect, app: &App) { ... }
    // Tab 切换: 左右方向键
    // 列表导航: 上下方向键 + Enter/Esc
}
```

快捷键：`/tasks` 命令打开面板，面板内左右方向键切换 tab。

### 3.4 受影响文件

| 文件 | 变更 |
|------|------|
| `peri-tui/src/app/panels/tasks/` | 新增目录，面板实现 |
| `peri-tui/src/app/panels/mod.rs` | 注册 TasksPanel |
| `peri-tui/src/app/commands/` | 新增 `/tasks` 命令 |
| `peri-tui/src/app/panels/cron.rs` | 渲染逻辑迁移到 tasks/cron_list.rs |

### 3.5 测试

- Agent threads 列表正确显示活跃/非活跃状态
- 排序：活跃在前
- Enter 懒加载 thread 详情
- Esc 返回列表
- Tab 切换到 Cron Tasks
- Cron tab 功能与现有一致

---

## 实现顺序

| 序号 | Feature | 依赖 | 预估复杂度 |
|------|---------|------|-----------|
| 1 | Cancel Propagation | 无 | 中（callback 机制 + executor 集成） |
| 2 | Compact per-thread | Feature 1 | 低（ancestor_len + compact 范围限制） |
| 3 | /tasks 面板 | Feature 1（数据源依赖 active_agents） | 高（新面板 + tab 系统 + 懒加载） |
