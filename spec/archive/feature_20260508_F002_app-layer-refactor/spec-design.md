# Feature: 20260508_F002 - app-layer-refactor

## 需求背景

`App` 结构体 26 字段 + `AppCore` 38 字段，共 64 字段混合 6 种职责（会话管理、应用配置、外部服务、面板状态、UI 状态、Agent 生命周期），导致：

- 新增功能定位相关字段成本高
- 测试隔离困难（config_path_override 等测试专用字段混入生产代码）
- 借用冲突频繁（`std::mem::take` 临时交换、`active` 临时替换）
- `event.rs` 2486 行假设 `&mut App` 全访问，无法局部借用

面板组件化重构（F001）已完成阶段 3（PanelManager 提取），消除了 13 个 `Option<Panel>` 字段。本 feature 完成剩余 9 个阶段的分层重构。

**关联设计**：

- [组件化面板架构](../feature_20260508_F001_panel-component-architecture/spec-design.md)（已完成）
- [App God Object 分析](../global/app-god-object-analysis.md)（调研基础）

## 目标

- App 从 26 字段压缩到 3 个子结构体（services / session_mgr / global_panels）
- AppCore 消除，字段拆分到 UiState / MessageState / CommandSystem / SessionMetadata
- ChatSession 从 7 字段结构化为 6 个子模块
- 消除所有 `std::mem::take` workaround
- 新增功能只需修改 1 个子结构体，而非整个 App

## 方案设计

### 目标架构

```
App (3 fields)
├── services: ServiceRegistry       ← 14 字段
├── session_mgr: SessionManager     ← 3 字段
└── global_panels: PanelManager     ← 已完成

ChatSession (6 fields)
├── ui: UiState                     ← 18 字段
├── messages: MessageState          ← 9 字段
├── session_panels: PanelManager    ← 已完成
├── agent: AgentComm                ← 22 字段（保持不变）
├── commands: CommandSystem         ← 3 字段
└── metadata: SessionMetadata       ← 3 字段

AppCore → 消除
```

### 类型定义

#### ServiceRegistry

App 级全局服务，跨 session 共享，大部分不可变或低频变更。

```rust
pub struct ServiceRegistry {
    pub peri_config: Option<PeriConfig>,
    pub cwd: String,
    pub provider_name: String,
    pub model_name: String,
    pub permission_mode: Arc<SharedPermissionMode>,
    pub thread_store: Arc<dyn ThreadStore>,
    pub mcp_pool: Option<Arc<McpClientPool>>,
    pub mcp_init_rx: Option<watch::Receiver<McpInitStatus>>,
    pub cron: CronState,
    pub plugin_data: Option<PluginLoadResult>,
    pub bg_event_tx: mpsc::Sender<AgentEvent>,
    pub bg_event_rx: Option<mpsc::Receiver<AgentEvent>>,
    pub config_path_override: Option<PathBuf>,
    pub claude_settings_override: Option<PathBuf>,
}
```

#### SessionManager

会话管理，sessions/active/session_areas 三个字段高度内聚。

```rust
pub struct SessionManager {
    pub sessions: Vec<ChatSession>,
    pub active: usize,
    pub session_areas: Vec<Rect>,
}
```

辅助方法：

```rust
impl SessionManager {
    fn current(&self) -> &ChatSession { &self.sessions[self.active] }
    fn current_mut(&mut self) -> &mut ChatSession { &mut self.sessions[self.active] }
}
```

#### UiState

会话级 UI 状态，每个 session 独立拥有。

```rust
pub struct UiState {
    pub textarea: TextArea<'static>,
    pub loading: bool,
    pub scroll_offset: u16,
    pub scroll_follow: bool,
    pub show_tool_messages: bool,
    pub hint_cursor: Option<usize>,
    pub input_history: Vec<String>,
    pub history_index: Option<usize>,
    pub draft_input: Option<String>,
    pub text_selection: TextSelection,
    pub messages_area: Option<Rect>,
    pub textarea_area: Option<Rect>,
    pub copy_message_until: Option<Instant>,
    pub copy_char_count: usize,
    pub panel_selection: PanelTextSelection,
    pub panel_area: Option<Rect>,
    pub panel_plain_lines: Vec<String>,
    pub panel_scroll_offset: u16,
}
```

#### MessageState

消息渲染管线，每 session 独立。

```rust
pub struct MessageState {
    pub view_messages: Vec<MessageViewModel>,
    pub round_start_vm_idx: usize,
    pub pipeline: MessagePipeline,
    pub render_tx: mpsc::UnboundedSender<RenderEvent>,
    pub render_cache: Arc<RwLock<RenderCache>>,
    pub render_notify: Arc<Notify>,
    pub last_render_version: u64,
    pub pending_messages: Vec<String>,
    pub last_submitted_text: Option<String>,
}
```

#### CommandSystem

命令注册与 Skills 元数据，创建后基本不可变。

```rust
pub struct CommandSystem {
    pub command_registry: CommandRegistry,
    pub command_help_list: Vec<(String, String, Vec<String>)>,
    pub skills: Vec<SkillMetadata>,
}
```

#### SessionMetadata

会话元数据，低频访问。

```rust
pub struct SessionMetadata {
    pub pending_attachments: Vec<PendingAttachment>,
    pub last_human_message: Option<String>,
    pub pre_submit_state_len: usize,
}
```

### 借用策略——字段投影拆分

每个阶段提取子结构体后，通过 `let App { services, session_mgr, global_panels } = &mut *app;` 解构，让 Rust 借用检查器验证不同子结构体可同时可变借用。

示例（event.rs 面板分发）：

```rust
let App { services, session_mgr, global_panels, .. } = &mut *app;
let SessionManager { sessions, active, .. } = session_mgr;
let ctx = PanelContext { services, sessions, active, .. };
global_panels.dispatch_key(input, &mut ctx);
```

消除现有 workaround：

- `std::mem::take` 临时交换（event.rs:560）→ 字段投影拆分后不再需要
- 临时 active 交换（main_ui.rs:71）→ `render_session_column` 接收 `&SessionManager + index` 参数
- 面板互斥 → PanelManager 已解决

### App 特殊字段归属

以下 App 级字段不属于任何子结构体，随阶段推进处理：

| 字段 | 归属 | 说明 |
|------|------|------|
| `setup_wizard` | ServiceRegistry | 全局生命周期，首次运行触发 |
| `oauth_prompt` | ServiceRegistry | MCP auth flow，跨 session |
| `mode_highlight_until` | ServiceRegistry | 权限模式闪烁 |
| `model_highlight_until` | ServiceRegistry | 模型切换闪烁 |
| `mcp_ready_shown_until` | ServiceRegistry | MCP 就绪提示 |
| `quit_pending_since` | ServiceRegistry | 双击退出计时 |

### 实施阶段

#### P1：提取 ServiceRegistry（低风险）

**目标**：App 14 个服务字段提取到 `ServiceRegistry`。

**迁移策略**：

1. 定义 `ServiceRegistry` 结构体
2. App 新增 `services: ServiceRegistry` 字段
3. 逐文件迁移 `app.peri_config` → `app.services.peri_config`（双写期）
4. 删除 App 中旧字段
5. `App::new()` 中初始化 `ServiceRegistry`

**改动范围**：`mod.rs`（结构体定义）、`agent_ops.rs`、`panel_ops.rs`、`event.rs`、`config/` 模块

**验证**：`cargo build -p peri-tui` + 全部 headless 测试通过

#### P2：提取 SessionManager（低风险）

**目标**：App 3 个会话字段提取到 `SessionManager`。

**迁移策略**：

1. 定义 `SessionManager` 结构体 + `current()` / `current_mut()` 辅助方法
2. App 新增 `session_mgr: SessionManager` 字段
3. 逐文件迁移 `app.sessions` → `app.session_mgr.sessions`（双写期）
4. 删除 App 中旧字段
5. `App::active_session()` / `App::active_session_mut()` 委托到 `session_mgr.current()`

**改动范围**：`mod.rs`、`agent_ops.rs`、`event.rs`、`main_ui.rs`

**验证**：`cargo build -p peri-tui` + 全部 headless 测试通过

#### P4：提取 UiState（中风险）

**目标**：AppCore 18 个 UI 字段提取到 `UiState`。

**迁移策略**：

1. 定义 `UiState` 结构体
2. ChatSession 新增 `ui: UiState` 字段
3. 逐文件迁移 `session.core.textarea` → `session.ui.textarea`（双写期）
4. 删除 AppCore 中对应字段

**关键注意**：textarea 是高频访问字段，迁移需覆盖所有读写点。

**改动范围**：`core.rs`、`event.rs`、`main_ui.rs`、`headless.rs`

**验证**：`cargo build -p peri-tui` + 全部 headless 测试通过

#### P5：提取 MessageState（中风险）

**目标**：AppCore 9 个消息字段提取到 `MessageState`。

**迁移策略**：同 P4 模式。`view_messages` 和 `pipeline` 是高频字段，需确保 `poll_agent` 和 `render` 路径完全迁移。

**改动范围**：`core.rs`、`agent_ops.rs`、`main_ui.rs`

#### P6：提取 CommandSystem（低风险）

**目标**：AppCore 3 个命令字段提取到 `CommandSystem`。

**改动范围**：`core.rs`、`event.rs`（command dispatch）

#### P7：提取 SessionMetadata（低风险）

**目标**：AppCore 3 个元数据字段提取到 `SessionMetadata`。

**改动范围**：`core.rs`、`agent_ops.rs`（submit_message）

#### P8：消除 AppCore（高风险）

**目标**：删除 `AppCore` 结构体，所有 `session.core.xxx` 路径替换为 `session.ui.xxx` / `session.messages.xxx` / `session.commands.xxx` / `session.metadata.xxx`。

**迁移策略**：

1. 确认 P4-P7 全部完成，AppCore 仅剩 `session_panels` 字段
2. 将 `session_panels` 移入 ChatSession 直接字段
3. 全项目搜索 `session.core.` 替换
4. 删除 `AppCore` 结构体定义

**验证**：`grep -r "session.core" peri-tui/src/` 返回 0 结果

#### P9：消除 God Object（高风险）

**目标**：App 仅保留 3 字段（services + session_mgr + global_panels），event.rs 通过字段投影分发。

**迁移策略**：

1. 确认 P8 完成
2. event.rs 重构：顶部解构 `let App { services, session_mgr, global_panels } = &mut *app;`
3. 各分支改为操作子结构体引用
4. `setup_wizard` / `oauth_prompt` 等特殊字段纳入 ServiceRegistry
5. 删除 App 中所有残留字段

**验证**：`App` 结构体仅 3 字段 + `std::mem::take` 出现次数为 0

### 阶段依赖图

```
P1 (ServiceRegistry)
 └→ P2 (SessionManager)
      ├→ P4 (UiState)
      │    ├→ P5 (MessageState)
      │    ├→ P6 (CommandSystem)
      │    └→ P7 (SessionMetadata)  ← 可与 P4 并行
      └→ P7 (SessionMetadata)
P4 + P5 + P6 + P7
 └→ P8 (消除 AppCore)
      └→ P9 (消除 God Object)
```

## 实现要点

### 每阶段通用流程

1. 定义目标子结构体（新文件或添加到现有模块）
2. App/AppCore 添加新字段（与旧字段共存）
3. 逐文件迁移访问路径（双写期，新旧路径均可编译）
4. 删除旧字段
5. `cargo test -p peri-tui` + `cargo clippy -p peri-tui`

### 双写过渡策略

每阶段引入新子结构体时，旧字段暂不删除。迁移完成后统一删除旧字段，避免半完成状态。双写期间新旧路径指向相同数据（新字段包裹旧字段的引用或直接移动）。

### 文件组织

新增类型定义文件：

| 文件 | 内容 |
|------|------|
| `app/service_registry.rs` | ServiceRegistry 定义 |
| `app/session_manager.rs` | SessionManager 定义 + 辅助方法 |
| `app/ui_state.rs` | UiState 定义 |
| `app/message_state.rs` | MessageState 定义 |
| `app/command_system.rs` | CommandSystem 定义 |
| `app/session_metadata.rs` | SessionMetadata 定义 |

`app/core.rs` 在 P8 阶段删除。

### PanelContext 扩展

P9 完成后，`PanelContext` 简化为引用子结构体：

```rust
pub struct PanelContext<'a> {
    pub services: &'a mut ServiceRegistry,
    pub sessions: &'a mut Vec<ChatSession>,
    pub active: usize,
}
```

## 约束一致性

| 约束 | 一致性 |
|------|--------|
| Workspace resolver = "2" | ✅ 仅 peri-tui 内部重组 |
| 禁止下层依赖上层 | ✅ 所有新类型在 peri-tui 内 |
| 字符串截断用字符级操作 | ✅ 不变 |
| 测试隔离 | ✅ config_path_override 移入 ServiceRegistry，headless 测试不变 |
| 无新增外部 crate | ✅ 纯内部重组 |
| 面板组件化兼容 | ✅ PanelManager/PanelComponent 不变 |

无架构偏离。

## 验收标准

- [ ] App 3 字段（services + session_mgr + global_panels）
- [ ] AppCore 消除，`grep -r "session.core" peri-tui/src/` 返回 0 结果
- [ ] ChatSession 6 个子模块（ui / messages / session_panels / agent / commands / metadata）
- [ ] 0 处 `std::mem::take` workaround
- [ ] 全部 headless 测试通过
- [ ] `cargo clippy -p peri-tui` 无警告
- [ ] event.rs 不含 `app.sessions[app.active].core.xxx` 直接访问
