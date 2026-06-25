# 组件化架构重构：面板系统

> **文档状态**: 设计稿
> **目标范围**: `peri-tui/src/event.rs`、`panel_ops.rs`、`app/mod.rs`、`app/core.rs`、`ui/main_ui.rs`、`ui/main_ui/status_bar.rs`
> **策略**: Approach C — 组件化架构，彻底解决根本问题

---

## 1. 动机与问题分析

### 1.1 现状数据

| 指标 | 数值 |
|------|------|
| `event.rs` 总行数 | 2486 |
| 面板优先级链层数 | 15 层（含 Setup Wizard、Interaction Prompts、OAuth） |
| 正常面板数 | 12 个 + OAuth 弹窗 + Setup Wizard |
| `panel_ops.rs` 行数 | 916（含 1036 行 headless 测试辅助） |
| `unwrap()` 调用 | 28 处（19 在 login_panel，8 在 model_panel，1 在 setup_wizard） |
| 互斥 open_* 方法 | 11 个，每个手动设置 3-10 个其他面板为 None |
| `status_bar.rs` 第二行 | ~170 行 match 链，与 event.rs 优先级链保持同步 |

### 1.2 核心问题

**P1: 事件分发是 O(N) 线性扫描，且顺序硬编码**

```rust
// event.rs 第 186-303 行，15 层 if-else 链
if app.setup_wizard.is_some() { ... return; }
if app.sessions[app.active].core.thread_browser.is_some() { ... return; }
if app.cron.cron_panel.is_some() { ... return; }
if app.oauth_prompt.is_some() { ... return; }
if app.mcp_panel.is_some() { ... return; }
if app.plugin_panel.is_some() { ... return; }
if app.sessions[app.active].core.agent_panel.is_some() { ... return; }
if app.sessions[app.active].core.hooks_panel.is_some() { ... return; }
if app.sessions[app.active].core.login_panel.is_some() { ... return; }
if app.sessions[app.active].core.model_panel.is_some() { ... return; }
if app.sessions[app.active].core.config_panel.is_some() { ... return; }
if app.status_panel.is_some() { ... return; }
if app.memory_panel.is_some() { ... return; }
// ... Interaction Prompts
```

新增面板必须插入正确位置，且需同步更新：

1. `event.rs` 的 Key 分发链
2. `event.rs` 的 Paste 分发链
3. `event.rs` 的 Mouse 分发链
4. `panel_ops.rs` 所有 `open_*` 方法的互斥列表
5. `main_ui.rs` 的 `active_panel_height` 计算
6. `main_ui.rs` 的 `render_session_column` 渲染序列
7. `status_bar.rs` 的 `render_second_row` 快捷键链

**P2: 互斥逻辑散布在所有 `open_*` 方法中**

每个 `open_*_panel()` 方法手动将 3-10 个其他面板设为 `None`。新增面板需要更新所有现有方法。目前已有 11 个方法，组合数为 O(N^2)。

```rust
// panel_ops.rs — open_model_panel (4 个互斥)
self.sessions[app.active].core.login_panel = None;
self.sessions[app.active].core.config_panel = None;
self.status_panel = None;
self.memory_panel = None;

// panel_ops.rs — open_plugin_panel (7 个互斥)
self.sessions[app.active].core.login_panel = None;
self.sessions[app.active].core.model_panel = None;
self.sessions[app.active].core.config_panel = None;
self.status_panel = None;
self.memory_panel = None;
self.mcp_panel = None;
```

注意互斥范围不一致：`open_model_panel` 不关闭 `mcp_panel`、`plugin_panel`、`cron_panel`、`thread_browser`、`agent_panel`、`hooks_panel`，而 `open_plugin_panel` 关闭其中部分但不关闭 `cron_panel`、`thread_browser`、`agent_panel`、`hooks_panel`。这暗示存在隐含的互斥分组。

**P3: 28 处 `unwrap()` — 面板已检查 `is_some()` 但仍用 `unwrap()`**

```rust
// event.rs — handle_login_panel 中有 19 处 unwrap()
if app.sessions[app.active].core.login_panel.is_some() {
    handle_login_panel(app, input);  // 进入后处处 .unwrap()
    return Ok(Some(Action::Redraw));
}

fn handle_login_panel(app: &mut App, input: Input) {
    app.sessions[app.active].core.login_panel.as_mut().unwrap().move_cursor(-1);
    // ... 19 处 unwrap
}
```

虽然外层已检查 `is_some()`，但编译器无法验证。应使用 `if let` 或将面板作为参数传入。

**P4: 状态栏快捷键与事件分发紧耦合**

`status_bar.rs` 的 `render_second_row` 有一个 ~170 行的 `match` 链，完全镜像 `event.rs` 的面板优先级和面板内部状态（如 `LoginPanelMode::Browse/Edit/New/ConfirmDelete`、`PluginPanelView`、`confirm_delete` 等）。新增面板或面板内部状态变化需要同步修改两处。

**P5: 面板分散在两个 struct 中**

```rust
// Session-scoped（在 AppCore 中）
pub struct AppCore {
    pub model_panel: Option<ModelPanel>,
    pub login_panel: Option<LoginPanel>,
    pub agent_panel: Option<AgentPanel>,
    pub hooks_panel: Option<HooksPanel>,
    pub config_panel: Option<ConfigPanel>,
    pub thread_browser: Option<ThreadBrowser>,
}

// Global-scoped（在 App 中）
pub struct App {
    pub mcp_panel: Option<McpPanel>,
    pub plugin_panel: Option<PluginPanel>,
    pub cron: CronState,  // cron.cron_panel: Option<CronPanel>
    pub status_panel: Option<StatusPanel>,
    pub memory_panel: Option<MemoryPanel>,
    pub oauth_prompt: Option<OAuthPrompt>,
    pub setup_wizard: Option<SetupWizardPanel>,
}
```

访问路径不统一：有的在 `app.sessions[app.active].core.xxx_panel`，有的在 `app.xxx_panel`，有的在 `app.cron.cron_panel`。

### 1.3 不重构的后果

- 新增面板（如未来的 `/settings` 全局面板）需要修改 7 个文件，遗漏任何一处都是 bug
- `event.rs` 继续增长到 3000+ 行，可维护性持续下降
- `unwrap()` 在多线程场景下虽目前安全，但重构时容易引入空指针

---

## 2. 架构设计

### 2.1 核心抽象

#### PanelKind 枚举

穷举所有面板类型。编译时保证所有面板都被处理。

```rust
/// 面板类型标识
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PanelKind {
    // Session-scoped（存储在 AppCore::session_panels 中）
    Model,
    Login,
    Agent,
    Hooks,
    Config,
    ThreadBrowser,
    // Global-scoped（存储在 App::global_panels 中）
    Mcp,
    Plugin,
    Cron,
    Status,
    Memory,
}

impl PanelKind {
    /// 面板优先级（数值越小越优先处理）
    pub fn priority(&self) -> u8 {
        match self {
            Self::ThreadBrowser => 0,
            Self::Cron => 1,
            Self::Mcp => 2,
            Self::Plugin => 3,
            Self::Agent => 4,
            Self::Hooks => 5,
            Self::Login => 6,
            Self::Model => 7,
            Self::Config => 8,
            Self::Status => 9,
            Self::Memory => 10,
        }
    }

    /// 互斥分组：同组面板同时只能打开一个
    pub fn mutex_group(&self) -> MutexGroup {
        match self {
            // 所有普通面板互斥（同一时刻只有一个面板打开）
            Self::Model
            | Self::Login
            | Self::Agent
            | Self::Hooks
            | Self::Config
            | Self::ThreadBrowser
            | Self::Mcp
            | Self::Plugin
            | Self::Cron
            | Self::Status
            | Self::Memory => MutexGroup::Normal,
        }
    }

    /// 面板作用域
    pub fn scope(&self) -> PanelScope {
        match self {
            Self::Model
            | Self::Login
            | Self::Agent
            | Self::Hooks
            | Self::Config
            | Self::ThreadBrowser => PanelScope::Session,

            Self::Mcp
            | Self::Plugin
            | Self::Cron
            | Self::Status
            | Self::Memory => PanelScope::Global,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MutexGroup {
    Normal,
    // 未来可扩展：Overlay, Inspector 等
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelScope {
    Session,
    Global,
}
```

#### PanelState 枚举

PanelManager 持有的面板状态。使用枚举而非 `Box<dyn Trait>` 以获得穷举匹配。

```rust
/// PanelManager 中的面板状态
pub enum PanelState {
    Model(ModelPanel),
    Login(LoginPanel),
    Agent(AgentPanel),
    Hooks(HooksPanel),
    Config(ConfigPanel),
    ThreadBrowser(ThreadBrowser),
    Mcp(McpPanel),
    Plugin(PluginPanel),
    Cron(CronPanel),
    Status(StatusPanel),
    Memory(MemoryPanel),
}

impl PanelState {
    pub fn kind(&self) -> PanelKind {
        match self {
            Self::Model(_) => PanelKind::Model,
            Self::Login(_) => PanelKind::Login,
            Self::Agent(_) => PanelKind::Agent,
            Self::Hooks(_) => PanelKind::Hooks,
            Self::Config(_) => PanelKind::Config,
            Self::ThreadBrowser(_) => PanelKind::ThreadBrowser,
            Self::Mcp(_) => PanelKind::Mcp,
            Self::Plugin(_) => PanelKind::Plugin,
            Self::Cron(_) => PanelKind::Cron,
            Self::Status(_) => PanelKind::Status,
            Self::Memory(_) => PanelKind::Memory,
        }
    }
}
```

#### EventResult 枚举

面板事件处理后的返回值。

```rust
/// 面板事件处理结果
pub enum EventResult {
    /// 事件已被消费，不需要继续处理
    Consumed,
    /// 事件未被消费，继续传递
    NotConsumed,
    /// 需要关闭当前面板
    ClosePanel,
    /// 需要打开另一个面板（如从面板内跳转）
    OpenPanel(PanelKind),
}
```

#### PanelContext 结构体

解决 `&mut panel + &mut app` 借用冲突的关键。面板处理器只借用 App 中 "除了 PanelManager 以外" 的部分。

```rust
/// 面板事件处理的上下文：App 中除 PanelManager 外的所有可变状态
pub struct PanelContext<'a> {
    pub sessions: &'a mut Vec<ChatSession>,
    pub active: usize,
    pub cwd: String,                          // Clone，避免借用
    pub peri_config: &'a mut Option<PeriConfig>,
    pub config_path_override: Option<PathBuf>, // Clone
    pub provider_name: &'a mut String,
    pub model_name: &'a mut String,
    pub mcp_pool: &'a mut Option<Arc<McpClientPool>>,
    pub cron: &'a mut CronState,
    pub plugin_data: &'a mut Option<PluginLoadResult>,
    pub bg_event_tx: &'a mpsc::Sender<AgentEvent>,
    pub thread_store: &'a Arc<dyn ThreadStore>,
}
```

#### PanelManager 结构体

集中管理面板的打开/关闭/查询。

```rust
/// 面板管理器：统一管理所有面板的生命周期
pub struct PanelManager {
    /// 当前激活的面板（None 表示无面板打开）
    active: Option<PanelState>,
}

impl PanelManager {
    pub fn new() -> Self {
        Self { active: None }
    }

    /// 查询当前激活的面板类型
    pub fn active_kind(&self) -> Option<PanelKind> {
        self.active.as_ref().map(|p| p.kind())
    }

    /// 查询当前激活面板是否为指定类型
    pub fn is_active(&self, kind: PanelKind) -> bool {
        self.active_kind() == Some(kind)
    }

    /// 是否有任何面板打开
    pub fn is_any_open(&self) -> bool {
        self.active.is_some()
    }

    /// 打开面板（自动关闭互斥面板）
    pub fn open(&mut self, state: PanelState) -> Option<PanelState> {
        let old = self.active.take();
        self.active = Some(state);
        old
    }

    /// 关闭当前面板
    pub fn close(&mut self) -> Option<PanelState> {
        self.active.take()
    }

    /// 关闭指定类型的面板（如果当前激活的是该类型）
    pub fn close_if(&mut self, kind: PanelKind) -> Option<PanelState> {
        if self.active_kind() == Some(kind) {
            self.active.take()
        } else {
            None
        }
    }

    /// 获取当前面板的可变引用（类型安全）
    pub fn get_mut<T>(&mut self) -> Option<&mut T>
    where
        T: 'static,
    {
        self.active.as_mut().and_then(|state| {
            // 使用 Any 进行 downcast
            state.as_any_mut().downcast_mut::<T>()
        })
    }

    /// 获取当前面板的不可变引用（类型安全）
    pub fn get<T>(&self) -> Option<&T>
    where
        T: 'static,
    {
        self.active.as_ref().and_then(|state| {
            state.as_any_ref().downcast_ref::<T>()
        })
    }

    /// 分发键盘事件到当前激活面板
    pub fn dispatch_key(
        &mut self,
        input: Input,
        ctx: &mut PanelContext<'_>,
    ) -> EventResult {
        let Some(state) = self.active.as_mut() else {
            return EventResult::NotConsumed;
        };
        match state {
            PanelState::Model(p) => p.handle_key(input, ctx),
            PanelState::Login(p) => p.handle_key(input, ctx),
            PanelState::Agent(p) => p.handle_key(input, ctx),
            PanelState::Hooks(p) => p.handle_key(input, ctx),
            PanelState::Config(p) => p.handle_key(input, ctx),
            PanelState::ThreadBrowser(p) => p.handle_key(input, ctx),
            PanelState::Mcp(p) => p.handle_key(input, ctx),
            PanelState::Plugin(p) => p.handle_key(input, ctx),
            PanelState::Cron(p) => p.handle_key(input, ctx),
            PanelState::Status(p) => p.handle_key(input, ctx),
            PanelState::Memory(p) => p.handle_key(input, ctx),
        }
    }

    /// 分发粘贴事件
    pub fn dispatch_paste(
        &mut self,
        text: &str,
        ctx: &mut PanelContext<'_>,
    ) -> EventResult {
        let Some(state) = self.active.as_mut() else {
            return EventResult::NotConsumed;
        };
        match state {
            PanelState::Model(p) => p.handle_paste(text, ctx),
            PanelState::Login(p) => p.handle_paste(text, ctx),
            PanelState::Config(p) => p.handle_paste(text, ctx),
            PanelState::Plugin(p) => p.handle_paste(text, ctx),
            PanelState::ThreadBrowser(p) => p.handle_paste(text, ctx),
            // 无文本输入的面板：消费事件（拦截，不传递到 textarea）
            PanelState::Agent(_)
            | PanelState::Hooks(_)
            | PanelState::Mcp(_)
            | PanelState::Cron(_)
            | PanelState::Status(_)
            | PanelState::Memory(_) => EventResult::Consumed,
        }
    }

    /// 分发鼠标滚轮事件
    pub fn dispatch_scroll(
        &mut self,
        lines: i16,
        ctx: &mut PanelContext<'_>,
    ) -> EventResult {
        let Some(state) = self.active.as_mut() else {
            return EventResult::NotConsumed;
        };
        match state {
            PanelState::Mcp(p) => {
                if lines > 0 { p.scroll_up(lines as u16); }
                else if lines < 0 { p.scroll_down((-lines) as u16); }
                EventResult::Consumed
            }
            PanelState::Plugin(p) => {
                if lines > 0 { p.scroll_offset = p.scroll_offset.saturating_sub(lines as u16); }
                else if lines < 0 {
                    let max = p.current_list_len() as u16;
                    p.scroll_offset = (p.scroll_offset + (-lines) as u16).min(max);
                }
                EventResult::Consumed
            }
            // 其他面板暂不处理滚轮
            _ => EventResult::Consumed,
        }
    }

    /// 查询状态栏快捷键提示
    pub fn status_bar_hints(&self) -> Vec<(&'static str, &'static str)> {
        let Some(state) = self.active.as_ref() else {
            return Vec::new();
        };
        match state {
            PanelState::Model(_) => vec![
                ("↑↓", "导航"), ("Enter", "确认"),
                ("Space", "选择/切换"), ("Esc", "关闭"),
            ],
            PanelState::Login(p) => {
                match p.mode {
                    LoginPanelMode::Browse => vec![
                        ("Enter", "选中"), ("Tab", "编辑"),
                        ("Ctrl+N", "新建"), ("Ctrl+D", "删除"), ("Esc", "关闭"),
                    ],
                    LoginPanelMode::Edit | LoginPanelMode::New => vec![
                        ("↑↓", "切换字段"), ("←→/Space", "切换Type"),
                        ("Enter", "保存"), ("Ctrl+V", "粘贴"), ("Esc", "取消"),
                    ],
                    LoginPanelMode::ConfirmDelete => vec![
                        ("Enter", "确认删除"), ("Esc", "取消"),
                    ],
                }
            }
            PanelState::Agent(_) => vec![
                ("↑↓", "选择"), ("Enter", "确认"), ("Esc", "取消"),
            ],
            PanelState::Hooks(_) => vec![
                ("↑↓", "导航"), ("Esc", "关闭"),
            ],
            PanelState::Config(p) => match p.mode {
                ConfigPanelMode::Browse => vec![
                    ("↑↓", "导航"), ("Enter", "编辑"), ("Esc", "关闭"),
                ],
                ConfigPanelMode::Edit => vec![
                    ("↑↓", "切换字段"), ("←→/Space", "切换"),
                    ("Enter", "保存"), ("Ctrl+V", "粘贴"), ("Esc", "取消"),
                ],
            },
            PanelState::ThreadBrowser(p) => {
                if p.confirm_delete {
                    vec![("Enter", "确认"), ("其他键", "取消")]
                } else {
                    vec![
                        ("↑↓", "移动"), ("Enter", "确认"),
                        ("Ctrl+D", "删除"), ("Esc", "关闭"), ("/", "搜索"),
                    ]
                }
            }
            PanelState::Mcp(p) => match &p.view {
                McpPanelView::ServerList => {
                    if p.confirm_delete.is_some() {
                        vec![("Enter", "确认"), ("其他键", "取消")]
                    } else {
                        vec![
                            ("↑↓", "移动"), ("Enter", "详情"),
                            ("Ctrl+R", "重连"), ("Ctrl+D", "删除"), ("Esc", "关闭"),
                        ]
                    }
                }
                McpPanelView::ServerDetail { .. } => vec![
                    ("↑↓", "移动"), ("Enter", "执行"), ("Esc", "返回"),
                ],
            },
            PanelState::Plugin(p) => {
                // 完整的 plugin 状态机快捷键
                p.status_bar_hints()
            }
            PanelState::Cron(p) => {
                if p.confirm_delete {
                    vec![("Enter", "确认"), ("其他键", "取消")]
                } else {
                    vec![
                        ("↑↓", "移动"), ("Enter", "切换"),
                        ("Ctrl+D", "删除"), ("Esc", "关闭"),
                    ]
                }
            }
            PanelState::Status(_) => vec![
                ("←→", "切换Tab"), ("Esc", "关闭"),
            ],
            PanelState::Memory(_) => vec![
                ("↑↓", "选择"), ("Enter", "编辑"), ("Esc", "关闭"),
            ],
        }
    }
}
```

### 2.2 PanelComponent Trait

面板行为接口。所有面板实现此 trait。

```rust
use std::any::Any;
use ratatui::layout::Rect;
use ratatui::Frame;
use tui_textarea::Input;

/// 面板组件 trait：统一渲染 + 事件处理 + 生命周期
pub trait PanelComponent: Any {
    /// 面板类型
    fn kind(&self) -> PanelKind;

    /// 处理键盘事件
    fn handle_key(&mut self, input: Input, ctx: &mut PanelContext<'_>) -> EventResult;

    /// 处理粘贴事件（默认拦截，不传递到 textarea）
    fn handle_paste(&mut self, _text: &str, _ctx: &mut PanelContext<'_>) -> EventResult {
        EventResult::Consumed
    }

    /// 处理鼠标滚轮事件（默认消费事件但不做操作）
    fn handle_scroll(&mut self, _lines: i16, _ctx: &mut PanelContext<'_>) -> EventResult {
        EventResult::Consumed
    }

    /// 计算面板所需高度
    fn desired_height(&self, screen_height: u16, screen_width: u16) -> u16;

    /// 渲染面板（委托到 ui/main_ui/panels/ 中的自由函数）
    fn render(&self, f: &mut Frame, app: &App, area: Rect);

    /// 用于 downcast
    fn as_any_ref(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}
```

**为什么不用 `handle_key` 直接返回 `EventResult`：**

现有处理函数返回 `()`（直接修改 `app`），改为返回 `EventResult` 可以让 `PanelManager` 统一处理"面板请求关闭自身"的情况，避免每个处理器都写 `app.xxx_panel = None`。

**为什么 `render` 接收 `&App` 而非 `PanelContext`：**

渲染阶段不需要修改 app 状态（`main_ui.rs` 中的 `render_*` 函数都只读 app 状态）。`Frame` + `&App` 足够。部分渲染函数签名是 `&mut App`（用于写回 `panel_area` 等渲染缓存），这些缓存可以迁移到面板自身或保留写入。

### 2.3 PanelState 实现 Any downcast

```rust
impl PanelState {
    fn as_any_ref(&self) -> &dyn Any {
        match self {
            Self::Model(p) => p as &dyn Any,
            Self::Login(p) => p as &dyn Any,
            Self::Agent(p) => p as &dyn Any,
            Self::Hooks(p) => p as &dyn Any,
            Self::Config(p) => p as &dyn Any,
            Self::ThreadBrowser(p) => p as &dyn Any,
            Self::Mcp(p) => p as &dyn Any,
            Self::Plugin(p) => p as &dyn Any,
            Self::Cron(p) => p as &dyn Any,
            Self::Status(p) => p as &dyn Any,
            Self::Memory(p) => p as &dyn Any,
        }
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        match self {
            Self::Model(p) => p as &mut dyn Any,
            Self::Login(p) => p as &mut dyn Any,
            Self::Agent(p) => p as &mut dyn Any,
            Self::Hooks(p) => p as &mut dyn Any,
            Self::Config(p) => p as &mut dyn Any,
            Self::ThreadBrowser(p) => p as &mut dyn Any,
            Self::Mcp(p) => p as &mut dyn Any,
            Self::Plugin(p) => p as &mut dyn Any,
            Self::Cron(p) => p as &mut dyn Any,
            Self::Status(p) => p as &mut dyn Any,
            Self::Memory(p) => p as &mut dyn Any,
        }
    }
}
```

---

## 3. 借用策略

### 3.1 问题本质

当前架构中，面板是 `App` 的字段，事件处理器需要 `&mut App` 来：

1. 修改面板状态（`app.xxx_panel.as_mut().unwrap()`)
2. 修改 app 级状态（`app.peri_config`、`app.provider_name` 等）
3. 发送消息（`app.sessions[app.active].core.view_messages.push(...)`）

将面板集中到 `PanelManager` 后，需要在一次调用中同时拥有 `&mut PanelManager`（处理面板事件）和 `&mut App其余字段`（修改配置等）。

### 3.2 方案：结构体解构（Destructuring Split）

Rust 借用检查器允许解构同一结构体的不同字段为独立借用：

```rust
pub async fn next_event(app: &mut App) -> Result<Option<Action>> {
    // ...
    let App {
        session_panels,       // &mut PanelManager（每个 session 一个）
        global_panels,        // &mut PanelManager（全局）
        sessions,             // &mut Vec<ChatSession>
        peri_config,           // &mut Option<PeriConfig>
        provider_name,        // &mut String
        model_name,           // &mut String
        mcp_pool,             // &mut Option<Arc<McpClientPool>>
        cron,                 // &mut CronState
        plugin_data,          // &mut Option<PluginLoadResult>
        bg_event_tx,          // &mpsc::Sender（共享）
        config_path_override, // &Option<PathBuf>（只读）
        thread_store,         // &Arc（共享）
        // ... 其他字段
        ..
    } = &mut *app;

    // 构建 PanelContext（借用 App 除 PanelManager 外的所有字段）
    let mut ctx = PanelContext {
        sessions,
        active: app.active, // Copy
        cwd: app.cwd.clone(),
        peri_config,
        config_path_override: config_path_override.clone(),
        provider_name,
        model_name,
        mcp_pool,
        cron,
        plugin_data,
        bg_event_tx,
        thread_store,
    };

    // 分发事件（PanelManager 独立借用）
    let result = global_panels.dispatch_key(input, &mut ctx);
    if result == EventResult::NotConsumed {
        session_panels.dispatch_key(input, &mut ctx);
    }
}
```

**关键点：**

- `session_panels` / `global_panels` 是 `PanelManager` 实例，直接从 `App` 解构出来
- `PanelContext` 借用 App 的其他所有字段
- 借用检查器验证两处借用不重叠（它们确实不重叠，因为是不同字段）

### 3.3 特殊面板处理

Setup Wizard、OAuth Prompt、Interaction Prompts（AskUser/HITL）不在 `PanelManager` 中管理。它们有特殊生命周期：

```rust
// next_event 中的处理顺序：
// 1. 全局按键（Shift+Tab/Alt+M/Ctrl+C 复制选区）— 不需要 PanelManager
// 2. Setup Wizard — 全屏覆盖，完全独立处理
// 3. Interaction Prompts — 来自 agent，优先级最高
// 4. OAuth Prompt — 来自 MCP auth flow
// 5. PanelManager 分发（session_panels → global_panels）
// 6. 主界面 textarea
```

---

## 4. 面板作用域

### 4.1 两个 PanelManager

```rust
pub struct AppCore {
    // ... 其他字段
    /// Session-scoped 面板管理器
    pub session_panels: PanelManager,
}

pub struct App {
    /// 所有聊天会话
    pub sessions: Vec<ChatSession>,  // 每个 session 含 AppCore.session_panels
    /// Global-scoped 面板管理器
    pub global_panels: PanelManager,
    // ... 其他字段
}
```

### 4.2 分发顺序

```
全局面板优先级 > Session 面板优先级

具体顺序（按 event.rs 现有优先级）：
1. ThreadBrowser (session, priority 0)
2. Cron (global, priority 1)
3. Mcp (global, priority 2)
4. Plugin (global, priority 3)
5. Agent (session, priority 4)
6. Hooks (session, priority 5)
7. Login (session, priority 6)
8. Model (session, priority 7)
9. Config (session, priority 8)
10. Status (global, priority 9)
11. Memory (global, priority 10)
```

互斥保证：`PanelManager::open()` 调用 `self.active.take()` 自动关闭前一个。由于同互斥组的面板只会存在于同一个 PanelManager 中，这个自动关闭足够。

但需要注意：如果一个 global 面板打开时，session 面板也需要关闭。解决方案：

```rust
impl App {
    /// 打开面板（处理跨作用域互斥）
    pub fn open_panel(&mut self, state: PanelState) {
        // 关闭所有面板
        self.global_panels.close();
        for session in &mut self.sessions {
            session.core.session_panels.close();
        }
        // 放入正确的 manager
        match state.kind().scope() {
            PanelScope::Session => {
                self.sessions[self.active].core.session_panels.open(state);
            }
            PanelScope::Global => {
                self.global_panels.open(state);
            }
        }
    }
}
```

### 4.3 Session 切换

当 `active` 切换时，global 面板保持打开，session 面板随 session 切换。这与当前行为一致。

---

## 5. 分阶段实施计划

### Phase 1: 基础设施定义（无行为变更）

**目标**：定义所有类型和 trait，不改变任何运行时行为。

**文件变更**：

| 文件 | 操作 |
|------|------|
| `peri-tui/src/app/panel_manager.rs` | 新建：定义 `PanelKind`、`PanelState`、`PanelManager`（空壳）、`PanelContext`、`EventResult`、`PanelScope`、`MutexGroup` |
| `peri-tui/src/app/panel_component.rs` | 新建：定义 `PanelComponent` trait |
| `peri-tui/src/app/mod.rs` | 修改：添加 `mod panel_manager; mod panel_component;` |

**关键类型签名**（详见第 2 节）：

```rust
// panel_manager.rs
pub enum PanelKind { Model, Login, Agent, Hooks, Config, ThreadBrowser, Mcp, Plugin, Cron, Status, Memory }
pub enum PanelState { Model(ModelPanel), Login(LoginPanel), ... }
pub struct PanelManager { active: Option<PanelState> }
pub struct PanelContext<'a> { ... }
pub enum EventResult { Consumed, NotConsumed, ClosePanel, OpenPanel(PanelKind) }

// panel_component.rs
pub trait PanelComponent: Any { ... }
```

**验证**：`cargo build -p peri-tui` 编译通过。所有新类型未使用（`#[allow(dead_code)]`）。

**估计时间**：0.5 天

---

### Phase 2: PanelManager 互斥管理（替换 panel_ops.rs 逻辑）

**目标**：将 `panel_ops.rs` 中的 `open_*` / `close_*` 方法迁移到使用 `PanelManager`。保留原有 `Option<XxxPanel>` 字段，通过 getter/setter 双写同步。

**文件变更**：

| 文件 | 操作 |
|------|------|
| `peri-tui/src/app/core.rs` | 添加 `session_panels: PanelManager` 字段 |
| `peri-tui/src/app/mod.rs` | 添加 `global_panels: PanelManager` 字段 |
| `peri-tui/src/app/panel_ops.rs` | 逐步替换 `open_*` 方法体 |

**迁移策略**：

每个 `open_*` 方法改为两步：

```rust
// Before:
pub fn open_model_panel(&mut self) {
    let cfg = self.peri_config.get_or_insert_with(PeriConfig::default);
    self.sessions[self.active].core.model_panel = Some(ModelPanel::from_config(cfg));
    self.sessions[self.active].core.login_panel = None;
    self.sessions[self.active].core.config_panel = None;
    self.status_panel = None;
    self.memory_panel = None;
}

// After（Phase 2 过渡期）:
pub fn open_model_panel(&mut self) {
    let cfg = self.peri_config.get_or_insert_with(PeriConfig::default);
    let panel = ModelPanel::from_config(cfg);

    // 新路径：通过 PanelManager
    self.sessions[self.active].core.session_panels.open(PanelState::Model(panel.clone()));
    self.global_panels.close(); // 关闭全局面板

    // 旧路径：保持兼容（双写）
    self.sessions[self.active].core.model_panel = Some(panel);
    self.sessions[self.active].core.login_panel = None;
    self.sessions[self.active].core.config_panel = None;
    self.status_panel = None;
    self.memory_panel = None;
}
```

**注意**：双写期间，`PanelManager` 是 source of truth，`Option<XxxPanel>` 字段是镜像。渲染和事件处理仍读旧字段。

**验证**：

- 所有现有 headless 测试通过
- 手动测试所有面板打开/关闭/互斥

**估计时间**：1.5 天

---

### Phase 3: 事件分发迁移（替换 15 层 if 链）

**目标**：`next_event()` 中的面板事件分发改为 `PanelManager::dispatch_key()`，删除所有 `handle_xxx_panel` 自由函数。

**文件变更**：

| 文件 | 操作 |
|------|------|
| `peri-tui/src/event.rs` | 重写 `next_event()`：删除 15 层 if 链，改为 `PanelManager::dispatch_key()` |
| `peri-tui/src/app/model_panel.rs` | 添加 `impl PanelComponent for ModelPanel` |
| `peri-tui/src/app/login_panel.rs` | 添加 `impl PanelComponent for LoginPanel` |
| `peri-tui/src/app/agent_panel.rs` | 添加 `impl PanelComponent for AgentPanel` |
| `peri-tui/src/app/hooks_panel.rs` | 添加 `impl PanelComponent for HooksPanel` |
| `peri-tui/src/app/config_panel.rs` | 添加 `impl PanelComponent for ConfigPanel` |
| `peri-tui/src/app/mcp_panel.rs` | 添加 `impl PanelComponent for McpPanel` |
| `peri-tui/src/app/plugin_panel.rs` | 添加 `impl PanelComponent for PluginPanel` |
| `peri-tui/src/app/cron_state.rs` | 添加 `impl PanelComponent for CronPanel` |
| `peri-tui/src/app/status_panel.rs` | 添加 `impl PanelComponent for StatusPanel` |
| `peri-tui/src/app/memory_panel.rs` | 添加 `impl PanelComponent for MemoryPanel` |

**每个面板的 handle_key 实现**：

将现有 `handle_xxx_panel(app, input)` 函数体迁移到 `impl PanelComponent for XxxPanel`。关键变化：

1. **消除 unwrap()**：面板自身是 `&mut self`，无需 `app.xxx_panel.as_mut().unwrap()`
2. **通过 PanelContext 访问 app 状态**：替代 `app.` 前缀
3. **返回 EventResult**：而非 void

示例（ModelPanel）：

```rust
impl PanelComponent for ModelPanel {
    fn kind(&self) -> PanelKind { PanelKind::Model }

    fn handle_key(&mut self, input: Input, ctx: &mut PanelContext<'_>) -> EventResult {
        match input {
            Input { key: Key::Esc, .. } => EventResult::ClosePanel,
            Input { key: Key::Up, .. } => {
                self.move_cursor(-1);
                EventResult::Consumed
            }
            Input { key: Key::Down, .. } => {
                self.move_cursor(1);
                EventResult::Consumed
            }
            Input { key: Key::Char(' ') | Key::Enter, .. } => {
                match self.cursor {
                    ROW_OPUS => {
                        self.active_tab = AliasTab::Opus;
                        self.apply_and_close(ctx);
                        EventResult::ClosePanel
                    }
                    ROW_SONNET => {
                        self.active_tab = AliasTab::Sonnet;
                        self.apply_and_close(ctx);
                        EventResult::ClosePanel
                    }
                    ROW_HAIKU => {
                        self.active_tab = AliasTab::Haiku;
                        self.apply_and_close(ctx);
                        EventResult::ClosePanel
                    }
                    ROW_EFFORT => {
                        self.cycle_effort(false);
                        EventResult::Consumed
                    }
                    _ => EventResult::Consumed,
                }
            }
            Input { key: Key::Left, .. } => {
                self.cycle_effort(true);
                EventResult::Consumed
            }
            Input { key: Key::Right, .. } => {
                self.cycle_effort(false);
                EventResult::Consumed
            }
            _ => EventResult::Consumed,
        }
    }

    fn desired_height(&self, _screen_height: u16, _screen_width: u16) -> u16 {
        12
    }

    fn render(&self, f: &mut Frame, app: &App, area: Rect) {
        crate::ui::main_ui::panels::model::render_model_panel(f, app, area);
    }
}
```

**next_event 重构后**（核心循环）：

```rust
pub async fn next_event(app: &mut App) -> Result<Option<Action>> {
    // 1. quit_pending 过期检查（不变）
    // 2. event::poll（不变）
    // 3. 全局按键拦截（Shift+Tab/Alt+M/Cmd+C/Ctrl+C 复制）
    // 4. Setup Wizard（特殊处理，不变）
    // 5. Interaction Prompts（特殊处理，不变）
    // 6. OAuth Prompt（特殊处理，不变）

    // 7. PanelManager 分发
    if app.global_panels.is_any_open() || app.sessions[app.active].core.session_panels.is_any_open()
    {
        let App {
            ref mut sessions,
            ref mut global_panels,
            ref mut peri_config,
            ref mut provider_name,
            ref mut model_name,
            ref mut mcp_pool,
            ref mut cron,
            ref mut plugin_data,
            ref bg_event_tx,
            ref config_path_override,
            ref thread_store,
            ref cwd,
            ..
        } = *app;

        let mut ctx = PanelContext {
            sessions,
            active: app.active,
            cwd: cwd.clone(),
            peri_config,
            config_path_override: config_path_override.clone(),
            provider_name,
            model_name,
            mcp_pool,
            cron,
            plugin_data,
            bg_event_tx,
            thread_store,
        };

        let result = if global_panels.is_any_open() {
            global_panels.dispatch_key(input, &mut ctx)
        } else {
            sessions[app.active].core.session_panels.dispatch_key(input, &mut ctx)
        };

        match result {
            EventResult::ClosePanel => {
                if global_panels.is_any_open() {
                    global_panels.close();
                } else {
                    sessions[app.active].core.session_panels.close();
                }
                app.sessions[app.active].core.panel_selection.clear();
                app.sessions[app.active].core.panel_area = None;
            }
            EventResult::OpenPanel(kind) => {
                // 处理面板跳转
            }
            EventResult::Consumed | EventResult::NotConsumed => {}
        }

        return Ok(Some(Action::Redraw));
    }

    // 8. 主界面 textarea 处理（不变）
    // ...
}
```

**粘贴事件迁移**：

同样通过 `PanelManager::dispatch_paste()`。面板自己决定是否消费粘贴事件（有文本输入字段的面板处理，其他面板拦截）。

**鼠标事件迁移**：

面板滚轮通过 `PanelManager::dispatch_scroll()`。面板区域判断逻辑保留在 `next_event` 中（基于 `panel_area` 坐标）。

**验证**：

- 所有面板打开/关闭/互斥正常
- 每个面板的所有按键功能正常
- 所有 headless 测试通过
- 手动测试鼠标滚轮、粘贴、Ctrl+C

**估计时间**：3 天

---

### Phase 4: 渲染分发迁移 + 状态栏解耦

**目标**：

1. `main_ui.rs` 的渲染逻辑改为从 `PanelManager` 查询
2. `status_bar.rs` 的快捷键显示从 `PanelManager::status_bar_hints()` 获取
3. `main_ui.rs` 的 `active_panel_height` 改为调用 `PanelComponent::desired_height()`

**文件变更**：

| 文件 | 操作 |
|------|------|
| `peri-tui/src/ui/main_ui.rs` | 重构 `render_session_column` 和 `active_panel_height` |
| `peri-tui/src/ui/main_ui/status_bar.rs` | 重构 `render_second_row` |

**渲染迁移**：

```rust
// Before: 12 个 if-is_some 检查
if app.sessions[session_idx].core.login_panel.is_some() {
    panels::login::render_login_panel(f, app, panel_area);
}
if app.sessions[session_idx].core.model_panel.is_some() {
    panels::model::render_model_panel(f, app, panel_area);
}
// ... 10 more

// After:
if let Some(state) = app.sessions[session_idx].core.session_panels.active_state() {
    state.render(f, app, panel_area);
} else if let Some(state) = app.global_panels.active_state() {
    state.render(f, app, panel_area);
}
```

**active_panel_height 迁移**：

```rust
fn active_panel_height(app: &App, screen_height: u16, screen_width: u16) -> u16 {
    let max_h = if app.global_panels.is_active(PanelKind::Plugin) {
        screen_height * 70 / 100
    } else {
        screen_height * 3 / 5
    };

    let raw = if let Some(state) = app.sessions[app.active].core.session_panels.active_state() {
        state.desired_height(screen_height, screen_width)
    } else if let Some(state) = app.global_panels.active_state() {
        state.desired_height(screen_height, screen_width)
    } else if let Some(InteractionPrompt::Approval(p)) =
        &app.sessions[app.active].agent.interaction_prompt
    {
        (p.items.len() as u16 * 2 + 5).max(5)
    } else if app.oauth_prompt.is_some() {
        9
    } else if let Some(InteractionPrompt::Questions(p)) =
        &app.sessions[app.active].agent.interaction_prompt
    {
        // ... 自适应计算
    } else {
        0
    };

    raw.min(max_h)
}
```

**状态栏迁移**：

```rust
fn render_second_row(f: &mut Frame, app: &App, area: Rect) {
    // ... left_spans 不变（copy提示、BG任务、Agent信息）

    let right_spans = match &app.sessions[app.active].agent.interaction_prompt {
        Some(_) if app.oauth_prompt.is_some() => { /* 特殊处理 */ }
        Some(InteractionPrompt::Questions(_)) => { /* 特殊处理 */ }
        Some(InteractionPrompt::Approval(_)) => { /* 特殊处理 */ }
        None => {
            // 从 PanelManager 获取快捷键提示
            let hints = if let Some(state) = app.global_panels.active_state() {
                state.status_bar_hints()
            } else if let Some(state) = app.sessions[app.active].core.session_panels.active_state()
            {
                state.status_bar_hints()
            } else {
                // 默认主界面快捷键
                default_hints(app)
            };
            format_hints(&hints)
        }
    };

    render_truncated_line(f, left_spans, right_spans, area);
}
```

**每个面板新增 `status_bar_hints()` 方法**：

返回 `Vec<(&'static str, &'static str)>`，每个元组是 `(key, desc)`。状态栏统一格式化。面板内部状态（如 `LoginPanelMode::Browse` vs `Edit`）由面板自己决定返回什么提示。

**验证**：

- 所有面板渲染正常
- 状态栏快捷键显示正确（包括面板内部状态变化时的切换）
- 多 session 分屏渲染正常

**估计时间**：2 天

---

### Phase 5: 清理 + 文档

**目标**：

1. 删除旧的 `Option<XxxPanel>` 字段（全部由 PanelManager 管理）
2. 删除 `panel_ops.rs`（已被 PanelManager 吸收）
3. 更新 `CLAUDE.md` 架构说明
4. 添加 headless 测试覆盖面板生命周期

**文件变更**：

| 文件 | 操作 |
|------|------|
| `peri-tui/src/app/core.rs` | 移除 `model_panel`/`login_panel`/`agent_panel`/`hooks_panel`/`config_panel`/`thread_browser` 字段 |
| `peri-tui/src/app/mod.rs` | 移除 `mcp_panel`/`plugin_panel`/`status_panel`/`memory_panel` 字段 |
| `peri-tui/src/app/panel_ops.rs` | 删除（功能迁移到各面板的 `PanelComponent` 实现） |
| `CLAUDE.md` | 更新面板系统架构说明 |
| `peri-tui/src/ui/headless.rs` | 添加面板生命周期测试 |

**headless 测试示例**：

```rust
#[tokio::test]
async fn test_panel_lifecycle_open_close() {
    let (mut app, mut handle) = App::new_headless(120, 30);

    // 打开 model 面板
    app.open_model_panel();
    assert!(app.sessions[0].core.session_panels.is_active(PanelKind::Model));

    // 打开 login 面板，model 应自动关闭
    app.open_login_panel();
    assert!(app.sessions[0].core.session_panels.is_active(PanelKind::Login));
    assert!(!app.sessions[0].core.session_panels.is_active(PanelKind::Model));

    // 关闭面板
    app.sessions[0].core.session_panels.close();
    assert!(!app.sessions[0].core.session_panels.is_any_open());
}

#[tokio::test]
async fn test_panel_mutex_cross_scope() {
    let (mut app, mut handle) = App::new_headless(120, 30);

    // 打开全局面板
    app.open_mcp_panel();
    assert!(app.global_panels.is_active(PanelKind::Mcp));

    // 打开 session 面板，全局面板应关闭
    app.open_model_panel();
    assert!(app.sessions[0].core.session_panels.is_active(PanelKind::Model));
    assert!(!app.global_panels.is_active(PanelKind::Mcp));
}
```

**验证**：

- `cargo test -p peri-tui` 全部通过
- `cargo clippy -p peri-tui` 无警告
- `lefthook run pre-commit` 通过

**估计时间**：1 天

---

## 6. 风险与缓解

### 6.1 借用检查器编译错误

**风险**：`PanelContext` 借用 App 的多个字段，某些 handler 可能需要同时访问面板和 PanelContext 中已被借用的字段。

**缓解**：

- Phase 3 中每个面板独立迁移，编译错误即时发现
- 使用 `let (a, b, ..) = &mut *app;` 解构而非 `PanelContext` 包含所有字段
- 必要时使用 `std::mem::take` + put back 模式（如现有的 `command_registry` 处理方式）

### 6.2 渲染函数签名不兼容

**风险**：部分 `render_xxx_panel(f, app, area)` 需要 `&mut App`（写回 `panel_area`、`panel_plain_lines` 等）。迁移到 `PanelComponent::render(&self, f, &App, area)` 后无法写回。

**缓解**：

- 方案 A：面板自身持有 `panel_area: Option<Rect>` 和 `panel_plain_lines: Vec<String>`（当前这些字段在 `AppCore` 中，所有面板共享）
- 方案 B：render 仍接收 `&mut App`，但通过 `panel_manager` 字段访问面板状态
- 推荐方案 B，减少字段迁移

### 6.3 Plugin 面板复杂度

**风险**：Plugin 面板是最复杂的面板（486 行 handler、多种内部视图/状态、异步安装/卸载）。迁移出错概率高。

**缓解**：

- Plugin 面板在 Phase 3 中最后迁移
- 保留 `handle_plugin_panel` 函数体完整迁移到 `impl PanelComponent for PluginPanel`
- 异步操作（install/uninstall spawn）保持通过 `bg_event_tx` 发送事件

### 6.4 ThreadBrowser 外部依赖

**风险**：`ThreadBrowser` 有搜索框（使用 `tui_textarea::TextArea`）和外部操作（`app.open_thread_with_feedback`）。

**缓解**：

- `ThreadBrowser` 保持对 `TextArea` 的内部持有
- `open_thread_with_feedback` 等操作通过 `PanelContext` 间接调用

### 6.5 CronPanel 不在 App 或 AppCore 直接字段中

**风险**：`CronPanel` 位于 `app.cron.cron_panel`，而非 `app.cron_panel`。

**缓解**：

- Phase 2 迁移时，`CronPanel` 从 `CronState` 中取出放入 `PanelManager`
- `CronState` 保留 `scheduler` 和 `trigger_rx`，`cron_panel` 字段迁移到 `global_panels`

### 6.6 Session 面板在不同 session 间独立

**风险**：每个 session 有自己的 `session_panels: PanelManager`。切换 session 时，旧 session 的面板保持其状态。

**缓解**：

- 这正是当前行为：面板状态在 `AppCore` 中，每个 session 独立
- `PanelManager` 作为 `AppCore` 字段自然继承了这一语义

---

## 7. 附录：当前面板清单

### 7.1 面板属性表

| 面板 | 变体名 | 作用域 | 存储位置 | 复杂度 | 互斥数 | unwrap 数 |
|------|--------|--------|----------|--------|--------|-----------|
| Model | `Option<ModelPanel>` | Session | `AppCore::model_panel` | 中 | 4 | 8 |
| Login | `Option<LoginPanel>` | Session | `AppCore::login_panel` | 高 | 4 | 19 |
| Agent | `Option<AgentPanel>` | Session | `AppCore::agent_panel` | 低 | 4 | 0 |
| Hooks | `Option<HooksPanel>` | Session | `AppCore::hooks_panel` | 低 | 4 | 0 |
| Config | `Option<ConfigPanel>` | Session | `AppCore::config_panel` | 中 | 3 | 0 |
| ThreadBrowser | `Option<ThreadBrowser>` | Session | `AppCore::thread_browser` | 高 | 0 | 0 |
| MCP | `Option<McpPanel>` | Global | `App::mcp_panel` | 高 | 7 | 0 |
| Plugin | `Option<PluginPanel>` | Global | `App::plugin_panel` | 极高 | 7 | 0 |
| Cron | `Option<CronPanel>` | Global | `App::cron.cron_panel` | 中 | 0 | 0 |
| Status | `Option<StatusPanel>` | Global | `App::status_panel` | 低 | 3 | 0 |
| Memory | `Option<MemoryPanel>` | Global | `App::memory_panel` | 低 | 4 | 0 |

### 7.2 特殊面板（不纳入 PanelManager）

| 面板 | 触发方式 | 生命周期 | 存储位置 |
|------|----------|----------|----------|
| Setup Wizard | 首次运行自动触发 | 全屏覆盖，保存或跳过后销毁 | `App::setup_wizard` |
| OAuth Prompt | MCP OAuth flow 触发 | 用户提交/取消后销毁 | `App::oauth_prompt` |
| HITL Approval | Agent 工具审批触发 | 用户确认后销毁 | `sessions[i].agent.interaction_prompt` |
| AskUser Questions | Agent 提问触发 | 用户回答后销毁 | `sessions[i].agent.interaction_prompt` |

### 7.3 互斥关系矩阵

基于 `panel_ops.rs` 中的 `open_*` 方法分析：

| 打开 | 自动关闭 |
|------|----------|
| Model | Login, Config, Status, Memory |
| Login | Model, Config, Status, Memory |
| Agent | （不显式关闭其他面板） |
| Hooks | Login, Config, Status, Memory |
| Config | Login, Model |
| ThreadBrowser | （不显式关闭其他面板） |
| MCP | Login, Model, Config, Status, Memory |
| Plugin | Login, Model, Config, Status, Memory, MCP |
| Cron | （不显式关闭其他面板） |
| Status | Config, Login, Model |
| Memory | Config, Login, Model, Status |

观察：

- Agent、Hooks、ThreadBrowser、Cron 四个面板不主动关闭其他面板，但会被其他面板关闭
- 实际运行时，由于事件分发链只允许一个面板激活（`if-else return` 链），隐含了全互斥
- **结论**：简化为所有面板同一互斥组

### 7.4 面板 handler 代码行数

基于 `event.rs` 中的 `handle_xxx_panel` 函数分析：

| 面板 | handler 行数 | 特殊状态 |
|------|-------------|----------|
| ThreadBrowser | ~200 | search_focused, confirm_delete |
| Login | ~230 | Browse/Edit/New/ConfirmDelete 4 种模式 |
| Model | ~90 | 4 行选择 + effort 切换 |
| Config | ~90 | Browse/Edit 模式 |
| Plugin | ~400 | Installed/Discover/Marketplaces/Detail/搜索/确认删除/添加 |
| MCP | ~70 | ServerList/ServerDetail + 确认删除 |
| Cron | ~55 | confirm_delete |
| Agent | ~25 | 简单列表选择 |
| Hooks | ~20 | 简单列表导航 |
| Status | ~20 | Tab 切换 |
| Memory | ~20 | 列表 + 编辑器 |

### 7.5 事件分发中的 `return Ok(Some(Action::Redraw))` 分析

每个面板在 `next_event()` 中的处理模式相同：

```rust
if app.xxx_panel.is_some() {
    handle_xxx_panel(app, input);  // 或 app.xxx_panel_op()
    return Ok(Some(Action::Redraw)); // 事件被消费，重绘
}
```

统一的 `PanelManager::dispatch_key()` 替代这个模式，返回 `EventResult` 决定是否消费事件。

---

## 8. 总结

本设计通过以下抽象解决第 1 节列出的 5 个核心问题：

| 问题 | 解决方案 | 所在阶段 |
|------|----------|----------|
| P1: O(N) 硬编码分发 | `PanelManager::dispatch_key()` 统一分发 | Phase 3 |
| P2: 互斥逻辑散布 | `PanelManager::open()` 自动关闭前一面板 | Phase 2 |
| P3: 28 处 unwrap | 面板作为 `&mut self` 参数传入，消除 unwrap | Phase 3 |
| P4: 状态栏耦合 | `PanelComponent::status_bar_hints()` 面板自描述 | Phase 4 |
| P5: 面板分散两处 | `PanelManager` 统一持有，`PanelScope` 区分作用域 | Phase 2 |

**总估计时间**：8 天（0.5 + 1.5 + 3 + 2 + 1），每个阶段独立可交付。
