# 执行计划：rusqlite 迁移到 sqlx

## 功能名称
rusqlite → sqlx 异步迁移

## 目标
将 `peri-agent` 的线程持久化层从 `rusqlite`（同步 + spawn_blocking）迁移到 `sqlx`（原生 async），去掉 `parking_lot::Mutex` + `spawn_blocking` 模式，简化异步代码。

## 技术栈
- Rust 2021, tokio async/await, sqlx 0.8 (runtime-tokio + sqlite), serde_json

## 设计文档路径
`spec/feature_20260504_F001_sqlx-migration/spec-design.md`

## 改动总览

- **涉及文件（6 个）**: `Cargo.toml`、`peri-agent/Cargo.toml`、`peri-agent/src/thread/sqlite_store.rs`、`peri-tui/src/app/mod.rs`、`peri-tui/src/app/panel_ops.rs`、`peri-tui/src/acp/main_acp.rs`
- **Task 依赖**: Task 1（依赖）→ Task 2（核心重写）→ Task 3（调用方适配），严格顺序执行
- **关键设计决策**: sqlx 仅用 runtime-tokio + sqlite features，不引入 macros/migrate；`App::new()` 和 `new_headless()` 变 async；移除 `Default` impl

---

### Task 0: 环境准备

#### 执行步骤

1. 确认当前代码可编译:
   ```bash
   cargo build -p peri-agent
   cargo build -p peri-tui
   ```

2. 确认现有测试通过:
   ```bash
   cargo test -p peri-agent --lib -- thread
   ```

#### 检查步骤

- `cargo build` 和 `cargo test` 均无错误

---

### Task 1: 依赖变更

#### 背景

`peri-agent` 当前依赖 `rusqlite 0.31`（bundled）和 `parking_lot 0.12`。迁移到 sqlx 需要替换这两个依赖。`parking_lot` 在 `peri-agent` 内仅被 `sqlite_store.rs` 使用，可安全移除。

#### 涉及文件

- `Cargo.toml`（workspace 根）
- `peri-agent/Cargo.toml`

#### 执行步骤

1. 在 workspace 根 `Cargo.toml` 的 `[workspace.dependencies]` 末尾（第 61 行 `url = "2"` 之后）添加:
   ```toml
   sqlx = { version = "0.8", features = ["runtime-tokio", "sqlite"] }
   ```

2. 编辑 `peri-agent/Cargo.toml`:
   - 删除第 23 行: `rusqlite = { version = "0.31", features = ["bundled"] }`
   - 删除第 24 行: `parking_lot.workspace = true`
   - 在原位置添加: `sqlx = { workspace = true }`

3. workspace 根的 `parking_lot = "0.12"`（第 59 行）保留不动——其他 crate（如 `peri-middlewares`）仍使用。

#### 检查步骤

- 确认 `peri-agent/Cargo.toml` 中无 `rusqlite` 和 `parking_lot` 引用
- 确认 workspace `Cargo.toml` 中有 `sqlx` 条目
- 此时 `cargo build -p peri-agent` 会报编译错误（因为 `sqlite_store.rs` 仍引用 `rusqlite`），这是预期的，Task 2 会修复

#### 单元测试

- 此 Task 为依赖变更，无新增逻辑。编译验证在 Task 2 完成后进行。

---

### Task 2: SqliteThreadStore 重写

#### 背景

`sqlite_store.rs` 当前使用 `Arc<Mutex<Connection>>` + `spawn_blocking` 模式在 tokio 中桥接同步 rusqlite。迁移到 sqlx 后，所有数据库操作天然 async，无需 `Mutex`、`spawn_blocking`、`Arc<Connection>`。

#### 涉及文件

- `peri-agent/src/thread/sqlite_store.rs`

#### 执行步骤

1. **替换 imports**（第 1-10 行）:

   将:
   ```rust
   use anyhow::{Context, Result};
   use async_trait::async_trait;
   use chrono::{DateTime, Utc};
   use parking_lot::Mutex;
   use rusqlite::{params, Connection};
   use std::path::PathBuf;
   use std::sync::Arc;
   ```

   替换为:
   ```rust
   use anyhow::{Context, Result};
   use async_trait::async_trait;
   use chrono::{DateTime, Utc};
   use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
   use sqlx::SqlitePool;
   use std::path::PathBuf;
   use std::str::FromStr;
   ```

2. **替换结构体**（第 15-17 行）:

   将:
   ```rust
   pub struct SqliteThreadStore {
       conn: Arc<Mutex<Connection>>,
   }
   ```

   替换为:
   ```rust
   pub struct SqliteThreadStore {
       pool: SqlitePool,
   }
   ```

3. **重写 `new()` 方法**（第 21-39 行）:

   将:
   ```rust
   pub fn new(db_path: impl Into<PathBuf>) -> Result<Self> {
       let db_path = db_path.into();
       if let Some(parent) = db_path.parent() {
           std::fs::create_dir_all(parent)
               .with_context(|| format!("创建目录失败: {}", parent.display()))?;
       }
       let conn = Connection::open(&db_path)
           .with_context(|| format!("打开 SQLite 失败: {}", db_path.display()))?;
       conn.execute_batch(
           "PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA foreign_keys=ON;",
       )?;
       let store = Self {
           conn: Arc::new(Mutex::new(conn)),
       };
       store.init_schema()?;
       Ok(store)
   }
   ```

   替换为:
   ```rust
   pub async fn new(db_path: impl Into<PathBuf>) -> Result<Self> {
       let db_path = db_path.into();
       if let Some(parent) = db_path.parent() {
           std::fs::create_dir_all(parent)
               .with_context(|| format!("创建目录失败: {}", parent.display()))?;
       }
       let options = SqliteConnectOptions::from_str(&format!(
           "sqlite://{}?mode=rwc",
           db_path.display()
       ))?
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
   ```

4. **重写 `default_path()`**（第 42-49 行）:

   将:
   ```rust
   pub fn default_path() -> Result<Self> {
   ```
   替换为:
   ```rust
   pub async fn default_path() -> Result<Self> {
   ```

   并将最后一行 `Self::new(db_path)` 改为 `Self::new(db_path).await`。

5. **重写 `init_schema()`**（第 52-78 行）:

   将:
   ```rust
   fn init_schema(&self) -> Result<()> {
       let conn = self.conn.lock();
       conn.execute_batch("...")?;
       Ok(())
   }
   ```

   替换为:
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

   注意：sqlx 的 `query().execute()` 不支持多条 SQL 语句。需要拆分为 3 次调用:
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
           )"
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
           )"
       )
       .execute(&self.pool)
       .await?;

       sqlx::query(
           "CREATE INDEX IF NOT EXISTS idx_messages_thread_id ON messages (thread_id ASC)"
       )
       .execute(&self.pool)
       .await?;

       Ok(())
   }
   ```

6. **重写 `create_thread`**（第 145-166 行）:

   将:
   ```rust
   async fn create_thread(&self, meta: ThreadMeta) -> Result<ThreadId> {
       let id = meta.id.clone();
       let conn = self.conn.clone();
       tokio::task::spawn_blocking(move || -> Result<()> {
           let conn = conn.lock();
           conn.execute(
               "INSERT INTO threads ...",
               params![...],
           )?;
           Ok(())
       })
       .await??;
       Ok(id)
   }
   ```

   替换为:
   ```rust
   async fn create_thread(&self, meta: ThreadMeta) -> Result<ThreadId> {
       let id = meta.id.clone();
       sqlx::query(
           "INSERT INTO threads (id, title, cwd, created_at, updated_at, message_count)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)"
       )
       .bind(&meta.id)
       .bind(&meta.title)
       .bind(&meta.cwd)
       .bind(meta.created_at.to_rfc3339())
       .bind(meta.updated_at.to_rfc3339())
       .bind(meta.message_count as i64)
       .execute(&self.pool)
       .await?;
       Ok(id)
   }
   ```

7. **重写 `append_messages`**（第 168-208 行）:

   替换为:
   ```rust
   async fn append_messages(&self, id: &ThreadId, msgs: &[BaseMessage]) -> Result<()> {
       if msgs.is_empty() {
           return Ok(());
       }
       let mut tx = self.pool.begin().await?;
       for msg in msgs {
           let message_id = msg.id().as_uuid().to_string();
           let role = role_of(msg);
           let content = serde_json::to_string(msg)?;
           sqlx::query(
               "INSERT OR IGNORE INTO messages (message_id, thread_id, role, content)
                VALUES (?1, ?2, ?3, ?4)"
           )
           .bind(&message_id)
           .bind(id.as_str())
           .bind(role)
           .bind(&content)
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

8. **重写 `load_messages`**（第 210-231 行）:

   替换为:
   ```rust
   async fn load_messages(&self, id: &ThreadId) -> Result<Vec<BaseMessage>> {
       let rows: Vec<(String,)> = sqlx::query_as(
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

9. **重写 `load_meta`**（第 233-262 行）:

   替换为:
   ```rust
   async fn load_meta(&self, id: &ThreadId) -> Result<ThreadMeta> {
       let row: (String, Option<String>, String, String, String, i64, i64) = sqlx::query_as(
           "SELECT t.id, t.title, t.cwd, t.created_at, t.updated_at, t.message_count,
                   (SELECT COALESCE(SUM(LENGTH(m.content)), 0) FROM messages m WHERE m.thread_id = t.id) as content_size
            FROM threads t WHERE t.id = ?1"
       )
       .bind(id.as_str())
       .fetch_one(&self.pool)
       .await?;

       meta_from_row(row.0, row.1, row.2, row.3, row.4, row.5, row.6)
   }
   ```

10. **重写 `update_meta`**（第 264-283 行）:

    替换为:
    ```rust
    async fn update_meta(&self, id: &ThreadId, meta: ThreadMeta) -> Result<()> {
        sqlx::query(
            "UPDATE threads SET title = ?1, cwd = ?2, updated_at = ?3, message_count = ?4 WHERE id = ?5"
        )
        .bind(&meta.title)
        .bind(&meta.cwd)
        .bind(meta.updated_at.to_rfc3339())
        .bind(meta.message_count as i64)
        .bind(id.as_str())
        .execute(&self.pool)
        .await?;
        Ok(())
    }
    ```

11. **重写 `list_threads`**（第 285-316 行）:

    替换为:
    ```rust
    async fn list_threads(&self) -> Result<Vec<ThreadMeta>> {
        let rows: Vec<(String, Option<String>, String, String, String, i64, i64)> = sqlx::query_as(
            "SELECT t.id, t.title, t.cwd, t.created_at, t.updated_at, t.message_count,
                    (SELECT COALESCE(SUM(LENGTH(m.content)), 0) FROM messages m WHERE m.thread_id = t.id) as content_size
             FROM threads t ORDER BY t.updated_at DESC"
        )
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter()
            .map(|row| meta_from_row(row.0, row.1, row.2, row.3, row.4, row.5, row.6))
            .collect()
    }
    ```

12. **重写 `delete_thread`**（第 318-331 行）:

    替换为:
    ```rust
    async fn delete_thread(&self, id: &ThreadId) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("DELETE FROM threads WHERE id = ?1")
            .bind(id.as_str())
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }
    ```

13. **更新测试辅助函数**（第 341-344 行）:

    将:
    ```rust
    fn make_store() -> SqliteThreadStore {
        let dir = tempdir().unwrap();
        SqliteThreadStore::new(dir.path().join("test.db")).unwrap()
    }
    ```
    替换为:
    ```rust
    async fn make_store() -> SqliteThreadStore {
        let dir = tempdir().unwrap();
        SqliteThreadStore::new(dir.path().join("test.db")).await.unwrap()
    }
    ```

14. **更新所有测试中的 `make_store()` 调用**（第 348、363、389、410、437 行附近）:

    将所有 `make_store()` 替换为 `make_store().await`。具体位置:
    - 第 348 行 `let store = make_store();` → `let store = make_store().await;`
    - 第 363 行 `let store = make_store();` → `let store = make_store().await;`
    - 第 389 行 `let store = make_store();` → `let store = make_store().await;`
    - 第 410 行 `let store = make_store();` → `let store = make_store().await;`
    - 第 437 行 `let store = make_store();` → `let store = make_store().await;`

15. **更新文档注释**（第 12-14 行）:

    将:
    ```rust
    /// 使用 WAL 模式提升并发读性能，parking_lot::Mutex 串行化写操作。
    ```
    替换为:
    ```rust
    /// 使用 WAL 模式提升并发读性能，sqlx SqlitePool 连接池管理并发。
    ```

#### 检查步骤

- 确认文件中无 `rusqlite`、`parking_lot`、`spawn_blocking` 引用
- 确认所有 7 个 trait 方法直接使用 `sqlx::query` 而非 `spawn_blocking`
- 确认 `new()` 和 `default_path()` 签名为 `async fn`
- 编译通过: `cargo build -p peri-agent`

#### 单元测试

- 运行现有 5 个测试，确认全部通过:
  ```bash
  cargo test -p peri-agent --lib -- thread::sqlite_store::tests
  ```
- 测试覆盖: create+load、list 排序、delete cascade、消息顺序、标题自动设置

---

### Task 3: 调用方适配

#### 背景

`SqliteThreadStore::new()` 和 `default_path()` 从 sync 变为 async，导致所有调用处需要加 `.await`。`App::new()` 和 `App::new_headless()` 也需相应变为 async。`Default for App` impl 无法保持（`default()` 必须 sync），需移除。

#### 涉及文件

- `peri-tui/src/app/mod.rs`
- `peri-tui/src/app/panel_ops.rs`
- `peri-tui/src/acp/main_acp.rs`
- `peri-tui/src/main.rs`
- `peri-tui/src/ui/headless.rs`
- `peri-tui/src/command/mod.rs`
- `peri-tui/src/command/loop_cmd.rs`
- `peri-tui/src/ui/main_ui/popups/oauth.rs`

#### 执行步骤

1. **`peri-tui/src/app/mod.rs`** — `App::new()` 变 async:

   a. 移除 `Default for App` impl（约第 126-129 行）:
      ```rust
      // 删除这段
      impl Default for App {
          fn default() -> Self {
              Self::new()
          }
      }
      ```

   b. 将 `pub fn new() -> Self`（第 132 行）改为 `pub async fn new() -> Self`。

   c. 修改第 159-164 行的 SqliteThreadStore 初始化:
      ```rust
      // 旧:
      let thread_store: Arc<dyn ThreadStore> =
          Arc::new(SqliteThreadStore::default_path().unwrap_or_else(|_| {
              SqliteThreadStore::new(std::env::temp_dir().join("zen-threads.db"))
                  .expect("无法创建临时 SQLite 数据库")
          }));
      ```
      替换为:
      ```rust
      let thread_store: Arc<dyn ThreadStore> = Arc::new(
          SqliteThreadStore::default_path().await.unwrap_or_else(|_| {
              SqliteThreadStore::new(std::env::temp_dir().join("zen-threads.db"))
                  .await
                  .expect("无法创建临时 SQLite 数据库")
          }),
      );
      ```

2. **`peri-tui/src/main.rs`** — `App::new()` 调用加 await:

   将第 167 行:
   ```rust
   let mut app = App::new();
   ```
   替换为:
   ```rust
   let mut app = App::new().await;
   ```

3. **`peri-tui/src/app/panel_ops.rs`** — `new_headless()` 变 async:

   a. 将 `pub fn new_headless(width: u16, height: u16) -> (App, HeadlessHandle)`（第 434 行）改为 `pub async fn new_headless(width: u16, height: u16) -> (App, HeadlessHandle)`。

   b. 修改第 445-450 行:
      ```rust
      // 旧:
      let thread_store: Arc<dyn ThreadStore> = Arc::new(
          SqliteThreadStore::new(std::env::temp_dir().join(db_name))
              .expect("无法创建测试用 SQLite 数据库"),
      );
      ```
      替换为:
      ```rust
      let thread_store: Arc<dyn ThreadStore> = Arc::new(
          SqliteThreadStore::new(std::env::temp_dir().join(db_name))
              .await
              .expect("无法创建测试用 SQLite 数据库"),
      );
      ```

4. **`peri-tui/src/acp/main_acp.rs`** — 调用加 await:

   将第 39-43 行:
   ```rust
   let thread_store: Arc<dyn ThreadStore> =
       Arc::new(SqliteThreadStore::default_path().unwrap_or_else(|_| {
           SqliteThreadStore::new(std::env::temp_dir().join("peri-acp-threads.db"))
               .expect("无法创建临时数据库")
       }));
   ```
   替换为:
   ```rust
   let thread_store: Arc<dyn ThreadStore> = Arc::new(
       SqliteThreadStore::default_path().await.unwrap_or_else(|_| {
           SqliteThreadStore::new(std::env::temp_dir().join("peri-acp-threads.db"))
               .await
               .expect("无法创建临时数据库")
       }),
   );
   ```

5. **全局替换 `App::new_headless(` → `App::new_headless(` + `.await`**:

   在以下文件中，将每个 `App::new_headless(...)` 调用改为 `App::new_headless(...).await`:

   - `peri-tui/src/ui/headless.rs`: 所有 `App::new_headless(...)` 和 `crate::app::App::new_headless(...)` 调用（约 20+ 处）
   - `peri-tui/src/command/mod.rs:200`: `App::new_headless(80, 24).0` → `App::new_headless(80, 24).await.0`（在 `fn headless_app()` 中，该函数也需改为 `async fn`，调用处加 `.await`）
   - `peri-tui/src/command/loop_cmd.rs:51`: 同上
   - `peri-tui/src/ui/main_ui/popups/oauth.rs:85,105`: `crate::app::App::new_headless(80, 30)` → `crate::app::App::new_headless(80, 30).await`

6. **辅助函数 `headless_app()` 改 async**:

   `peri-tui/src/command/mod.rs:199-201`:
   ```rust
   // 旧:
   fn headless_app() -> App {
       App::new_headless(80, 24).0
   }
   ```
   替换为:
   ```rust
   async fn headless_app() -> App {
       App::new_headless(80, 24).await.0
   }
   ```
   调用 `headless_app()` 的测试方法中加 `.await`。

   同理修改 `peri-tui/src/command/loop_cmd.rs:50-52` 中的 `headless_app()`。

#### 检查步骤

- 确认 `App` 不再实现 `Default` trait
- 确认 `App::new()` 和 `App::new_headless()` 签名为 `async fn`
- 确认所有 `SqliteThreadStore::new()` 和 `default_path()` 调用后有 `.await`
- 确认所有 `App::new()` 和 `App::new_headless()` 调用后有 `.await`
- 全量编译: `cargo build -p peri-tui`
- 全量测试: `cargo test -p peri-tui`

#### 单元测试

- 运行全量测试确认所有 headless 测试通过:
  ```bash
  cargo test -p peri-tui
  ```
- 特别关注 `headless.rs` 中的测试（~20+ 个）和 `command/mod.rs`、`command/loop_cmd.rs` 中的测试

---

### Task 4: 验收

#### 前置条件

- Task 1、2、3 全部完成

#### 执行步骤

1. **全量编译**:
   ```bash
   cargo build
   ```

2. **全量测试**:
   ```bash
   cargo test
   ```

3. **验证依赖清理**:
   ```bash
   cargo tree -p peri-agent | grep -E "rusqlite|parking_lot"
   ```
   预期输出为空（rusqlite 和 parking_lot 不再是 peri-agent 的依赖）。

4. **验证 sqlx 依赖**:
   ```bash
   cargo tree -p peri-agent | grep sqlx
   ```
   预期输出包含 `sqlx` 及其 sqlite 相关子依赖。

5. **运行 TUI 冒烟测试**:
   ```bash
   cargo run -p peri-tui -- --help
   ```
   确认二进制可正常启动。

#### 失败指引

- 编译失败 → 检查 Task 1 依赖是否正确、Task 2 中 imports 是否完整
- `thread::sqlite_store` 测试失败 → Task 2 重写逻辑有误，检查 SQL 语句和 bind 参数
- headless 测试失败 → Task 3 中遗漏 `.await`，全局搜索 `App::new_headless(` 确认所有调用点
- 依赖树仍含 rusqlite → Task 1 中 `peri-agent/Cargo.toml` 未完全移除

