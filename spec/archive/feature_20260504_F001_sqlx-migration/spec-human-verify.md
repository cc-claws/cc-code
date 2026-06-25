# 人类验收清单：rusqlite → sqlx 异步迁移

> 来源: spec-plan.md (主) + spec-design.md (辅)
> 生成时间: 2026-05-04

---

## 场景 1: 依赖清理

#### - [x] 1.1 [A] peri-agent 不再依赖 rusqlite
```bash
cargo tree -p peri-agent | grep -E "rusqlite|parking_lot"
```
→ 期望精确: (空输出，无匹配)

**来源:** Task 1 + Task 4
**目的:** 确认旧依赖完全移除

#### - [x] 1.2 [A] peri-agent 正确引入 sqlx
```bash
cargo tree -p peri-agent | grep sqlx
```
→ 期望包含: sqlx

**来源:** Task 1
**目的:** 确认 sqlx 依赖生效

#### - [x] 1.3 [A] workspace Cargo.toml 包含 sqlx 条目
```bash
grep -n "sqlx" Cargo.toml
```
→ 期望包含: sqlx = { version = "0.8"

**来源:** Task 1 步骤 1
**目的:** 确认 workspace 依赖声明

#### - [x] 1.4 [A] peri-agent/Cargo.toml 无 rusqlite 和 parking_lot
```bash
grep -E "rusqlite|parking_lot" peri-agent/Cargo.toml
```
→ 期望精确: (空输出)

**来源:** Task 1 步骤 2
**目的:** 确认 crate 级依赖清理

#### - [x] 1.5 [A] workspace parking_lot 条目保留
```bash
grep "parking_lot" Cargo.toml
```
→ 期望包含: parking_lot

**来源:** Task 1 步骤 3 (design 补充)
**目的:** 确认不误删其他 crate 的依赖

---

## 场景 2: SqliteThreadStore 核心重写

#### - [x] 2.1 [A] 文件中无 rusqlite/parking_lot/spawn_blocking 残留
```bash
grep -n -E "rusqlite|parking_lot|spawn_blocking|Arc<Mutex" peri-agent/src/thread/sqlite_store.rs
```
→ 期望精确: (空输出)

**来源:** Task 2 检查步骤
**目的:** 确认旧 API 引用全部清除

#### - [x] 2.2 [A] 结构体使用 SqlitePool
```bash
grep -A2 "pub struct SqliteThreadStore" peri-agent/src/thread/sqlite_store.rs
```
→ 期望包含: pool: SqlitePool

**来源:** Task 2 步骤 2
**目的:** 确认结构体字段正确替换

#### - [x] 2.3 [A] new() 和 default_path() 签名为 async
```bash
grep -n "pub async fn new\|pub async fn default_path" peri-agent/src/thread/sqlite_store.rs
```
→ 期望包含: pub async fn new
→ 期望包含: pub async fn default_path

**来源:** Task 2 步骤 3-4 + 检查步骤
**目的:** 确认构造函数异步化

#### - [x] 2.4 [A] init_schema 拆分为多次 execute 调用
```bash
grep -c "sqlx::query" peri-agent/src/thread/sqlite_store.rs
```
→ 期望包含: CREATE TABLE IF NOT EXISTS threads
→ 期望包含: CREATE TABLE IF NOT EXISTS messages
→ 期望包含: CREATE INDEX IF NOT EXISTS

**来源:** Task 2 步骤 5 (plan 纠正 design)
**目的:** 确认单语句执行，避免 sqlx 多语句限制

#### - [x] 2.5 [A] 所有 7 个 trait 方法直接使用 sqlx::query
```bash
grep -c "sqlx::query" peri-agent/src/thread/sqlite_store.rs
```
→ 期望包含: sqlx::query (至少 7 次出现，覆盖 create/append/load_meta/load_messages/list/delete/update)

**来源:** Task 2 检查步骤
**目的:** 确认全部方法已重写为 sqlx

#### - [x] 2.6 [A] 文档注释已更新
```bash
grep "sqlx SqlitePool" peri-agent/src/thread/sqlite_store.rs
```
→ 期望包含: sqlx SqlitePool 连接池管理并发

**来源:** Task 2 步骤 15
**目的:** 确认注释与实现一致

---

## 场景 3: 单元测试通过

#### - [x] 3.1 [A] sqlite_store 5 个测试全部通过
```bash
cargo test -p peri-agent --lib -- thread::sqlite_store::tests
```
→ 期望包含: test result: ok

**来源:** Task 2 单元测试
**目的:** 确认核心存储逻辑正确

#### - [x] 3.2 [A] make_store() 已改为 async
```bash
grep -A2 "fn make_store" peri-agent/src/thread/sqlite_store.rs
```
→ 期望包含: async fn make_store

**来源:** Task 2 步骤 13
**目的:** 确认测试辅助函数异步化

#### - [x] 3.3 [A] 所有测试中 make_store() 调用有 .await
```bash
grep "make_store()" peri-agent/src/thread/sqlite_store.rs | grep -v "\.await"
```
→ 期望精确: (空输出，所有调用均带 .await)

**来源:** Task 2 步骤 14
**目的:** 确认无遗漏的 async 调用

---

## 场景 4: 调用方适配

#### - [x] 4.1 [A] App 不再实现 Default trait
```bash
grep -n "impl Default for App" peri-tui/src/app/mod.rs
```
→ 期望精确: (空输出)

**来源:** Task 3 步骤 1a
**目的:** 确认 Default impl 已移除

#### - [x] 4.2 [A] App::new() 签名为 async
```bash
grep "pub async fn new" peri-tui/src/app/mod.rs
```
→ 期望包含: pub async fn new

**来源:** Task 3 步骤 1b
**目的:** 确认 App 构造函数异步化

#### - [x] 4.3 [A] main.rs 中 App::new() 调用有 .await
```bash
grep "App::new()" peri-tui/src/main.rs
```
→ 期望包含: App::new().await

**来源:** Task 3 步骤 2
**目的:** 确认 main 入口正确 await

#### - [x] 4.4 [A] new_headless() 签名为 async
```bash
grep "pub async fn new_headless" peri-tui/src/app/panel_ops.rs
```
→ 期望包含: pub async fn new_headless

**来源:** Task 3 步骤 3a
**目的:** 确认测试辅助构造函数异步化

#### - [x] 4.5 [A] main_acp.rs 中 SqliteThreadStore 调用有 .await
```bash
grep -A5 "SqliteThreadStore::default_path" peri-tui/src/acp/main_acp.rs
```
→ 期望包含: .await

**来源:** Task 3 步骤 4
**目的:** 确认 ACP 入口正确 await

#### - [x] 4.6 [A] 所有 App::new_headless 调用有 .await
```bash
grep -rn "new_headless(" peri-tui/src/ | grep -v "\.await" | grep -v "pub async fn new_headless"
```
→ 期望精确: (空输出)

**来源:** Task 3 步骤 5
**目的:** 确认无遗漏的 new_headless 调用

#### - [x] 4.7 [A] headless_app() 辅助函数已改 async
```bash
grep -rn "fn headless_app" peri-tui/src/
```
→ 期望包含: async fn headless_app

**来源:** Task 3 步骤 6
**目的:** 确认辅助函数异步化

---

## 场景 5: 全量编译与测试

#### - [x] 5.1 [A] 全量编译通过
```bash
cargo build 2>&1
```
→ 期望精确: Finished

**来源:** Task 4 步骤 1
**目的:** 确认所有 crate 编译无错

#### - [x] 5.2 [A] 全量测试通过
```bash
cargo test 2>&1
```
→ 期望包含: test result: ok

**来源:** Task 4 步骤 2
**目的:** 确认所有测试通过

#### - [x] 5.3 [A] TUI 二进制可正常启动
```bash
cargo run -p peri-tui -- --help 2>&1
```
→ 期望包含: USAGE 或 --help

**来源:** Task 4 步骤 5
**目的:** 确认二进制可执行

---

## 场景 6: 边界与回归 (design 补充)

#### - [x] 6.1 [A] sqlx features 仅含 runtime-tokio + sqlite，无多余 feature
```bash
grep "sqlx" Cargo.toml
```
→ 期望包含: "runtime-tokio", "sqlite"
→ 期望精确: (不含 "macros" 或 "migrate")

**来源:** spec-design.md 依赖变更
**目的:** 确认最小 feature 集合

#### - [x] 6.2 [A] ThreadStore trait 接口未变更
```bash
grep -A20 "pub trait ThreadStore" peri-agent/src/thread/mod.rs
```
→ 期望包含: async fn create_thread
→ 期望包含: async fn append_messages
→ 期望包含: async fn load_messages
→ 期望包含: async fn load_meta
→ 期望包含: async fn update_meta
→ 期望包含: async fn list_threads
→ 期望包含: async fn delete_thread

**来源:** spec-design.md 不变项
**目的:** 确认 trait 接口稳定

#### - [x] 6.3 [A] Schema 结构未变更（threads + messages 两张表）
```bash
grep -A8 "CREATE TABLE IF NOT EXISTS threads" peri-agent/src/thread/sqlite_store.rs
```
→ 期望包含: id TEXT PRIMARY KEY
→ 期望包含: title TEXT
→ 期望包含: cwd TEXT NOT NULL DEFAULT ''
→ 期望包含: created_at TEXT NOT NULL
→ 期望包含: updated_at TEXT NOT NULL
→ 期望包含: message_count INTEGER NOT NULL DEFAULT 0

**来源:** spec-design.md 不变项
**目的:** 确认数据模型兼容

#### - [x] 6.4 [A] SQLite 连接池配置正确
```bash
grep -A3 "SqlitePoolOptions" peri-agent/src/thread/sqlite_store.rs
```
→ 期望包含: max_connections(5)
→ 期望包含: journal_mode
→ 期望包含: WAL
→ 期望包含: foreign_keys
→ 期望包含: ON

**来源:** spec-design.md 实现方案
**目的:** 确认连接池和 PRAGMA 配置

#### - [x] 6.5 [A] append_messages 使用事务
```bash
grep -n "pool.begin\|tx.commit" peri-agent/src/thread/sqlite_store.rs
```
→ 期望包含: pool.begin
→ 期望包含: tx.commit

**来源:** spec-design.md 实现方案
**目的:** 确认写操作事务保护

---

## 验收后清理

此功能无持久化服务或后台进程，无需清理。
