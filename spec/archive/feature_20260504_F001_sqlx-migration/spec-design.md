# 设计文档：rusqlite 迁移到 sqlx

## 概述

将 `peri-agent` 的线程持久化层从 `rusqlite`（同步 + `spawn_blocking`）迁移到 `sqlx`（原生 async），简化异步代码并去掉手动 `Mutex` + `spawn_blocking` 模式。

## 动机

- **原生 async**：`sqlx` 的 `SqlitePool` 天然 async，无需 `spawn_blocking` 桥接 tokio
- **去掉 Mutex**：`parking_lot::Mutex<Connection>` 串行化写操作由 sqlx 内置连接池替代
- **初创项目统一选型**：未来如需支持其他数据库，sqlx 可平滑扩展

## 范围

- **仅迁移** `peri-agent/src/thread/sqlite_store.rs`
- `ThreadStore` trait 接口不变，调用方（`peri-tui`）仅需调整初始化处加 `.await`
- 不引入 sqlx macros/migrate，仅用 runtime API

## 依赖变更

### `peri-agent/Cargo.toml`

```diff
- rusqlite = { version = "0.31", features = ["bundled"] }
- parking_lot.workspace = true
+ sqlx = { workspace = true }
```

### Workspace `Cargo.toml` `[workspace.dependencies]`

```toml
sqlx = { version = "0.8", features = ["runtime-tokio", "sqlite"] }
```

> 注意：不加 `"macros"` 和 `"migrate"` feature。`bundled` SQLite 通过 sqlx 的 `sqlite` feature 的 `bundled` 子 feature 启用：
>
> ```toml
> sqlx = { version = "0.8", features = ["runtime-tokio", "sqlite"] }
> ```
>
> 实际 bundling 由 `bundled-sqlite` feature 控制（sqlx 0.8 默认在 `sqlite` feature 下启用 bundled）。

### parking_lot 检查

需确认 `peri-agent` 内是否有其他模块使用 `parking_lot`。若无，可安全移除。

## 实现方案

### 结构体变更

```rust
// 旧
pub struct SqliteThreadStore {
    conn: Arc<Mutex<Connection>>,
}

// 新
pub struct SqliteThreadStore {
    pool: SqlitePool,
}
```

### 初始化

```rust
pub async fn new(db_path: impl Into<PathBuf>) -> Result<Self> {
    let db_path = db_path.into();
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let options = SqliteConnectOptions::from_str(
        &format!("sqlite://{}?mode=rwc", db_path.display())
    )?
    .pragma("journal_mode", "WAL")
    .pragma("synchronous", "NORMAL")
    .pragma("foreign_keys", "ON");

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await?;

    let store = Self { pool };
    store.init_schema().await?;
    Ok(store)
}

pub async fn default_path() -> Result<Self> {
    let db_path = dirs_next::home_dir()
        .context("无法获取 home 目录")?
        .join(".peri")
        .join("threads")
        .join("threads.db");
    Self::new(db_path).await
}
```

### Schema 初始化

```rust
async fn init_schema(&self) -> Result<()> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS threads (
            id          TEXT PRIMARY KEY,
            title       TEXT,
            cwd         TEXT NOT NULL DEFAULT '',
            created_at  TEXT NOT NULL,
            updated_at  TEXT NOT NULL,
            message_count INTEGER NOT NULL DEFAULT 0
        );
        CREATE TABLE IF NOT EXISTS messages (
            message_id  TEXT PRIMARY KEY,
            thread_id   TEXT NOT NULL,
            role        TEXT NOT NULL,
            content     TEXT NOT NULL,
            FOREIGN KEY (thread_id) REFERENCES threads(id) ON DELETE CASCADE
        );
        CREATE INDEX IF NOT EXISTS idx_messages_thread_id
            ON messages (thread_id ASC);"
    )
    .execute(&self.pool)
    .await?;
    Ok(())
}
```

### 各方法示例

**`append_messages`**（典型事务操作）：

```rust
async fn append_messages(&self, id: &ThreadId, msgs: &[BaseMessage]) -> Result<()> {
    if msgs.is_empty() { return Ok(()); }

    let mut tx = self.pool.begin().await?;
    for msg in &msgs {
        let message_id = msg.id().as_uuid().to_string();
        let role = role_of(msg);
        let content = serde_json::to_string(msg)?;
        sqlx::query(
            "INSERT OR IGNORE INTO messages (message_id, thread_id, role, content)
             VALUES (?1, ?2, ?3, ?4)"
        )
        .bind(message_id)
        .bind(id.as_str())
        .bind(role)
        .bind(content)
        .execute(&mut *tx)
        .await?;
    }
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "UPDATE threads SET updated_at = ?1,
            message_count = (SELECT COUNT(*) FROM messages WHERE thread_id = ?2)
         WHERE id = ?2"
    )
    .bind(&now)
    .bind(id.as_str())
    .execute(&mut *tx)
    .await?;

    if let Some(title) = extract_title(msgs) {
        sqlx::query("UPDATE threads SET title = ?1 WHERE id = ?2 AND title IS NULL")
            .bind(&title)
            .bind(id.as_str())
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(())
}
```

**`load_messages`**（查询）：

```rust
async fn load_messages(&self, id: &ThreadId) -> Result<Vec<BaseMessage>> {
    let rows = sqlx::query_as::<_, (String,)>(
        "SELECT content FROM messages WHERE thread_id = ?1 ORDER BY rowid"
    )
    .bind(id.as_str())
    .fetch_all(&self.pool)
    .await?;

    rows.into_iter()
        .map(|(content,)| serde_json::from_str(&content).map_err(Into::into))
        .collect()
}
```

## 调用方变更

`new()` 和 `default_path()` 从 sync 变为 async，需调整 3 处调用：

### 1. `peri-tui/src/app/mod.rs`

```rust
// 旧
let thread_store: Arc<dyn ThreadStore> = Arc::new(
    SqliteThreadStore::default_path().unwrap_or_else(|_| {
        SqliteThreadStore::new(std::env::temp_dir().join("zen-threads.db"))
            .expect("无法创建临时 SQLite 数据库")
    })
);

// 新
let thread_store: Arc<dyn ThreadStore> = Arc::new(
    SqliteThreadStore::default_path().await.unwrap_or_else(|_| {
        SqliteThreadStore::new(std::env::temp_dir().join("zen-threads.db"))
            .await
            .expect("无法创建临时 SQLite 数据库")
    })
);
```

### 2. `peri-tui/src/acp/main_acp.rs`

同上模式，加 `.await`。

### 3. `peri-tui/src/app/panel_ops.rs`（测试辅助）

同上模式，加 `.await`。

## 测试调整

`sqlite_store.rs` 内现有 5 个测试用例无需修改逻辑，仅因 `new()` 变 async 需确保在 `#[tokio::test]` 环境下调用 `new().await`（当前已是 `#[tokio::test]`，无需额外改动）。

`make_store()` 辅助函数需改为 async：

```rust
async fn make_store() -> SqliteThreadStore {
    let dir = tempdir().unwrap();
    SqliteThreadStore::new(dir.path().join("test.db")).await.unwrap()
}
```

## 风险与缓解

| 风险 | 缓解 |
|------|------|
| sqlx bundled 编译时间较长 | 初创阶段可接受；后续可考虑 CI 缓存 |
| `SqliteConnectOptions` 路径格式 | `sqlite:///absolute/path?mode=rwc`，需注意 URL 编码 |
| 连接池在 SQLite 场景下收益有限 | `max_connections(5)` 足够，SQLite 写仍串行（WAL 下读并行） |

## 不变项

- `ThreadStore` trait 接口不变
- Schema 不变（threads + messages 两张表）
- 辅助函数 `role_of` / `meta_from_row` / `extract_title` 逻辑不变
- 测试用例逻辑不变
