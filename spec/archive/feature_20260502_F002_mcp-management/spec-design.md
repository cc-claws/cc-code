# Feature: 20260502_F002 - mcp-management

## 需求背景

MCP 中间件（F001）已实现基本连接和工具调用能力，但存在两个问题：

1. **初始化时机不合理**：当前 MCP 连接池在首次 `submit_message()` 时通过 `block_in_place` + `block_on` 同步初始化，阻塞 TUI 主线程，用户无法在初始化期间进行任何操作（包括输入）。MCP 服务器连接涉及子进程启动和网络握手，可能耗时数十秒。

2. **缺乏运行时管理**：用户无法查看 MCP 服务器状态、无法在运行时重连失败的服务器、无法删除不需要的服务器。所有配置变更需要手动编辑配置文件后重启应用。

## 目标

- MCP 连接池在 App 创建后立即在后台初始化，不阻塞 TUI 渲染和用户输入
- 用户首次发送消息时如 MCP 尚未就绪则异步等待（最多 30s），保证工具列表完整
- 新增 `/mcp` 命令面板：查看服务器状态、工具/资源详情、重连失败服务器、持久删除服务器配置
- ACP 模式 v1 不涉及 MCP 能力
- Headless 测试保持现有行为不变

**v1 不包含：** 通过面板添加 MCP 服务器（需手动编辑配置文件）。

## 方案设计

### 整体架构

```
run_app()
  ├─ App::new()                    // 同步创建
  ├─ app.spawn_mcp_init()          // spawn 后台 MCP 初始化 task
  │    └─ McpClientPool::run_initialize()
  │         ├─ 加载配置（.mcp.json + settings.json）
  │         ├─ 逐个连接服务器
  │         └─ 更新 watch::Sender<McpInitStatus>
  ├─ 事件循环（用户可正常交互）
  │    ├─ 状态栏轮询 mcp_init_rx 显示进度
  │    └─ /mcp 命令打开管理面板
  └─ submit_message()
       └─ agent task 内部异步等待 MCP 就绪（如未完成）

/mcp 面板:
  ├─ ServerList: 服务器列表 + 状态
  ├─ Detail: 工具/资源列表（Tab 切换）
  └─ 操作: Ctrl+R 重连、Ctrl+D 删除（持久化）
```

### MCP 提前初始化

#### McpInitStatus 状态机

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum McpInitStatus {
    /// 尚未开始初始化
    Pending,
    /// 正在连接中，已成功连接 N 个服务器
    Initializing { connected: usize, total: usize },
    /// 初始化完成，共 N 个服务器就绪
    Ready { total: usize },
    /// 初始化完全失败（如配置加载失败）
    Failed(String),
}
```

#### McpClientPool 扩展

```rust
pub struct McpClientPool {
    clients: HashMap<String, Arc<McpClientHandle>>,
    services: Vec<RunningService<RoleClient, ()>>,
    // 新增：配置快照，用于重连时重新构建 transport
    configs: HashMap<String, McpServerConfig>,
}
```

**新增方法**：

| 方法 | 说明 |
|------|------|
| `new_pending()` | 创建空池（Pending 状态） |
| `run_initialize(pool, cwd, status_tx)` | 后台执行初始化，每连一个 server 更新 status |
| `reconnect(&self, server_name)` | 重连指定服务器 |
| `remove_server(&mut self, server_name)` | 断开并移除指定服务器 |
| `server_infos(&self)` | 获取所有服务器摘要（供面板使用） |
| `get_tools(&self, server_name)` | 获取指定服务器的工具列表 |
| `get_resources(&self, server_name)` | 获取指定服务器的资源列表 |

#### TUI 入口集成

在 `run_app()` 中，`App::new()` 之后立即 spawn 后台初始化：

```rust
async fn run_app(terminal: &mut Terminal<...>) -> Result<()> {
    let mut app = App::new();
    // ... 权限模式、setup wizard ...

    // 后台初始化 MCP 连接池（不阻塞 UI）
    app.spawn_mcp_init();

    // 事件循环（不变）
    'event_loop: loop { ... }
}
```

`spawn_mcp_init()` 实现：

```rust
impl App {
    pub fn spawn_mcp_init(&mut self) {
        let pool = Arc::new(McpClientPool::new_pending());
        self.mcp_pool = Some(pool.clone());

        let (init_tx, init_rx) = tokio::sync::watch::channel(McpInitStatus::Pending);
        self.mcp_init_rx = Some(init_rx);

        let cwd = self.cwd.clone();
        tokio::spawn(async move {
            McpClientPool::run_initialize(pool, Path::new(&cwd), init_tx).await;
        });
    }
}
```

#### Lazy Wait 策略

`submit_message()` 中移除现有的 `block_in_place` + `block_on` 同步初始化代码。改为在 spawn 的 agent task 内部异步等待：

```rust
// agent task 内部（已移入 tokio::spawn 的 async block 中）
if let Some(ref rx) = mcp_init_rx {
    let mut rx = rx.clone();
    if !matches!(*rx.borrow(), McpInitStatus::Ready { .. }) {
        let _ = tokio::time::timeout(
            Duration::from_secs(30),
            async { while !matches!(*rx.borrow(), McpInitStatus::Ready { .. }) { rx.changed().await.ok(); } }
        ).await;
    }
}
```

当 `mcp_init_rx` 为 `None`（Headless 测试）时跳过等待。

#### 状态栏显示

事件循环中轮询 `mcp_init_rx`，在状态栏显示连接进度：

| 状态 | 显示 |
|------|------|
| `Pending` | 不显示（极短暂） |
| `Initializing { connected, total }` | `[i] MCP 连接中 ({connected}/{total})...` |
| `Ready { total }` | 无 MCP 时不显示；有服务器时首次就绪后显示 3 秒 `[i] MCP 就绪 (N servers)` 然后消失 |
| `Failed(msg)` | `[i] MCP 初始化失败: {msg}` |

### `/mcp` 命令面板

#### 命令注册

新增 `command/mcp.rs`，注册 `McpCommand` 到 `default_registry()`：

```rust
pub struct McpCommand;

impl Command for McpCommand {
    fn name(&self) -> &str { "mcp" }
    fn description(&self) -> &str { "管理 MCP 服务器连接" }
    fn execute(&self, app: &mut App, _args: &str) {
        let infos = app.mcp_pool
            .as_ref()
            .map(|p| p.server_infos())
            .unwrap_or_default();
        app.mcp_panel = Some(McpPanel::new(infos));
    }
}
```

#### 数据结构

```rust
pub struct McpPanel {
    /// 服务器列表信息
    servers: Vec<McpServerInfo>,
    /// 当前选中索引
    cursor: usize,
    /// 当前视图层级
    view: McpPanelView,
    /// 确认删除弹窗（server name）
    confirm_delete: Option<String>,
}

pub enum McpPanelView {
    /// 服务器列表
    ServerList,
    /// 工具列表（进入某个 server 查看工具）
    ToolList {
        server_name: String,
        tools: Vec<Tool>,
    },
    /// 资源列表（Tab 切换）
    ResourceList {
        server_name: String,
        resources: Vec<Resource>,
    },
}

pub struct McpServerInfo {
    pub name: String,
    pub transport_type: String,     // "stdio" / "http"
    pub status: ClientStatus,       // Connected / Failed(msg) / Disconnected
    pub tool_count: usize,
    pub resource_count: usize,
}
```

#### 面板交互

**Browse 模式（ServerList）**：

| 按键 | 行为 |
|------|------|
| `↑` / `↓` | 移动光标 |
| `Enter` | 进入详情（查看工具列表） |
| `Ctrl+R` | 重连选中服务器（仅 Failed 状态可用） |
| `Ctrl+D` | 删除选中服务器（弹出确认） |
| `Esc` | 关闭面板 |

**Detail 模式（ToolList / ResourceList）**：

| 按键 | 行为 |
|------|------|
| `↑` / `↓` | 移动光标 |
| `Tab` | 切换工具/资源视图 |
| `Esc` | 返回服务器列表 |

**确认删除弹窗**：

`Ctrl+D` 触发后进入确认模式，与 cron 面板行为一致：

- 只响应 `Enter`（确认删除）和 `Esc`（取消）
- 其他按键均视为取消

#### 删除流程

1. `Ctrl+D` → 设置 `confirm_delete = Some(server_name)`
2. 渲染确认弹窗："确定删除 {name}？此操作将从配置文件中永久移除"
3. `Enter` 确认 → 执行删除：
   a. 调用 `pool.remove_server(&name)` 断开连接
   b. 调用 `mcp_config::remove_server_from_config(&cwd, &name)` 持久化删除
   c. 刷新面板列表
4. `Esc` 取消

#### 重连流程

1. `Ctrl+R`（仅 Failed 状态可用，Connected 状态忽略）
2. 在后台 tokio task 中调用 `pool.reconnect(&server_name)`
3. 更新面板中对应 server 的状态

#### 渲染

复用 `BorderedPanel` + `SelectableList` 组件，与其他面板（agents、cron）风格一致。

**ServerList 行格式**：

```
● server_name          [stdio]  Connected    5 tools, 2 resources
○ another-server       [http]   Failed (...) —
```

- `●` = Connected, `○` = Failed/Disconnected
- 传输类型、状态、工具/资源计数

**ToolList 行格式**：

```
  tool_name              工具描述（截断到可用宽度）
```

### McpConfig 持久化扩展

新增 `peri-middlewares/src/mcp/config.rs` 方法：

```rust
/// 从配置文件中删除指定 server 条目
/// 优先从项目级 .mcp.json 删除，若不存在则从全局 settings.json 删除
pub fn remove_server_from_config(cwd: &Path, server_name: &str) -> Result<(), McpConfigError> {
    // 1. 尝试从 {cwd}/.mcp.json 删除
    // 2. 若 .mcp.json 中无该 server → 尝试从 ~/.peri/settings.json 的 mcpServers 删除
    // 3. 写回修改后的 JSON（保持格式化）
}
```

## 实现要点

### 依赖变更

无新增依赖。使用 `tokio::sync::watch` 进行初始化状态通信（tokio 已是项目依赖）。

### 关键技术难点

1. **后台初始化与 Agent 任务的协调**：`spawn_mcp_init()` 在 `run_app()` 中调用（异步上下文），`submit_message()` 在 agent task 中等待。需确保 `watch::Receiver` 可以跨 task clone 使用。

2. **配置快照存储**：`McpClientPool` 需要保存原始 `McpServerConfig` 以支持重连（重新构建 transport）。当前 `initialize()` 方法丢弃了配置。

3. **运行时并发安全**：`McpClientPool` 的 `clients` 字段当前是 `HashMap`，新增的 `reconnect`、`remove_server`、`server_infos` 方法需要考虑并发访问。由于 `McpClientPool` 通过 `Arc` 共享，内部需改为 `RwLock` 保护或使用 `DashMap`。推荐 `RwLock`（读多写少，与现有 HashMap 兼容）。

4. **配置文件修改的原子性**：`remove_server_from_config` 需要读取 → 修改 → 写回 JSON。应使用临时文件 + rename 模式保证原子性，防止写入中断导致配置丢失。

5. **删除后 pool 一致性**：删除 server 后，`McpMiddleware::collect_tools()` 不再返回该 server 的工具。如果 agent 正在使用该 server 的工具，`McpToolBridge.invoke()` 会返回 `NotConnected` 错误（已有处理）。

### 模块变更清单

| 文件 | 变更类型 | 说明 |
|------|---------|------|
| `peri-middlewares/src/mcp/client.rs` | 修改 | 新增 `new_pending`、`run_initialize`、`reconnect`、`remove_server`、`server_infos`、`get_tools`、`get_resources`；`clients` 改为 `RwLock<HashMap>`；新增 `configs` 字段 |
| `peri-middlewares/src/mcp/config.rs` | 修改 | 新增 `remove_server_from_config` |
| `peri-middlewares/src/mcp/mod.rs` | 修改 | 重导出 `McpInitStatus` |
| `peri-tui/src/command/mcp.rs` | 新增 | `McpCommand` 实现 |
| `peri-tui/src/command/mod.rs` | 修改 | 注册 `McpCommand` |
| `peri-tui/src/app/mcp_panel.rs` | 新增 | `McpPanel`、`McpPanelView`、`McpServerInfo` |
| `peri-tui/src/app/mod.rs` | 修改 | 新增 `mcp_init_rx`、`mcp_panel` 字段；`spawn_mcp_init` 方法 |
| `peri-tui/src/app/agent_ops.rs` | 修改 | 移除 block_in_place 初始化，改为 agent task 内异步等待 |
| `peri-tui/src/main.rs` | 修改 | `run_app()` 中调用 `app.spawn_mcp_init()` |
| `peri-tui/src/event.rs` | 修改 | 新增 `handle_mcp_panel` 键盘处理 |
| `peri-tui/src/ui/status_bar.rs` | 修改 | 状态栏显示 MCP 初始化进度 |

## 约束一致性

### 与 constraints.md 一致性

| 约束 | 一致性 | 说明 |
|------|--------|------|
| Workspace 多 crate 分层 | ✓ 一致 | 新增方法在 middlewares 层，TUI 集成在 TUI 层 |
| 异步优先 | ✓ 一致 | 后台初始化使用 tokio::spawn，等待使用 watch channel |
| Middleware Chain 模式 | ✓ 一致 | McpMiddleware 行为不变，仅 pool 生命周期提前 |
| 工具系统 BaseTool trait | ✓ 一致 | 无变更 |
| 事件驱动 TUI 通信 | ✓ 一致 | 状态栏通过轮询 watch::Receiver 获取进度 |
| 日志用 tracing | ✓ 一致 | 新增日志使用 tracing 宏 |
| 文件组织每模块一目录 | ✓ 一致 | 新增 `mcp_panel.rs` 和 `command/mcp.rs` |

### 面板快捷键一致性

| 规范 | 一致性 | 说明 |
|------|--------|------|
| `↑/↓` 竖向导航 | ✓ | ServerList 和 Detail 中均使用 |
| `Enter` 确认/进入 | ✓ | 进入详情、确认删除 |
| `Esc` 关闭/取消 | ✓ | 关闭面板、返回列表、取消删除 |
| `Ctrl+字母` 操作键 | ✓ | `Ctrl+R` 重连、`Ctrl+D` 删除 |
| 禁止 `Shift+字母` | ✓ | 无 Shift 组合键 |
| 确认弹窗 Enter/Esc | ✓ | 与 cron 面板一致 |

### 架构偏离

无偏离。所有变更遵循现有架构模式。

## 验收标准

- [ ] TUI 启动后 MCP 连接池在后台初始化，不阻塞 UI 渲染和用户输入
- [ ] 状态栏显示 MCP 初始化进度（连接中 N/M → 就绪）
- [ ] 首次发送消息时如 MCP 未就绪则异步等待（最多 30s），不阻塞 UI
- [ ] MCP 就绪后 LLM 可正常发现和调用 MCP 工具
- [ ] `/mcp` 命令打开管理面板，显示所有已配置服务器及状态
- [ ] 面板中选中服务器 → Enter 查看工具/资源详情
- [ ] 面板中 Tab 切换工具/资源视图
- [ ] 面板中 Ctrl+R 重连 Failed 状态的服务器
- [ ] 面板中 Ctrl+D 删除服务器（弹出确认 → Enter 确认 → 持久删除配置文件条目）
- [ ] 删除后 pool 不再提供该服务器工具，配置文件中对应条目已移除
- [ ] Headless 测试保持现有行为（mcp_pool: None，无等待逻辑）
- [ ] App 退出时正确清理所有 MCP 连接和子进程（已有行为不变）
- [ ] 新增单元测试覆盖：McpInitStatus 状态机、remove_server_from_config 配置修改
