# 面板组件化架构重构 执行计划（上）

**目标:** 将 TUI 面板系统从分散的 Option<XxxPanel> + 15 层 if-else 链重构为 PanelManager + PanelComponent trait 的组件化架构，新增面板只需定义 PanelState 变体 + 实现 PanelComponent trait。

**技术栈:** Rust 2021, ratatui, tui_textarea, enum-based dispatch（非 dyn trait）

**设计文档:** spec/feature_20260508_F001_panel-component-architecture/spec-design.md

**全局设计文档:** spec/global/component-architecture-design.md

**改动总览:** 新建 `panel_manager.rs` / `panel_component.rs` 定义核心类型（PanelKind/PanelState/PanelManager/PanelContext/EventResult/PanelComponent），在 `AppCore`/`App` 添加 PanelManager 实例并同步 11 个 `open_*` 方法的双写逻辑，将 5 个简单面板（Model/Agent/Hooks/Status/Memory）的事件处理迁移为 `impl PanelComponent`。Task 1→2→3 严格顺序依赖，Task 1 输出被全部后续 Task 使用。关键设计决策：枚举分发（非 dyn trait）、双 PanelManager（session/global）、PanelContext 解耦借用。

---

### Task 0: 环境准备（上）

- [x] 验证 Rust 工具链可用：`rustc --version && cargo --version`
- [x] 验证全量编译通过：`cargo build -p peri-tui 2>&1 | tail -5`，预期输出包含 "Finished"
- [x] 验证全量测试通过：`cargo test -p peri-tui 2>&1 | tail -10`，预期 "test result: ok"
- [x] 验证 clippy 无警告：`cargo clippy -p peri-tui 2>&1 | grep -E "warning|error" | head -5`，预期无输出

---

### Task 1: 基础设施定义

**背景:**
[业务语境] 当前 TUI 有 11 个正常面板，分散在 `AppCore`（6 个 session-scoped）和 `App`（4 个 global-scoped）+ `CronState`（1 个）中，事件分发依赖 15 层 if-else 链。本 Task 定义面板组件化架构的核心类型和 trait，为后续 Task 2-7 的逐步迁移提供编译时可用的基础设施。
[修改原因] 现有面板无统一抽象，新增面板需同步修改 7 个文件。引入 `PanelKind`/`PanelState`/`PanelComponent`/`PanelManager`/`PanelContext`/`EventResult` 建立穷举式类型系统。
[上下游影响] 本 Task 输出的 `PanelKind`、`PanelState`、`PanelComponent` trait 被 Task 2-7 中所有面板的 `impl PanelComponent` 直接使用。本 Task 无前置依赖。

**涉及文件:**

- 新建: `peri-tui/src/app/panel_manager.rs`
- 新建: `peri-tui/src/app/panel_component.rs`
- 修改: `peri-tui/src/app/mod.rs`（添加 module 声明 + re-export）

**执行步骤:**

- [x] 新建 `peri-tui/src/app/panel_manager.rs`，定义核心枚举和结构体
  - 位置: 新文件，完整内容如下
  - 在文件顶部标注 `#![allow(dead_code)]`
  - 定义 `PanelKind` 枚举（11 个变体，区分 Session/Global 作用域），附带 `priority()`、`mutex_group()`、`scope()` 方法
  - 定义 `MutexGroup` 和 `PanelScope` 辅助枚举
  - 定义 `EventResult` 枚举（`Consumed`/`NotConsumed`/`ClosePanel`/`OpenPanel(PanelKind)`）
  - 定义 `PanelState` 枚举（11 个变体，每个变体持有对应面板类型），附带 `kind()` 方法
  - 定义 `PanelContext<'a>` 结构体（11 个字段，对应 event.rs 中面板 handler 实际访问的 App 字段）
  - 定义 `PanelManager` 空壳结构体（仅 `active: Option<PanelState>` 字段），实现以下方法，全部返回默认值或 `NotConsumed`：
    - `new() -> Self`
    - `active_kind(&self) -> Option<PanelKind>`
    - `is_active(&self, kind: PanelKind) -> bool`
    - `is_any_open(&self) -> bool`
    - `open(&mut self, state: PanelState) -> Option<PanelState>`
    - `close(&mut self) -> Option<PanelState>`
    - `close_if(&mut self, kind: PanelKind) -> Option<PanelState>`
    - `dispatch_key(&mut self, input: Input, ctx: &mut PanelContext<'_>) -> EventResult` — 返回 `EventResult::NotConsumed`
    - `dispatch_paste(&mut self, text: &str, ctx: &mut PanelContext<'_>) -> EventResult` — 返回 `EventResult::NotConsumed`
    - `dispatch_scroll(&mut self, lines: i16, ctx: &mut PanelContext<'_>) -> EventResult` — 返回 `EventResult::NotConsumed`
    - `status_bar_hints(&self) -> Vec<(&'static str, &'static str)>` — 返回空 `Vec`
  - 关键依赖项（use 语句）：

    ```rust
    use std::any::Any;
    use std::path::PathBuf;
    use std::sync::Arc;
    use tokio::sync::mpsc;
    use tui_textarea::Input;

    use super::agent_panel::AgentPanel;
    use super::chat_session::ChatSession;
    use super::config_panel::ConfigPanel;
    use super::cron_state::{CronPanel, CronState};
    use super::hooks_panel::HooksPanel;
    use super::login_panel::LoginPanel;
    use super::memory_panel::MemoryPanel;
    use super::mcp_panel::McpPanel;
    use super::model_panel::ModelPanel;
    use super::plugin_panel::PluginPanel;
    use super::status_panel::StatusPanel;
    use crate::config::PeriConfig;
    use crate::events::AgentEvent;
    use crate::thread::ThreadBrowser;
    use peri_middlewares::mcp::McpClientPool;
    use peri_middlewares::plugin::PluginLoadResult;
    use peri_middlewares::thread_store::ThreadStore;
    ```

  - 原因: 这些类型是整个面板组件化架构的类型基础，Task 2-7 将逐步为 PanelManager 填充真实行为

- [x] 为 `PanelState` 实现 `Any` downcast 方法
  - 位置: `peri-tui/src/app/panel_manager.rs`，`PanelState` impl 块内
  - 添加 `fn as_any_ref(&self) -> &dyn Any` 和 `fn as_any_mut(&mut self) -> &mut dyn Any`
  - 两个方法分别对 11 个变体做 `p as &dyn Any` / `p as &mut dyn Any` 的 match
  - 原因: `PanelManager::get::<T>()` 和 `get_mut::<T>()` 在 Task 2+ 需要通过 Any trait 做 downcast 类型安全访问

- [x] 新建 `peri-tui/src/app/panel_component.rs`，定义 `PanelComponent` trait
  - 位置: 新文件，完整内容如下
  - 在文件顶部标注 `#![allow(dead_code)]`
  - 定义 `PanelComponent` trait，supertrait 为 `Any`，包含以下方法：

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

  - 关键依赖项：

    ```rust
    use std::any::Any;
    use ratatui::layout::Rect;
    use ratatui::Frame;
    use tui_textarea::Input;

    use super::panel_manager::{EventResult, PanelContext, PanelKind};
    use super::App;
    ```

  - 原因: `PanelComponent` 是所有面板的统一行为接口，Task 3-5 中每个面板文件将添加 `impl PanelComponent for XxxPanel`

- [x] 修改 `peri-tui/src/app/mod.rs`，添加模块声明和 re-export
  - 位置: `mod.rs` L33（`mod panel_ops;` 之后），添加两行：

    ```rust
    pub mod panel_component;
    pub mod panel_manager;
    ```

  - 在 re-export 区域（L83 `pub use mcp_panel::{DetailAction, McpPanel, McpPanelView};` 之后），添加：

    ```rust
    pub use panel_component::PanelComponent;
    pub use panel_manager::{
        EventResult, MutexGroup, PanelContext, PanelKind, PanelManager, PanelScope, PanelState,
    };
    ```

  - 原因: `panel_component` 和 `panel_manager` 需要被 event.rs、各面板文件、main_ui.rs、status_bar.rs 引用，必须 pub 声明

- [x] 验证编译通过
  - 运行: `cargo build -p peri-tui 2>&1`
  - 预期: 编译成功，无错误。新类型标注了 `#[allow(dead_code)]`，不会有 unused 警告
  - 原因: 本 Task 目标是纯类型定义，不改变任何运行时行为

- [x] 为 `PanelKind` 和 `PanelManager` 编写单元测试
  - 测试文件: `peri-tui/src/app/panel_manager.rs`（在文件底部 `#[cfg(test)] mod tests` 块中）
  - 测试场景:
    - `test_panel_kind_scope`: 验证 `PanelKind::Model.scope() == PanelScope::Session`，`PanelKind::Mcp.scope() == PanelScope::Global`
    - `test_panel_kind_priority_unique`: 收集所有 `PanelKind` 变体的 `priority()` 值，验证 11 个值互不相同且范围为 0-10
    - `test_panel_state_kind_roundtrip`: 对 `PanelState` 的每个变体构造实例，验证 `state.kind()` 返回对应的 `PanelKind`（注意：部分面板构造函数需要参数，对构造函数参数要求复杂的变体可跳过，至少覆盖无参或简单构造的变体）
    - `test_panel_manager_new_is_empty`: `PanelManager::new().is_any_open() == false`，`active_kind() == None`
    - `test_panel_manager_open_close`: 打开一个 `PanelState::Status(StatusPanel::new())`，验证 `is_active(PanelKind::Status) == true`，关闭后 `is_any_open() == false`
    - `test_event_result_variants`: 验证 `EventResult` 的 4 个变体可以构造（`Consumed`、`NotConsumed`、`ClosePanel`、`OpenPanel(PanelKind::Model)`）
  - 运行命令: `cargo test -p peri-tui --lib -- panel_manager::tests`
  - 预期: 所有测试通过

**检查步骤:**

- [x] 验证新文件存在且模块声明正确
  - `grep -n "pub mod panel_component\|pub mod panel_manager" peri-tui/src/app/mod.rs`
  - 预期: 输出两行匹配，分别对应 L34 和 L35 附近
- [x] 验证 PanelKind 枚举包含全部 11 个变体
  - `grep -c "Model\|Login\|Agent\|Hooks\|Config\|ThreadBrowser\|Mcp\|Plugin\|Cron\|Status\|Memory" peri-tui/src/app/panel_manager.rs | head -1`
  - 预期: 匹配数 >= 11（枚举定义 + impl match 分支会重复）
- [x] 验证 PanelState 枚举包含全部 11 个变体
  - `grep "PanelState::" peri-tui/src/app/panel_manager.rs | grep -v "//" | wc -l`
  - 预期: 至少 22 行（kind() 方法 11 个 + as_any_ref 11 个 + as_any_mut 11 个）
- [x] 验证 re-export 链完整
  - `grep "pub use panel" peri-tui/src/app/mod.rs`
  - 预期: 输出包含 `panel_component::PanelComponent` 和 `panel_manager::{EventResult, MutexGroup, PanelContext, PanelKind, PanelManager, PanelScope, PanelState}`
- [x] 验证全量编译通过
  - `cargo build -p peri-tui 2>&1 | tail -3`
  - 预期: 输出包含 "Finished" 且无 error
- [x] 验证单元测试通过
  - `cargo test -p peri-tui --lib -- panel_manager::tests 2>&1 | tail -5`
  - 预期: 输出包含 "test result: ok" 且无失败

---

### Task 2: PanelManager 互斥管理

**背景:**
[业务语境] 当前 11 个面板分散在 `AppCore`（6 个 session 面板）、`App`（4 个全局面板）、`CronState`（1 个全局面板）中，互斥逻辑由每个 `open_*` 方法手动将 3-10 个其他面板设为 `None`。本 Task 将 `PanelManager` 实例注入 `AppCore` 和 `App`，新增 `App::open_panel()` 统一处理跨作用域互斥，并采用双写策略：`PanelManager` 作为 source of truth，旧 `Option<XxxPanel>` 字段作为镜像保持渲染和事件处理兼容。`CronPanel` 从 `CronState` 中取出迁移到 `App::global_panels`。
[修改原因] 现有互斥逻辑散布在 11 个 `open_*` 方法中，组合数为 O(N^2)，MCP/Cron/ThreadBrowser 三个面板甚至没有互斥处理。将互斥逻辑统一到 `PanelManager::open()` 自动关闭前一面板 + `App::open_panel()` 额外关闭跨作用域面板。
[上下游影响] 本 Task 输出被 Task 3-5（事件分发迁移）依赖：`App::open_panel()` 和 `PanelManager` 实例是后续事件分发替换的基础。本 Task 依赖 Task 1（`PanelKind`/`PanelState`/`PanelManager` 类型已定义）。

**涉及文件:**

- 修改: `peri-tui/src/app/core.rs`（添加 `session_panels` 字段 + 初始化）
- 修改: `peri-tui/src/app/mod.rs`（添加 `global_panels` 字段 + 初始化 + `open_panel()`/`close_all_panels()` 方法）
- 修改: `peri-tui/src/app/panel_ops.rs`（9 个 `open_*`/`close_*` 方法双写 + 新增 `open_mcp_panel`/`open_cron_panel` 方法 + `new_headless` 更新）
- 修改: `peri-tui/src/app/cron_state.rs`（移除 `cron_panel` 字段）
- 修改: `peri-tui/src/app/cron_ops.rs`（改用 `global_panels.get_mut::<CronPanel>()` 访问）
- 修改: `peri-tui/src/command/mcp.rs`（改用 `open_mcp_panel()`）
- 修改: `peri-tui/src/command/cron.rs`（改用 `open_cron_panel()`）
- 修改: `peri-tui/src/app/thread_ops.rs`（改用 `open_panel()` 打开 ThreadBrowser）
- 修改: `peri-tui/src/ui/headless.rs`（`app.cron.cron_panel` 改为 `global_panels` 赋值）
- 修改: `peri-tui/src/ui/main_ui/panels/cron.rs`（headless 测试中 `cron.cron_panel` 改为 `global_panels`）

**执行步骤:**

- [x] 在 `core.rs` 的 `AppCore` 中添加 `session_panels: PanelManager` 字段
  - 位置: `peri-tui/src/app/core.rs` struct 定义 L46（`model_panel` 字段之前）
  - 在 `pub model_panel: Option<ModelPanel>` 之前插入: `pub session_panels: super::panel_manager::PanelManager,`
  - 在 `AppCore::new()` 的 `Self { ... }` 初始化块中（L98-138），在 `model_panel: None,` 之前插入: `session_panels: super::panel_manager::PanelManager::new(),`
  - 原因: `session_panels` 管理 session-scoped 面板（Model/Login/Agent/Hooks/Config/ThreadBrowser），随 session 独立生命周期

- [x] 在 `mod.rs` 的 `App` 中添加 `global_panels: PanelManager` 字段
  - 位置: `peri-tui/src/app/mod.rs` struct 定义 L118（`mcp_panel` 字段之前）
  - 在 `pub mcp_panel: Option<McpPanel>,` 之前插入: `pub global_panels: super::panel_manager::PanelManager,`
  - 在 `App::new()` 的 `Self { ... }` 初始化块中（L198-228），在 `mcp_panel: None,` 之前插入: `global_panels: super::panel_manager::PanelManager::new(),`
  - 原因: `global_panels` 管理 global-scoped 面板（Mcp/Plugin/Cron/Status/Memory），跨 session 保持

- [x] 在 `mod.rs` 中添加 `App::open_panel()` 方法
  - 位置: `peri-tui/src/app/mod.rs`，在 `get_current_task_duration()` 方法（L501）之后
  - 实现逻辑:

    ```rust
    /// 打开面板（统一处理跨作用域互斥）：关闭所有 manager 中的面板后，放入正确的 manager
    pub fn open_panel(&mut self, state: super::panel_manager::PanelState) {
        self.global_panels.close();
        for session in &mut self.sessions {
            session.core.session_panels.close();
        }
        match state.kind().scope() {
            super::panel_manager::PanelScope::Session => {
                self.sessions[self.active].core.session_panels.open(state);
            }
            super::panel_manager::PanelScope::Global => {
                self.global_panels.open(state);
            }
        }
    }
    ```

  - 原因: `PanelManager::open()` 自动关闭同一 manager 中的前一面板；`open_panel()` 额外关闭另一个 manager 中的面板，实现跨作用域互斥

- [x] 在 `mod.rs` 中添加 `App::close_all_panels()` 方法
  - 位置: 紧接 `open_panel()` 方法之后
  - 实现逻辑:

    ```rust
    /// 关闭所有面板（session + global）
    pub fn close_all_panels(&mut self) {
        self.global_panels.close();
        for session in &mut self.sessions {
            session.core.session_panels.close();
        }
    }
    ```

  - 原因: Ctrl+C 中断和 session 切换时需要一次性关闭所有面板

- [x] 从 `CronState` 中移除 `cron_panel` 字段
  - 位置: `peri-tui/src/app/cron_state.rs` L49
  - 将 `pub cron_panel: Option<CronPanel>,` 从 `CronState` struct 定义中删除
  - 在 `CronState::new()` 中删除 `cron_panel: None,` 初始化
  - 原因: CronPanel 迁移到 `App::global_panels` 统一管理

- [x] 修改 `cron_ops.rs` 中所有 `self.cron.cron_panel` 引用，改用 `global_panels`
  - 位置: `peri-tui/src/app/cron_ops.rs` L4, L11, L18, L30, L39, L55, L65, L72
  - 替换规则: `self.cron.cron_panel` 通过 `self.global_panels.get_mut::<CronPanel>()` 访问
  - `cron_panel_move_up` (L4): `if let Some(panel) = self.global_panels.get_mut::<CronPanel>() { panel.move_cursor(-1); }`
  - `cron_panel_move_down` (L11): `if let Some(panel) = self.global_panels.get_mut::<CronPanel>() { panel.move_cursor(1); }`
  - `cron_panel_toggle` (L18): `if let Some(panel) = self.global_panels.get_mut::<CronPanel>() { ... panel.refresh(&self.cron.scheduler); }`
  - `cron_panel_request_delete` (L30): `if let Some(panel) = self.global_panels.get_mut::<CronPanel>() { ... }`
  - `cron_panel_confirm_delete` (L39): `if let Some(panel) = self.global_panels.get_mut::<CronPanel>() { ... if panel.tasks.is_empty() { self.global_panels.close(); ... } }`
  - `cron_panel_cancel_delete` (L65): `if let Some(panel) = self.global_panels.get_mut::<CronPanel>() { panel.confirm_delete = false; }`
  - `cron_panel_close` (L72): `self.global_panels.close(); self.sessions[self.active].core.panel_selection.clear(); self.sessions[self.active].core.panel_area = None;`
  - 需在文件顶部添加: `use super::cron_state::CronPanel;`（若不存在）
  - 原因: CronPanel 已从 CronState 迁移到 global_panels

- [x] 新增 `open_mcp_panel()` 方法到 `panel_ops.rs`
  - 位置: `peri-tui/src/app/panel_ops.rs`，在 `close_memory_panel()` 方法（L269）之后
  - 实现逻辑:

    ```rust
    /// 打开 MCP 面板（替代 command/mcp.rs 中的直接赋值）
    pub fn open_mcp_panel(&mut self) {
        let infos = self.mcp_pool.as_ref()
            .map(|p| p.all_server_infos())
            .unwrap_or_default();
        if infos.is_empty() {
            let vm = crate::ui::message_view::MessageViewModel::system(
                "无 MCP 服务器配置（请在 .mcp.json 或 settings.json 中添加）".to_string(),
            );
            self.sessions[self.active].core.view_messages.push(vm.clone());
            let _ = self.sessions[self.active].core.render_tx
                .send(crate::ui::render_thread::RenderEvent::AddMessage(vm));
            return;
        }
        let panel = McpPanel::new(infos);
        // 新路径：通过 PanelManager
        self.open_panel(super::panel_manager::PanelState::Mcp(panel.clone()));
        // 旧路径：双写镜像（渲染和事件处理仍读旧字段）
        self.mcp_panel = Some(panel);
    }
    ```

  - 原因: MCP 面板原来在 `command/mcp.rs` 中直接赋值无互斥处理，统一到 PanelManager

- [x] 新增 `open_cron_panel()` 方法到 `panel_ops.rs`
  - 位置: 紧接 `open_mcp_panel()` 之后
  - 实现逻辑:

    ```rust
    /// 打开 Cron 面板（替代 command/cron.rs 中的直接赋值）
    pub fn open_cron_panel(&mut self) {
        let tasks: Vec<_> = self.cron.scheduler.lock()
            .list_tasks().into_iter().cloned().collect();
        if tasks.is_empty() {
            let vm = crate::ui::message_view::MessageViewModel::system("无定时任务".to_string());
            self.sessions[self.active].core.view_messages.push(vm.clone());
            let _ = self.sessions[self.active].core.render_tx
                .send(crate::ui::render_thread::RenderEvent::AddMessage(vm));
            return;
        }
        let panel = CronPanel::new(tasks);
        // 新路径：通过 PanelManager（CronPanel 已从 CronState 移除，无旧字段镜像）
        self.open_panel(super::panel_manager::PanelState::Cron(panel));
    }
    ```

  - 原因: Cron 面板原来在 `command/cron.rs` 中直接赋值 `app.cron.cron_panel`，现该字段已从 CronState 移除

- [x] 修改 `command/mcp.rs` 改用 `open_mcp_panel()`
  - 位置: `peri-tui/src/command/mcp.rs` L15-35
  - 将 `McpCommand::execute()` 方法体替换为: `app.open_mcp_panel();`
  - 原因: 互斥逻辑统一到 `open_mcp_panel()` 内部

- [x] 修改 `command/cron.rs` 改用 `open_cron_panel()`
  - 位置: `peri-tui/src/command/cron.rs` L17-38
  - 将 `CronCommand::execute()` 方法体替换为: `app.open_cron_panel();`
  - 移除不再需要的 `use crate::app::CronPanel;` 导入
  - 原因: CronPanel 不再在 CronState 中，统一到 `open_cron_panel()`

- [x] 修改 `thread_ops.rs` 的 ThreadBrowser 打开逻辑
  - 位置: `peri-tui/src/app/thread_ops.rs` L349-353
  - 将 `self.sessions[self.active].core.thread_browser = Some(ThreadBrowser::new(...))` 改为:

    ```rust
    let browser = ThreadBrowser::new(filtered, self.thread_store.clone(), branch);
    // 新路径：通过 PanelManager
    self.open_panel(super::panel_manager::PanelState::ThreadBrowser(browser.clone()));
    // 旧路径：双写镜像
    self.sessions[self.active].core.thread_browser = Some(browser);
    ```

  - 原因: ThreadBrowser 原来直接赋值无互斥处理，需统一到 PanelManager

- [x] 修改 `panel_ops.rs` 中 9 个已有 `open_*` 方法，添加 PanelManager 双写
  - 互斥矩阵（精确列出每个方法关闭的面板）：

    | 方法 | PanelState 变体 | 旧互斥（保留） |
    |------|----------------|----------------|
    | `open_model_panel` L7 | `PanelState::Model` | login, config, status, memory |
    | `open_login_panel` L61 | `PanelState::Login` | model, config, status, memory |
    | `open_config_panel` L198 | `PanelState::Config` | login, model |
    | `open_status_panel` L238 | `PanelState::Status` | config, login, model |
    | `open_memory_panel` L253 | `PanelState::Memory` | config, login, model, status |
    | `open_plugin_panel` L271 | `PanelState::Plugin` | login, model, config, status, memory, mcp |
    | `open_agent_panel` L795 | `PanelState::Agent` | 无 |
    | `open_hooks_panel` L875 | `PanelState::Hooks` | login, config, status, memory |

  - 每个方法的修改模式（以 `open_model_panel` 为例）:

    ```rust
    pub fn open_model_panel(&mut self) {
        let cfg = self.peri_config.get_or_insert_with(PeriConfig::default);
        let panel = ModelPanel::from_config(cfg);
        // 新路径：通过 PanelManager（source of truth）
        self.open_panel(super::panel_manager::PanelState::Model(panel.clone()));
        // 旧路径：双写镜像（渲染和事件处理仍读旧字段）
        self.sessions[self.active].core.model_panel = Some(panel);
        // 旧互斥（保留，后续 Task 7 清理时删除）
        self.sessions[self.active].core.login_panel = None;
        self.sessions[self.active].core.config_panel = None;
        self.status_panel = None;
        self.memory_panel = None;
    }
    ```

  - 注意: `PluginPanel` 需要实现 `Clone`（若未实现则在 `plugin_panel.rs` 中添加 `#[derive(Clone)]` 或手动实现）
  - 注意: `AgentPanel` 的 `new()` 接受 `(Vec<AgentItem>, Option<String>)`，clone 时需先构造再 clone
  - 原因: 双写期间 PanelManager 是 source of truth，旧 Option 字段是镜像供渲染和事件处理读取

- [x] 修改 9 个 `close_*` 方法，同步关闭 PanelManager
  - `close_model_panel` (L18): 在旧字段 `None` 赋值之后添加 `self.sessions[self.active].core.session_panels.close_if(PanelKind::Model);`
  - `close_login_panel` (L73): 添加 `self.sessions[self.active].core.session_panels.close_if(PanelKind::Login);`
  - `close_config_panel` (L208): 添加 `self.sessions[self.active].core.session_panels.close_if(PanelKind::Config);`
  - `close_status_panel` (L247): 添加 `self.global_panels.close_if(PanelKind::Status);`
  - `close_memory_panel` (L267): 添加 `self.global_panels.close_if(PanelKind::Memory);`
  - `close_plugin_panel` (L561): 添加 `self.global_panels.close_if(PanelKind::Plugin);`
  - `close_agent_panel` (L803): 添加 `self.sessions[self.active].core.session_panels.close_if(PanelKind::Agent);`
  - `close_hooks_panel` (L894): 添加 `self.sessions[self.active].core.session_panels.close_if(PanelKind::Hooks);`
  - `agent_panel_clear` (L868): 添加 `self.sessions[self.active].core.session_panels.close_if(PanelKind::Agent);`
  - 需在文件顶部添加: `use super::panel_manager::PanelKind;`
  - 原因: 关闭操作需同时清理 PanelManager 中的状态，保持双写一致

- [x] 修改 `model_panel_confirm()` 和 `agent_panel_confirm()` 同步清理 PanelManager
  - 位置: `peri-tui/src/app/panel_ops.rs`
  - `model_panel_confirm()` L55: 在 `self.sessions[self.active].core.model_panel = None;` 之后添加 `self.sessions[self.active].core.session_panels.close_if(PanelKind::Model);`
  - `agent_panel_confirm()` L863: 在 `self.sessions[self.active].core.agent_panel = None;` 之后添加 `self.sessions[self.active].core.session_panels.close_if(PanelKind::Agent);`
  - 原因: 确认选择后关闭面板需同步清理 PanelManager

- [x] 更新 `panel_ops.rs` 中 `new_headless()` 的 App 初始化
  - 位置: `peri-tui/src/app/panel_ops.rs` L993-1026（`let app = App { ... }` 块）
  - 在 `mcp_panel: None,` 之前添加: `global_panels: super::panel_manager::PanelManager::new(),`
  - 原因: headless 测试的 App 构造需与生产构造保持一致（`AppCore::new()` 已包含 `session_panels` 初始化）

- [x] 更新 `headless.rs` 中直接赋值 `app.cron.cron_panel` 的测试代码
  - 位置: `peri-tui/src/ui/headless.rs` L1069, L2447, L2501
  - 将 `app.cron.cron_panel = Some(CronPanel::new(tasks))` 改为:

    ```rust
    app.global_panels.open(crate::app::panel_manager::PanelState::Cron(
        crate::app::CronPanel::new(tasks),
    ));
    ```

  - 将断言 `app.cron.cron_panel.as_ref().unwrap()` 改为 `app.global_panels.get::<crate::app::CronPanel>().unwrap()`
  - 将断言 `app.cron.cron_panel.as_mut().unwrap()` 改为 `app.global_panels.get_mut::<crate::app::CronPanel>().unwrap()`
  - 原因: CronPanel 已从 CronState 移除到 global_panels

- [x] 更新 `ui/main_ui/panels/cron.rs` 中 headless 测试的 CronPanel 赋值
  - 位置: `peri-tui/src/ui/main_ui/panels/cron.rs` L146
  - 将 `app.cron.cron_panel = Some(CronPanel::new(vec![]))` 改为:

    ```rust
    app.global_panels.open(crate::app::panel_manager::PanelState::Cron(CronPanel::new(vec![])));
    ```

  - 原因: 与 CronPanel 迁移保持一致

- [x] 为 `open_panel()`/`close_all_panels()`/跨作用域互斥编写单元测试
  - 测试文件: `peri-tui/src/app/panel_ops.rs` 的 `#[cfg(any(test, feature = "headless"))] impl App` 块末尾
  - 测试场景:
    - `test_open_panel_cross_scope_mutex`: 打开 Model 面板（session）后调用 `open_panel(PanelState::Mcp(...))`，验证 `session_panels.is_any_open() == false` 且 `global_panels.is_active(PanelKind::Mcp) == true`
    - `test_open_panel_same_scope_auto_close`: 连续两次 `open_panel(PanelState::Model(...))` 和 `open_panel(PanelState::Login(...))`，验证只有最后一个面板在 `session_panels` 中
    - `test_close_all_panels`: 打开 session 和 global 面板后调用 `close_all_panels()`，验证 `session_panels.is_any_open() == false` 且 `global_panels.is_any_open() == false`
    - `test_cron_panel_in_global_panels`: 调用 `open_cron_panel()` 后，验证 `global_panels.is_active(PanelKind::Cron)` 为 true
  - 运行命令: `cargo test -p peri-tui --lib -- test_open_panel test_close_all test_cron_panel_in_global`
  - 预期: 所有测试通过

**检查步骤:**

- [x] 验证编译通过
  - `cargo build -p peri-tui 2>&1 | tail -20`
  - 预期: 编译成功，无错误
- [x] 验证所有现有测试通过
  - `cargo test -p peri-tui 2>&1 | tail -30`
  - 预期: 所有测试通过，无失败
- [x] 验证 PanelManager 字段存在于 AppCore 和 App 中
  - `grep -n "session_panels:" peri-tui/src/app/core.rs && grep -n "global_panels:" peri-tui/src/app/mod.rs`
  - 预期: 各输出 2 行匹配（字段声明 + 初始化）
- [x] 验证 CronState 中不再包含 cron_panel 字段
  - `grep -n "cron_panel" peri-tui/src/app/cron_state.rs`
  - 预期: 无匹配（字段已移除）
- [x] 验证 cron_ops.rs 不再引用 self.cron.cron_panel
  - `grep -n "self.cron.cron_panel" peri-tui/src/app/cron_ops.rs`
  - 预期: 无匹配
- [x] 验证 open_panel 方法存在
  - `grep -n "pub fn open_panel" peri-tui/src/app/mod.rs`
  - 预期: 输出 1 行匹配
- [x] 验证 headless 测试不再引用 cron.cron_panel
  - `grep -rn "cron\.cron_panel" peri-tui/src/ui/headless.rs peri-tui/src/ui/main_ui/panels/cron.rs`
  - 预期: 无匹配
- [x] 验证 clippy 无警告
  - `cargo clippy -p peri-tui 2>&1 | tail -10`
  - 预期: 无 warning 或 error

---

### Task 3: 简单面板事件迁移（Model, Agent, Hooks, Status, Memory）

**背景:**
[业务语境] 将 Model/Agent/Hooks/Status/Memory 5 个简单面板的事件处理从 `event.rs` 中的 if-else 分发链 + `handle_xxx_panel` 自由函数迁移到 `PanelComponent` trait 的 `handle_key` 方法。这 5 个面板 handler 逻辑简单（20-90 行），无文本输入字段（除 ModelPanel），是迁移的最佳起点。
[修改原因] 当前 `event.rs` L251-303 中 5 个面板各自有 `if app.xxx_panel.is_some() { handle_xxx_panel(app, input); return Ok(Some(Action::Redraw)); }` 的分发模式，ModelPanel handler 有 8 处 `unwrap()`。迁移后通过 `PanelManager::dispatch_key()` 统一分发，面板作为 `&mut self` 传入消除 unwrap。
[上下游影响] 本 Task 依赖 Task 1（PanelKind/PanelState/PanelComponent/PanelManager/PanelContext/EventResult 类型定义）和 Task 2（PanelManager 添加到 AppCore/App + open/close 双写）。本 Task 输出的 5 个 `impl PanelComponent for XxxPanel` 被 Task 6（渲染迁移 + 状态栏解耦）和 Task 7（清理旧字段）依赖。

**涉及文件:**

- 修改: `peri-tui/src/app/model_panel.rs`（添加 `impl PanelComponent for ModelPanel`）
- 修改: `peri-tui/src/app/agent_panel.rs`（添加 `impl PanelComponent for AgentPanel`）
- 修改: `peri-tui/src/app/hooks_panel.rs`（添加 `impl PanelComponent for HooksPanel`）
- 修改: `peri-tui/src/app/status_panel.rs`（添加 `impl PanelComponent for StatusPanel`）
- 修改: `peri-tui/src/app/memory_panel.rs`（添加 `impl PanelComponent for MemoryPanel`）
- 修改: `peri-tui/src/event.rs`（添加 PanelManager 分发入口，5 个面板改走新路径）
- 修改: `peri-tui/src/app/panel_ops.rs`（model_panel_confirm / agent_panel_confirm 中添加 PanelManager 同步逻辑）

**执行步骤:**

- [x] 为 ModelPanel 实现 PanelComponent trait
  - 位置: `peri-tui/src/app/model_panel.rs` 文件末尾（`#[cfg(test)]` 之前）
  - 在文件顶部添加 use 语句：

    ```rust
    use std::any::Any;
    use ratatui::layout::Rect;
    use ratatui::Frame;
    use tui_textarea::Input;

    use super::panel_component::PanelComponent;
    use super::panel_manager::{EventResult, PanelContext, PanelKind};
    use super::App;
    ```

  - 添加 `impl PanelComponent for ModelPanel` 块，包含以下方法：
    - `fn kind(&self) -> PanelKind { PanelKind::Model }`
    - `fn handle_key(&mut self, input: Input, ctx: &mut PanelContext<'_>) -> EventResult`
      - 将 `event.rs` L1485-1574 的 `handle_model_panel` 函数体迁移到此方法
      - `Key::Esc` → 返回 `EventResult::ClosePanel`（替代 `app.close_model_panel()`）
      - `Key::Up` → `self.move_cursor(-1); EventResult::Consumed`（消除 `unwrap()`，self 直接访问）
      - `Key::Down` → `self.move_cursor(1); EventResult::Consumed`（同上）
      - `Key::Char(' ') | Key::Enter` → 匹配 `self.cursor`：
        - `ROW_OPUS` → `self.active_tab = AliasTab::Opus;` 调用 `Self::apply_and_close(self, ctx)` → 返回 `EventResult::ClosePanel`
        - `ROW_SONNET` → `self.active_tab = AliasTab::Sonnet;` 调用 `Self::apply_and_close(self, ctx)` → 返回 `EventResult::ClosePanel`
        - `ROW_HAIKU` → `self.active_tab = AliasTab::Haiku;` 调用 `Self::apply_and_close(self, ctx)` → 返回 `EventResult::ClosePanel`
        - `ROW_EFFORT` → `self.cycle_effort(false); EventResult::Consumed`
        - `_` → `EventResult::Consumed`
      - `Key::Left` → `self.cycle_effort(true); EventResult::Consumed`（消除 `unwrap()`）
      - `Key::Right` → `self.cycle_effort(false); EventResult::Consumed`（消除 `unwrap()`）
      - `_` → `EventResult::Consumed`
    - 添加私有辅助方法 `fn apply_and_close(panel: &ModelPanel, ctx: &mut PanelContext<'_>)`
      - 此方法执行 `panel_ops.rs` L23-56 `model_panel_confirm` 的核心逻辑：
        1. 调用 `panel.apply_to_config(ctx.peri_config.as_mut().unwrap())`
        2. 构建 alias_label 和 effort_display 字符串
        3. 向 `ctx.sessions[ctx.active].core.view_messages` push 系统消息 "模型已切换为: {alias_label} ({effort_display} effort)"
        4. 调用 `App::save_config(ctx.peri_config.as_ref().unwrap(), ctx.config_path_override.as_deref())`，失败时 push 错误消息
        5. 通过 `ctx.provider_name` 和 `ctx.model_name` 更新 provider/model 显示名称（从 `LlmProvider::from_config` 获取）
    - `fn desired_height(&self, _screen_height: u16, _screen_width: u16) -> u16 { 12 }`
    - `fn render(&self, f: &mut Frame, app: &App, area: Rect)` → 委托到 `crate::ui::main_ui::panels::model::render_model_panel(f, app, area)`
    - `fn as_any_ref(&self) -> &dyn Any { self }`
    - `fn as_any_mut(&mut self) -> &mut dyn Any { self }`
    - `fn status_bar_hints(&self) -> Vec<(&'static str, &'static str)>` → 返回 `vec![("↑↓", "导航"), ("Enter", "确认"), ("Space", "选择/切换"), ("Esc", "关闭")]`
  - 原因: 消除 8 处 `unwrap()`，将面板状态操作集中到 `&mut self`，返回 `EventResult::ClosePanel` 让 PanelManager 处理关闭

- [x] 为 AgentPanel 实现 PanelComponent trait
  - 位置: `peri-tui/src/app/agent_panel.rs` 文件末尾（`#[cfg(test)]` 之前）
  - 在文件顶部添加 use 语句（同 ModelPanel 模式）
  - 添加 `impl PanelComponent for AgentPanel` 块：
    - `fn kind(&self) -> PanelKind { PanelKind::Agent }`
    - `fn handle_key(&mut self, input: Input, ctx: &mut PanelContext<'_>) -> EventResult`
      - 将 `event.rs` L1203-1229 的 `handle_agent_panel` 函数体迁移到此方法
      - `Key::Char('c') + ctrl` → `EventResult::NotConsumed`（传递给全局 Ctrl+C 处理）
      - `Key::Esc` → 返回 `EventResult::ClosePanel`（替代 `app.close_agent_panel()` + 清理 `panel_selection`/`panel_area`）
      - `Key::Up` → `self.move_cursor(-1); self.scroll_offset = ensure_cursor_visible(self.cursor as u16, self.scroll_offset, 10); EventResult::Consumed`
      - `Key::Down` → `self.move_cursor(1); self.scroll_offset = ensure_cursor_visible(self.cursor as u16, self.scroll_offset, 10); EventResult::Consumed`
      - `Key::Enter` → 执行 `agent_panel_confirm` 核心逻辑（从 `panel_ops.rs` L826-863 迁移）：
        1. 调用 `self.get_selection()` 获取 `(is_none, agent_id)`
        2. `is_none` → 调用 `ctx.sessions[ctx.active].set_agent_id(None)`（需通过 PanelContext 间接调用），push "Agent 已重置" 系统消息
        3. 有 `agent_id` → 调用 `ctx.sessions[ctx.active].set_agent_id(Some(id))`，push "Agent 已切换为: {name} ({id})" 系统消息
        4. 返回 `EventResult::ClosePanel`
      - `_` → `EventResult::Consumed`
    - `fn desired_height(&self, _screen_height: u16, _screen_width: u16) -> u16 { (self.agents.len() as u16 * 2 + 6).max(6) }`
    - `fn render` → 委托到 `crate::ui::main_ui::panels::agent::render_agent_panel(f, app, area)`
    - `fn as_any_ref/as_any_mut` → 标准 `self` 转换
    - `fn status_bar_hints` → `vec![("↑↓", "选择"), ("Enter", "确认"), ("Esc", "取消")]`
  - 原因: 将 `agent_panel_move_up/down/confirm` 三个 App 方法内联到面板自身，消除对 `app.sessions[active].core.agent_panel.as_mut().unwrap()` 的间接访问

- [x] 为 HooksPanel 实现 PanelComponent trait
  - 位置: `peri-tui/src/app/hooks_panel.rs` 文件末尾（`#[cfg(test)]` 之前）
  - 添加 `impl PanelComponent for HooksPanel` 块：
    - `fn kind(&self) -> PanelKind { PanelKind::Hooks }`
    - `fn handle_key(&mut self, input: Input, _ctx: &mut PanelContext<'_>) -> EventResult`
      - 将 `event.rs` L1233-1253 的 `handle_hooks_panel` 函数体迁移到此方法
      - `Key::Char('c') + ctrl` → `EventResult::NotConsumed`
      - `Key::Esc` → 返回 `EventResult::ClosePanel`
      - `Key::Up` → `self.move_cursor(-1); self.scroll_offset = ensure_cursor_visible(self.cursor_line(), self.scroll_offset, 10); EventResult::Consumed`
      - `Key::Down` → `self.move_cursor(1); self.scroll_offset = ensure_cursor_visible(self.cursor_line(), self.scroll_offset, 10); EventResult::Consumed`
      - `_` → `EventResult::Consumed`
    - `fn desired_height(&self, _screen_height: u16, _screen_width: u16) -> u16 { self.total_content_lines().max(8) }`
    - `fn render` → 委托到 `crate::ui::main_ui::panels::hooks::render_hooks_panel(f, app, area)`
    - `fn as_any_ref/as_any_mut` → 标准 `self` 转换
    - `fn status_bar_hints` → `vec![("↑↓", "导航"), ("Esc", "关闭")]`
  - 原因: HooksPanel 是最简单的面板之一（20 行 handler），迁移风险最低

- [x] 为 StatusPanel 实现 PanelComponent trait
  - 位置: `peri-tui/src/app/status_panel.rs` 文件末尾
  - 添加 `impl PanelComponent for StatusPanel` 块：
    - `fn kind(&self) -> PanelKind { PanelKind::Status }`
    - `fn handle_key(&mut self, input: Input, _ctx: &mut PanelContext<'_>) -> EventResult`
      - 将 `event.rs` L1668-1687 的 `handle_status_panel` 函数体迁移到此方法
      - `Key::Esc` → 返回 `EventResult::ClosePanel`
      - `Key::Left` → `self.tab.prev(); EventResult::Consumed`
      - `Key::Right` → `self.tab.next(); EventResult::Consumed`
      - `_` → `EventResult::Consumed`
    - `fn desired_height(&self, _screen_height: u16, _screen_width: u16) -> u16 { 14 }`
    - `fn render` → 委托到 `crate::ui::main_ui::panels::status::render_status_panel(f, app, area)`
    - `fn as_any_ref/as_any_mut` → 标准 `self` 转换
    - `fn status_bar_hints` → `vec![("←→", "切换Tab"), ("Esc", "关闭")]`
  - 原因: StatusPanel 只有 Tab 切换和 Esc 关闭，迁移最简单

- [x] 为 MemoryPanel 实现 PanelComponent trait
  - 位置: `peri-tui/src/app/memory_panel.rs` 文件末尾（`#[cfg(test)]` 之前）
  - 添加 `impl PanelComponent for MemoryPanel` 块：
    - `fn kind(&self) -> PanelKind { PanelKind::Memory }`
    - `fn handle_key(&mut self, input: Input, ctx: &mut PanelContext<'_>) -> EventResult`
      - 将 `event.rs` L1689-1710 的 `handle_memory_panel` 函数体迁移到此方法
      - `Key::Up` → `self.move_cursor_up(); EventResult::Consumed`
      - `Key::Down` → `self.move_cursor_down(); EventResult::Consumed`
      - `Key::Enter` → 返回 `EventResult::OpenPanel(PanelKind::Memory)`（使用特殊标记，调用方通过匹配此结果判断需执行编辑器打开逻辑）
        - 注意：Enter 打开编辑器涉及 TUI 挂起/恢复（raw mode 切换），不能在 `handle_key` 内执行。返回值使用 `EventResult::OpenPanel(PanelKind::Memory)` 作为信号，在 `event.rs` 的分发代码中检测到 `PanelKind::Memory == 当前面板 Kind` 时调用 `app.memory_panel_open_editor()`
      - `Key::Esc` → 返回 `EventResult::ClosePanel`
      - `_` → `EventResult::Consumed`
    - `fn handle_paste(&mut self, _text: &str, _ctx: &mut PanelContext<'_>) -> EventResult { EventResult::Consumed }`
    - `fn desired_height(&self, _screen_height: u16, _screen_width: u16) -> u16 { (self.entries.len() as u16 * 2 + 4).max(6) }`
    - `fn render` → 委托到 `crate::ui::main_ui::panels::memory::render_memory_panel(f, app, area)`
    - `fn as_any_ref/as_any_mut` → 标准 `self` 转换
    - `fn status_bar_hints` → `vec![("↑↓", "选择"), ("Enter", "编辑"), ("Esc", "关闭")]`
  - 原因: MemoryPanel 的 Enter 需要在调用方处理编辑器打开（涉及 TUI raw mode 切换），通过 `EventResult::OpenPanel(PanelKind::Memory)` 返回特殊标记

- [x] 修改 `event.rs`，添加 PanelManager 分发入口（双写过渡）
  - 位置: `peri-tui/src/event.rs` L251-303（5 个面板的 if-else 分发区域）
  - 在现有 `if app.sessions[app.active].core.agent_panel.is_some()` 之前（L251），插入 PanelManager 分发代码块：

    ```rust
    // PanelManager 分发（已迁移的面板）
    if app.sessions[app.active].core.session_panels.is_any_open() {
        let kind = app.sessions[app.active].core.session_panels.active_kind();
        match kind {
            Some(PanelKind::Model) | Some(PanelKind::Agent) | Some(PanelKind::Hooks) => {
                // 构造 PanelContext（与 Task 2 双写一致的解构模式）
                let App {
                    ref mut sessions,
                    ref mut peri_config,
                    ref mut provider_name,
                    ref mut model_name,
                    ref mut mcp_pool,
                    ref mut cron,
                    ref mut plugin_data,
                    ref mut config_path_override,
                    ref bg_event_tx,
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

                let result = app.sessions[app.active].core.session_panels.dispatch_key(input, &mut ctx);

                // 处理返回结果
                match result {
                    EventResult::ClosePanel => {
                        app.sessions[app.active].core.session_panels.close();
                        // 同步旧字段（双写）
                        match kind {
                            Some(PanelKind::Model) => { app.sessions[app.active].core.model_panel = None; }
                            Some(PanelKind::Agent) => { app.sessions[app.active].core.agent_panel = None; }
                            Some(PanelKind::Hooks) => { app.sessions[app.active].core.hooks_panel = None; }
                            _ => {}
                        }
                        app.sessions[app.active].core.panel_selection.clear();
                        app.sessions[app.active].core.panel_area = None;
                    }
                    EventResult::OpenPanel(open_kind) if open_kind == kind.unwrap() => {
                        // MemoryPanel Enter → 打开编辑器
                        if kind == Some(PanelKind::Memory) {
                            if let Err(e) = app.memory_panel_open_editor() {
                                tracing::error!("Failed to open editor: {}", e);
                            }
                        }
                    }
                    _ => {}
                }
                return Ok(Some(Action::Redraw));
            }
            _ => {} // 其他 session 面板（Login/Config/ThreadBrowser）走旧路径
        }
    }
    if app.global_panels.is_any_open() {
        let kind = app.global_panels.active_kind();
        match kind {
            Some(PanelKind::Status) | Some(PanelKind::Memory) => {
                // 同上构造 PanelContext 并分发
                // ...（同上解构模式）
                let result = app.global_panels.dispatch_key(input, &mut ctx);
                match result {
                    EventResult::ClosePanel => {
                        app.global_panels.close();
                        match kind {
                            Some(PanelKind::Status) => { app.status_panel = None; }
                            Some(PanelKind::Memory) => { app.memory_panel = None; }
                            _ => {}
                        }
                    }
                    EventResult::OpenPanel(open_kind) if open_kind == kind.unwrap() => {
                        if kind == Some(PanelKind::Memory) {
                            if let Err(e) = app.memory_panel_open_editor() {
                                tracing::error!("Failed to open editor: {}", e);
                            }
                        }
                    }
                    _ => {}
                }
                return Ok(Some(Action::Redraw));
            }
            _ => {} // 其他全局面板（Mcp/Plugin/Cron）走旧路径
        }
    }
    ```

  - 保留 L251-303 中 ThreadBrowser/Cron/OAuth/MCP/Plugin/Login/Config 的旧 if-else 链不变
  - 将 L251-255（agent_panel）、L257-261（hooks_panel）、L269-273（model_panel）的旧分发代码注释掉（不删除，保留双写参考），添加 `// [Task 3] 已迁移到 PanelManager 分发` 注释
  - 将 L281-285（status_panel）、L287-303（memory_panel）的旧分发代码注释掉，添加相同注释
  - 在 `Event::Paste` 分支（L716-718 model_panel 拦截 + L759-770 批量拦截），为已迁移面板添加 PanelManager 分发：
    - L716-718 `model_panel` 拦截 → 改为检查 `session_panels.is_active(PanelKind::Model)`
    - L759-770 批量拦截列表中将 `agent_panel`、`hooks_panel`、`status_panel`、`memory_panel` 的 `is_some()` 检查改为 PanelManager `is_active()` 检查
  - 原因: 双写过渡期保证已迁移面板走 PanelManager 新路径，未迁移面板（Login/Config/ThreadBrowser/MCP/Plugin/Cron）走旧 if-else 路径

- [x] 修改 `panel_ops.rs` 的 open/close 方法，同步 PanelManager（双写）
  - 位置: `peri-tui/src/app/panel_ops.rs`
  - 在 `open_model_panel` 方法（L7-15）末尾，`self.sessions[self.active].core.model_panel = Some(...)` 之后，添加：

    ```rust
    // 同步 PanelManager（双写）
    self.sessions[self.active].core.session_panels.open(PanelState::Model(panel));
    self.global_panels.close();
    ```

    - 注意：`panel` 变量需 clone 或在赋值前构造（Rust 所有权），改为先 clone 再分别赋值：

    ```rust
    let panel = ModelPanel::from_config(cfg);
    let panel_clone = panel.clone(); // ModelPanel 需实现 Clone
    self.sessions[self.active].core.model_panel = Some(panel);
    self.sessions[self.active].core.session_panels.open(PanelState::Model(panel_clone));
    ```

  - 在 `close_model_panel`（L18-20）中添加 `self.sessions[self.active].core.session_panels.close_if(PanelKind::Model);`
  - 在 `open_agent_panel`（L795-800）末尾添加 `self.sessions[self.active].core.session_panels.open(PanelState::Agent(panel.clone()));`（AgentPanel 已实现 Clone）
  - 在 `close_agent_panel`（L803-804）中添加 `self.sessions[self.active].core.session_panels.close_if(PanelKind::Agent);`
  - 在 `open_hooks_panel`（L875-891）末尾添加 `self.sessions[self.active].core.session_panels.open(PanelState::Hooks(panel.clone()));` + `self.global_panels.close();`
  - 在 `close_hooks_panel`（L894-895）中添加 `self.sessions[self.active].core.session_panels.close_if(PanelKind::Hooks);`
  - 在 `open_status_panel`（L238-244）末尾添加 `self.global_panels.open(PanelState::Status(panel.clone()));`
  - 在 `close_status_panel`（L247-248）中添加 `self.global_panels.close_if(PanelKind::Status);`
  - 在 `open_memory_panel`（L254-264）末尾添加 `self.global_panels.open(PanelState::Memory(panel.clone()));`
  - 在 `close_memory_panel`（L267-268）中添加 `self.global_panels.close_if(PanelKind::Memory);`
  - 原因: 双写期保证 PanelManager 和旧 Option<XxxPanel> 字段同步，渲染和其他未迁移逻辑仍读旧字段

- [x] 为 5 个面板的 handle_key 编写单元测试
  - 测试文件: 分别在各面板文件底部的 `#[cfg(test)] mod tests` 中添加
  - 测试场景（以 ModelPanel 为例，其余面板类似）：
    - `test_model_panel_handle_key_esc`: 构造 ModelPanel 实例，调用 `handle_key(Input { key: Key::Esc, .. }, &mut ctx)` → 预期返回 `EventResult::ClosePanel`
    - `test_model_panel_handle_key_up_down`: 调用 `handle_key(Up)` → 验证 `cursor` 变化；调用 `handle_key(Down)` → 验证 `cursor` 变化
    - `test_model_panel_handle_key_enter_opus`: 设置 `cursor = ROW_OPUS`，调用 `handle_key(Enter)` → 预期返回 `EventResult::ClosePanel`，`active_tab == AliasTab::Opus`
    - `test_model_panel_handle_key_enter_effort`: 设置 `cursor = ROW_EFFORT`，调用 `handle_key(Enter)` → 预期返回 `EventResult::Consumed`（不关闭），`buf_thinking_effort` 发生变化
    - `test_model_panel_handle_key_left_right`: 设置 `cursor = ROW_EFFORT`，调用 `handle_key(Left)` → 验证 `cycle_effort(true)` 被调用
    - `test_agent_panel_handle_key_enter`: 构造带 agents 的 AgentPanel，`cursor = 1`，调用 `handle_key(Enter)` → 预期返回 `EventResult::ClosePanel`，验证 ctx 中 view_messages 包含 "Agent 已切换为"
    - `test_hooks_panel_handle_key_esc`: 调用 `handle_key(Esc)` → 预期返回 `EventResult::ClosePanel`
    - `test_status_panel_handle_key_left`: 调用 `handle_key(Left)` → 预期返回 `EventResult::Consumed`，`tab` 切换
    - `test_memory_panel_handle_key_enter`: 调用 `handle_key(Enter)` → 预期返回 `EventResult::OpenPanel(PanelKind::Memory)`
  - PanelContext 构造：测试中构造最小 PanelContext（大部分字段可用 default/dummy 值，仅验证 handle_key 返回值和面板自身状态变化）
  - 运行命令: `cargo test -p peri-tui --lib -- "panel::tests::test_.*_handle_key" 2>&1`
  - 预期: 所有测试通过

**检查步骤:**

- [x] 验证 5 个面板文件都包含 `impl PanelComponent`
  - `grep -l "impl PanelComponent for" peri-tui/src/app/{model,agent,hooks,status,memory}_panel.rs | wc -l`
  - 预期: 5
- [x] 验证 ModelPanel 无 unwrap 调用
  - `grep "unwrap()" peri-tui/src/app/model_panel.rs`
  - 预期: 仅在 `#[cfg(test)]` 块中有匹配（已有的测试代码），`impl PanelComponent` 块中无 unwrap
- [x] 验证 event.rs 中 5 个面板的旧分发代码已被注释
  - `grep -n "Task 3.*已迁移" peri-tui/src/event.rs`
  - 预期: 至少 5 行注释标记
- [x] 验证 PanelManager 分发入口存在
  - `grep -n "session_panels.dispatch_key\|global_panels.dispatch_key" peri-tui/src/event.rs`
  - 预期: 至少 2 行匹配
- [x] 验证 panel_ops.rs 中 open/close 方法包含 PanelManager 同步
  - `grep -c "session_panels.open\|session_panels.close\|global_panels.open\|global_panels.close" peri-tui/src/app/panel_ops.rs`
  - 预期: 至少 10 行匹配（5 个面板各 open + close）
- [x] 验证全量编译通过
  - `cargo build -p peri-tui 2>&1 | tail -3`
  - 预期: 输出包含 "Finished" 且无 error
- [x] 验证 handle_key 单元测试通过
  - `cargo test -p peri-tui --lib -- "panel::tests::test_.*_handle_key" 2>&1 | tail -5`
  - 预期: 输出包含 "test result: ok" 且无失败

---

### Acceptance: 局部验收（Task 1-3）

**前置条件:** Task 1-3 全部执行完成。

- [x] 运行全量测试套件：`cargo test -p peri-tui 2>&1 | tail -20`，预期所有测试通过
- [x] 验证 `panel_manager.rs` 和 `panel_component.rs` 存在且编译通过：`grep -c "pub struct PanelManager\|pub trait PanelComponent" peri-tui/src/app/panel_manager.rs peri-tui/src/app/panel_component.rs`
- [x] 验证 `AppCore` 包含 `session_panels` 字段：`grep "session_panels: PanelManager" peri-tui/src/app/core.rs`
- [x] 验证 `App` 包含 `global_panels` 字段：`grep "global_panels: PanelManager" peri-tui/src/app/mod.rs`
- [x] 验证 5 个简单面板已实现 PanelComponent：`grep -rl "impl PanelComponent for" peri-tui/src/app/model_panel.rs peri-tui/src/app/agent_panel.rs peri-tui/src/app/hooks_panel.rs peri-tui/src/app/status_panel.rs peri-tui/src/app/memory_panel.rs | wc -l`，预期 5
- [x] 验证 event.rs 中 5 个简单面板的旧分发已注释或迁移：`grep -c "Task 3.*已迁移\|PanelManager::dispatch" peri-tui/src/event.rs`
- [x] 验证 panel_ops.rs 双写同步存在：`grep -c "session_panels.open\|global_panels.open" peri-tui/src/app/panel_ops.rs`
- [x] 验证 clippy 无警告：`cargo clippy -p peri-tui 2>&1 | grep -E "warning|error" | grep -v "generated" | head -5`
- [x] 验证编译：`cargo build -p peri-tui 2>&1 | tail -3`

**失败排查:**

- 编译失败 → 检查 Task 1 的类型定义是否完整（PanelKind 11 变体、PanelState 11 变体）
- panel_ops.rs 双写不一致 → 检查 Task 2 的 open_* 方法是否全部包含 PanelManager 同步
- event.rs 旧分发与新分发冲突 → 检查 Task 3 的 PanelManager 分发入口是否覆盖了注释掉的旧分支
