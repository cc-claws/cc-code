# Feature: 20260508_F001 - 面板组件化架构重构

## 需求背景

当前 TUI 面板系统存在 5 个核心问题，导致新增面板成本高、维护困难：

**P1: 事件分发 O(N) 线性扫描，顺序硬编码**
`event.rs` 有 15 层 if-else 链处理面板事件。新增面板必须插入正确位置，且需同步更新 7 个文件（Key/Paste/Mouse 分发链、panel_ops.rs 互斥列表、main_ui.rs 高度计算和渲染序列、status_bar.rs 快捷键链）。

**P2: 互斥逻辑散布在所有 open_\* 方法中**
每个 `open_*_panel()` 方法手动将 3-10 个其他面板设为 `None`。11 个方法的组合数为 O(N^2)，且互斥范围不一致，存在隐含的互斥分组。

**P3: 28 处 unwrap()**
面板已通过 `is_some()` 检查但仍用 `unwrap()` 访问，编译器无法验证安全性。仅 `login_panel` 就有 19 处。

**P4: 状态栏快捷键与事件分发紧耦合**
`status_bar.rs` 的 `render_second_row` 有 ~170 行 match 链，完全镜像 `event.rs` 的面板优先级和内部状态。

**P5: 面板分散在两个 struct 中**
Session-scoped 面板在 `AppCore` 中，Global-scoped 面板在 `App` 中，`CronPanel` 更是在 `app.cron.cron_panel` 中。访问路径不统一。

### 现状数据

| 指标 | 数值 |
|------|------|
| `event.rs` 总行数 | 2486 |
| 面板优先级链层数 | 15 层 |
| 正常面板数 | 11 个 + OAuth 弹窗 + Setup Wizard |
| `panel_ops.rs` 行数 | 916 |
| unwrap() 调用 | 28 处 |
| 互斥 open_* 方法 | 11 个 |
| `status_bar.rs` 第二行 | ~170 行 match 链 |

## 目标

- 新增面板只需：定义 `PanelState` 变体 + 实现 `PanelComponent` trait，无需修改 `event.rs`/`panel_ops.rs`/`status_bar.rs`
- 消除所有面板相关 `unwrap()` 调用
- 统一面板生命周期管理（打开/关闭/互斥）到 `PanelManager`
- 解耦状态栏快捷键显示，面板自描述快捷键
- 保持所有现有功能不变，分 5 阶段渐进迁移

## 方案设计

### 核心抽象

#### PanelKind 枚举

穷举所有面板类型。编译时保证所有面板都被处理。

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PanelKind {
    // Session-scoped
    Model, Login, Agent, Hooks, Config, ThreadBrowser,
    // Global-scoped
    Mcp, Plugin, Cron, Status, Memory,
}

impl PanelKind {
    pub fn priority(&self) -> u8 { /* ... */ }
    pub fn mutex_group(&self) -> MutexGroup { /* 目前统一为 Normal */ }
    pub fn scope(&self) -> PanelScope { /* Session 或 Global */ }
}
```

#### PanelState 枚举

使用枚举而非 `Box<dyn Trait>` 以获得穷举匹配。

```rust
pub enum PanelState {
    Model(ModelPanel), Login(LoginPanel), Agent(AgentPanel),
    Hooks(HooksPanel), Config(ConfigPanel), ThreadBrowser(ThreadBrowser),
    Mcp(McpPanel), Plugin(PluginPanel), Cron(CronPanel),
    Status(StatusPanel), Memory(MemoryPanel),
}
```

#### EventResult 枚举

```rust
pub enum EventResult {
    Consumed,       // 事件已被消费
    NotConsumed,    // 继续传递
    ClosePanel,     // 关闭当前面板
    OpenPanel(PanelKind), // 跳转到其他面板
}
```

#### PanelComponent Trait

```rust
pub trait PanelComponent: Any {
    fn kind(&self) -> PanelKind;
    fn handle_key(&mut self, input: Input, ctx: &mut PanelContext<'_>) -> EventResult;
    fn handle_paste(&mut self, _text: &str, _ctx: &mut PanelContext<'_>) -> EventResult {
        EventResult::Consumed
    }
    fn handle_scroll(&mut self, _lines: i16, _ctx: &mut PanelContext<'_>) -> EventResult {
        EventResult::Consumed
    }
    fn desired_height(&self, screen_height: u16, screen_width: u16) -> u16;
    fn render(&self, f: &mut Frame, app: &App, area: Rect);
    fn as_any_ref(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}
```

#### PanelManager 结构体

集中管理面板的打开/关闭/查询和事件分发。

```rust
pub struct PanelManager {
    active: Option<PanelState>,
}

impl PanelManager {
    pub fn open(&mut self, state: PanelState) -> Option<PanelState>;  // 自动关闭前一个
    pub fn close(&mut self) -> Option<PanelState>;
    pub fn dispatch_key(&mut self, input: Input, ctx: &mut PanelContext<'_>) -> EventResult;
    pub fn dispatch_paste(&mut self, text: &str, ctx: &mut PanelContext<'_>) -> EventResult;
    pub fn dispatch_scroll(&mut self, lines: i16, ctx: &mut PanelContext<'_>) -> EventResult;
    pub fn status_bar_hints(&self) -> Vec<(&'static str, &'static str)>;
}
```

#### PanelContext 结构体

解决 `&mut panel + &mut app` 借用冲突。面板处理器只借用 App 中"除 PanelManager 以外"的部分。

```rust
pub struct PanelContext<'a> {
    pub sessions: &'a mut Vec<ChatSession>,
    pub active: usize,
    pub cwd: String,
    pub peri_config: &'a mut Option<PeriConfig>,
    pub config_path_override: Option<PathBuf>,
    pub provider_name: &'a mut String,
    pub model_name: &'a mut String,
    pub mcp_pool: &'a mut Option<Arc<McpClientPool>>,
    pub cron: &'a mut CronState,
    pub plugin_data: &'a mut Option<PluginLoadResult>,
    pub bg_event_tx: &'a mpsc::Sender<AgentEvent>,
    pub thread_store: &'a Arc<dyn ThreadStore>,
}
```

### 借用策略

使用 Rust 结构体解构（Destructuring Split）同时获取 `&mut PanelManager` 和 `&mut App其余字段`：

```rust
let App {
    ref mut sessions,
    ref mut global_panels,
    ref mut peri_config,
    ref mut provider_name,
    // ...
    ..
} = *app;

let mut ctx = PanelContext { sessions, peri_config, provider_name, /* ... */ };
let result = global_panels.dispatch_key(input, &mut ctx);
```

### 面板作用域

两个 `PanelManager` 实例：

- `AppCore::session_panels` — Session-scoped 面板，随 session 切换
- `App::global_panels` — Global-scoped 面板，跨 session 保持

跨作用域互斥通过 `App::open_panel()` 统一处理：打开任何面板前关闭所有 manager 中的面板。

### 特殊面板

Setup Wizard、OAuth Prompt、Interaction Prompts 不纳入 PanelManager：

- 它们有特殊生命周期（全屏覆盖、来自 agent/MCP 触发）
- 在 `next_event` 中优先级高于 PanelManager

### 分阶段实施计划

#### Phase 1: 基础设施定义（无行为变更）

| 文件 | 操作 |
|------|------|
| `app/panel_manager.rs` | 新建：定义 `PanelKind`、`PanelState`、`PanelManager`（空壳）、`PanelContext`、`EventResult` |
| `app/panel_component.rs` | 新建：定义 `PanelComponent` trait |
| `app/mod.rs` | 添加 module 声明 |

验证：`cargo build -p peri-tui` 编译通过。新类型未使用，标注 `#[allow(dead_code)]`。

#### Phase 2: PanelManager 互斥管理（替换 panel_ops.rs 逻辑）

| 文件 | 操作 |
|------|------|
| `app/core.rs` | 添加 `session_panels: PanelManager` |
| `app/mod.rs` | 添加 `global_panels: PanelManager` |
| `app/panel_ops.rs` | 逐步替换 `open_*` 方法体，双写过渡 |

迁移策略：双写期间 `PanelManager` 是 source of truth，`Option<XxxPanel>` 是镜像。

#### Phase 3: 事件分发迁移（替换 15 层 if 链）

| 文件 | 操作 |
|------|------|
| `event.rs` | 删除 15 层 if 链，改为 `PanelManager::dispatch_key()` |
| 各面板文件 | 添加 `impl PanelComponent for XxxPanel`，迁移 `handle_xxx_panel` 函数体 |

关键变化：消除 `unwrap()`（面板作为 `&mut self` 传入），通过 `PanelContext` 访问 app 状态，返回 `EventResult`。

#### Phase 4: 渲染分发迁移 + 状态栏解耦

| 文件 | 操作 |
|------|------|
| `ui/main_ui.rs` | 重构渲染逻辑，从 `PanelManager` 查询 |
| `ui/main_ui/status_bar.rs` | 从 `PanelManager::status_bar_hints()` 获取快捷键 |

每个面板新增 `status_bar_hints()` 方法，面板自描述快捷键，状态栏统一格式化。

#### Phase 5: 清理 + 文档

| 文件 | 操作 |
|------|------|
| `app/core.rs` | 移除旧 `Option<XxxPanel>` 字段 |
| `app/mod.rs` | 移除旧面板字段 |
| `app/panel_ops.rs` | 删除 |
| `CLAUDE.md` | 更新面板系统架构说明 |
| `ui/headless.rs` | 添加面板生命周期测试 |

## 实现要点

### 关键技术决策

1. **枚举而非 trait object**：`PanelState` 使用枚举存储面板，穷举匹配保证编译时完整性，避免动态分发开销
2. **双 PanelManager**：session/global 分离，与现有 session 切换语义一致
3. **PanelContext 解耦借用**：通过结构体解构解决 `&mut panel + &mut app` 借用冲突
4. **渐进式迁移**：双写过渡期保持旧接口可用，降低风险

### 风险与缓解

| 风险 | 缓解措施 |
|------|----------|
| PanelContext 借用冲突 | 每个面板独立迁移，编译器即时反馈；必要时用 `std::mem::take` + put back |
| 渲染函数需 `&mut App` | 推荐 render 仍接收 `&mut App`，通过 panel_manager 字段访问面板状态 |
| Plugin 面板复杂度高（486 行） | 最后迁移，保持函数体完整迁移 |
| CronPanel 位于 `CronState` 内 | Phase 2 从 `CronState` 中取出放入 `global_panels` |
| ThreadBrowser 有搜索框和外部操作 | 保持 `TextArea` 内部持有，外部操作通过 `PanelContext` 间接调用 |

## 约束一致性

与 `spec/global/constraints.md` 和 `spec/global/architecture.md` 的一致性：

- **Rust 2021 + tokio async**：完全一致，新类型均为同步结构体
- **事件系统**：`AgentEvent` 不受影响，仅重构 TUI 层面板事件分发
- **文件组织**：每模块一目录，新增 `panel_manager.rs` / `panel_component.rs` 遵循此规则
- **字符串截断**：无新增截断逻辑
- **测试隔离**：headless 测试通过 `config_path_override` 重定向，新测试遵循相同模式
- **面板快捷键规范**：`status_bar_hints()` 遵循 CLAUDE.md 中的统一按键约定，快捷键提示由状态栏第二行负责

**架构偏离**：无偏离。本次重构是现有架构的内聚优化，不改变 TUI 与 agent 的通信模型。

## 验收标准

- [ ] `event.rs` 面板分发逻辑从 15 层 if-else 链简化为 `PanelManager::dispatch_key()` 调用
- [ ] `panel_ops.rs` 中 11 个 `open_*` 方法的互斥逻辑统一到 `PanelManager::open()`
- [ ] 面板相关 28 处 `unwrap()` 全部消除
- [ ] `status_bar.rs` 的 `render_second_row` 从 ~170 行 match 链简化为 `PanelManager::status_bar_hints()` 查询
- [ ] 所有旧 `Option<XxxPanel>` 字段迁移到 `PanelManager`
- [ ] 所有现有 headless 测试通过
- [ ] `cargo clippy -p peri-tui` 无警告
- [ ] 手动测试：11 个面板的打开/关闭/互斥/按键/渲染/状态栏快捷键均正常
- [ ] 新增面板生命周期 headless 测试覆盖
- [ ] 多 session 分屏下面板状态独立、切换正常
