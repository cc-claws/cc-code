# Agent 统一存储机制设计

> 日期：2026-05-24
> 状态：Draft

## 1. 概述

重新设计 agent 存储机制，使 subagent 和主 agent 共享同一套身份和存储模型。每个 agent（包括 subagent）都是一个独立的 thread，拥有自己的消息历史、配置快照和生命周期管理。

### 核心原则

- **Agent 是一等公民**：subagent 和主 agent 本质上是同一种实体，只是创建��数不同
- **Thread 为主表**：复用现有 `threads` 表，扩展字段，不新建表
- **快照式继承**：子 agent 创建时锁定父 agent 的消息快照，之后父子独立演化
- **配置冻结**：agent 创建后配置完全不可变

### 决策记录

通过 21 轮问答达成以下关键决策：

| # | 决策点 | 选择 |
|---|--------|------|
| 1 | 存储粒度 | Thread 级独立 |
| 2 | 上下文继承 | 快照式（创建时锁定） |
| 3 | Agent 生命周期 | Session 级 |
| 4 | 存储结构 | 扩展 threads 表，不新建表 |
| 5 | Thread-Agent 关系 | 每个 agent 一个 thread row |
| 6 | Session 标识 | 主 agent thread_id = session_id |
| 7 | 配置存储 | 完整快照（JSON） |
| 8 | 上下文组装层 | Persistence 层透明 |
| 9 | 写入路径 | 只写 own thread |
| 10 | 结果回传 | 完整文本 + 子 thread_id 引用 |
| 11 | Compact | 独立 per-agent |
| 12 | Session 恢复 | 主 agent 立即加载，子 agent 懒加载 |
| 13 | 快照深度 | 无硬限制，带物化缓存 |
| 14 | 缓存失效 | 仅 compact 时 |
| 15 | 取消传播 | Sync 级联 / Background 独立于父，绑定 session |

## 2. 数据模型

### 2.1 Schema 变更

在现有 `threads` 表上新增字段（不新建表，不修改 `messages` 表）：

```sql
ALTER TABLE threads ADD COLUMN parent_thread_id TEXT;
ALTER TABLE threads ADD COLUMN snapshot_at_message_id TEXT;
ALTER TABLE threads ADD COLUMN hidden BOOLEAN DEFAULT FALSE;
ALTER TABLE threads ADD COLUMN cancel_policy TEXT DEFAULT 'cascade';
ALTER TABLE threads ADD COLUMN config TEXT;
ALTER TABLE threads ADD COLUMN cached_context TEXT;
ALTER TABLE threads ADD COLUMN status TEXT DEFAULT 'active';
```

完整字段说明：

| 字段 | 类型 | 说明 |
|------|------|------|
| `parent_thread_id` | TEXT | 父 agent thread，NULL = 根 agent |
| `snapshot_at_message_id` | TEXT | 快照截止消息 ID |
| `hidden` | BOOLEAN | true = 子 agent，不显示在主列表 |
| `cancel_policy` | TEXT | `cascade`（sync）/ `independent`（background） |
| `config` | TEXT | JSON 完整配置快照 |
| `cached_context` | TEXT | 物化缓存（compact 时失效） |
| `status` | TEXT | `active` / `done` / `cancelled` / `error` |

### 2.2 关键语义

- **Session = 根 thread**：`parent_thread_id IS NULL AND hidden = false` 的 thread 即为 session 入口，其 `id` 充当 session_id
- **子 agent = 隐藏 thread**：`hidden = true`，通过 `parent_thread_id` 构成树
- **快照**：`snapshot_at_message_id` 锁定创建时父 thread 的最后一条消息，运行时通过它组装继承的上下文
- **配置冻结**：`config` JSON 在创建时一次性写入，运行时不可变

### 2.3 查询模式

```sql
-- 获取直接子 thread（支持多层嵌套）
SELECT * FROM threads WHERE parent_thread_id = :thread_id;

-- 获取 session 下所有 thread（递归 CTE，支持多层嵌套）
WITH RECURSIVE session_threads AS (
    SELECT * FROM threads WHERE id = :root_id
    UNION ALL
    SELECT t.* FROM threads t JOIN session_threads st ON t.parent_thread_id = st.id
)
SELECT * FROM session_threads;

-- 恢复 session（只加载主 agent）
SELECT * FROM threads WHERE id = :root_id;
```

### 2.4 数据库迁移

现有数据自动兼容：所有新字段有默认值。现有 thread 的 `parent_thread_id = NULL`（根 agent）、`hidden = false`、`status = 'active'`。

## 3. 上下文组装

### 3.1 核心机制

`SqliteThreadStore` 新增 `load_context(thread_id)` 方法，对上层透明。persistence 层负责递归组装祖先快照 + 自身消息。

```rust
impl SqliteThreadStore {
    async fn load_context(&self, thread_id: &str) -> Result<Vec<BaseMessage>> {
        let thread = self.get_thread(thread_id)?;
        let mut messages = Vec::new();

        // 递归组装祖先快照（从根到父，按创建顺序）
        if let Some(parent_id) = thread.parent_thread_id {
            let ancestor_chain = self.resolve_ancestor_chain(thread_id)?;
            for ancestor in ancestor_chain {
                let ancestor_msgs = self.load_messages_up_to(
                    &ancestor.id,
                    &ancestor.snapshot_at_message_id,
                )?;
                messages.extend(ancestor_msgs);
            }
        }

        // 拼接自身消息
        let own_messages = self.load_messages(thread_id)?;
        messages.extend(own_messages);

        Ok(messages)
    }
}
```

### 3.2 物化缓存

- **写入时机**：首次 `load_context()` 后，序列化存入 `cached_context` 字段
- **命中时**：直接反序列化返回，跳过递归查询
- **失效时机**：仅 compact 触发时清空
- **追加优化**：`add_message()` 不失效缓存，下次 `load_context()` 只需 append 新消息到缓存末尾

### 3.3 递归链示例

```
agent_4 (snapshot → agent_2's msg_15)
  └── agent_2 (snapshot → agent_1's msg_8)
        └── agent_1 (top_level, 无 snapshot)

load_context(agent_4):
  1. resolve_ancestor_chain → [agent_1, agent_2]
  2. agent_1 的 messages (全部)
  3. agent_2 的 messages WHERE id <= msg_15
  4. agent_4 自身的 messages (全部)
```

无深度硬限制，通过缓存保证性能。

## 4. 运行时模型

### 4.1 AgentRuntime

每个活跃 agent 在运行时对应一个 `AgentRuntime`：

```rust
struct AgentRuntime {
    thread_id: ThreadId,
    cancel_token: CancellationToken,
    status: AgentStatus,           // Active | Done | Cancelled | Error
    event_sink: Box<dyn EventSink>,
}

enum CancelPolicy {
    Cascade,       // sync 子 agent，跟随父取消
    Independent,   // background 子 agent，仅跟随 session 根取消
}
```

### 4.2 Session 作为 Agent 容器

```rust
struct AcpSession {
    session_id: String,            // = 根 thread 的 id
    root_thread_id: ThreadId,
    active_agents: HashMap<ThreadId, AgentRuntime>,
    cancel_token: CancellationToken,  // session 级取消令牌
    // ... 现有字段
}
```

### 4.3 取消传播

```
session.cancel_token 取消
  → 遍历 active_agents，全部取消（包括 background）

父 agent 取消（ReAct 循环中断）
  → 遍历 active_agents
    → cancel_policy == Cascade：取消
    → cancel_policy == Independent：不动
```

实现方式：`Cascade` 子 agent 的 CancellationToken 用 `tokio::select!` 监听父 token + 自身 token；`Independent` 子 agent 只监听 session 根 token + 自身 token。

### 4.4 状态流转

```
Created → Active → Done
                 → Cancelled
                 → Error
```

状态变更时同步更新 `threads` 表的 `status` 字段。

## 5. SubAgent 创建与执行

### 5.1 创建流程

```
1. LLM 返回 tool_call: Agent(prompt, fork, background, subagent_type)
2. SubAgentMiddleware.before_tool() 拦截
3. 获取当前 agent 的 thread_id 和最后一条 message_id
4. INSERT INTO threads:
     id = 新 UUID,
     parent_thread_id = 当前 thread_id,
     snapshot_at_message_id = 当前最后一条 message_id,
     hidden = true,
     cancel_policy = background ? 'independent' : 'cascade',
     config = 冻结完整配置快照,
     status = 'active'
5. 创建 AgentRuntime:
     a. CancellationToken（cascade: 关联父 token; independent: 关联 session 根 token）
     b. event_sink 指向 session 共享 sink（事件带 source_agent_id = 新 thread_id）
     c. persistence.load_context(new_thread_id) 加载初始上下文
6. 注册到 Session.active_agents[new_thread_id]
7. 构建并执行 agent（复用 build_agent 逻辑）
```

### 5.2 执行模式统一

| 参数 | cancel_policy | snapshot_at_message_id | 父 agent 等待 |
|------|-------------|----------------------|-------------|
| 默认 | cascade | NULL（不继承上下文） | 是 |
| fork=true | cascade | 设置（继承父上下文快照） | 是 |
| background=true | independent | 可选（可设可不设） | 否 |

三种模式统一为一个 agent 创建流程，只是参数组合不同。不再有独立的 `invoke_fork()` / `invoke_background()` 分支。

注意：`snapshot_at_message_id` 为 NULL 时，`load_context()` 跳过祖先组装，直接返回自身消息。这避免了 normal 模式下无意义的快照记录。

### 5.3 结果回传

子 agent 完成 → 事件通知父 agent → 父 agent 写入 ToolResult 到**自己的** thread：

```
ToolResult {
    content: 完整结果文本,
    child_thread_id: 子 thread 的 id    // 引用，支持快速索引
}
```

### 5.4 清理

子 agent 执行结束（done/cancelled/error）→ 从 `active_agents` 移除 → 更新 thread `status`。SQLite 记录永久保留。

## 6. Persistence 层 API

### 6.1 ThreadStore trait 扩展

```rust
#[async_trait]
pub trait ThreadStore: Send + Sync {
    // 现有方法不变
    async fn create_thread(&self, thread: &ThreadMeta) -> Result<()>;
    async fn get_thread(&self, id: &str) -> Result<Option<ThreadMeta>>;
    async fn load_messages(&self, thread_id: &str) -> Result<Vec<BaseMessage>>;
    async fn save_message(&self, thread_id: &str, message: &BaseMessage) -> Result<()>;

    // 新增方法
    /// 加载完整上下文（祖先快照 + 自身消息），带缓存
    async fn load_context(&self, thread_id: &str) -> Result<Vec<BaseMessage>>;

    /// 获取直接子 thread
    async fn list_child_threads(&self, parent_id: &str) -> Result<Vec<ThreadMeta>>;

    /// 获取 session 下所有 thread
    async fn list_session_threads(&self, root_id: &str) -> Result<Vec<ThreadMeta>>;

    /// 更新 thread 状态
    async fn update_thread_status(&self, id: &str, status: &str) -> Result<()>;

    /// 清空物化缓存（compact 时调用）
    async fn invalidate_context_cache(&self, thread_id: &str) -> Result<()>;

    /// 获取 thread 的子集消息（用于快照截取）
    async fn load_messages_up_to(
        &self,
        thread_id: &str,
        message_id: &str,
    ) -> Result<Vec<BaseMessage>>;
}
```

### 6.2 AgentState 适配

```rust
impl AgentState {
    // 现有 add_message 逻辑不变，仍然写 own thread

    // 新增：初始化时使用 load_context
    pub async fn with_thread_context(
        thread_id: &str,
        store: &dyn ThreadStore,
    ) -> Result<Self> {
        let messages = store.load_context(thread_id).await?;
        // 构建 AgentState，绑定 own_thread_id
    }
}
```

关键点：`AgentState` 持有 `own_thread_id`，`add_message()` 只写 own thread。上下文中的祖先消息是只读前缀。

## 7. Compact 与上下文预算

### 7.1 独立 compact

每个 agent 独立管理上下文预算，compact 只压缩**自身 thread 的消息**。

```
上下文结构（load_context 返回）:
  [祖先快照消息 (只读)] + [自身消息 (可 compact)]

compact 时:
  1. 仅操作自身消息，生成摘要
  2. 替换自身 thread 的 messages
  3. 清空 cached_context → 下次 load_context 重新组装
  4. 祖先快照不受影响
```

### 7.2 ContextBudget 检查

预算计算只算**自身消息**的 token 数，不算祖先快照。祖先快照是创建时锁定的不可变前缀，compact 无法也不应缩减它。

### 7.3 compact 后消息结构

与现有规则一致：`[Human(摘要+续接), System(re_inject)...]`，写入自身 thread。

## 8. Session 恢复

### 8.1 恢复流程

```
用户执行 -c/--continue 或 -r/--resume:
  1. 查找根 thread（parent_thread_id IS NULL）
  2. load_context(root_thread_id) 加载主 agent 完整上下文
  3. 恢复 AcpSession:
     - session_id = root_thread_id
     - active_agents 插入主 agent 的 AgentRuntime
  4. 渲染主 agent 消息到 TUI
  5. 子 agent thread 不加载（懒加载）
```

### 8.2 懒加载

```
用户通过 /tasks 面板的 tab 切换到某个子 agent:
  1. list_child_threads(root_thread_id) 获取子 thread 列表
  2. 选中某个子 thread → load_messages(child_thread_id) 加载其自身消息
  3. 渲染到 tab 视图
  4. 不需要 load_context（人类阅读只需自身消息）
```

关键区分：
- **继续对话**（主 agent）：用 `load_context()`（LLM 需要完整上下文）
- **浏览历史**（子 agent tab）：用 `load_messages()`（人类阅读只需自身消息）

### 8.3 /tasks 面板 Agent Threads Tab

排序规则：活跃 agent 在前，非活跃在后。

```
Session 的 agent threads:
  ● Main Agent (thread_1) [active]     ← 主 agent
  ○ Code Reviewer (thread_2) [done]
  ○ Explorer (thread_3) [done]
  ○ Background Task (thread_4) [cancelled]
```

thread 的 `title` 字段复用为 agent 显示名，子 agent 创建时从 agent 定义的名称填充。

## 9. 受影响的模块

### 变更

| 模块 | 变更范围 |
|------|---------|
| `peri-agent/src/thread/` | SqliteThreadStore 新增字段 + migration + 新方法 |
| `peri-agent/src/agent/state.rs` | 新增 `own_thread_id`，`add_message` 绑定 own thread |
| `peri-middlewares/src/subagent/` | 重写：创建子 thread + AgentRuntime |
| `peri-acp/src/session/` | AcpSession 新增 `active_agents`，取消传播逻辑 |
| `peri-acp/src/session/executor.rs` | 适配多 agent，compact per-thread |
| `peri-acp/src/session/compact_runner.rs` | compact 只压缩 own thread |
| `peri-tui/src/app/agent_ops/` | 事件路由适配 |
| `peri-tui/src/app/panels/` | `/tasks` 面板新增 agent threads tab |

### 不受影响

| 模块 | 原因 |
|------|------|
| `peri-agent/src/agent/events.rs` | ExecutorEvent 结构不变 |
| `peri-agent/src/llm/` | LLM 适配层不关心消息来源 thread |
| `peri-agent/src/middleware/` | Middleware trait 签名不变 |
| 其他中间件 | 除 subagent 外不受影响 |
| `peri-widgets/` / `peri-lsp/` / `langfuse-client/` / `peri-cli/` | 不涉及 |

### 事件流

事件流本身不变，只是 `source_agent_id` 的语义从"临时 agent 标识"变为"持久化 thread_id"。

```
子 agent 执行 → ExecutorEvent (source_agent_id = child_thread_id)
  → EventSink.push_event()
  → TUI pump_notifications
  → map_executor_event() → AgentEvent
  → handle_agent_event() 路由到对应 UI 区域
```
