# MCP 管理面板与后台初始化 执行计划

**目标:** 将 MCP 连接池从同步阻塞初始化改为后台异步初始化，新增 /mcp 命令面板支持服务器状态查看、重连和删除。

**技术栈:** Rust, tokio async/await, tokio::sync::watch, parking_lot::RwLock, ratatui (BorderedPanel + ScrollableArea), peri-widgets

**设计文档:** spec/feature_20260502_F002_mcp-management/spec-design.md

## 改动总览

本次改动横跨 `peri-middlewares/src/mcp/`（数据层）和 `peri-tui/src/`（TUI 层）两个 crate，共 11 个文件（3 新建 + 8 修改）。Task 1-2 在 middlewares 层扩展 McpClientPool 和配置持久化能力，Task 3-5 在 TUI 层实现后台初始化、命令注册、面板交互和渲染。依赖关系：Task 1 → Task 3（后台初始化需要 new_pending/run_initialize），Task 1+2 → Task 4（面板操作需要 server_infos/remove_server_from_config），Task 4 → Task 5（渲染需要面板数据结构）。关键设计决策：`McpClientPool.clients` 改为 `parking_lot::RwLock<HashMap>` 支持并发读写（经确认 parking_lot 已是项目依赖）；配置删除使用 `serde_json::Value` 操作全局 settings.json 以保留其他字段。

---

### Task 0: 环境准备

**背景:**
确保构建和测试工具链在当前开发环境中可用，避免后续 Task 因环境问题阻塞。

**执行步骤:**

- [x] 验证 workspace 构建可用
  - 运行 `cargo build` 确认所有 crate 编译成功
- [x] 验证测试框架可用
  - 运行 `cargo test -p peri-middlewares --lib -- mcp::client::tests` 确认 MCP 模块现有测试通过
  - 运行 `cargo test -p peri-tui --lib` 确认 TUI 模块现有测试通过

**检查步骤:**

- [x] 构建成功
  - `cargo build 2>&1 | tail -5`
  - 预期: 输出包含 `Finished` 且无 error
- [x] MCP 模块测试通过
  - `cargo test -p peri-middlewares --lib -- mcp:: 2>&1 | tail -10`
  - 预期: 所有 MCP 相关测试通过
- [x] TUI 模块测试通过
  - `cargo test -p peri-tui --lib 2>&1 | tail -10`
  - 预期: 所有测试通过

---

### Task 1: McpInitStatus 状态机与 McpClientPool 扩展

**背景:**
MCP 连接池当前通过 `McpClientPool::initialize()` 同步阻塞初始化，用户无法感知初始化进度，也无法在运行时管理服务器连接。本 Task 新增 `McpInitStatus` 枚举追踪初始化进度，扩展 `McpClientPool` 支持后台初始化、重连失败服务器、删除服务器和查询服务器详情，为 Task 3 的 TUI 后台初始化和 Task 4/5 的管理面板提供数据层基础。

**涉及文件:**

- 修改: `peri-middlewares/src/mcp/client.rs`
- 修改: `peri-middlewares/src/mcp/mod.rs`

**执行步骤:**

- [x] 在 client.rs 中定义 McpInitStatus 枚举，位于 ClientStatus 枚举之后（~L18）
  - 位置: `client.rs:McpInitStatus`（L18 之后，McpPoolError 之前）
  - 定义四个变体：`Pending`、`Initializing { connected: usize, total: usize }`、`Ready { total: usize }`、`Failed(String)`
  - 派生 `Debug, Clone, PartialEq`
  - 原因: TUI 状态栏和 agent task 通过此枚举判断 MCP 是否就绪

- [x] 将 McpClientPool 的 `clients` 字段改为 `RwLock<HashMap<String, Arc<McpClientHandle>>>`，新增 `configs` 字段
  - 位置: `client.rs:McpClientPool` 结构体定义（L43-46）
  - 将 `clients: HashMap<...>` 改为 `clients: parking_lot::RwLock<HashMap<String, Arc<McpClientHandle>>>`
  - 新增 `configs: HashMap<String, McpServerConfig>` 字段，保存原始配置用于重连
  - 新增 `use super::config::McpServerConfig;` 导入（文件顶部）
  - 原因: `reconnect`/`remove_server`/`server_infos` 需要并发读写访问 `clients`，`configs` 保存配置快照供重连使用

- [x] 将 McpClientPool 的 `services` 字段改为 `Mutex<Vec<RunningService<RoleClient, ()>>>`
  - 位置: `client.rs:McpClientPool` 结构体定义（L43-46，紧接 clients 修改）
  - 将 `services: Vec<...>` 改为 `services: tokio::sync::Mutex<Vec<RunningService<RoleClient, ()>>>`
  - 原因: `reconnect` 和 `remove_server` 需要在 async 上下文中安全地增删 services

- [x] 新增 `new_pending()` 构造函数
  - 位置: `client.rs:impl McpClientPool`（L52，`initialize` 方法之前）
  - 创建空 clients（RwLock）、空 services（Mutex）、空 configs 的 McpClientPool
  - 将现有 `new_empty()` 改为调用 `new_pending()` 的别名，移除 `#[cfg(test)]` 限制
  - 伪代码:

    ```rust
    pub fn new_pending() -> Self {
        Self {
            clients: parking_lot::RwLock::new(HashMap::new()),
            services: tokio::sync::Mutex::new(Vec::new()),
            configs: HashMap::new(),
        }
    }
    #[cfg(test)]
    pub fn new_empty() -> Self {
        Self::new_pending()
    }
    ```

  - 原因: 后台初始化流程需要一个公开的空池构造器，`new_empty` 保留兼容性

- [x] 新增 `run_initialize()` 静态方法
  - 位置: `client.rs:impl McpClientPool`（L52 区域，`new_pending` 之后，`initialize` 之前）
  - 签名: `pub async fn run_initialize(pool: Arc<Self>, cwd: &Path, status_tx: watch::Sender<McpInitStatus>)`
  - 新增 `use tokio::sync::watch;` 导入（文件顶部）
  - 逻辑流程:
    1. 调用 `super::load_merged_config(cwd)` 获取配置，失败时发送 `Failed("配置加载失败: ...")` 并 return
    2. 将每个 `(name, McpServerConfig)` 存入 `pool.configs`（通过 `pool.configs.insert(...)`，configs 不需要锁因为只在初始化时写入）
    3. 发送 `Initializing { connected: 0, total: config.mcp_servers.len() }`
    4. 遍历 `config.mcp_servers`，对每个 server 复用现有 `initialize()` L61-152 的连接逻辑（TransportConfig 构建、超时连接、工具/资源发现），所有差异点如下:
       - `pool.clients` 操作改为 `pool.clients.write()`
       - `pool.services` 操作改为 `pool.services.lock().await`
       - `pool.insert_failed` 改为接受 `&Arc<Self>` 参数，内部操作 `pool.clients.write()`
       - 每成功连接一个 server 后递增 `connected` 计数，发送 `Initializing { connected, total }`
    5. 遍历完成后发送 `Ready { total: 成功连接数 }`
  - 原因: 后台初始化需要通过 `watch` channel 通知 TUI 进度，`Arc<Self>` 共享所有权供 TUI 和 agent task 并发访问

- [x] 重构 `insert_failed()` 为接受 `&Arc<Self>` 参数
  - 位置: `client.rs:impl McpClientPool`（~L158）
  - 签名: `fn insert_failed(pool: &Arc<Self>, name: &str, reason: String)`
  - 内部操作 `pool.clients.write().insert(name.to_string(), Arc::new(McpClientHandle { ... }))`
  - 原因: `run_initialize` 和 `reconnect` 均通过 `Arc<Self>` 调用，需要统一接口

- [x] 新增 `reconnect()` 方法
  - 位置: `client.rs:impl McpClientPool`（`shutdown` 方法之后）
  - 签名: `pub async fn reconnect(self: &Arc<Self>, server_name: &str) -> Result<(), McpPoolError>`
  - 逻辑:
    1. 从 `self.configs` 获取该 server 的原始配置，找不到则返回 `McpPoolError::NotConnected { server: server_name.to_string(), status: ClientStatus::Disconnected }`
    2. 从 `self.services.lock().await` 中找到并 close 对应的 RunningService（通过遍历 clients 中该 server 的 peer 匹配，或记录 service index 映射），然后移除
    3. 从 `self.clients.write()` 中移除旧 handle
    4. 复用 `run_initialize` 中的连接逻辑（TransportConfig 构建、超时连接、工具/资源发现），成功后更新 `self.clients.write()` 和 `self.services.lock().await`
    5. 失败时调用 `Self::insert_failed(self, server_name, reason)` 写回
  - 原因: 重连需要断开旧连接、重建 transport、重新发现工具和资源

- [x] 新增 `remove_server()` 方法
  - 位置: `client.rs:impl McpClientPool`（`reconnect` 方法之后）
  - 签名: `pub async fn remove_server(self: &Arc<Self>, server_name: &str)`
  - 逻辑:
    1. 从 `self.clients.write()` 中移除该 server 的 handle
    2. 从 `self.services.lock().await` 中找到 Connected 状态对应的 service 并 close_with_timeout(SHUTDOWN_TIMEOUT)
    3. 从 `self.configs` 中移除该 server 的配置
  - 原因: 删除 server 需要同时清理 clients、services、configs 三个数据结构

- [x] 新增 `ServerInfo` 结构体和 `server_infos()` 方法
  - 位置: `client.rs:ServerInfo`（在 `McpInitStatus` 定义之后），`server_infos` 方法在 `remove_server` 之后
  - `ServerInfo` 定义:

    ```rust
    pub struct ServerInfo {
        pub name: String,
        pub transport_type: String,
        pub status: ClientStatus,
        pub tool_count: usize,
        pub resource_count: usize,
    }
    ```

  - `server_infos()` 签名: `pub fn server_infos(&self) -> Vec<ServerInfo>`
  - 遍历 `self.clients.read().values()`，对每个 handle 构建 `ServerInfo`
  - `transport_type` 从 `self.configs.get(&handle.name)` 判断：有 `command` → "stdio"，有 `url` → "http"，均无 → "unknown"
  - 原因: TUI 面板（Task 4/5）需要服务器摘要信息用于列表渲染

- [x] 新增 `get_tools()` 和 `get_resources()` 方法
  - 位置: `client.rs:impl McpClientPool`（`server_infos` 方法之后）
  - `get_tools(&self, server_name: &str) -> Vec<Tool>`: 从 `self.clients.read().get(server_name)` 获取 tools clone，不存在时返回空 Vec
  - `get_resources(&self, server_name: &str) -> Vec<Resource>`: 从 `self.clients.read().get(server_name)` 获取 resources clone，不存在时返回空 Vec
  - 原因: TUI 面板详情视图需要展示单个 server 的工具/资源列表

- [x] 更新现有方法适配 RwLock/Mutex
  - 位置: `client.rs:impl McpClientPool`（各现有方法内部）
  - `get_client()`: `self.clients.read().get(name).cloned()`，返回 `Option<Arc<McpClientHandle>>`
  - `get_all_clients()`: `self.clients.read().values().filter(|c| matches!(c.status, ClientStatus::Connected)).cloned().collect()`，返回 `Vec<Arc<McpClientHandle>>`
  - `has_resources()`: `self.clients.read().values().any(|c| matches!(c.status, ClientStatus::Connected) && !c.resources.is_empty())`
  - `resource_summary()`: `self.clients.read().values()` 遍历
  - `shutdown()`: 通过 `self.clients.write()` 更新状态和清理 peer，通过 `self.services.lock().await` 获取 services 列表并 close
  - 原因: clients 和 services 字段类型变更，所有访问点需同步更新

- [x] 在 mod.rs 中重导出 McpInitStatus 和 ServerInfo
  - 位置: `peri-middlewares/src/mcp/mod.rs`（L12）
  - 在 `pub use client::{...}` 行中追加 `McpInitStatus` 和 `ServerInfo`
  - 原因: Task 3/4/5 需要从 mcp 模块顶层引用这些类型

- [x] 为 McpInitStatus 和 McpClientPool 新增方法编写单元测试
  - 测试文件: `peri-middlewares/src/mcp/client.rs`（文件底部 `#[cfg(test)] mod tests`）
  - 测试场景:
    - `test_mcp_init_status_equality`: 验证 `McpInitStatus::Pending == Pending`、`Initializing{1,2} != Initializing{2,2}`、`Ready{3} == Ready{3}`、`Failed("a") != Failed("b")`
    - `test_new_pending_creates_empty_pool`: `new_pending()` 创建的 pool，`clients.read().is_empty()` 为 true，`configs.is_empty()` 为 true
    - `test_server_infos_empty_pool`: 空 pool 的 `server_infos()` 返回空 Vec
    - `test_insert_failed_creates_failed_handle`: 构造 `new_pending()` pool，调用 `Self::insert_failed(&pool, "test-server", "timeout".into())`，验证 `server_infos()` 返回包含 name="test-server" 且 status 为 `Failed("timeout")` 的条目
    - `test_remove_server`: 构造 pool 并通过 `clients.write()` 插入两个 handle（一个 Connected 一个 Failed），调用 `remove_server(&pool, "server-a").await`，验证 `server_infos()` 仅包含 "server-b"
    - `test_get_tools_resources`: 构造 pool 并通过 `clients.write()` 插入带 tools 和 resources 的 handle，验证 `get_tools("s")` 和 `get_resources("s")` 返回非空列表，`get_tools("nonexistent")` 返回空列表
  - 运行命令: `cargo test -p peri-middlewares --lib -- mcp::client::tests`
  - 预期: 所有测试通过

**检查步骤:**

- [x] 验证 McpInitStatus 和 ServerInfo 已在 mod.rs 中重导出
  - `grep -n "McpInitStatus\|ServerInfo" peri-middlewares/src/mcp/mod.rs`
  - 预期: L12 的 pub use 行中包含 `McpInitStatus` 和 `ServerInfo`

- [x] 验证 McpClientPool 结构体包含 RwLock clients、Mutex services、configs 字段
  - `grep -A 6 "pub struct McpClientPool" peri-middlewares/src/mcp/client.rs`
  - 预期: 输出包含 `RwLock`、`Mutex`、`configs`

- [x] 验证新增方法存在
  - `grep -n "pub async fn run_initialize\|pub async fn reconnect\|pub async fn remove_server\|pub fn server_infos\|pub fn get_tools\|pub fn get_resources\|pub fn new_pending" peri-middlewares/src/mcp/client.rs`
  - 预期: 每个方法名各出现一次

- [x] 验证编译通过
  - `cargo build -p peri-middlewares 2>&1 | tail -5`
  - 预期: 输出 "Finished" 且无 error

- [x] 验证所有测试通过
  - `cargo test -p peri-middlewares --lib 2>&1 | tail -10`
  - 预期: 所有 test 结果为 "ok"，无 FAILED

---

### Task 2: McpConfig 持久化扩展

**背景:**
[业务语境] 用户通过 /mcp 面板删除 MCP 服务器时，需要将配置文件中的对应条目持久移除，而非仅在内存中删除。
[修改原因] 当前 config.rs 只有加载和合并逻辑，缺少删除能力。删除必须操作两层配置（项目级 .mcp.json 和全局 settings.json），且全局配置需保留 settings.json 中其他非 MCP 字段不变。
[上下游影响] Task 5 的 /mcp 面板删除流程直接调用本 Task 提供的 `remove_server_from_config` 函数。本 Task 无前置依赖。

**涉及文件:**

- 修改: `peri-middlewares/src/mcp/config.rs`

**执行步骤:**

- [x] 在 McpConfigError 枚举中新增 WriteError 变体
  - 位置: `peri-middlewares/src/mcp/config.rs:McpConfigError` (~L34, 在 ReadError 之后)
  - 新增变体:

    ```rust
    #[error("MCP 配置文件写入失败: {path}: {source}")]
    WriteError {
        path: String,
        #[source]
        source: std::io::Error,
    }
    ```

- [x] 实现 remove_server_from_config 公共函数
  - 位置: `peri-middlewares/src/mcp/config.rs` (~L188, 在 load_merged_config 函数之后)
  - 函数签名: `pub fn remove_server_from_config(cwd: &Path, server_name: &str) -> Result<(), McpConfigError>`
  - 逻辑分为三步:
    1. **尝试项目级删除**: 读取 `{cwd}/.mcp.json`，反序列化为 `McpConfigFile`，检查 `mcp_servers` 是否包含 `server_name`。包含则移除该条目，使用 `atomic_write_json` 写回。返回 `Ok(())`。
    2. **尝试全局删除**: 项目级未找到时，读取 `~/.peri/settings.json` 为 `serde_json::Value`。遍历两个可能的路径 `config.mcpServers` 和顶层 `mcpServers`（与 `load_global_config` L83-88 逻辑一致），找到包含 `server_name` 的那个路径并从中移除。使用 `atomic_write_json` 写回完整的 `serde_json::Value`（保留 settings.json 中所有其他字段）。未在任何路径找到则返回 `Ok(())`（幂等）。
    3. **路径计算**: 全局路径复用 `dirs_next::home_dir().unwrap_or_else(|| PathBuf::from(".")).join(".peri").join("settings.json")`，与 `load_merged_config` L147-150 一致。

- [x] 实现原子写入辅助函数 atomic_write_json
  - 位置: `peri-middlewares/src/mcp/config.rs` (~L188, 在 remove_server_from_config 之前)
  - 函数签名: `fn atomic_write_json(path: &Path, value: &serde_json::Value) -> Result<(), McpConfigError>`
  - 逻辑:
    1. 在目标文件同目录创建 `tempfile::Builder::new().suffix(".tmp").tempfile_in(path.parent().unwrap_or(Path::new(".")))`
    2. 使用 `serde_json::to_string_pretty(value)` 序列化，写入临时文件
    3. 调用 `std::fs::rename(tmp_path, path)` 原子替换（同一文件系统保证 rename 原子性）
    4. `tempfile::NamedTempFile` 的 Drop 会自动清理残留临时文件

- [x] 为 remove_server_from_config 编写单元测试
  - 测试文件: `peri-middlewares/src/mcp/config.rs` (现有 `#[cfg(test)] mod tests` 块，~L190)
  - 测试场景:
    - 从项目级 .mcp.json 删除存在的 server: 创建含两个 server 的 .mcp.json 临时文件，调用 `remove_server_from_config(临时目录, "server-a")`，验证文件中仅剩 "server-b"
    - 从全局 settings.json (config.mcpServers 路径) 删除: 创建含 `{"config":{"mcpServers":{"gh":{...}}}}` 的临时 settings.json，调用函数删除 "gh"，验证 settings.json 中 mcpServers 为空对象，其他顶层字段保留
    - 从全局 settings.json (顶层 mcpServers 路径) 删除: 创建含 `{"mcpServers":{"fs":{...}},"otherSetting":42}` 的临时 settings.json，调用函数删除 "fs"，验证 mcpServers 为空，"otherSetting" 保留
    - 删除不存在的 server: 对空 .mcp.json 和空 settings.json 调用删除，返回 Ok(()) 且文件不变
  - 运行命令: `cargo test -p peri-middlewares --lib -- mcp::config::tests::test_remove_server`
  - 预期: 所有测试通过

**检查步骤:**

- [x] 验证 remove_server_from_config 函数存在且签名正确
  - `grep -n "pub fn remove_server_from_config" peri-middlewares/src/mcp/config.rs`
  - 预期: 输出包含函数签名行
- [x] 验证 McpConfigError 包含 WriteError 变体
  - `grep -n "WriteError" peri-middlewares/src/mcp/config.rs`
  - 预期: 输出包含 WriteError 变体定义
- [x] 验证 atomic_write_json 使用 tempfile + rename 模式
  - `grep -n "atomic_write_json\|tempfile\|rename" peri-middlewares/src/mcp/config.rs`
  - 预期: 输出包含 tempfile 创建和 rename 调用
- [x] 运行 crate 全量测试确保无回归
  - `cargo test -p peri-middlewares --lib`
  - 预期: 全部测试通过，无编译错误

---

### Task 3: TUI 后台初始化集成

**背景:**
[业务语境] 将 MCP 连接池从 `submit_message()` 中的同步 `block_in_place` 初始化改为 App 创建后立即后台初始化，消除首次发消息时的 UI 阻塞（MCP 服务器连接可能耗时数十秒）。
[修改原因] 当前 `agent_ops.rs:submit_message()` L142-153 使用 `tokio::task::block_in_place` + `block_on` 同步初始化 MCP 连接池，阻塞 TUI 主线程，用户无法在初始化期间进行任何操作。
[上下游影响] 本 Task 依赖 Task 1 提供的 `McpInitStatus` 枚举、`McpClientPool::new_pending()` 和 `McpClientPool::run_initialize()` 方法。Task 5 的状态栏进度显示依赖本 Task 写入的 `mcp_init_rx` 字段。

**涉及文件:**

- 修改: `peri-tui/src/app/mod.rs`
- 修改: `peri-tui/src/app/agent_ops.rs`
- 修改: `peri-tui/src/main.rs`

**执行步骤:**

- [x] 在 App 结构体中新增 `mcp_init_rx` 字段 — 用于 agent task 内异步等待 MCP 就绪状态
  - 位置: `peri-tui/src/app/mod.rs:App` (~L94, 在 `mcp_pool` 字段之后)
  - 新增字段:

    ```rust
    /// MCP 后台初始化状态接收端（spawn_mcp_init 创建，submit_message agent task 内等待）
    pub mcp_init_rx: Option<tokio::sync::watch::Receiver<peri_middlewares::mcp::McpInitStatus>>,
    ```

- [x] 在 App::new() 的 Self 初始化中新增 `mcp_init_rx: None` — 保持 Headless 测试现有行为
  - 位置: `peri-tui/src/app/mod.rs:App::new()` (~L184, 在 `mcp_pool: None` 之后)
  - 新增初始化行: `mcp_init_rx: None,`
  - 原因: Headless 测试不调用 `spawn_mcp_init()`，`mcp_init_rx` 保持 None，submit_message 中跳过等待

- [x] 在 App 中实现 `spawn_mcp_init()` 方法 — 在 run_app 中调用，后台启动 MCP 连接池初始化
  - 位置: `peri-tui/src/app/mod.rs` (~L186, 在 `App::new()` 方法之后)
  - 新增方法:

    ```rust
    /// 后台初始化 MCP 连接池（不阻塞 UI），在 run_app 中 App::new() 之后调用
    pub fn spawn_mcp_init(&mut self) {
        use peri_middlewares::mcp::{McpClientPool, McpInitStatus};
        use std::path::Path;

        let pool = Arc::new(McpClientPool::new_pending());
        self.mcp_pool = Some(pool.clone());

        let (init_tx, init_rx) = tokio::sync::watch::channel(McpInitStatus::Pending);
        self.mcp_init_rx = Some(init_rx);

        let cwd = self.cwd.clone();
        tokio::spawn(async move {
            McpClientPool::run_initialize(pool, Path::new(&cwd), init_tx).await;
        });
    }
    ```

- [x] 在 `run_app()` 中调用 `app.spawn_mcp_init()` — 在 setup wizard 检测之后、事件循环之前
  - 位置: `peri-tui/src/main.rs:run_app()` (~L189, 在 setup wizard 检测块结束 `}` 之后、L191 `// Spinner tick` 注释之前)
  - 插入: `app.spawn_mcp_init();`
  - 原因: MCP 初始化需在 setup wizard 判断之后执行（setup wizard 期间不需要 MCP），但需在事件循环之前（确保用户首次发消息时后台初始化已开始）

- [x] 移除 `submit_message()` 中的 `block_in_place` 同步初始化代码 — 替换为引用 clone
  - 位置: `peri-tui/src/app/agent_ops.rs:submit_message()` (~L142-154, 整个 `if self.mcp_pool.is_none() { ... }` 块)
  - 删除 L142-154 全部代码（12 行惰性初始化块），替换为:

    ```rust
    let mcp_pool = self.mcp_pool.clone();
    let mcp_init_rx = self.mcp_init_rx.clone();
    ```

  - 原因: MCP 连接池已由 `spawn_mcp_init()` 在后台创建并赋值到 `self.mcp_pool`，此处仅需 clone 引用

- [x] 在 agent task 内部添加异步等待 MCP 就绪逻辑 — 在 `tokio::spawn` 的 async block 中、`run_universal_agent` 调用之前
  - 位置: `peri-tui/src/app/agent_ops.rs:submit_message()` (~L157, 在 `tokio::spawn(async move {` 之后、`agent::run_universal_agent(` 之前)
  - 插入:

    ```rust
    // 异步等待 MCP 后台初始化完成（最多 30 秒）
    if let Some(ref rx) = mcp_init_rx {
        let mut rx = rx.clone();
        if !matches!(*rx.borrow(), peri_middlewares::mcp::McpInitStatus::Ready { .. }) {
            let _ = tokio::time::timeout(
                std::time::Duration::from_secs(30),
                async {
                    while !matches!(
                        *rx.borrow(),
                        peri_middlewares::mcp::McpInitStatus::Ready { .. }
                    ) {
                        rx.changed().await.ok();
                    }
                },
            )
            .await;
        }
    }
    ```

  - 原因: 当 `mcp_init_rx` 为 `None`（Headless 测试未调用 `spawn_mcp_init`）时跳过等待，保持现有行为

- [x] 为 spawn_mcp_init 和异步等待逻辑编写集成测试
  - 测试文件: `peri-tui/src/app/agent_ops.rs` (现有 `#[cfg(test)] mod tests` 块)
  - 测试场景:
    - `test_mcp_init_rx_defaults_to_none`: 调用 `App::new()` 后验证 `app.mcp_init_rx` 为 `None`，`app.mcp_pool` 为 `None`
    - `test_submit_message_without_spawn_mcp_init`: 构造 App（不调用 spawn_mcp_init），验证 submit_message 路径中 mcp_init_rx 为 None 时等待逻辑被跳过（通过验证 mcp_pool 保持 None 不变）
  - 运行命令: `cargo test -p peri-tui --lib -- app::agent_ops::tests::test_mcp_init`
  - 预期: 所有测试通过

**检查步骤:**

- [x] 验证 App 结构体包含 mcp_init_rx 字段
  - `grep -n "mcp_init_rx" peri-tui/src/app/mod.rs`
  - 预期: 输出包含字段定义和 `mcp_init_rx: None` 初始化
- [x] 验证 spawn_mcp_init 方法存在
  - `grep -n "pub fn spawn_mcp_init" peri-tui/src/app/mod.rs`
  - 预期: 输出包含方法签名
- [x] 验证 block_in_place 初始化代码已移除
  - `grep -n "block_in_place\|McpClientPool::initialize" peri-tui/src/app/agent_ops.rs`
  - 预期: 无匹配结果（该同步初始化代码已被移除）
- [x] 验证 agent task 内包含异步等待逻辑
  - `grep -n "mcp_init_rx\|McpInitStatus::Ready" peri-tui/src/app/agent_ops.rs`
  - 预期: 输出包含 clone 和等待逻辑
- [x] 验证 main.rs 中调用 spawn_mcp_init
  - `grep -n "spawn_mcp_init" peri-tui/src/main.rs`
  - 预期: 输出包含调用行
- [x] 运行 TUI crate 编译检查
  - `cargo build -p peri-tui 2>&1 | tail -5`
  - 预期: 编译成功，无错误
- [x] 运行 TUI crate 全量测试确保无回归
  - `cargo test -p peri-tui --lib`
  - 预期: 全部测试通过

---

### Task 4: /mcp 命令与面板数据结构

**背景:**
[业务语境] 用户需要通过 `/mcp` 命令打开管理面板查看所有 MCP 服务器的连接状态、工具/资源详情，并执行重连和删除操作。
[修改原因] 当前 TUI 无 `/mcp` 命令，无 `McpPanel` 数据结构，无面板操作方法。需要新建命令注册、面板状态定义和全套面板操作方法（导航、进入详情、Tab 切换、删除确认、重连、关闭）。
[上下游影响] 本 Task 依赖 Task 1 提供的 `ServerInfo`、`McpClientPool::server_infos()`、`get_tools()`、`get_resources()`、`reconnect()`、`remove_server()` 方法，以及 Task 2 提供的 `remove_server_from_config` 函数。本 Task 的 `McpPanel` 数据结构和操作方法被 Task 5（面板渲染与状态栏集成）直接使用。

**涉及文件:**

- 新建: `peri-tui/src/command/mcp.rs`
- 修改: `peri-tui/src/command/mod.rs`
- 新建: `peri-tui/src/app/mcp_panel.rs`
- 修改: `peri-tui/src/app/mod.rs`

**执行步骤:**

- [x] 新建 `command/mcp.rs`，实现 McpCommand — 注册 `/mcp` 命令入口
  - 位置: `peri-tui/src/command/mcp.rs`（新建文件）
  - 参考 `command/cron.rs` 的结构模式（struct + Command impl + execute 从 App 获取数据并创建面板）
  - 代码:

    ```rust
    use super::Command;
    use crate::app::App;

    pub struct McpCommand;

    impl Command for McpCommand {
        fn name(&self) -> &str { "mcp" }

        fn description(&self) -> &str { "管理 MCP 服务器连接" }

        fn execute(&self, app: &mut App, _args: &str) {
            let infos = app.mcp_pool
                .as_ref()
                .map(|p| p.server_infos())
                .unwrap_or_default();

            if infos.is_empty() {
                let vm = crate::ui::message_view::MessageViewModel::system(
                    "无 MCP 服务器配置（请在 .mcp.json 或 settings.json 中添加）".to_string()
                );
                app.core.view_messages.push(vm.clone());
                let _ = app.core.render_tx.send(crate::ui::render_thread::RenderEvent::AddMessage(vm));
                return;
            }

            app.mcp_panel = Some(crate::app::McpPanel::new(infos));
        }
    }
    ```

  - 原因: 与 CronCommand 保持一致的模式——空数据时显示系统消息，有数据时创建面板

- [x] 在 `command/mod.rs` 中注册 McpCommand — 将模块声明和注册加入默认注册表
  - 位置: `peri-tui/src/command/mod.rs`（L1 模块声明区域和 L12-24 `default_registry()` 函数内）
  - 在 L1 模块声明区域添加: `pub mod mcp;`
  - 在 `default_registry()` 函数中 `r.register(Box::new(cron::CronCommand));` 之后添加: `r.register(Box::new(mcp::McpCommand));`
  - 原因: 遵循现有命令注册模式，所有命令必须在 `default_registry()` 中注册才能被 dispatch

- [x] 新建 `app/mcp_panel.rs`，定义 McpPanel、McpPanelView 数据结构 — 面板状态管理
  - 位置: `peri-tui/src/app/mcp_panel.rs`（新建文件）
  - 定义数据结构:

    ```rust
    use peri_middlewares::mcp::{ClientStatus, ServerInfo};
    use rmcp::model::{Resource, Tool};

    /// MCP 管理面板
    #[derive(Debug)]
    pub struct McpPanel {
        /// 服务器列表信息（来自 McpClientPool::server_infos()）
        pub servers: Vec<ServerInfo>,
        /// 当前选中索引
        pub cursor: usize,
        /// 当前视图层级
        pub view: McpPanelView,
        /// 确认删除弹窗（server name），None 表示非确认状态
        pub confirm_delete: Option<String>,
    }

    /// 面板视图层级
    #[derive(Debug)]
    pub enum McpPanelView {
        /// 服务器列表
        ServerList,
        /// 工具列表
        ToolList {
            server_name: String,
            tools: Vec<Tool>,
        },
        /// 资源列表
        ResourceList {
            server_name: String,
            resources: Vec<Resource>,
        },
    }
    ```

  - 实现 `McpPanel::new()` 构造函数:

    ```rust
    impl McpPanel {
        pub fn new(servers: Vec<ServerInfo>) -> Self {
            Self {
                servers,
                cursor: 0,
                view: McpPanelView::ServerList,
                confirm_delete: None,
            }
        }
    }
    ```

  - 原因: `ServerInfo` 直接复用 Task 1 在 middlewares 层定义的类型，TUI 层不重复定义；`McpPanelView` 枚举区分三个视图层级；`confirm_delete` 使用 `Option<String>` 存储 server name（与 cron 面板的 `confirm_delete: bool` 不同，因为需要知道删除目标）

- [x] 在 `app/mcp_panel.rs` 中实现面板操作方法 — 全部放在 `impl crate::app::App` 块中（与 cron_ops.rs 模式一致）
  - 位置: `peri-tui/src/app/mcp_panel.rs`（文件底部，数据结构定义之后）
  - **mcp_panel_move_up**: `cursor = cursor.saturating_sub(1)`
  - **mcp_panel_move_down**: `cursor = min(cursor + 1, servers.len().saturating_sub(1))`
  - **mcp_panel_enter**: 仅在 `view == ServerList` 时生效，获取 `servers[cursor]` 的 name，从 `app.mcp_pool` 调用 `get_tools()`，设置 `view = ToolList { server_name, tools }`，重置 `cursor = 0`
  - **mcp_panel_back**: 仅在 `view != ServerList` 时生效，设置 `view = ServerList`，重置 `cursor = 0`
  - **mcp_panel_tab**: 在 ToolList 和 ResourceList 之间切换。当前为 ToolList 时，调用 `app.mcp_pool.get_resources(&server_name)` 设置为 ResourceList；当前为 ResourceList 时，调用 `app.mcp_pool.get_tools(&server_name)` 设置为 ToolList。切换时重置 `cursor = 0`
  - **mcp_panel_request_delete**: 仅在 `view == ServerList` 时生效，设置 `confirm_delete = Some(servers[cursor].name.clone())`
  - **mcp_panel_confirm_delete**: 仅在 `confirm_delete.is_some()` 时生效，执行三步删除:
    1. 从 `confirm_delete` 取出 server name
    2. 调用 `app.mcp_pool.as_ref()` 获取 pool clone，在 `tokio::spawn` 中调用 `pool.remove_server(&name).await`（因为 remove_server 是 async）
    3. 调用 `peri_middlewares::mcp::remove_server_from_config(std::path::Path::new(&app.cwd), &name)` 持久化删除
    4. 刷新 `servers` 列表: `servers = app.mcp_pool.as_ref().map(|p| p.server_infos()).unwrap_or_default()`
    5. 修正 `cursor` 使其不越界（参考 `CronPanel::refresh` 逻辑）
    6. 设置 `confirm_delete = None`
    7. 列表为空时关闭面板: `app.mcp_panel = None`
  - **mcp_panel_cancel_delete**: 设置 `confirm_delete = None`
  - **mcp_panel_reconnect**: 仅在 `view == ServerList` 且选中 server 状态为 `Failed` 时生效。在 `tokio::spawn` 中调用 `pool.reconnect(&name)`。刷新 servers 列表
  - **mcp_panel_close**: 设置 `app.mcp_panel = None`
  - 伪代码:

    ```rust
    impl crate::app::App {
        pub fn mcp_panel_move_up(&mut self) {
            if let Some(ref mut panel) = self.mcp_panel {
                panel.cursor = panel.cursor.saturating_sub(1);
            }
        }

        pub fn mcp_panel_move_down(&mut self) {
            if let Some(ref mut panel) = self.mcp_panel {
                let max = panel.servers.len().saturating_sub(1);
                if panel.cursor < max {
                    panel.cursor += 1;
                }
            }
        }

        pub fn mcp_panel_enter(&mut self) {
            if let Some(ref mut panel) = self.mcp_panel {
                if !matches!(panel.view, McpPanelView::ServerList) { return; }
                if panel.cursor >= panel.servers.len() { return; }
                let name = panel.servers[panel.cursor].name.clone();
                let tools = self.mcp_pool
                    .as_ref()
                    .map(|p| p.get_tools(&name))
                    .unwrap_or_default();
                panel.view = McpPanelView::ToolList { server_name: name, tools };
                panel.cursor = 0;
            }
        }

        pub fn mcp_panel_back(&mut self) {
            if let Some(ref mut panel) = self.mcp_panel {
                if matches!(panel.view, McpPanelView::ServerList) { return; }
                panel.view = McpPanelView::ServerList;
                panel.cursor = 0;
            }
        }

        pub fn mcp_panel_tab(&mut self) {
            if let Some(ref mut panel) = self.mcp_panel {
                match &panel.view {
                    McpPanelView::ToolList { server_name, .. } => {
                        let name = server_name.clone();
                        let resources = self.mcp_pool
                            .as_ref()
                            .map(|p| p.get_resources(&name))
                            .unwrap_or_default();
                        panel.view = McpPanelView::ResourceList { server_name: name, resources };
                        panel.cursor = 0;
                    }
                    McpPanelView::ResourceList { server_name, .. } => {
                        let name = server_name.clone();
                        let tools = self.mcp_pool
                            .as_ref()
                            .map(|p| p.get_tools(&name))
                            .unwrap_or_default();
                        panel.view = McpPanelView::ToolList { server_name: name, tools };
                        panel.cursor = 0;
                    }
                    McpPanelView::ServerList => {}
                }
            }
        }

        pub fn mcp_panel_request_delete(&mut self) {
            if let Some(ref mut panel) = self.mcp_panel {
                if !matches!(panel.view, McpPanelView::ServerList) { return; }
                if panel.cursor >= panel.servers.len() { return; }
                panel.confirm_delete = Some(panel.servers[panel.cursor].name.clone());
            }
        }

        pub fn mcp_panel_confirm_delete(&mut self) {
            if let Some(ref mut panel) = self.mcp_panel {
                let name = match panel.confirm_delete.take() {
                    Some(n) => n,
                    None => return,
                };
                // 异步断开连接
                if let Some(pool) = self.mcp_pool.clone() {
                    let name_clone = name.clone();
                    tokio::spawn(async move {
                        pool.remove_server(&name_clone).await;
                    });
                }
                // 持久化删除配置
                let _ = peri_middlewares::mcp::remove_server_from_config(
                    std::path::Path::new(&self.cwd), &name
                );
                // 刷新列表
                panel.servers = self.mcp_pool
                    .as_ref()
                    .map(|p| p.server_infos())
                    .unwrap_or_default();
                if panel.cursor >= panel.servers.len() && !panel.servers.is_empty() {
                    panel.cursor = panel.servers.len() - 1;
                }
                if panel.servers.is_empty() {
                    self.mcp_panel = None;
                }
            }
        }

        pub fn mcp_panel_cancel_delete(&mut self) {
            if let Some(ref mut panel) = self.mcp_panel {
                panel.confirm_delete = None;
            }
        }

        pub fn mcp_panel_reconnect(&mut self) {
            if let Some(ref mut panel) = self.mcp_panel {
                if !matches!(panel.view, McpPanelView::ServerList) { return; }
                if panel.cursor >= panel.servers.len() { return; }
                let status = &panel.servers[panel.cursor].status;
                if !matches!(status, ClientStatus::Failed(_)) { return; }
                let name = panel.servers[panel.cursor].name.clone();
                if let Some(pool) = self.mcp_pool.clone() {
                    tokio::spawn(async move {
                        let _ = pool.reconnect(&name).await;
                    });
                }
                // 刷新列表以反映重连状态
                panel.servers = self.mcp_pool
                    .as_ref()
                    .map(|p| p.server_infos())
                    .unwrap_or_default();
            }
        }

        pub fn mcp_panel_close(&mut self) {
            self.mcp_panel = None;
        }
    }
    ```

  - 原因: 所有操作方法放在 `impl crate::app::App` 块中，与 `cron_ops.rs` 的 `impl crate::app::App` 模式一致；async 操作（remove_server、reconnect）通过 `tokio::spawn` 在后台执行，不阻塞 UI

- [x] 在 `app/mod.rs` 中添加模块声明和 McpPanel 字段
  - 位置: `peri-tui/src/app/mod.rs`（L18 `mod cron_ops;` 之后添加模块声明，L68 `pub use cron_state::{CronPanel, CronState};` 之后添加 re-export，L94 App 结构体 `mcp_pool` 字段之后添加 `mcp_panel` 字段，L184 `mcp_pool: None` 之后添加 `mcp_panel: None` 初始化）
  - 模块声明: `mod mcp_panel;`
  - re-export: `pub use mcp_panel::{McpPanel, McpPanelView};`
  - App 结构体新增字段:

    ```rust
    /// MCP 管理面板状态
    pub mcp_panel: Option<McpPanel>,
    ```

  - App::new() 新增初始化: `mcp_panel: None,`
  - 原因: 与 cron 面板模式完全一致（模块声明 + re-export + Option 字段 + None 初始化）

- [x] 为 McpPanel 数据结构和面板操作方法编写单元测试
  - 测试文件: `peri-tui/src/app/mcp_panel.rs`（文件底部 `#[cfg(test)] mod tests` 块）
  - 测试场景:
    - `test_mcp_panel_new`: 构造 `McpPanel::new(vec![])` 验证初始状态（cursor=0, view=ServerList, confirm_delete=None）；构造含 3 个 ServerInfo 的面板验证 servers 长度
    - `test_mcp_panel_move_cursor`: 构造含 3 个 server 的面板，连续调用 move_up 5 次验证 cursor 停留在 0；连续调用 move_down 5 次验证 cursor 停留在 2
    - `test_mcp_panel_close`: 构造面板，验证 `app.mcp_panel.is_some()`，调用 `mcp_panel_close()`，验证 `app.mcp_panel.is_none()`
    - `test_mcp_panel_request_cancel_delete`: 构造面板，调用 request_delete，验证 confirm_delete 为 Some(name)；调用 cancel_delete，验证 confirm_delete 为 None
  - 运行命令: `cargo test -p peri-tui --lib -- app::mcp_panel::tests`
  - 预期: 所有测试通过

**检查步骤:**

- [x] 验证 command/mcp.rs 文件存在且 McpCommand 实现了 Command trait
  - `grep -n "struct McpCommand\|fn name\|fn description\|fn execute" peri-tui/src/command/mcp.rs`
  - 预期: 四个匹配行，包含 "mcp" 名称和 "管理 MCP 服务器连接" 描述

- [x] 验证 McpCommand 已在 default_registry 中注册
  - `grep -n "mcp::McpCommand\|pub mod mcp" peri-tui/src/command/mod.rs`
  - 预期: 两行匹配，一行模块声明、一行注册调用

- [x] 验证 app/mcp_panel.rs 包含 McpPanel、McpPanelView 定义和全部操作方法
  - `grep -n "pub struct McpPanel\|pub enum McpPanelView\|pub fn mcp_panel_" peri-tui/src/app/mcp_panel.rs`
  - 预期: McpPanel 结构体、McpPanelView 枚举、10 个操作方法（move_up/down/enter/back/tab/request_delete/confirm_delete/cancel_delete/reconnect/close）

- [x] 验证 App 结构体包含 mcp_panel 字段且初始化为 None
  - `grep -n "mcp_panel" peri-tui/src/app/mod.rs`
  - 预期: 包含模块声明、re-export、字段定义和 None 初始化

- [x] 验证编译通过
  - `cargo build -p peri-tui 2>&1 | tail -5`
  - 预期: 输出 "Finished" 且无 error

- [x] 验证所有测试通过
  - `cargo test -p peri-tui --lib -- app::mcp_panel::tests 2>&1 | tail -10`
  - 预期: 所有 test 结果为 "ok"，无 FAILED

---

### Task 5: MCP 面板渲染与状态栏集成

**背景:**
[业务语境] 用户通过 `/mcp` 命令打开管理面板后，需要看到 MCP 服务器的实时状态、工具/资源详情，并通过状态栏了解 MCP 后台初始化进度。
[修改原因] 当前无 MCP 面板渲染代码、无 `handle_mcp_panel` 键盘处理、状态栏不显示 MCP 初始化进度。需要新增面板渲染（ServerList/ToolList/ResourceList 三个视图）、event.rs 键盘分发、状态栏 MCP 进度显示和面板快捷键提示。
[上下游影响] 本 Task 依赖 Task 3 提供的 `mcp_init_rx` 字段（状态栏轮询）、Task 4 提供的 `McpPanel`/`McpPanelView` 数据结构和全部面板操作方法。本 Task 的渲染和键盘处理是 `/mcp` 面板功能的最后一块拼图。

**涉及文件:**

- 新建: `peri-tui/src/ui/main_ui/panels/mcp.rs`
- 修改: `peri-tui/src/ui/main_ui/panels/mod.rs`
- 修改: `peri-tui/src/ui/main_ui.rs`
- 修改: `peri-tui/src/ui/main_ui/status_bar.rs`
- 修改: `peri-tui/src/event.rs`

**执行步骤:**

- [ ] 新建 `panels/mcp.rs`，实现 `render_mcp_panel` 函数 — MCP 面板三个视图的完整渲染逻辑
  - 位置: `peri-tui/src/ui/main_ui/panels/mcp.rs`（新建文件）
  - 参考 `panels/cron.rs` 的完整渲染模式（BorderedPanel + ScrollableArea + panel_area/panel_plain_lines/panel_scroll_offset 元数据存储 + highlight_line_spans 选区高亮）
  - 函数签名: `pub(crate) fn render_mcp_panel(f: &mut Frame, app: &mut App, area: Rect)`
  - 函数入口检查 `app.mcp_panel.is_some()`，不存在时直接 return
  - **标题**: 根据 `panel.view` 动态切换:
    - `ServerList` → " MCP 服务器 "
    - `ToolList { ref server_name, .. }` → " {server_name} — 工具列表 "
    - `ResourceList { ref server_name, .. }` → " {server_name} — 资源列表 "
  - **BorderedPanel**: 标题样式 `theme::THINKING` + `Modifier::BOLD`，边框 `theme::BORDER`（与 cron 面板一致）
  - **ServerList 视图行格式**（遍历 `panel.servers`，`i == panel.cursor` 时标记光标）:

    ```
    ❯  server_name          [stdio]  Connected    5 tools, 2 resources
       another-server       [http]   Failed (...) —
    ```

    - 光标标识: `❯` / `  `（与 cron 一致）
    - 状态图标: Connected → `●`（`theme::SAGE`），Failed/Disconnected → `○`（`theme::ERROR`）
    - 传输类型: `[stdio]` 或 `[http]`，样式 `theme::MUTED`
    - 状态文字: `Connected`（`theme::SAGE`）或 `Failed(reason)`（`theme::ERROR`，reason 截断到 20 字符）
    - 工具/资源计数: Connected 时显示 `N tools, M resources`（`theme::MUTED`），Failed 时显示 `—`
    - 光标行文本样式 `theme::TEXT` + `Modifier::BOLD`，非光标行 `theme::TEXT`
  - **ToolList 视图行格式**（遍历 `panel.tools`）:

    ```
       tool_name              工具描述（截断到可用宽度）
    ```

    - 光标标识: `❯` / `  `
    - tool 名称: `theme::SAGE`（与工具名配色一致）
    - 描述: `tool.description.as_deref().unwrap_or("")`，截断到 `inner.width - name_width - 6` 字符，样式 `theme::MUTED`
  - **ResourceList 视图行格式**（遍历 `panel.resources`）:

    ```
       uri                    名称（截断到可用宽度）
    ```

    - 光标标识: `❯` / `  `
    - uri: `theme::THINKING`
    - 名称: `resource.name.as_deref().unwrap_or("")`，截断到可用宽度，样式 `theme::MUTED`
  - **空列表引导**:
    - ServerList 空时: `"  （无 MCP 服务器配置，请编辑 .mcp.json 或 settings.json）"`
    - ToolList 空时: `"  （该服务器未暴露工具）"`
    - ResourceList 空时: `"  （该服务器未暴露资源）"`
    - 样式 `theme::MUTED`
  - **底部提示行**（空行 + 提示行，与 cron 面板一致）:
    - 确认删除模式（`panel.confirm_delete.is_some()`）:

      ```
       ⚠ 确定删除 {server_name}？此操作将从配置文件中永久移除  Enter:确认  其他键:取消
      ```

      `⚠` 和 "确认删除" 使用 `theme::ERROR` + `Modifier::BOLD`，按键 `theme::MUTED` + `Modifier::BOLD`
    - ServerList 正常模式: `"↑↓:移动  Enter:详情  Ctrl+R:重连  Ctrl+D:删除  Esc:关闭"`
    - ToolList/ResourceList 模式: `"↑↓:移动  Tab:切换视图  Esc:返回"`
  - **面板元数据存储**（与 cron.rs L139-145 完全一致）:

    ```rust
    app.core.panel_area = Some(inner);
    app.core.panel_scroll_offset = panel.scroll_offset;
    app.core.panel_plain_lines = lines.iter().map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect()).collect();
    ```

  - **面板选区高亮**（与 cron.rs L148-180 完全一致，复用 `highlight_line_spans`）
  - **ScrollableArea 渲染**: `ScrollState::with_offset(panel.scroll_offset)` + `ScrollbarStyle(theme::MUTED)`
  - 原因: 与 cron 面板保持完全一致的渲染模式（BorderedPanel + ScrollableArea + 元数据存储 + 选区高亮），确保面板间行为统一

- [ ] 在 `panels/mod.rs` 中注册 mcp 模块 — 使面板渲染函数可被 main_ui 调用
  - 位置: `peri-tui/src/ui/main_ui/panels/mod.rs`（L4 `pub mod thread_browser;` 之后）
  - 添加: `pub mod mcp;`
  - 原因: 遵循现有面板模块注册模式

- [ ] 在 `main_ui.rs` 中添加 MCP 面板渲染分发 — 在 cron 面板之后
  - 位置: `peri-tui/src/ui/main_ui.rs`（L96 `panels::cron::render_cron_panel(f, app, panel_area);` 之后）
  - 插入:

    ```rust
    if app.mcp_panel.is_some() {
        panels::mcp::render_mcp_panel(f, app, panel_area);
    }
    ```

  - 原因: 面板按优先级互斥渲染（同一时间只打开一个），MCP 面板与 cron 面板同级

- [ ] 在 `main_ui.rs` 的 `active_panel_height()` 中添加 MCP 面板高度计算 — 在 cron 面板分支之后
  - 位置: `peri-tui/src/ui/main_ui.rs:active_panel_height()`（L153 cron 面板分支 `} else if app.cron.cron_panel.is_some() { ... }` 的 `}` 之后）
  - 插入新分支:

    ```rust
    } else if let Some(panel) = &app.mcp_panel {
        let item_count = match &panel.view {
            crate::app::McpPanelView::ServerList => panel.servers.len(),
            crate::app::McpPanelView::ToolList { tools, .. } => tools.len(),
            crate::app::McpPanelView::ResourceList { resources, .. } => resources.len(),
        };
        (item_count as u16 + 4).max(6)
    }
    ```

  - 原因: MCP 面板高度取决于当前视图的列表项数，与其他面板的高度计算模式一致

- [ ] 在 `status_bar.rs` 的 `render_first_row()` 中添加 MCP 初始化进度显示 — 在任务运行时长之前
  - 位置: `peri-tui/src/ui/main_ui/status_bar.rs:render_first_row()`（L112 重试状态块结束 `}` 之后、L113 任务运行时长 `if app.core.loading {` 之前）
  - 插入:

    ```rust
    // MCP 初始化进度
    {
        if let Some(ref rx) = app.mcp_init_rx {
            let status = rx.borrow().clone();
            use peri_middlewares::mcp::McpInitStatus;
            match status {
                McpInitStatus::Initializing { connected, total } => {
                    spans.push(Span::styled(" │ ", Style::default().fg(theme::MUTED)));
                    spans.push(Span::styled(
                        format!(" [i] MCP 连接中 ({}/{})...", connected, total),
                        Style::default().fg(theme::MUTED),
                    ));
                }
                McpInitStatus::Ready { total } if total > 0 => {
                    // 首次就绪后显示 3 秒（通过 mcp_ready_shown_until 字段控制）
                    if let Some(until) = app.mcp_ready_shown_until {
                        if std::time::Instant::now() < until {
                            spans.push(Span::styled(" │ ", Style::default().fg(theme::MUTED)));
                            spans.push(Span::styled(
                                format!(" [i] MCP 就绪 ({} servers)", total),
                                Style::default().fg(theme::SAGE),
                            ));
                        }
                    }
                }
                McpInitStatus::Failed(ref msg) => {
                    spans.push(Span::styled(" │ ", Style::default().fg(theme::MUTED)));
                    spans.push(Span::styled(
                        format!(" [i] MCP 初始化失败: {}", msg),
                        Style::default().fg(theme::ERROR),
                    ));
                }
                McpInitStatus::Pending | McpInitStatus::Ready { .. } => {}
            }
        }
    }
    ```

  - 注意: `McpInitStatus::Ready { total: 0 }`（无服务器配置）不显示任何内容
  - 注意: 需要在 `render_first_row` 顶部检测状态转换并设置 `mcp_ready_shown_until`（仅在首次 Ready 且 total > 0 时设置 `Instant::now() + Duration::from_secs(3)`）
  - 原因: MCP 初始化进度让用户了解后台连接池状态，Failed 状态持续显示引导用户排查问题

- [ ] 在 App 结构体中新增 `mcp_ready_shown_until` 字段 — 控制就绪提示的 3 秒显示窗口
  - 位置: `peri-tui/src/app/mod.rs:App`（`mcp_init_rx` 字段之后）
  - 新增字段:

    ```rust
    /// MCP 就绪提示显示截止时间（首次 Ready 时设置，3 秒后消失）
    pub mcp_ready_shown_until: Option<std::time::Instant>,
    ```

  - 在 `App::new()` 中初始化: `mcp_ready_shown_until: None,`
  - 原因: `McpInitStatus::Ready` 是持久状态，需要额外字段控制就绪提示的 3 秒自动消失

- [ ] 在 `status_bar.rs` 的 `render_second_row()` 中添加 MCP 面板快捷键提示 — 在 cron 面板分支之后
  - 位置: `peri-tui/src/ui/main_ui/status_bar.rs:render_second_row()`（L286 cron 面板分支 `} else if app.cron.cron_panel.is_some() { ... }` 的 `}` 之后、L287 `} else if app.core.login_panel.is_some() {` 之前）
  - 插入新分支:

    ```rust
    } else if app.mcp_panel.is_some() {
        // 根据 McpPanelView 显示不同快捷键
        let view_label = app.mcp_panel.as_ref().map(|p| match &p.view {
            crate::app::McpPanelView::ServerList => {
                if p.confirm_delete.is_some() {
                    vec![
                        Span::styled("Enter", Style::default().fg(theme::WARNING).add_modifier(Modifier::BOLD)),
                        Span::styled(":确认  ", Style::default().fg(theme::MUTED)),
                        Span::styled("其他键", Style::default().fg(theme::WARNING).add_modifier(Modifier::BOLD)),
                        Span::styled(":取消", Style::default().fg(theme::MUTED)),
                    ]
                } else {
                    vec![
                        Span::styled("↑↓", Style::default().fg(theme::WARNING).add_modifier(Modifier::BOLD)),
                        Span::styled(":移动  ", Style::default().fg(theme::MUTED)),
                        Span::styled("Enter", Style::default().fg(theme::WARNING).add_modifier(Modifier::BOLD)),
                        Span::styled(":详情  ", Style::default().fg(theme::MUTED)),
                        Span::styled("Ctrl+R", Style::default().fg(theme::WARNING).add_modifier(Modifier::BOLD)),
                        Span::styled(":重连  ", Style::default().fg(theme::MUTED)),
                        Span::styled("Ctrl+D", Style::default().fg(theme::WARNING).add_modifier(Modifier::BOLD)),
                        Span::styled(":删除  ", Style::default().fg(theme::MUTED)),
                        Span::styled("Esc", Style::default().fg(theme::ERROR).add_modifier(Modifier::BOLD)),
                        Span::styled(":关闭", Style::default().fg(theme::MUTED)),
                    ]
                }
            }
            crate::app::McpPanelView::ToolList { .. }
            | crate::app::McpPanelView::ResourceList { .. } => {
                vec![
                    Span::styled("↑↓", Style::default().fg(theme::WARNING).add_modifier(Modifier::BOLD)),
                    Span::styled(":移动  ", Style::default().fg(theme::MUTED)),
                    Span::styled("Tab", Style::default().fg(theme::WARNING).add_modifier(Modifier::BOLD)),
                    Span::styled(":切换  ", Style::default().fg(theme::MUTED)),
                    Span::styled("Esc", Style::default().fg(theme::ERROR).add_modifier(Modifier::BOLD)),
                    Span::styled(":返回", Style::default().fg(theme::MUTED)),
                ]
            }
        });
        view_label.unwrap_or_default()
    }
    ```

  - 原因: 与 cron 面板快捷键提示模式一致，根据视图状态和确认删除状态动态切换提示内容

- [ ] 在 `event.rs` 中添加 MCP 面板键盘分发 — 在 cron 面板之后
  - 位置: `peri-tui/src/event.rs:next_event()`（L182 `handle_cron_panel` 块之后、L184 `// /agents 面板优先处理` 之前）
  - 插入:

    ```rust
    // MCP 面板优先处理
    if app.mcp_panel.is_some() {
        handle_mcp_panel(app, input);
        return Ok(Some(Action::Redraw));
    }
    ```

  - 原因: MCP 面板与 cron 面板同级，打开时拦截所有按键事件

- [ ] 在 `event.rs` 中实现 `handle_mcp_panel` 函数 — 处理 MCP 面板全部按键
  - 位置: `peri-tui/src/event.rs`（`handle_cron_panel` 函数 L1183 之后）
  - 函数签名: `fn handle_mcp_panel(app: &mut App, input: Input)`
  - 逻辑:
    1. **确认删除模式拦截**（`confirm_delete.is_some()`）: 只响应 `Enter`（调用 `app.mcp_panel_confirm_delete()`）和其他任意键（调用 `app.mcp_panel_cancel_delete()`），与 cron 面板一致
    2. **ServerList 模式** match input:
       - `Key::Up` → `app.mcp_panel_move_up()`
       - `Key::Down` → `app.mcp_panel_move_down()`
       - `Key::Enter` → `app.mcp_panel_enter()`
       - `Key::Esc` → `app.mcp_panel_close()` + `app.core.panel_selection.clear()` + `app.core.panel_area = None`
       - `Key::Char('r')` + `ctrl: true` → `app.mcp_panel_reconnect()`
       - `Key::Char('d')` + `ctrl: true` → `app.mcp_panel_request_delete()`
       - `Key::Char('c')` + `ctrl: true` → 忽略（面板中 Ctrl+C 不退出）
    3. **ToolList/ResourceList 模式** match input:
       - `Key::Up` → `app.mcp_panel_move_up()`
       - `Key::Down` → `app.mcp_panel_move_down()`
       - `Key::Tab` → `app.mcp_panel_tab()`
       - `Key::Esc` → `app.mcp_panel_back()`
       - `Key::Char('c')` + `ctrl: true` → 忽略
  - 伪代码:

    ```rust
    fn handle_mcp_panel(app: &mut App, input: Input) {
        // 确认删除模式下只处理 Enter 和 Esc
        if app.mcp_panel.as_ref().map_or(false, |p| p.confirm_delete.is_some()) {
            match input {
                Input { key: Key::Enter, .. } => app.mcp_panel_confirm_delete(),
                _ => app.mcp_panel_cancel_delete(),
            }
            return;
        }

        // 判断当前视图层级
        let is_server_list = app.mcp_panel.as_ref().map_or(true, |p| {
            matches!(p.view, crate::app::McpPanelView::ServerList)
        });

        match input {
            Input { key: Key::Char('c'), ctrl: true, .. } => { /* 忽略 */ }
            Input { key: Key::Up, .. } => app.mcp_panel_move_up(),
            Input { key: Key::Down, .. } => app.mcp_panel_move_down(),
            if is_server_list {
                Input { key: Key::Enter, .. } => app.mcp_panel_enter(),
                Input { key: Key::Esc, .. } => {
                    app.mcp_panel_close();
                    app.core.panel_selection.clear();
                    app.core.panel_area = None;
                }
                Input { key: Key::Char('r'), ctrl: true, .. } => app.mcp_panel_reconnect(),
                Input { key: Key::Char('d'), ctrl: true, .. } => app.mcp_panel_request_delete(),
            } else {
                Input { key: Key::Tab, .. } => app.mcp_panel_tab(),
                Input { key: Key::Esc, .. } => app.mcp_panel_back(),
            }
            _ => {}
        }
    }
    ```

  - 注意: 上面的 match arm 语法是伪代码，实际 Rust 实现需要将 `is_server_list` 判断提取为 if/else 分支，分别 match input（与 cron 面板风格一致）
  - 原因: ServerList 和 Detail 视图的按键集不同，需按视图分发

- [ ] 在 `event.rs` 的 Paste 处理中添加 MCP 面板拦截 — 防止粘贴文本进入 textarea
  - 位置: `peri-tui/src/event.rs`（L548-553 paste 拦截块，在 `|| app.cron.cron_panel.is_some()` 之后）
  - 在条件链中追加: `|| app.mcp_panel.is_some()`
  - 修改后的完整条件:

    ```rust
    if app.core.thread_browser.is_some()
        || app.core.agent_panel.is_some()
        || app.cron.cron_panel.is_some()
        || app.mcp_panel.is_some()
    {
        return Ok(Some(Action::Redraw));
    }
    ```

  - 原因: MCP 面板无文本输入字段，粘贴文本应被拦截

- [ ] 为 MCP 面板渲染编写 headless 测试
  - 测试文件: `peri-tui/src/ui/main_ui/panels/mcp.rs`（文件底部 `#[cfg(test)] mod tests` 块）
  - 测试场景:
    - `test_mcp_panel_empty_server_list`: 创建含空 servers 列表的 McpPanel，设置到 `app.mcp_panel = Some(...)`，通过 `handle.terminal.draw()` 渲染，验证快照包含 `.mcp.json` 引导文字
    - `test_mcp_panel_server_list_with_items`: 创建含 2 个 ServerInfo（1 个 Connected + 1 个 Failed）的 McpPanel，渲染后验证快照包含 `●`（Connected 图标）和 `○`（Failed 图标）
  - 运行命令: `cargo test -p peri-tui --lib -- ui::main_ui::panels::mcp::tests`
  - 预期: 所有测试通过

**检查步骤:**

- [ ] 验证 panels/mcp.rs 文件存在且包含 render_mcp_panel 函数
  - `grep -n "pub(crate) fn render_mcp_panel" peri-tui/src/ui/main_ui/panels/mcp.rs`
  - 预期: 匹配到函数签名

- [ ] 验证 panels/mod.rs 包含 mcp 模块声明
  - `grep -n "pub mod mcp" peri-tui/src/ui/main_ui/panels/mod.rs`
  - 预期: 匹配到模块声明

- [ ] 验证 main_ui.rs 包含 MCP 面板渲染分发
  - `grep -n "mcp_panel\|panels::mcp" peri-tui/src/ui/main_ui.rs`
  - 预期: 匹配到渲染调用和高度计算分支

- [ ] 验证 status_bar.rs 包含 MCP 初始化进度显示
  - `grep -n "McpInitStatus\|mcp_init_rx\|MCP 连接中\|MCP 就绪\|MCP 初始化失败" peri-tui/src/ui/main_ui/status_bar.rs`
  - 预期: 匹配到 MCP 进度显示相关代码

- [ ] 验证 status_bar.rs 包含 MCP 面板快捷键提示
  - `grep -n "mcp_panel\|McpPanelView" peri-tui/src/ui/main_ui/status_bar.rs`
  - 预期: 匹配到 MCP 面板快捷键分支

- [ ] 验证 event.rs 包含 handle_mcp_panel 函数和分发调用
  - `grep -n "handle_mcp_panel\|mcp_panel.is_some()" peri-tui/src/event.rs`
  - 预期: 匹配到函数定义、分发调用和 paste 拦截

- [ ] 验证 App 结构体包含 mcp_ready_shown_until 字段
  - `grep -n "mcp_ready_shown_until" peri-tui/src/app/mod.rs`
  - 预期: 包含字段定义和 None 初始化

- [ ] 验证编译通过
  - `cargo build -p peri-tui 2>&1 | tail -5`
  - 预期: 输出 "Finished" 且无 error

- [ ] 验证所有测试通过
  - `cargo test -p peri-tui --lib -- ui::main_ui::panels::mcp::tests 2>&1 | tail -10`
  - 预期: 所有 test 结果为 "ok"，无 FAILED

- [ ] 运行 TUI crate 全量测试确保无回归
  - `cargo test -p peri-tui --lib 2>&1 | tail -15`
  - 预期: 全部测试通过，无 FAILED

---

### Task 6: MCP 管理面板与后台初始化 验收

**前置条件:**

- 启动命令: `cargo run -p peri-tui`
- 配置文件: 在 `{cwd}/.mcp.json` 中配置至少一个 MCP 服务器（如 `{"mcpServers":{"test":{"command":"echo"}}}` ），用于测试初始化流程
- Headless 测试无需真实 MCP 服务器

**端到端验证:**

- [x] 1. 运行完整测试套件确保无回归
  - `cargo test 2>&1 | tail -20`
  - 预期: 全部测试通过，无 FAILED
  - 失败排查: 根据失败测试名称定位到对应 Task，检查该 Task 的单元测试步骤

- [x] 2. 验证 MCP 后台初始化不阻塞 UI（编译检查）
  - `grep -n "block_in_place" peri-tui/src/app/agent_ops.rs`
  - 预期: 不再包含 MCP 初始化相关的 `block_in_place` 调用（仅 langfuse 可能保留）
  - `grep -n "spawn_mcp_init" peri-tui/src/main.rs`
  - 预期: 在 `run_app()` 中有调用
  - 失败排查: 检查 Task 3 的 agent_ops.rs 修改步骤

- [x] 3. 验证 McpInitStatus 状态机和 Pool 扩展方法
  - `grep -n "McpInitStatus" peri-middlewares/src/mcp/client.rs`
  - 预期: 枚举定义包含 Pending/Initializing/Ready/Failed 四个变体
  - `grep -n "pub async fn run_initialize\|pub async fn reconnect\|pub async fn remove_server\|pub fn server_infos\|pub fn get_tools\|pub fn get_resources" peri-middlewares/src/mcp/client.rs`
  - 预期: 6 个新方法签名均存在
  - 失败排查: 检查 Task 1 的执行步骤

- [x] 4. 验证配置删除持久化功能
  - `grep -n "pub fn remove_server_from_config" peri-middlewares/src/mcp/config.rs`
  - 预期: 函数签名存在，接受 `(cwd: &Path, server_name: &str)` 参数
  - `cargo test -p peri-middlewares --lib -- mcp::config::tests::test_remove_server 2>&1 | tail -10`
  - 预期: 删除相关测试全部通过
  - 失败排查: 检查 Task 2 的执行步骤

- [x] 5. 验证 /mcp 命令注册和面板数据结构
  - `grep -n "mcp" peri-tui/src/command/mod.rs`
  - 预期: 包含 `pub mod mcp;` 和 `r.register(Box::new(mcp::McpCommand));`
  - `grep -n "mcp_panel" peri-tui/src/app/mod.rs`
  - 预期: 包含字段定义和 `None` 初始化
  - 失败排查: 检查 Task 4 的命令注册和面板字段步骤

- [x] 6. 验证面板渲染和键盘处理集成
  - `grep -n "handle_mcp_panel\|mcp_panel" peri-tui/src/event.rs`
  - 预期: 包含面板键盘处理分发和 handle_mcp_panel 函数
  - `grep -n "render_mcp_panel\|panels::mcp" peri-tui/src/ui/main_ui.rs`
  - 预期: 包含渲染分发调用
  - `grep -n "mcp_init_rx\|MCP" peri-tui/src/ui/main_ui/status_bar.rs`
  - 预期: 包含状态栏 MCP 初始化进度显示逻辑
  - 失败排查: 检查 Task 5 的各集成步骤

- [x] 7. Headless 测试保持现有行为不变
  - `cargo test -p peri-tui --lib 2>&1 | tail -15`
  - 预期: 所有现有测试通过，无回归
  - 失败排查: 检查 Task 3 的 Headless 兼容性设计（mcp_init_rx 默认 None，不调用 spawn_mcp_init）
