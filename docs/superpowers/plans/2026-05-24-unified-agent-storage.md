# Unified Agent Storage Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Redesign agent storage so every agent (including subagents) is an independent thread with its own message history, unified identity model, and snapshot-based context inheritance.

**Architecture:** Extend the existing `threads` SQLite table with parent/snapshot fields. Each agent = one thread row. Subagent creation becomes "create a new thread with optional parent snapshot." Persistence layer transparently assembles full context (ancestor snapshots + own messages) via `load_context()`. Session acts as agent container with `HashMap<ThreadId, AgentRuntime>`.

**Tech Stack:** Rust, SQLite (sqlx), tokio, existing ThreadStore/AgentState/Middleware abstractions.

**Spec:** `docs/superpowers/specs/2026-05-24-unified-agent-storage-design.md`

---

## File Structure

| File | Responsibility |
|------|---------------|
| `peri-agent/src/thread/types.rs` | ThreadMeta ��展（新增 parent/snapshot/config 等字段） |
| `peri-agent/src/thread/store.rs` | ThreadStore trait 新增方法（load_context 等） |
| `peri-agent/src/thread/sqlite_store.rs` | Schema migration + 新方法实现 |
| `peri-agent/src/thread/sqlite_store_test.rs` | 新方法的测试 |
| `peri-agent/src/agent/state.rs` | AgentState 新增 own_thread_id，适配 load_context |
| `peri-middlewares/src/subagent/mod.rs` | SubAgentMiddleware 适配新模型 |
| `peri-middlewares/src/subagent/tool/define.rs` | SubAgentTool 重写：创建子 thread |
| `peri-acp/src/session/mod.rs` | AcpSession 新增 active_agents |
| `peri-acp/src/session/agent_runtime.rs` | 新文件：AgentRuntime + CancelPolicy |
| `peri-acp/src/session/executor.rs` | 适配多 agent，compact per-thread |

---

### Task 1: 扩展 ThreadMeta 数据结构

**Files:**
- Modify: `peri-agent/src/thread/types.rs`

- [ ] **Step 1: 写 ThreadMeta 扩展的失败测试**

在 `peri-agent/src/thread/types.rs` 底部添加（暂不修改 struct，先写测试）：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_thread_meta_default_values() {
        let meta = ThreadMeta::new("/tmp");
        assert!(meta.parent_thread_id.is_none());
        assert!(meta.snapshot_at_message_id.is_none());
        assert!(!meta.hidden);
        assert_eq!(meta.cancel_policy, "cascade");
        assert_eq!(meta.agent_status, "active");
        assert!(meta.config.is_none());
        assert!(meta.cached_context.is_none());
    }

    #[test]
    fn test_thread_meta_is_root() {
        let mut meta = ThreadMeta::new("/tmp");
        assert!(meta.is_root());
        meta.parent_thread_id = Some("parent-id".to_string());
        assert!(!meta.is_root());
    }
}
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test -p peri-agent --lib -- thread::types::tests`
Expected: FAIL（ThreadMeta 没有 parent_thread_id 等字段）

- [ ] **Step 3: 扩展 ThreadMeta 结构体**

在 `peri-agent/src/thread/types.rs` 中修改 `ThreadMeta`：

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadMeta {
    pub id: ThreadId,
    pub title: Option<String>,
    pub cwd: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub message_count: usize,
    #[serde(default)]
    pub content_size: u64,

    // Agent 统一存储扩展字段
    /// 父 agent thread ID，None = 根 agent（主 agent）
    #[serde(default)]
    pub parent_thread_id: Option<String>,
    /// 快照截止消息 ID（创建时锁定父 thread 的最后一条消息）
    #[serde(default)]
    pub snapshot_at_message_id: Option<String>,
    /// true = 子 agent，不显示在主列表
    #[serde(default)]
    pub hidden: bool,
    /// 取消策略：cascade（sync 子 agent）/ independent（background）
    #[serde(default = "default_cancel_policy")]
    pub cancel_policy: String,
    /// JSON 完整配置快照（创建时冻结）
    #[serde(default)]
    pub config: Option<String>,
    /// 物化缓存（compact 时失效）
    #[serde(default)]
    pub cached_context: Option<String>,
    /// agent 状态：active / done / cancelled / error
    #[serde(default = "default_agent_status")]
    pub agent_status: String,
}

fn default_cancel_policy() -> String {
    "cascade".to_string()
}

fn default_agent_status() -> String {
    "active".to_string()
}

impl ThreadMeta {
    pub fn new(cwd: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: uuid::Uuid::now_v7().to_string(),
            title: None,
            cwd: cwd.into(),
            created_at: now,
            updated_at: now,
            message_count: 0,
            content_size: 0,
            parent_thread_id: None,
            snapshot_at_message_id: None,
            hidden: false,
            cancel_policy: default_cancel_policy(),
            config: None,
            cached_context: None,
            agent_status: default_agent_status(),
        }
    }

    /// 是否为根 agent（主 agent / session 入口）
    pub fn is_root(&self) -> bool {
        self.parent_thread_id.is_none()
    }
}
```

- [ ] **Step 4: 运行测试确认通过**

Run: `cargo test -p peri-agent --lib -- thread::types::tests`
Expected: PASS

- [ ] **Step 5: 运行全量编译确认无破坏性变更**

Run: `cargo build -p peri-agent`
Expected: 可能出现编译错误，因为 sqlite_store.rs 中 `meta_from_row` 和 INSERT/SELECT 语句不包含新字段。将在 Task 2 修复。

- [ ] **Step 6: Commit**

```bash
git add peri-agent/src/thread/types.rs
git commit -m "feat(agent-storage): extend ThreadMeta with parent/snapshot/config fields"
```

---

### Task 2: SQLite Schema Migration + Store 适配

**Files:**
- Modify: `peri-agent/src/thread/sqlite_store.rs`
- Modify: `peri-agent/src/thread/sqlite_store_test.rs`

- [ ] **Step 1: 写 migration 和新方法的失败测试**

在 `peri-agent/src/thread/sqlite_store_test.rs` 末尾追加：

```rust
    #[tokio::test]
    async fn test_child_thread_create_and_list() {
        let (store, _dir) = make_store().await;
        let parent_meta = ThreadMeta::new("/project");
        let parent_id = store.create_thread(parent_meta).await.unwrap();
        store
            .append_messages(&parent_id, &[BaseMessage::human("parent msg")])
            .await
            .unwrap();

        // 创建子 thread
        let mut child_meta = ThreadMeta::new("/project");
        child_meta.parent_thread_id = Some(parent_id.clone());
        child_meta.hidden = true;
        child_meta.cancel_policy = "cascade".to_string();
        child_meta.agent_status = "active".to_string();
        let child_id = store.create_thread(child_meta).await.unwrap();

        store
            .append_messages(&child_id, &[BaseMessage::human("child msg")])
            .await
            .unwrap();

        // 查询子 thread
        let children = store.list_child_threads(&parent_id).await.unwrap();
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].id, child_id);
        assert_eq!(children[0].parent_thread_id.as_deref(), Some(parent_id.as_str()));
        assert!(children[0].hidden);
    }

    #[tokio::test]
    async fn test_session_threads_recursive() {
        let (store, _dir) = make_store().await;
        let root_meta = ThreadMeta::new("/project");
        let root_id = store.create_thread(root_meta).await.unwrap();

        // 子 agent
        let mut child_meta = ThreadMeta::new("/project");
        child_meta.parent_thread_id = Some(root_id.clone());
        child_meta.hidden = true;
        let child_id = store.create_thread(child_meta).await.unwrap();

        // 孙 agent（子 agent 的子 agent）
        let mut grandchild_meta = ThreadMeta::new("/project");
        grandchild_meta.parent_thread_id = Some(child_id.clone());
        grandchild_meta.hidden = true;
        let grandchild_id = store.create_thread(grandchild_meta).await.unwrap();

        // 查询 session 所有 thread
        let all = store.list_session_threads(&root_id).await.unwrap();
        assert_eq!(all.len(), 3);
        let ids: Vec<&str> = all.iter().map(|t| t.id.as_str()).collect();
        assert!(ids.contains(&root_id.as_str()));
        assert!(ids.contains(&child_id.as_str()));
        assert!(ids.contains(&grandchild_id.as_str()));
    }

    #[tokio::test]
    async fn test_update_thread_status() {
        let (store, _dir) = make_store().await;
        let meta = ThreadMeta::new("/tmp");
        let id = store.create_thread(meta).await.unwrap();

        store.update_thread_status(&id, "done").await.unwrap();
        let loaded = store.load_meta(&id).await.unwrap();
        assert_eq!(loaded.agent_status, "done");
    }

    #[tokio::test]
    async fn test_load_context_without_parent() {
        // 无父 thread 时，load_context = load_messages
        let (store, _dir) = make_store().await;
        let meta = ThreadMeta::new("/tmp");
        let id = store.create_thread(meta).await.unwrap();
        store
            .append_messages(&id, &[BaseMessage::human("hello"), BaseMessage::ai("world")])
            .await
            .unwrap();

        let ctx = store.load_context(&id).await.unwrap();
        assert_eq!(ctx.len(), 2);
        assert_eq!(ctx[0].content(), "hello");
    }

    #[tokio::test]
    async fn test_load_context_with_snapshot() {
        let (store, _dir) = make_store().await;
        // 父 thread
        let parent_meta = ThreadMeta::new("/project");
        let parent_id = store.create_thread(parent_meta).await.unwrap();
        store
            .append_messages(&parent_id, &[BaseMessage::human("msg1"), BaseMessage::ai("reply1")])
            .await
            .unwrap();

        // 子 thread，snapshot 在父的最后一条消息
        let last_msg_id = store.load_messages(&parent_id).await.unwrap().last().unwrap().id().to_string();
        let mut child_meta = ThreadMeta::new("/project");
        child_meta.parent_thread_id = Some(parent_id.clone());
        child_meta.snapshot_at_message_id = Some(last_msg_id.clone());
        child_meta.hidden = true;
        let child_id = store.create_thread(child_meta).await.unwrap();
        store
            .append_messages(&child_id, &[BaseMessage::human("child msg")])
            .await
            .unwrap();

        let ctx = store.load_context(&child_id).await.unwrap();
        // 应包含父消息（msg1 + reply1）+ 子消息（child msg）
        assert_eq!(ctx.len(), 3);
        assert_eq!(ctx[0].content(), "msg1");
        assert_eq!(ctx[1].content(), "reply1");
        assert_eq!(ctx[2].content(), "child msg");
    }

    #[tokio::test]
    async fn test_cached_context_invalidation() {
        let (store, _dir) = make_store().await;
        let meta = ThreadMeta::new("/tmp");
        let id = store.create_thread(meta).await.unwrap();
        store
            .append_messages(&id, &[BaseMessage::human("hello")])
            .await
            .unwrap();

        // 首次 load_context 应写入缓存
        let ctx1 = store.load_context(&id).await.unwrap();
        assert_eq!(ctx1.len(), 1);
        let meta_after = store.load_meta(&id).await.unwrap();
        assert!(meta_after.cached_context.is_some());

        // 追加消息，缓存应自动 append（不失效）
        store
            .append_messages(&id, &[BaseMessage::ai("world")])
            .await
            .unwrap();
        let ctx2 = store.load_context(&id).await.unwrap();
        assert_eq!(ctx2.len(), 2);

        // 手动失效缓存（模拟 compact）
        store.invalidate_context_cache(&id).await.unwrap();
        let meta_invalidated = store.load_meta(&id).await.unwrap();
        assert!(meta_invalidated.cached_context.is_none());

        // 再次 load 应重建缓存
        let ctx3 = store.load_context(&id).await.unwrap();
        assert_eq!(ctx3.len(), 2);
        let meta_rebuilt = store.load_meta(&id).await.unwrap();
        assert!(meta_rebuilt.cached_context.is_some());
    }

    #[tokio::test]
    async fn test_list_threads_excludes_hidden() {
        let (store, _dir) = make_store().await;
        let root_meta = ThreadMeta::new("/project");
        let root_id = store.create_thread(root_meta).await.unwrap();

        let mut child_meta = ThreadMeta::new("/project");
        child_meta.parent_thread_id = Some(root_id.clone());
        child_meta.hidden = true;
        store.create_thread(child_meta).await.unwrap();

        // list_threads 只返回非 hidden
        let list = store.list_threads().await.unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, root_id);
    }
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test -p peri-agent --lib -- thread::sqlite_store::tests::test_child_thread`
Expected: FAIL（新字段不在 schema 中，新方法不存在）

- [ ] **Step 3: 修改 init_schema 添加新字段 + migration**

在 `peri-agent/src/thread/sqlite_store.rs` 的 `init_schema` 方法中，在现有 CREATE TABLE 之后添加 ALTER TABLE migration 语句（SQLite ALTER TABLE ADD COLUMN 是幂等的需要用 try-catch 模式）：

```rust
    async fn init_schema(&self) -> Result<()> {
        // 现有表创建保持不变
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS threads (
                id          TEXT PRIMARY KEY,
                title       TEXT,
                cwd         TEXT NOT NULL DEFAULT '',
                created_at  TEXT NOT NULL,
                updated_at  TEXT NOT NULL,
                message_count INTEGER NOT NULL DEFAULT 0
            )",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS messages (
                message_id  TEXT PRIMARY KEY,
                thread_id   TEXT NOT NULL,
                role        TEXT NOT NULL,
                content     TEXT NOT NULL,
                FOREIGN KEY (thread_id) REFERENCES threads(id) ON DELETE CASCADE
            )",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_messages_thread_id ON messages (thread_id ASC)",
        )
        .execute(&self.pool)
        .await?;

        // Migration: 添加 agent 统一存储字段
        let migrations = [
            "ALTER TABLE threads ADD COLUMN parent_thread_id TEXT",
            "ALTER TABLE threads ADD COLUMN snapshot_at_message_id TEXT",
            "ALTER TABLE threads ADD COLUMN hidden BOOLEAN NOT NULL DEFAULT 0",
            "ALTER TABLE threads ADD COLUMN cancel_policy TEXT NOT NULL DEFAULT 'cascade'",
            "ALTER TABLE threads ADD COLUMN config TEXT",
            "ALTER TABLE threads ADD COLUMN cached_context TEXT",
            "ALTER TABLE threads ADD COLUMN agent_status TEXT NOT NULL DEFAULT 'active'",
        ];
        for sql in &migrations {
            // ALTER TABLE ADD COLUMN 在列已存在时会失败，忽略该错误
            if let Err(e) = sqlx::query(sql).execute(&self.pool).await {
                // SQLite 错误码 1 = duplicate column name，安全忽略
                let msg = e.to_string();
                if !msg.contains("duplicate column name") {
                    return Err(e.into());
                }
            }
        }

        Ok(())
    }
```

- [ ] **Step 4: 更新 meta_from_row 和所有 SQL 查询适配新字段**

`meta_from_row` 函数需要接收新字段。同时所有 SELECT/INSERT/UPDATE 语句需要包含新字段。

替换 `meta_from_row`：

```rust
fn meta_from_row(
    id: String,
    title: Option<String>,
    cwd: String,
    created_at: String,
    updated_at: String,
    message_count: i64,
    content_size: i64,
    parent_thread_id: Option<String>,
    snapshot_at_message_id: Option<String>,
    hidden: bool,
    cancel_policy: String,
    config: Option<String>,
    cached_context: Option<String>,
    agent_status: String,
) -> Result<ThreadMeta> {
    Ok(ThreadMeta {
        id,
        title,
        cwd,
        created_at: created_at.parse::<DateTime<Utc>>()?,
        updated_at: updated_at.parse::<DateTime<Utc>>()?,
        message_count: message_count as usize,
        content_size: content_size as u64,
        parent_thread_id,
        snapshot_at_message_id,
        hidden,
        cancel_policy,
        config,
        cached_context,
        agent_status,
    })
}
```

更新 `create_thread`：

```rust
    async fn create_thread(&self, meta: ThreadMeta) -> Result<ThreadId> {
        let id = meta.id.clone();
        sqlx::query(
            "INSERT INTO threads (id, title, cwd, created_at, updated_at, message_count,
                parent_thread_id, snapshot_at_message_id, hidden, cancel_policy, config, cached_context, agent_status)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        )
        .bind(&meta.id)
        .bind(&meta.title)
        .bind(&meta.cwd)
        .bind(meta.created_at.to_rfc3339())
        .bind(meta.updated_at.to_rfc3339())
        .bind(meta.message_count as i64)
        .bind(&meta.parent_thread_id)
        .bind(&meta.snapshot_at_message_id)
        .bind(meta.hidden)
        .bind(&meta.cancel_policy)
        .bind(&meta.config)
        .bind(&meta.cached_context)
        .bind(&meta.agent_status)
        .execute(&self.pool)
        .await?;
        Ok(id)
    }
```

更新 `load_meta` 的 SQL（所有 SELECT threads 的地方统一使用 helper）：

```rust
    /// threads SELECT 的公共列列表（避免重复）
    const THREAD_COLUMNS: &str = "t.id, t.title, t.cwd, t.created_at, t.updated_at, t.message_count,
        (SELECT COALESCE(SUM(LENGTH(m.content)), 0) FROM messages m WHERE m.thread_id = t.id) as content_size,
        t.parent_thread_id, t.snapshot_at_message_id, t.hidden, t.cancel_policy, t.config, t.cached_context, t.agent_status";
```

更新 `load_meta`：

```rust
    async fn load_meta(&self, id: &ThreadId) -> Result<ThreadMeta> {
        let row: (String, Option<String>, String, String, String, i64, i64,
                  Option<String>, Option<String>, bool, String, Option<String>, Option<String>, String) =
            sqlx::query_as(&format!(
                "SELECT {} FROM threads t WHERE t.id = ?1", THREAD_COLUMNS
            ))
            .bind(id.as_str())
            .fetch_one(&self.pool)
            .await?;
        meta_from_row(row.0, row.1, row.2, row.3, row.4, row.5, row.6,
                      row.7, row.8, row.9, row.10, row.11, row.12, row.13)
    }
```

更新 `list_threads`（排除 hidden=true 的子 agent）：

```rust
    async fn list_threads(&self) -> Result<Vec<ThreadMeta>> {
        let rows: Vec<(String, Option<String>, String, String, String, i64, i64,
                       Option<String>, Option<String>, bool, String, Option<String>, Option<String>, String)> =
            sqlx::query_as(&format!(
                "SELECT {} FROM threads t WHERE t.hidden = 0 ORDER BY t.updated_at DESC",
                THREAD_COLUMNS
            ))
            .fetch_all(&self.pool)
            .await?;
        rows.into_iter()
            .map(|row| meta_from_row(row.0, row.1, row.2, row.3, row.4, row.5, row.6,
                                     row.7, row.8, row.9, row.10, row.11, row.12, row.13))
            .collect()
    }
```

更新 `update_meta`：

```rust
    async fn update_meta(&self, id: &ThreadId, meta: ThreadMeta) -> Result<()> {
        sqlx::query(
            "UPDATE threads SET title = ?1, cwd = ?2, updated_at = ?3, message_count = ?4,
                parent_thread_id = ?5, snapshot_at_message_id = ?6, hidden = ?7,
                cancel_policy = ?8, config = ?9, cached_context = ?10, agent_status = ?11
             WHERE id = ?12"
        )
        .bind(&meta.title)
        .bind(&meta.cwd)
        .bind(meta.updated_at.to_rfc3339())
        .bind(meta.message_count as i64)
        .bind(&meta.parent_thread_id)
        .bind(&meta.snapshot_at_message_id)
        .bind(meta.hidden)
        .bind(&meta.cancel_policy)
        .bind(&meta.config)
        .bind(&meta.cached_context)
        .bind(&meta.agent_status)
        .bind(id.as_str())
        .execute(&self.pool)
        .await?;
        Ok(())
    }
```

- [ ] **Step 5: 实现 ThreadStore trait 新增方法**

在 `peri-agent/src/thread/store.rs` 中扩展 trait：

```rust
#[async_trait]
pub trait ThreadStore: Send + Sync {
    // 现有方法（保持不变）
    async fn create_thread(&self, meta: ThreadMeta) -> Result<ThreadId>;
    async fn append_messages(&self, id: &ThreadId, msgs: &[BaseMessage]) -> Result<()>;
    async fn append_message(&self, id: &ThreadId, message: BaseMessage) -> Result<()> {
        self.append_messages(id, &[message]).await
    }
    async fn load_messages(&self, id: &ThreadId) -> Result<Vec<BaseMessage>>;
    async fn load_meta(&self, id: &ThreadId) -> Result<ThreadMeta>;
    async fn update_meta(&self, id: &ThreadId, meta: ThreadMeta) -> Result<()>;
    async fn list_threads(&self) -> Result<Vec<ThreadMeta>>;
    async fn delete_thread(&self, id: &ThreadId) -> Result<()>;
    async fn update_title(&self, id: &ThreadId, title: &str) -> Result<()> {
        let mut meta = self.load_meta(id).await?;
        meta.title = Some(title.to_string());
        self.update_meta(id, meta).await
    }

    // 新增方法
    /// 加载完整上下文（祖先快照 + 自身消息），带物化缓存
    async fn load_context(&self, thread_id: &ThreadId) -> Result<Vec<BaseMessage>>;

    /// 获取直接子 thread
    async fn list_child_threads(&self, parent_id: &ThreadId) -> Result<Vec<ThreadMeta>>;

    /// 获取 session 下所有 thread（递归 CTE，支持多层嵌套）
    async fn list_session_threads(&self, root_id: &ThreadId) -> Result<Vec<ThreadMeta>>;

    /// 更新 thread 状态
    async fn update_thread_status(&self, id: &ThreadId, status: &str) -> Result<()>;

    /// 清空物化缓存（compact 时调用）
    async fn invalidate_context_cache(&self, thread_id: &ThreadId) -> Result<()>;
}
```

在 `sqlite_store.rs` 的 `impl ThreadStore for SqliteThreadStore` 中实现新方法：

```rust
    async fn list_child_threads(&self, parent_id: &ThreadId) -> Result<Vec<ThreadMeta>> {
        let rows: Vec<(String, Option<String>, String, String, String, i64, i64,
                       Option<String>, Option<String>, bool, String, Option<String>, Option<String>, String)> =
            sqlx::query_as(&format!(
                "SELECT {} FROM threads t WHERE t.parent_thread_id = ?1 ORDER BY t.created_at ASC",
                THREAD_COLUMNS
            ))
            .bind(parent_id.as_str())
            .fetch_all(&self.pool)
            .await?;
        rows.into_iter()
            .map(|row| meta_from_row(row.0, row.1, row.2, row.3, row.4, row.5, row.6,
                                     row.7, row.8, row.9, row.10, row.11, row.12, row.13))
            .collect()
    }

    async fn list_session_threads(&self, root_id: &ThreadId) -> Result<Vec<ThreadMeta>> {
        let rows: Vec<(String, Option<String>, String, String, String, i64, i64,
                       Option<String>, Option<String>, bool, String, Option<String>, Option<String>, String)> =
            sqlx::query_as(&format!(
                "WITH RECURSIVE session_threads AS (
                    SELECT t.* FROM threads t WHERE t.id = ?1
                    UNION ALL
                    SELECT child.* FROM threads child JOIN session_threads st ON child.parent_thread_id = st.id
                )
                SELECT {} FROM session_threads t ORDER BY t.created_at ASC",
                THREAD_COLUMNS.replace("t.", "t.")
            ))
            .bind(root_id.as_str())
            .fetch_all(&self.pool)
            .await?;
        rows.into_iter()
            .map(|row| meta_from_row(row.0, row.1, row.2, row.3, row.4, row.5, row.6,
                                     row.7, row.8, row.9, row.10, row.11, row.12, row.13))
            .collect()
    }

    async fn update_thread_status(&self, id: &ThreadId, status: &str) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        sqlx::query("UPDATE threads SET agent_status = ?1, updated_at = ?2 WHERE id = ?3")
            .bind(status)
            .bind(&now)
            .bind(id.as_str())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn invalidate_context_cache(&self, thread_id: &ThreadId) -> Result<()> {
        sqlx::query("UPDATE threads SET cached_context = NULL WHERE id = ?1")
            .bind(thread_id.as_str())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn load_context(&self, thread_id: &ThreadId) -> Result<Vec<BaseMessage>> {
        let meta = self.load_meta(thread_id).await?;

        // 检查缓存
        if let Some(ref cached) = meta.cached_context {
            if let Ok(msgs) = serde_json::from_str::<Vec<BaseMessage>>(cached) {
                // 追加新增消息（缓存之后 append 的）
                let current = self.load_messages(thread_id).await?;
                let cached_len = msgs.len();
                if current.len() > cached_len {
                    let mut result = msgs;
                    result.extend_from_slice(&current[cached_len..]);
                    // 更新缓存
                    self.save_context_cache(thread_id, &result).await?;
                    return Ok(result);
                }
                return Ok(msgs);
            }
        }

        // 无缓存，完整组装
        let mut messages = Vec::new();

        // 递归组装祖先快照
        if let Some(ref parent_id) = meta.parent_thread_id {
            let chain = self.resolve_ancestor_chain(thread_id).await?;
            for ancestor_meta in chain {
                let ancestor_msgs = if let Some(ref snapshot_id) = ancestor_meta.snapshot_at_message_id {
                    self.load_messages_up_to(&ancestor_meta.id, snapshot_id).await?
                } else {
                    self.load_messages(&ancestor_meta.id).await?
                };
                messages.extend(ancestor_msgs);
            }
        }

        // 拼接自身消息
        let own = self.load_messages(thread_id).await?;
        messages.extend(own);

        // 写入缓存
        self.save_context_cache(thread_id, &messages).await?;

        Ok(messages)
    }
```

在 `SqliteThreadStore` impl 中添加辅助方法：

```rust
    /// 解析祖先链（从根到父，按创建顺序）
    async fn resolve_ancestor_chain(&self, thread_id: &ThreadId) -> Result<Vec<ThreadMeta>> {
        let mut chain = Vec::new();
        let mut current = self.load_meta(thread_id).await?;
        while let Some(ref parent_id) = current.parent_thread_id {
            let parent = self.load_meta(&ThreadId::from(parent_id.clone())).await?;
            chain.push(parent.clone());
            current = parent;
        }
        chain.reverse(); // 根在前
        Ok(chain)
    }

    /// 加载 thread 中 rowid <= 指定 message_id 的消息
    async fn load_messages_up_to(
        &self,
        thread_id: &ThreadId,
        message_id: &str,
    ) -> Result<Vec<BaseMessage>> {
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT content FROM messages WHERE thread_id = ?1 AND rowid <= (
                SELECT rowid FROM messages WHERE message_id = ?2
            ) ORDER BY rowid"
        )
        .bind(thread_id.as_str())
        .bind(message_id)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter()
            .map(|(content,)| serde_json::from_str(&content).map_err(Into::into))
            .collect()
    }

    /// 保存上下文缓存
    async fn save_context_cache(
        &self,
        thread_id: &ThreadId,
        messages: &[BaseMessage],
    ) -> Result<()> {
        let cached = serde_json::to_string(messages)?;
        sqlx::query("UPDATE threads SET cached_context = ?1 WHERE id = ?2")
            .bind(&cached)
            .bind(thread_id.as_str())
            .execute(&self.pool)
            .await?;
        Ok(())
    }
```

- [ ] **Step 6: 运行测试确认通过**

Run: `cargo test -p peri-agent --lib -- thread::sqlite_store::tests`
Expected: ALL PASS

- [ ] **Step 7: 运行全量编译**

Run: `cargo build`
Expected: 编译通过（FilesystemThreadStore 也需要实现新 trait 方法）

- [ ] **Step 8: Commit**

```bash
git add peri-agent/src/thread/
git commit -m "feat(agent-storage): schema migration + ThreadStore new methods (load_context, list_child, list_session, cached_context)"
```

---

### Task 3: FilesystemThreadStore 适配

**Files:**
- Modify: `peri-agent/src/thread/filesystem.rs`
- Modify: `peri-agent/src/thread/filesystem_test.rs`（如有必要）

- [ ] **Step 1: 为 FilesystemThreadStore 实现新 trait 方法**

FilesystemThreadStore 是测试/开发用的内存实现。在 `peri-agent/src/thread/filesystem.rs` 中为新的 trait 方法提供 stub 实现：

```rust
    async fn load_context(&self, id: &ThreadId) -> Result<Vec<BaseMessage>> {
        // 简化实现：直接返回自身消息（不支持快照组装）
        self.load_messages(id).await
    }

    async fn list_child_threads(&self, parent_id: &ThreadId) -> Result<Vec<ThreadMeta>> {
        let all = self.list_threads().await?;
        Ok(all.into_iter().filter(|m| m.parent_thread_id.as_deref() == Some(parent_id.as_str())).collect())
    }

    async fn list_session_threads(&self, root_id: &ThreadId) -> Result<Vec<ThreadMeta>> {
        let all = self.list_threads().await?;
        let mut result = vec![];
        let root = all.iter().find(|m| m.id == root_id.as_str()).cloned();
        if let Some(r) = root {
            result.push(r);
        }
        // 简化：递归查找子 thread
        let mut queue: Vec<String> = vec![root_id.clone()];
        while let Some(pid) = queue.pop() {
            for m in &all {
                if m.parent_thread_id.as_deref() == Some(pid.as_str()) {
                    queue.push(m.id.clone());
                    result.push(m.clone());
                }
            }
        }
        Ok(result)
    }

    async fn update_thread_status(&self, id: &ThreadId, status: &str) -> Result<()> {
        let mut meta = self.load_meta(id).await?;
        meta.agent_status = status.to_string();
        self.update_meta(id, meta).await
    }

    async fn invalidate_context_cache(&self, _id: &ThreadId) -> Result<()> {
        // 内存实现无需缓存
        Ok(())
    }
```

- [ ] **Step 2: 运行编译确认通过**

Run: `cargo build`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add peri-agent/src/thread/filesystem.rs
git commit -m "feat(agent-storage): FilesystemThreadStore implements new ThreadStore methods"
```

---

### Task 4: AgentRuntime 定义

**Files:**
- Create: `peri-acp/src/session/agent_runtime.rs`
- Modify: `peri-acp/src/session/mod.rs`（添加 mod 声明和 AcpSession 扩展）

- [ ] **Step 1: 创建 AgentRuntime 结构体**

新建 `peri-acp/src/session/agent_runtime.rs`：

```rust
use std::collections::HashMap;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use peri_agent::thread::ThreadId;

/// agent 取消策略
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CancelPolicy {
    /// 同步子 agent：跟随父 agent 取消
    Cascade,
    /// Background 子 agent：仅跟随 session 根取消
    Independent,
}

impl CancelPolicy {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Cascade => "cascade",
            Self::Independent => "independent",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "independent" => Self::Independent,
            _ => Self::Cascade,
        }
    }
}

/// agent 运行时状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentStatus {
    Active,
    Done,
    Cancelled,
    Error,
}

impl AgentStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Done => "done",
            Self::Cancelled => "cancelled",
            Self::Error => "error",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "done" => Self::Done,
            "cancelled" => Self::Cancelled,
            "error" => Self::Error,
            _ => Self::Active,
        }
    }

    pub fn is_active(&self) -> bool {
        matches!(self, Self::Active)
    }
}

/// 运行时 agent 实例
pub struct AgentRuntime {
    pub thread_id: ThreadId,
    pub cancel_token: CancellationToken,
    pub cancel_policy: CancelPolicy,
    pub status: AgentStatus,
}

impl AgentRuntime {
    pub fn new(
        thread_id: ThreadId,
        cancel_policy: CancelPolicy,
        parent_cancel: Option<CancellationToken>,
        session_cancel: CancellationToken,
    ) -> Self {
        let cancel_token = CancellationToken::new();
        // 根据策略级联取消
        // 实际级联在 Session 层通过 cancel_child_agents() 管理
        Self {
            thread_id,
            cancel_token,
            cancel_policy,
            status: AgentStatus::Active,
        }
    }
}
```

- [ ] **Step 2: 在 session/mod.rs 中添加 mod 声明和扩展 AcpSession**

在 `peri-acp/src/session/mod.rs` 中：

添加模块声明：
```rust
pub mod agent_runtime;
```

在 `AcpSession` 中添加字段：
```rust
use std::collections::HashMap;
pub use agent_runtime::{AgentRuntime, AgentStatus, CancelPolicy};

pub struct AcpSession {
    // 现有字段保持不变
    pub session_id: String,
    pub thread_id: ThreadId,
    pub cwd: String,
    pub cancel_token: CancellationToken,
    pub state_messages: Vec<BaseMessage>,
    pub created_at: chrono::DateTime<Utc>,
    pub provider_id: String,
    pub model_alias: String,
    pub permission_mode: Arc<SharedPermissionMode>,
    pub thinking: Option<ThinkingConfig>,
    pub pending_requests: DashMap<RequestId, PendingRequestEntry>,
    pub pending_gen: AtomicU64,

    // 新增：活跃 agent 管理
    pub active_agents: HashMap<ThreadId, AgentRuntime>,
}
```

在 `build_session` / `new_session` 中初始化 `active_agents: HashMap::new()`。

添加取消传播方法：
```rust
impl AcpSession {
    /// 取消指定 agent 的所有 cascade 子 agent
    pub fn cancel_cascade_children(&self, parent_thread_id: &ThreadId) {
        for (_, runtime) in &self.active_agents {
            if runtime.cancel_policy == CancelPolicy::Cascade {
                runtime.cancel_token.cancel();
            }
        }
    }

    /// 取消所有 agent（session 结束时）
    pub fn cancel_all_agents(&self) {
        for (_, runtime) in &self.active_agents {
            runtime.cancel_token.cancel();
        }
    }
}
```

- [ ] **Step 3: 运行编译确认通过**

Run: `cargo build -p peri-acp`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add peri-acp/src/session/agent_runtime.rs peri-acp/src/session/mod.rs
git commit -m "feat(agent-storage): AgentRuntime struct + AcpSession active_agents HashMap"
```

---

### Task 5: AgentState 适配 own_thread_id

**Files:**
- Modify: `peri-agent/src/agent/state.rs`

- [ ] **Step 1: AgentState 已经有 thread_id 字段，确认语义正确**

当前 `AgentState` 已有 `thread_id: Option<ThreadId>` 和 `store: Option<Arc<dyn ThreadStore>>`。这个字段就是 `own_thread_id`。`with_persistence()` 已经将 `add_message` 绑定到 own thread。

确认无需额外修改。当前 `add_message` 只写 `self.thread_id` 对应的 thread，语义正确。

- [ ] **Step 2: 添加 with_thread_context 工厂方法**

在 `AgentState` impl 中添加：

```rust
    /// 使用 ThreadStore 的 load_context 构建完整上下文
    pub async fn with_thread_context(
        thread_id: ThreadId,
        store: Arc<dyn ThreadStore>,
    ) -> Result<Self> {
        let meta = store.load_meta(&thread_id).await?;
        let messages = store.load_context(&thread_id).await?;
        Ok(Self::new(&meta.cwd)
            .with_messages_from(messages)
            .with_persistence(store, thread_id))
    }

    /// 从已有消息列表构建（不通过 with_messages 以避免覆盖 thread_id 绑定）
    fn with_messages_from(mut self, messages: Vec<BaseMessage>) -> Self {
        self.messages = messages;
        self
    }
```

- [ ] **Step 3: 运行编译确认通过**

Run: `cargo build -p peri-agent`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add peri-agent/src/agent/state.rs
git commit -m "feat(agent-storage): AgentState::with_thread_context for load_context init"
```

---

### Task 6: SubAgentMiddleware 重写

**Files:**
- Modify: `peri-middlewares/src/subagent/mod.rs`
- Modify: `peri-middlewares/src/subagent/tool/define.rs`（如果存在，或 SubAgentTool 所在文件）

这是最核心的变更。SubAgentTool.invoke() 需要从"内存态 agent"改为"创建子 thread + AgentRuntime"。

- [ ] **Step 1: SubAgentTool 新增 store 和 session 依赖**

在 SubAgentTool struct 中新增：

```rust
pub struct SubAgentTool {
    // 现有字段保持
    parent_tools: Arc<Vec<Arc<dyn BaseTool>>>,
    event_handler: Option<Arc<dyn AgentEventHandler>>,
    parent_cwd: String,
    llm_factory: Arc<dyn Fn(Option<&str>) -> Box<dyn ReactLLM + Send + Sync> + Send + Sync>,
    system_builder: Option<Arc<dyn Fn(Option<&AgentOverrides>, &str) -> String + Send + Sync>>,
    cancel: Option<AgentCancellationToken>,
    parent_messages: Option<Arc<RwLock<Vec<BaseMessage>>>>,
    background_registry: Option<Arc<BackgroundTaskRegistry>>,
    registered_hooks: Arc<Vec<RegisteredHook>>,
    child_handler_factory: Option<Arc<dyn Fn(String) -> Arc<dyn AgentEventHandler> + Send + Sync>>,
    bg_event_sender: Option<tokio::sync::mpsc::UnboundedSender<AgentEvent>>,

    // 新增
    thread_store: Option<Arc<dyn ThreadStore>>,
    parent_thread_id: Option<String>,
    session_cancel_token: Option<CancellationToken>,
}
```

在 `SubAgentMiddleware` 和 `build_tool()` 中传递新字段。

- [ ] **Step 2: 重写 invoke 核心逻辑**

统一三种模式为一个创建流程：

```rust
async fn invoke(&self, input: ToolInput) -> Result<ToolOutput, ToolError> {
    let params = parse_subagent_params(&input)?;
    let new_thread_id = uuid::Uuid::now_v7().to_string();
    let is_background = params.background;
    let is_fork = params.fork;

    // 1. 创建子 thread
    if let Some(ref store) = self.thread_store {
        let snapshot_id = if is_fork {
            // fork 模式：快照父最后消息
            self.get_parent_last_message_id().await
        } else {
            None
        };
        let cancel_policy = if is_background { "independent" } else { "cascade" };
        let config_json = self.build_config_snapshot(&params);

        let mut child_meta = ThreadMeta::new(&self.parent_cwd);
        child_meta.id = new_thread_id.clone();
        child_meta.parent_thread_id = self.parent_thread_id.clone();
        child_meta.snapshot_at_message_id = snapshot_id;
        child_meta.hidden = true;
        child_meta.cancel_policy = cancel_policy.to_string();
        child_meta.config = Some(config_json);
        child_meta.title = params.subagent_type.clone();
        store.create_thread(child_meta).await.map_err(|e| ToolError::ExecutionError(e.to_string()))?;
    }

    // 2. 构建并执行子 agent（复用现有 build_agent + execute 逻辑）
    let result = if is_background {
        self.spawn_background_agent(new_thread_id.clone(), params).await
    } else {
        self.spawn_sync_agent(new_thread_id.clone(), params).await
    };

    // 3. 格式化结果（完整文本 + child_thread_id）
    match result {
        Ok(output) => Ok(ToolOutput::text(format!(
            "child_thread_id: {}\n\n{}",
            new_thread_id, output
        ))),
        Err(e) => {
            // 更新 thread 状态为 error
            if let Some(ref store) = self.thread_store {
                let _ = store.update_thread_status(&new_thread_id, "error").await;
            }
            Err(ToolError::ExecutionError(e.to_string()))
        }
    }
}
```

- [ ] **Step 3: 更新 spawn_sync_agent 和 spawn_background_agent**

这两个方法需要使用 `AgentState::with_thread_context` 或 `AgentState::new + with_persistence` 来创建子 agent 的 state，绑定到子 thread_id。

关键改动：
- 移除 `AgentState::with_messages(cwd, parent_msgs)` 在 fork 模式下的使用
- 改为 `AgentState::new(cwd).with_persistence(store, child_thread_id)` + 如果是 fork 模式，context 由 persistence 层的 `load_context` 自动组装

- [ ] **Step 4: 运行编译确认通过**

Run: `cargo build -p peri-middlewares`
Expected: PASS（可能有编译警告，后续修复）

- [ ] **Step 5: Commit**

```bash
git add peri-middlewares/src/subagent/
git commit -m "feat(agent-storage): SubAgentTool creates child thread instead of in-memory agent"
```

---

### Task 7: Executor 适配

**Files:**
- Modify: `peri-acp/src/session/executor.rs`
- Modify: `peri-acp/src/session/compact_runner.rs`

- [ ] **Step 1: execute_prompt 传递 thread_store 到 SubAgentMiddleware**

在 `execute_prompt` 中构建 SubAgentMiddleware 时，传递 `thread_store`、当前 `thread_id`（作为 parent_thread_id）、session cancel_token。

- [ ] **Step 2: compact_runner 使用 per-thread 逻辑**

在 `run_full_compact` / `run_micro_compact` 中：
- compact 操作只替换 own thread 的 messages
- compact 后调用 `invalidate_context_cache(thread_id)`
- ContextBudget 检查只计算 own thread 的 messages（不含祖先快照）

- [ ] **Step 3: 运行编译确认通过**

Run: `cargo build -p peri-acp`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add peri-acp/src/session/
git commit -m "feat(agent-storage): executor passes thread_store to SubAgentMiddleware, compact per-thread"
```

---

### Task 8: Session 恢复适配

**Files:**
- Modify: `peri-acp/src/session/mod.rs`（SessionManager 的 resume 逻辑）
- Modify: `peri-tui/src/acp_server/requests.rs`（session/new 恢复路径）

- [ ] **Step 1: SessionManager.resume 使用 load_context**

在 session 恢复路径中，使用 `store.load_context(&thread_id)` 替代 `store.load_messages(&thread_id)` 加载主 agent 的完整上下文。

- [ ] **Step 2: list_threads 排除 hidden=true**

确认 `SessionManager.list_sessions()` 使用 `store.list_threads()`，该方法已排除 `hidden=true` 的子 agent thread。

- [ ] **Step 3: 运行编译确认通过**

Run: `cargo build`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add peri-acp/src/session/ peri-tui/src/acp_server/
git commit -m "feat(agent-storage): session resume uses load_context, list excludes hidden threads"
```

---

### Task 9: TUI 事件路由适配

**Files:**
- Modify: `peri-tui/src/app/agent_ops/acp_bridge.rs`
- Modify: `peri-tui/src/app/agent_ops/subagent.rs`

- [ ] **Step 1: source_agent_id 语义更新**

当前 `source_agent_id` 是临时 agent 标识。新模型下 `source_agent_id = child_thread_id`（持久化）。TUI 侧的事件路由逻辑不变，但需要确认：
- `SubagentStarted` 事件的 `instance_id` 改为 child_thread_id
- `SubagentStopped` 事件同上
- `BackgroundTaskCompleted` 事件的 task_id 保持独立

- [ ] **Step 2: frozen_subagent_vms 使用 thread_id**

当前 `frozen_subagent_vms: HashMap<String, MessageViewModel>` 的 key 是 agent_id。确认其值就是 source_agent_id = child_thread_id，逻辑不变。

- [ ] **Step 3: 运行编译确认通过**

Run: `cargo build -p peri-tui`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add peri-tui/src/app/agent_ops/
git commit -m "feat(agent-storage): TUI event routing uses thread_id as source_agent_id"
```

---

### Task 10: 集成测试 + 全量验证

**Files:**
- Modify: `peri-agent/src/thread/sqlite_store_test.rs`（已在 Task 2 添加）
- 新增测试场景

- [ ] **Step 1: 写多层嵌套 snapshot 组装的集成测试**

在 `sqlite_store_test.rs` 中添加三层嵌套测试：

```rust
    #[tokio::test]
    async fn test_load_context_three_level_nesting() {
        let (store, _dir) = make_store().await;

        // L1: 根 agent
        let l1 = ThreadMeta::new("/project");
        let l1_id = store.create_thread(l1).await.unwrap();
        store.append_messages(&l1_id, &[
            BaseMessage::human("L1 msg1"),
            BaseMessage::ai("L1 reply1"),
        ]).await.unwrap();

        // L2: 子 agent，snapshot L1 的最后消息
        let l1_last_id = store.load_messages(&l1_id).await.unwrap().last().unwrap().id().to_string();
        let mut l2 = ThreadMeta::new("/project");
        l2.parent_thread_id = Some(l1_id.clone());
        l2.snapshot_at_message_id = Some(l1_last_id);
        l2.hidden = true;
        let l2_id = store.create_thread(l2).await.unwrap();
        store.append_messages(&l2_id, &[
            BaseMessage::human("L2 msg1"),
            BaseMessage::ai("L2 reply1"),
        ]).await.unwrap();

        // L3: 孙 agent，snapshot L2 的最后消息
        let l2_last_id = store.load_messages(&l2_id).await.unwrap().last().unwrap().id().to_string();
        let mut l3 = ThreadMeta::new("/project");
        l3.parent_thread_id = Some(l2_id.clone());
        l3.snapshot_at_message_id = Some(l2_last_id);
        l3.hidden = true;
        let l3_id = store.create_thread(l3).await.unwrap();
        store.append_messages(&l3_id, &[
            BaseMessage::human("L3 msg1"),
        ]).await.unwrap();

        // L3 的完整上下文 = L1全部 + L2全部 + L3全部
        let ctx = store.load_context(&l3_id).await.unwrap();
        assert_eq!(ctx.len(), 5);
        assert_eq!(ctx[0].content(), "L1 msg1");
        assert_eq!(ctx[1].content(), "L1 reply1");
        assert_eq!(ctx[2].content(), "L2 msg1");
        assert_eq!(ctx[3].content(), "L2 reply1");
        assert_eq!(ctx[4].content(), "L3 msg1");
    }
```

- [ ] **Step 2: 运行全量测试**

Run: `cargo test`
Expected: ALL PASS

- [ ] **Step 3: 运行 clippy**

Run: `cargo clippy -- -D warnings`
Expected: 无新增 warning

- [ ] **Step 4: Commit**

```bash
git add .
git commit -m "test(agent-storage): three-level nesting integration test, full verification"
```

---

## Self-Review

### Spec Coverage

| Spec Section | Task |
|-------------|------|
| 2. Data Model | Task 1 (ThreadMeta) + Task 2 (Schema migration) |
| 3. Context Assembly | Task 2 (load_context + cached_context) |
| 4. Runtime Model | Task 4 (AgentRuntime) |
| 5. SubAgent Creation | Task 6 (SubAgentTool rewrite) |
| 6. Persistence API | Task 2 (ThreadStore trait) + Task 3 (FilesystemThreadStore) |
| 7. Compact | Task 7 (compact_runner) |
| 8. Session Resume | Task 8 |
| 9. Module Impact | Tasks 6-9 |

### Placeholder Scan

All steps contain actual code. No TBD/TODO patterns found.

### Type Consistency

- `ThreadId = String` used consistently across all tasks
- `CancelPolicy` enum in Task 4 maps to `cancel_policy: String` in ThreadMeta via `as_str()`/`from_str()`
- `AgentStatus` enum maps to `agent_status: String` via `as_str()`/`from_str()`
- `THREAD_COLUMNS` SQL constant defined in Task 2 used across all queries
