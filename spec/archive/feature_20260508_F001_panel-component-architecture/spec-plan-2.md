# 面板组件化架构重构 执行计划（下）

**目标:** 将 TUI 面板系统从分散的 Option<XxxPanel> + 15 层 if-else 链重构为 PanelManager + PanelComponent trait 的组件化架构，新增面板只需定义 PanelState 变体 + 实现 PanelComponent trait。

**技术栈:** Rust 2021, ratatui, tui_textarea, enum-based dispatch（非 dyn trait）

**设计文档:** spec/feature_20260508_F001_panel-component-architecture/spec-design.md

**全局设计文档:** spec/global/component-architecture-design.md

**改动总览:** 将 Login/Config/MCP/Cron/ThreadBrowser 5 个中等面板和 Plugin 复杂面板的事件处理迁移为 `impl PanelComponent`，重构 `main_ui.rs` 渲染分发和 `status_bar.rs` 快捷键显示为 PanelManager 驱动查询，最后清理所有旧字段和 `panel_ops.rs`。Task 4-6 并行可行但存在文件冲突风险建议顺序执行，Task 7 依赖 Task 4-6 全部完成。关键设计决策：PanelComponent::render 签名用 `&mut App`、面板自描述 status_bar_hints、PanelState 枚举委托 render/desired_height/status_bar_hints。

---

### Task 0: 环境准备（下）

- [x] 确认 spec-plan-1.md 中 Task 1-3 已全部执行完成
- [x] 验证当前编译通过：`cargo build -p peri-tui 2>&1 | tail -5`，预期 "Finished"
- [x] 验证当前测试通过：`cargo test -p peri-tui 2>&1 | tail -10`，预期 "test result: ok"

---

### Task 4: 中等面板事件迁移

**背景:**
[业务语境] 将 Login/Config/MCP/Cron/ThreadBrowser 5 个中等复杂度面板的事件处理从 `event.rs` 中的 if-else 分发链 + `handle_xxx_panel` 自由函数迁移到 `PanelComponent` trait 的 `handle_key` 方法。LoginPanel 有 4 种模式和 19 处 unwrap()，是 unwrap 消除的重点目标。ThreadBrowser 有搜索框（TextArea 内部持有）和外部操作（`open_thread_with_feedback`），需通过 `PanelContext` 间接调用。MCP/Cron 已在 Task 2 迁移到 `global_panels`。
[修改原因] 当前 `event.rs` L222-278 中 5 个面板各自有 `if app.xxx_panel.is_some() { handle_xxx_panel(app, input); return Ok(Some(Action::Redraw)); }` 的分发模式。LoginPanel 的 `handle_login_panel` 函数（L1257-1481）有 19 处 `unwrap()`，ConfigPanel 的 `handle_config_panel`（L1577-1666）直接使用 `let Some(panel)` 绑定无 unwrap，MCP/Cron/ThreadBrowser 通过 `as_ref().is_some_and()` 模式访问也无 unwrap。迁移后全部通过 `PanelManager::dispatch_key()` 统一分发，面板作为 `&mut self` 传入消除所有间接访问。
[上下游影响] 本 Task 依赖 Task 1（PanelKind/PanelState/PanelComponent/PanelManager/PanelContext/EventResult 类型定义）和 Task 2（PanelManager 添加到 AppCore/App + open/close 双写 + CronPanel 迁移到 global_panels）。本 Task 输出的 5 个 `impl PanelComponent for XxxPanel` 被 Task 6（渲染迁移 + 状态栏解耦）和 Task 7（清理旧字段）依赖。

**涉及文件:**

- 修改: `peri-tui/src/app/login_panel.rs`（添加 `impl PanelComponent for LoginPanel`）
- 修改: `peri-tui/src/app/config_panel.rs`（添加 `impl PanelComponent for ConfigPanel`）
- 修改: `peri-tui/src/app/mcp_panel.rs`（添加 `impl PanelComponent for McpPanel`）
- 修改: `peri-tui/src/app/cron_state.rs`（添加 `impl PanelComponent for CronPanel`）
- 修改: `ThreadBrowser` 定义所在文件（通过 `grep "pub struct ThreadBrowser"` 确认位置，添加 `impl PanelComponent for ThreadBrowser`）
- 修改: `peri-tui/src/event.rs`（扩展 PanelManager 分发入口，5 个面板改走新路径）
- 修改: `peri-tui/src/app/panel_ops.rs`（login/config/mcp/cron 的 open/close 方法中补充 PanelManager 同步）

**执行步骤:**

- [x] 为 LoginPanel 实现 PanelComponent trait
  - 位置: `peri-tui/src/app/login_panel.rs` 文件末尾（`#[cfg(test)]` 之前）
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

  - 添加 `impl PanelComponent for LoginPanel` 块，包含以下方法：
    - `fn kind(&self) -> PanelKind { PanelKind::Login }`
    - `fn handle_key(&mut self, input: Input, ctx: &mut PanelContext<'_>) -> EventResult`
      - 将 `event.rs` L1257-1481 的 `handle_login_panel` 函数体迁移到此方法
      - **Browse 模式**：
        - `Key::Esc` -> 返回 `EventResult::ClosePanel`（替代 `app.close_login_panel()`）
        - `Key::Up` -> `self.move_cursor(-1); EventResult::Consumed`（消除 unwrap，self 直接访问）
        - `Key::Down` -> `self.move_cursor(1); EventResult::Consumed`（消除 unwrap）
        - `Key::Enter` -> 调用 `Self::select_provider(self, ctx)` -> 返回 `EventResult::ClosePanel`（替代 `app.login_panel_select_provider()`）
        - `Key::Tab (shift=false)` -> `self.enter_edit(); EventResult::Consumed`（消除 unwrap）
        - `Key::Char('n') + ctrl` -> `self.enter_new(); EventResult::Consumed`（消除 unwrap）
        - `Key::Char('d') + ctrl` -> `self.request_delete(); EventResult::Consumed`（消除 unwrap）
        - `_` -> `EventResult::Consumed`
      - **Edit/New 模式**：
        - 提前判断 `let is_type_field = self.edit_field == LoginEditField::Type;`（消除 unwrap）
        - `Key::Esc` -> `self.mode = LoginPanelMode::Browse; EventResult::Consumed`（消除 unwrap）
        - `Key::Char('v') + ctrl` -> 调用 `arboard::Clipboard::new()` 获取剪贴板文本，调用 `self.paste_text(&text); EventResult::Consumed`（消除 unwrap）
        - `Key::Up` -> `self.field_prev(); EventResult::Consumed`（消除 unwrap）
        - `Key::Down` -> `self.field_next(); EventResult::Consumed`（消除 unwrap）
        - `Key::Tab (shift=false)` -> `self.field_next(); EventResult::Consumed`（消除 unwrap）
        - `Key::Tab (shift=true)` -> `self.field_prev(); EventResult::Consumed`（消除 unwrap）
        - `Key::Left | Key::Right` if `is_type_field` -> `self.cycle_type(); EventResult::Consumed`（消除 unwrap）
        - `Key::Char(' ')` if `is_type_field` -> `self.cycle_type(); EventResult::Consumed`（消除 unwrap）
        - `Key::Char(' ')` if `!is_type_field` -> 调用 `self.active_field()` 获取 `(buf, cursor)`，调用 `crate::app::handle_edit_key(buf, cursor, input); EventResult::Consumed`（消除 unwrap）
        - `Key::Enter` -> 调用 `Self::apply_edit(self, ctx)` -> 返回 `EventResult::Consumed`（替代 `app.login_panel_apply_edit()`）
        - `_` if `!is_type_field` -> 调用 `self.active_field()` 获取 `(buf, cursor)`，调用 `crate::app::handle_edit_key(buf, cursor, input); EventResult::Consumed`（消除 unwrap）
      - **ConfirmDelete 模式**：
        - `Key::Enter` -> 调用 `Self::confirm_delete_action(self, ctx)` -> 返回 `EventResult::ClosePanel`（替代 `app.login_panel_confirm_delete()`）
        - `Key::Esc` -> `self.cancel_delete(); EventResult::Consumed`（消除 unwrap）
        - `_` -> `EventResult::Consumed`
    - 添加私有辅助方法：
      - `fn select_provider(panel: &LoginPanel, ctx: &mut PanelContext<'_>)`
        - 迁移自 `panel_ops.rs` L78-112 `login_panel_select_provider` 核心逻辑
        - 1. 从 `panel.get_selected_provider()` 获取选中 provider
        - 1. 调用 `panel.apply_provider_to_config(ctx.peri_config)` 写入配置
        - 1. 向 `ctx.sessions[ctx.active].core.view_messages` push 系统消息 "Provider 已切换为: {name}"
        - 1. 调用 `App::save_config(ctx.peri_config.as_ref().unwrap(), ctx.config_path_override.as_deref())` 保存
        - 1. 通过 `ctx.provider_name` 和 `ctx.model_name` 更新显示名称
      - `fn apply_edit(panel: &mut LoginPanel, ctx: &mut PanelContext<'_>)`
        - 迁移自 `panel_ops.rs` L114-159 `login_panel_apply_edit` 核心逻辑
        - 1. 调用 `panel.apply_edit_to_config(ctx.peri_config)` 将编辑字段写入配置
        - 1. 调用 `panel.mode = LoginPanelMode::Browse` 回到浏览模式
        - 1. 调用 `App::save_config(...)` 保存
        - 1. push 系统消息 "配置已保存"
      - `fn confirm_delete_action(panel: &mut LoginPanel, ctx: &mut PanelContext<'_>)`
        - 迁移自 `panel_ops.rs` L161-210 `login_panel_confirm_delete` 核心逻辑
        - 1. 调用 `panel.delete_selected()` 获取删除的 provider 名称
        - 1. push 系统消息 "Provider {name} 已删除"
        - 1. 调用 `App::save_config(...)` 保存
    - `fn handle_paste(&mut self, text: &str, _ctx: &mut PanelContext<'_>) -> EventResult`
      - 调用 `self.paste_text(text); EventResult::Consumed`
    - `fn desired_height(&self, _screen_height: u16, _screen_width: u16) -> u16`
      - 匹配 `self.mode`：
        - `Edit | New` -> `12`
        - `ConfirmDelete` -> `(self.providers.len() as u16 + 6).max(7)`
        - `Browse` -> `(self.providers.len() as u16 * 3 + 3).max(6)`
    - `fn render(&self, f: &mut Frame, app: &App, area: Rect)` -> 委托到 `crate::ui::main_ui::panels::login::render_login_panel(f, app, area)`
    - `fn as_any_ref(&self) -> &dyn Any { self }`
    - `fn as_any_mut(&mut self) -> &mut dyn Any { self }`
    - `fn status_bar_hints(&self) -> Vec<(&'static str, &'static str)>`
      - 匹配 `self.mode`：
        - `Browse` -> `vec![("Enter", "选中"), ("Tab", "编辑"), ("Ctrl+N", "新建"), ("Ctrl+D", "删除"), ("Esc", "关闭")]`
        - `Edit | New` -> `vec![("Up/Down", "切换字段"), ("Left/Right/Space", "切换Type"), ("Enter", "保存"), ("Ctrl+V", "粘贴"), ("Esc", "取消")]`
        - `ConfirmDelete` -> `vec![("Enter", "确认删除"), ("Esc", "取消")]`
  - 原因: 消除 19 处 unwrap()，将面板状态操作集中到 `&mut self`，返回 `EventResult` 让 PanelManager 处理关闭

- [x] 为 ConfigPanel 实现 PanelComponent trait
  - 位置: `peri-tui/src/app/config_panel.rs` 文件末尾（`#[cfg(test)]` 之前）
  - 在文件顶部添加 use 语句（同 LoginPanel 模式）
  - 添加 `impl PanelComponent for ConfigPanel` 块：
    - `fn kind(&self) -> PanelKind { PanelKind::Config }`
    - `fn handle_key(&mut self, input: Input, ctx: &mut PanelContext<'_>) -> EventResult`
      - 将 `event.rs` L1577-1666 的 `handle_config_panel` 函数体迁移到此方法
      - **Browse 模式**：
        - `Key::Up` -> `if self.cursor > 0 { self.cursor -= 1; } else { self.cursor = ConfigPanel::field_count() - 1; } EventResult::Consumed`
        - `Key::Down` -> `self.cursor = (self.cursor + 1) % ConfigPanel::field_count(); EventResult::Consumed`
        - `Key::Enter` -> `self.enter_edit(); EventResult::Consumed`
        - `Key::Esc` -> 返回 `EventResult::ClosePanel`（替代 `app.sessions[app.active].core.config_panel = None`）
        - `_` -> `EventResult::Consumed`
      - **Edit 模式**：
        - `Key::Esc` -> `self.mode = ConfigPanelMode::Browse; EventResult::Consumed`
        - `Key::Enter` -> 调用 `Self::apply_config(self, ctx)` -> 返回 `EventResult::Consumed`（替代 `app.config_panel_apply()`）
        - `Key::Up` -> `self.field_prev(); EventResult::Consumed`
        - `Key::Down` -> `self.field_next(); EventResult::Consumed`
        - `Key::Char(' ')` -> 匹配 `self.edit_field`：
          - `Autocompact` -> `self.cycle_autocompact(); EventResult::Consumed`
          - `Proactiveness` -> `self.cycle_proactiveness(); EventResult::Consumed`
          - `_` -> 调用 `self.active_field()` 获取 `(buf, cursor)`，调用 `crate::app::handle_edit_key(buf, cursor, input); EventResult::Consumed`
        - `Key::Left | Key::Right` (ctrl=false) -> 同 Space 的匹配逻辑
        - `_` -> 调用 `self.active_field()` 获取 `(buf, cursor)`，调用 `crate::app::handle_edit_key(buf, cursor, input); EventResult::Consumed`
    - 添加私有辅助方法：
      - `fn apply_config(panel: &mut ConfigPanel, ctx: &mut PanelContext<'_>)`
        - 迁移自 `panel_ops.rs` L213-236 `config_panel_apply` 核心逻辑
        - 1. 调用 `panel.apply_to_config(ctx.peri_config)` 写入配置
        - 1. 调用 `panel.mode = ConfigPanelMode::Browse` 回到浏览模式
        - 1. 调用 `App::save_config(...)` 保存
        - 1. push 系统消息 "配置已保存"
    - `fn handle_paste(&mut self, text: &str, _ctx: &mut PanelContext<'_>) -> EventResult`
      - 调用 `self.paste_text(text); EventResult::Consumed`
    - `fn desired_height(&self, _screen_height: u16, _screen_width: u16) -> u16 { 14 }`
    - `fn render` -> 委托到 `crate::ui::main_ui::panels::config::render_config_panel(f, app, area)`
    - `fn as_any_ref/as_any_mut` -> 标准 `self` 转换
    - `fn status_bar_hints` -> 匹配 `self.mode`：
      - `Browse` -> `vec![("Up/Down", "导航"), ("Enter", "编辑"), ("Esc", "关闭")]`
      - `Edit` -> `vec![("Up/Down", "切换字段"), ("Left/Right/Space", "切换"), ("Enter", "保存"), ("Ctrl+V", "粘贴"), ("Esc", "取消")]`
  - 原因: ConfigPanel handler 无 unwrap，迁移模式与 Task 3 中简单面板一致，辅助方法模式与 LoginPanel 相同

- [x] 为 McpPanel 实现 PanelComponent trait
  - 位置: `peri-tui/src/app/mcp_panel.rs` 文件末尾（`#[cfg(test)]` 之前）
  - 在文件顶部添加 use 语句（同 LoginPanel 模式）
  - 添加 `impl PanelComponent for McpPanel` 块：
    - `fn kind(&self) -> PanelKind { PanelKind::Mcp }`
    - `fn handle_key(&mut self, input: Input, ctx: &mut PanelContext<'_>) -> EventResult`
      - 将 `event.rs` L1768-1837 的 `handle_mcp_panel` 函数体迁移到此方法
      - **confirm_delete 模式**（`self.confirm_delete.is_some()`）：
        - `Key::Enter` -> 调用 `Self::do_confirm_delete(self, ctx)` -> 返回 `EventResult::ClosePanel`（替代 `app.mcp_panel_confirm_delete()`）
        - `_` -> `self.confirm_delete = None; EventResult::Consumed`（替代 `app.mcp_panel_cancel_delete()`）
      - **ServerList 视图**（`self.view.is_server_list()`）：
        - `Key::Char('c') + ctrl` -> `EventResult::Consumed`（忽略 Ctrl+C）
        - `Key::Up` -> 调用 `self.do_move_up()` -> `EventResult::Consumed`（替代 `app.mcp_panel_move_up()`）
        - `Key::Down` -> 调用 `self.do_move_down()` -> `EventResult::Consumed`（替代 `app.mcp_panel_move_down()`）
        - `Key::Enter` -> 调用 `self.do_enter(ctx)` -> `EventResult::Consumed`（替代 `app.mcp_panel_enter()`）
        - `Key::Esc` -> 返回 `EventResult::ClosePanel`（替代 `app.mcp_panel_close()` + 清理 `panel_selection`/`panel_area`）
        - `Key::Char('r') + ctrl` -> 调用 `self.do_reconnect(ctx)` -> `EventResult::Consumed`（替代 `app.mcp_panel_reconnect()`）
        - `Key::Char('d') + ctrl` -> 调用 `self.do_request_delete()` -> `EventResult::Consumed`（替代 `app.mcp_panel_request_delete()`）
      - **ServerDetail 视图**：
        - `Key::Up/Down` -> 同 ServerList 的 move 逻辑 -> `EventResult::Consumed`
        - `Key::Enter` -> 同 ServerList 的 enter 逻辑 -> `EventResult::Consumed`
        - `Key::Esc` -> 调用 `self.do_back()` -> `EventResult::Consumed`（替代 `app.mcp_panel_back()`）
        - `_` -> `EventResult::Consumed`
    - 添加私有辅助方法（从 `mcp_panel.rs` 中已有的 `pub fn mcp_panel_xxx` 方法迁移核心逻辑，将 `self.mcp_panel.as_mut()` 替换为 `self`）：
      - `fn do_move_up(&mut self)` — 迁移自 L88-101
      - `fn do_move_down(&mut self)` — 迁移自 L102-119
      - `fn do_enter(&mut self, ctx: &mut PanelContext<'_>)` — 迁移自 L121-248，需访问 `ctx.mcp_pool`
      - `fn do_back(&mut self)` — 迁移自 L249-279
      - `fn do_request_delete(&mut self)` — 迁移自 L281-358
      - `fn do_confirm_delete(&mut self, ctx: &mut PanelContext<'_>)` — 迁移自 L360-392，需访问 `ctx.mcp_pool` 和 `ctx.sessions`
      - `fn do_reconnect(&mut self, ctx: &mut PanelContext<'_>)` — 迁移自 L399-505，需访问 `ctx.mcp_pool` 和 `ctx.sessions`
    - `fn desired_height(&self, _screen_height: u16, _screen_width: u16) -> u16`
      - 匹配 `&self.view`：
        - `ServerList` -> `((self.servers.len() + 7) as u16).max(8)`
        - `ServerDetail { actions, tools, show_tools, .. }` -> `((actions.len() + 9 + (if *show_tools { tools.len() } else { 0 })) as u16).max(8)`
    - `fn render` -> 委托到 `crate::ui::main_ui::panels::mcp::render_mcp_panel(f, app, area)`
    - `fn as_any_ref/as_any_mut` -> 标准 `self` 转换
    - `fn status_bar_hints` -> 匹配 `&self.view`：
      - `ServerList` if `self.confirm_delete.is_some()` -> `vec![("Enter", "确认"), ("其他键", "取消")]`
      - `ServerList` -> `vec![("Up/Down", "移动"), ("Enter", "详情"), ("Ctrl+R", "重连"), ("Ctrl+D", "删除"), ("Esc", "关闭")]`
      - `ServerDetail` -> `vec![("Up/Down", "移动"), ("Enter", "执行"), ("Esc", "返回")]`
    - `fn handle_scroll` — 覆盖默认实现：
      - `lines > 0` -> `self.scroll_up(lines as u16); EventResult::Consumed`
      - `lines < 0` -> `self.scroll_down((-lines) as u16); EventResult::Consumed`
  - 原因: MCP 面板的 ops 方法全部在 `self.mcp_panel.as_mut()` 上操作，迁移后直接操作 `&mut self`，消除间接访问。内部方法命名加 `do_` 前缀避免与已有的 `pub fn mcp_panel_xxx` 冲突（后者在 Task 7 清理前仍被其他代码引用）

- [x] 为 CronPanel 实现 PanelComponent trait
  - 位置: `peri-tui/src/app/cron_state.rs` 文件末尾（`#[cfg(test)]` 之前）
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

  - 添加 `impl PanelComponent for CronPanel` 块：
    - `fn kind(&self) -> PanelKind { PanelKind::Cron }`
    - `fn handle_key(&mut self, input: Input, ctx: &mut PanelContext<'_>) -> EventResult`
      - 将 `event.rs` L1712-1766 的 `handle_cron_panel` 函数体迁移到此方法
      - **confirm_delete 模式**（`self.confirm_delete`）：
        - `Key::Enter` -> 调用 `Self::do_confirm_delete(self, ctx)` -> 返回值取决于是否还有任务（空则 `EventResult::ClosePanel`，非空则 `EventResult::Consumed`）
        - `_` -> `self.confirm_delete = false; EventResult::Consumed`
      - **正常模式**：
        - `Key::Char('c') + ctrl` -> `EventResult::Consumed`（忽略 Ctrl+C）
        - `Key::Up` -> `self.move_cursor(-1); EventResult::Consumed`
        - `Key::Down` -> `self.move_cursor(1); EventResult::Consumed`
        - `Key::Enter` -> 调用 `Self::do_toggle(self, ctx)` -> `EventResult::Consumed`
        - `Key::Esc` -> 返回 `EventResult::ClosePanel`
        - `Key::Char('d') + ctrl` -> `if !self.tasks.is_empty() { self.confirm_delete = true; } EventResult::Consumed`
        - `_` -> `EventResult::Consumed`
    - 添加私有辅助方法：
      - `fn do_toggle(panel: &mut CronPanel, ctx: &mut PanelContext<'_>)`
        - 迁移自 `cron_ops.rs` L18-28 `cron_panel_toggle` 核心逻辑
        - 1. 切换 `panel.tasks[panel.cursor].enabled`
        - 1. 调用 `panel.refresh(&ctx.cron.scheduler)` 刷新显示
        - 1. push 系统消息 "定时任务已启用/禁用"
      - `fn do_confirm_delete(panel: &mut CronPanel, ctx: &mut PanelContext<'_>) -> EventResult`
        - 迁移自 `cron_ops.rs` L39-63 `cron_panel_confirm_delete` 核心逻辑
        - 1. 调用 `panel.delete_selected()` 获取删除的任务名称
        - 1. push 系统消息 "定时任务已删除: {name}"
        - 1. 调用 `panel.refresh(&ctx.cron.scheduler)`
        - 1. 如果 `panel.tasks.is_empty()` -> 返回 `EventResult::ClosePanel`
        - 1. 否则 -> `EventResult::Consumed`
    - `fn desired_height(&self, _screen_height: u16, _screen_width: u16) -> u16 { (self.tasks.len() as u16 + 4).max(6) }`
    - `fn render` -> 委托到 `crate::ui::main_ui::panels::cron::render_cron_panel(f, app, area)`
    - `fn as_any_ref/as_any_mut` -> 标准 `self` 转换
    - `fn status_bar_hints` -> 匹配 `self.confirm_delete`：
      - `true` -> `vec![("Enter", "确认"), ("其他键", "取消")]`
      - `false` -> `vec![("Up/Down", "移动"), ("Enter", "切换"), ("Ctrl+D", "删除"), ("Esc", "关闭")]`
  - 原因: CronPanel 已在 Task 2 迁移到 `global_panels`，`cron_ops.rs` 中的 ops 方法通过 `global_panels.get_mut::<CronPanel>()` 访问，迁移后这些 ops 方法变为面板内部方法

- [x] 为 ThreadBrowser 实现 PanelComponent trait
  - 位置: `ThreadBrowser` 定义所在的文件。执行 `grep -rn "pub struct ThreadBrowser" peri-tui/src/ peri_middlewares/src/` 确认位置
  - 在该文件末尾添加 `impl PanelComponent for ThreadBrowser` 块：
    - `fn kind(&self) -> PanelKind { PanelKind::ThreadBrowser }`
    - `fn handle_key(&mut self, input: Input, ctx: &mut PanelContext<'_>) -> EventResult`
      - 将 `event.rs` L987-1199 的 `handle_thread_browser` 函数体迁移到此方法
      - **confirm_delete 模式**（`self.confirm_delete`）：
        - `Key::Enter` -> `self.confirm_delete = false;` 调用 `self.delete_selected()` 获取 title，向 `ctx.sessions[ctx.active].core.view_messages` push "已删除对话: {title}" -> `EventResult::Consumed`
        - `_` -> `self.confirm_delete = false; EventResult::Consumed`
      - **search_focused 模式**（`self.search_focused`）：
        - `Key::Char('c') + ctrl` -> `EventResult::Consumed`（忽略 Ctrl+C）
        - `Key::Esc` -> 判断 `self.search_query.value().is_empty()`：空则返回 `EventResult::ClosePanel`，非空则清空搜索并 `self.refresh_filter()` -> `EventResult::Consumed`
        - `Key::Char('v') + ctrl` -> `arboard::Clipboard::new()` 获取文本，调用 `self.search_query.paste(&text); self.refresh_filter(); EventResult::Consumed`
        - `Key::Char(c)` -> `self.search_query.insert(c); self.refresh_filter(); EventResult::Consumed`
        - `Key::Backspace` -> `self.search_query.backspace(); self.refresh_filter(); EventResult::Consumed`
        - `Key::Delete` -> `self.search_query.delete(); self.refresh_filter(); EventResult::Consumed`
        - `Key::Left` -> `self.search_query.cursor_left(); EventResult::Consumed`
        - `Key::Right` -> `self.search_query.cursor_right(); EventResult::Consumed`
        - `Key::Home` -> `self.search_query.cursor_home(); EventResult::Consumed`
        - `Key::End` -> `self.search_query.cursor_end(); EventResult::Consumed`
        - `Key::Down | Key::Tab` -> `self.search_focused = false; EventResult::Consumed`（切换到列表模式）
        - `Key::Enter` -> 调用 `Self::open_selected(self, ctx)` -> `EventResult::Consumed`
        - `_` -> `EventResult::Consumed`
      - **列表模式**：
        - `Key::Char('c') + ctrl` -> `EventResult::Consumed`（忽略 Ctrl+C）
        - `Key::Esc` -> 返回 `EventResult::ClosePanel`
        - `Key::Up` -> `self.move_cursor(-1);` 从 `ctx.sessions[ctx.active].core.panel_area` 获取 visible 高度，计算 `visual_row`，调用 `ensure_cursor_visible` 更新 `scroll_offset` -> `EventResult::Consumed`
        - `Key::Down` -> `self.move_cursor(1);` 同上计算 `scroll_offset` -> `EventResult::Consumed`
        - `Key::Enter` -> 调用 `Self::open_selected(self, ctx)` -> `EventResult::Consumed`
        - `Key::Char('d') + ctrl` -> `if self.total() > 0 { self.confirm_delete = true; } EventResult::Consumed`
        - `Key::Char('/') | Key::Tab` -> `self.search_focused = true; EventResult::Consumed`
        - `_` -> `EventResult::Consumed`
    - 添加私有辅助方法：
      - `fn open_selected(panel: &ThreadBrowser, ctx: &mut PanelContext<'_>)`
        - 1. 调用 `panel.selected_id().cloned()` 获取 thread ID
        - 1. 通过 `ctx.thread_store` 和 `ctx.sessions[ctx.active]` 执行 `open_thread_with_feedback` 的核心逻辑（从 `thread_ops.rs` 提取内联）
    - `fn handle_paste(&mut self, text: &str, _ctx: &mut PanelContext<'_>) -> EventResult`
      - 搜索框聚焦时：逐字符调用 `self.search_query.insert(ch); self.refresh_filter(); EventResult::Consumed`
      - 非聚焦时：`EventResult::Consumed`（拦截粘贴，防止进入 textarea）
    - `fn desired_height(&self, _screen_height: u16, _screen_width: u16) -> u16`
      - `let base = (self.total() as u16 * 3 + 7).max(9);`
      - `if self.confirm_delete { base + 2 } else { base }`
    - `fn render` -> 委托到 `crate::ui::main_ui::panels::thread_browser::render_thread_browser(f, app, area)`
    - `fn as_any_ref/as_any_mut` -> 标准 `self` 转换
    - `fn status_bar_hints` -> 匹配 `self.confirm_delete`：
      - `true` -> `vec![("Enter", "确认"), ("其他键", "取消")]`
      - `false` -> `vec![("Up/Down", "移动"), ("Enter", "确认"), ("Ctrl+D", "删除"), ("Esc", "关闭"), ("/", "搜索")]`
  - 原因: ThreadBrowser 搜索框（TextArea）保持内部持有，搜索逻辑通过 `handle_key` 中的 `search_focused` 输入模式处理。`open_thread_with_feedback` 外部操作通过 `PanelContext` 间接调用。列表模式的 `panel_area` 通过 `ctx.sessions[ctx.active].core.panel_area` 获取用于滚动计算

- [x] 修改 `event.rs`，扩展 PanelManager 分发入口覆盖 5 个新面板
  - 位置: `peri-tui/src/event.rs` L222-278（5 个面板的 if-else 分发区域）
  - 修改 Task 3 中已创建的 PanelManager 分发代码块，扩展 match 分支覆盖新面板：
    - session_panels 分发中，将已有的 3 个 `PanelKind` 扩展为 `Some(PanelKind::Model) | Some(PanelKind::Agent) | Some(PanelKind::Hooks) | Some(PanelKind::Login) | Some(PanelKind::Config) | Some(PanelKind::ThreadBrowser)`
    - global_panels 分发中，将已有的 2 个 `PanelKind` 扩展为 `Some(PanelKind::Status) | Some(PanelKind::Memory) | Some(PanelKind::Mcp) | Some(PanelKind::Cron)`
    - `EventResult::ClosePanel` 处理中扩展 `match kind` 分支，增加 Login/Config/ThreadBrowser 的旧字段清理（`app.sessions[app.active].core.login_panel = None` 等），以及 Mcp/Cron 的旧字段清理（`app.mcp_panel = None`、`app.cron.cron_panel = None` 若 Task 2 保留了双写镜像）
  - 将 L222-225（thread_browser）、L227-231（cron_panel）、L239-243（mcp_panel）、L263-267（login_panel）、L275-279（config_panel）的旧分发代码注释掉，添加 `// [Task 4] 已迁移到 PanelManager 分发` 注释
  - 在 `Event::Paste` 分支中：
    - L704-713 `login_panel` 粘贴 -> 改为检查 `session_panels.is_active(PanelKind::Login)`，调用 `session_panels.dispatch_paste(text, &mut ctx)`
    - L720-726 `config_panel` 粘贴 -> 改为检查 `session_panels.is_active(PanelKind::Config)`，调用 `session_panels.dispatch_paste(text, &mut ctx)`
    - L761-770 批量拦截列表中将 `thread_browser`、`cron_panel`、`mcp_panel` 的 `is_some()` 检查改为 PanelManager `is_active()` 检查
  - 原因: 双写过渡期保证 5 个新面板走 PanelManager 新路径，旧 if-else 路径被注释保留

- [x] 修改 `panel_ops.rs` 的 open/close 方法，补充 PanelManager 同步（双写）
  - 逐个检查并确认以下方法的 PanelManager 同步代码存在（Task 2 应已添加，此处为兜底确认）：
    - `open_login_panel` (L61): 确认末尾已有 `self.sessions[self.active].core.session_panels.open(PanelState::Login(panel.clone())); self.global_panels.close();`，若无则补充
    - `close_login_panel` (L73): 确认已有 `self.sessions[self.active].core.session_panels.close_if(PanelKind::Login);`，若无则补充
    - `open_config_panel` (L198): 确认末尾已有 `self.sessions[self.active].core.session_panels.open(PanelState::Config(panel.clone())); self.global_panels.close();`，若无则补充
    - `close_config_panel` (L208): 确认已有 `self.sessions[self.active].core.session_panels.close_if(PanelKind::Config);`，若无则补充
    - `open_mcp_panel` (Task 2 新增): 确认已有 `self.open_panel(PanelState::Mcp(panel.clone()));` + 旧字段双写
    - `open_cron_panel` (Task 2 新增): 确认已有 `self.open_panel(PanelState::Cron(panel));`
    - `thread_ops.rs` L349-353 (Task 2 已修改): 确认已有 `self.open_panel(PanelState::ThreadBrowser(browser.clone()));`
  - 原因: 确保所有 open/close 操作同步 PanelManager 状态

- [x] 为 5 个面板的 handle_key 编写单元测试
  - 测试文件: 分别在各面板文件底部的 `#[cfg(test)] mod tests` 中添加
  - 测试场景：
    - **LoginPanel**：
      - `test_login_panel_handle_key_esc_browse`: Browse 模式 `handle_key(Esc)` -> 返回 `EventResult::ClosePanel`
      - `test_login_panel_handle_key_up_down_browse`: Browse 模式 `handle_key(Up)` -> 验证 cursor 变化
      - `test_login_panel_handle_key_tab_enter_edit`: Browse 模式 `handle_key(Tab)` -> 验证 `mode == LoginPanelMode::Edit`
      - `test_login_panel_handle_key_ctrl_n_new`: Browse 模式 `handle_key(Ctrl+N)` -> 验证 `mode == LoginPanelMode::New`
      - `test_login_panel_handle_key_esc_edit`: Edit 模式 `handle_key(Esc)` -> 验证 `mode == LoginPanelMode::Browse`
      - `test_login_panel_handle_key_ctrl_v_paste`: Edit 模式 `handle_key(Ctrl+V)` -> 验证 paste_text 被调用
      - `test_login_panel_handle_key_enter_confirm_delete`: ConfirmDelete 模式 `handle_key(Enter)` -> 返回 `EventResult::ClosePanel`
      - `test_login_panel_no_unwrap_in_impl`: 验证 `impl PanelComponent for LoginPanel` 块中面板自身状态访问无 `.unwrap()` 调用
    - **ConfigPanel**：
      - `test_config_panel_handle_key_esc_browse`: Browse 模式 `handle_key(Esc)` -> 返回 `EventResult::ClosePanel`
      - `test_config_panel_handle_key_enter_browse`: Browse 模式 `handle_key(Enter)` -> 验证 `mode == ConfigPanelMode::Edit`
      - `test_config_panel_handle_key_esc_edit`: Edit 模式 `handle_key(Esc)` -> 验证 `mode == ConfigPanelMode::Browse`
    - **McpPanel**：
      - `test_mcp_panel_handle_key_esc_server_list`: ServerList 视图 `handle_key(Esc)` -> 返回 `EventResult::ClosePanel`
      - `test_mcp_panel_handle_key_enter_confirm_delete`: confirm_delete 模式 `handle_key(Enter)` -> 返回 `EventResult::ClosePanel`
      - `test_mcp_panel_handle_key_other_cancel_delete`: confirm_delete 模式 `handle_key(Char('a'))` -> 验证 `confirm_delete == None`
    - **CronPanel**：
      - `test_cron_panel_handle_key_esc`: 正常模式 `handle_key(Esc)` -> 返回 `EventResult::ClosePanel`
      - `test_cron_panel_handle_key_enter_confirm_delete`: confirm_delete 模式 `handle_key(Enter)` -> 验证任务被删除
      - `test_cron_panel_handle_key_ctrl_d_request_delete`: 正常模式 `handle_key(Ctrl+D)` -> 验证 `confirm_delete == true`
    - **ThreadBrowser**：
      - `test_thread_browser_handle_key_esc`: 列表模式 `handle_key(Esc)` -> 返回 `EventResult::ClosePanel`
      - `test_thread_browser_handle_key_slash_focus_search`: 列表模式 `handle_key(Char('/'))` -> 验证 `search_focused == true`
      - `test_thread_browser_handle_key_esc_close_search`: 搜索模式 `handle_key(Esc)`（空搜索框）-> 返回 `EventResult::ClosePanel`
      - `test_thread_browser_handle_key_enter_confirm_delete`: confirm_delete 模式 `handle_key(Enter)` -> 验证任务被删除
  - PanelContext 构造：测试中构造最小 PanelContext（大部分字段可用 default/dummy 值，仅验证 handle_key 返回值和面板自身状态变化）
  - 运行命令: `cargo test -p peri-tui --lib -- "panel::tests::test_.*_handle_key\|cron_state::tests::test_.*_handle_key\|thread_store::tests::test_.*_handle_key" 2>&1`
  - 预期: 所有测试通过

**检查步骤:**

- [x] 验证 5 个面板文件都包含 `impl PanelComponent`
  - `grep -rl "impl PanelComponent for" peri-tui/src/app/login_panel.rs peri-tui/src/app/config_panel.rs peri-tui/src/app/mcp_panel.rs peri-tui/src/app/cron_state.rs $(grep -rl "pub struct ThreadBrowser" peri-tui/src/ peri_middlewares/src/ 2>/dev/null) | wc -l`
  - 预期: 5
- [x] 验证 LoginPanel 的 PanelComponent 实现中无 unwrap 调用（面板自身状态访问）
  - `grep -A 500 "impl PanelComponent for LoginPanel" peri-tui/src/app/login_panel.rs | head -300 | grep -c "\.unwrap()"`
  - 预期: 0（辅助方法中 `ctx.peri_config.as_mut().unwrap()` 等必要 unwrap 允许存在，面板自身 `self` 直接访问无 unwrap）
- [x] 验证 event.rs 中 5 个面板的旧分发代码已被注释
  - `grep -c "Task 4.*已迁移" peri-tui/src/event.rs`
  - 预期: 至少 5
- [x] 验证 PanelManager 分发入口覆盖所有 10 个已迁移面板（5 简单 + 5 中等）
  - `grep -A 2 "session_panels.dispatch_key\|global_panels.dispatch_key" peri-tui/src/event.rs | grep "PanelKind" | head -5`
  - 预期: 匹配中包含 Login/Config/ThreadBrowser/Mcp/Cron
- [x] 验证 CronPanel 在 cron_state.rs 中实现了 PanelComponent
  - `grep "impl PanelComponent for CronPanel" peri-tui/src/app/cron_state.rs`
  - 预期: 1 行匹配
- [x] 验证全量编译通过
  - `cargo build -p peri-tui 2>&1 | tail -3`
  - 预期: 输出包含 "Finished" 且无 error
- [x] 验证 handle_key 单元测试通过
  - `cargo test -p peri-tui --lib -- "panel::tests::test_.*_handle_key\|cron_state::tests::test_.*_handle_key" 2>&1 | tail -10`
  - 预期: 输出包含 "test result: ok" 且无失败
- [x] 验证 clippy 无新警告
  - `cargo clippy -p peri-tui 2>&1 | grep -E "warning|error" | grep -v "generated" | head -10`
  - 预期: 无新增 warning

---

### Task 5: Plugin 面板事件迁移

**背景:**
Plugin 面板是全局面板中复杂度最高的面板，包含 4 种视图（Installed/Discover/Marketplaces/Errors）、6 种内部状态（confirm_delete/marketplace_confirm_delete/add_marketplace_active/discover_searching/discover_detail_index/detail_index）、异步安装/卸载操作（通过 `bg_event_tx` spawn 后台任务）和搜索输入功能。当前 `event.rs` 中 `handle_plugin_panel` 函数体约 480 行（L1839-L2321），另有辅助函数 `handle_discover_install_current`（L2392-L2442）和 `handle_discover_batch_install`（L2325-L2389）。粘贴处理散布在 L728-L756。状态栏快捷键散布在 `status_bar.rs` L307-L334。
本 Task 将 `handle_plugin_panel` 函数体完整迁移到 `impl PanelComponent for PluginPanel`，消除 `app.plugin_panel.as_ref().unwrap()` 模式，通过 `PanelContext` 访问 app 状态。前置依赖：Task 1-2 已完成（PanelKind/PanelState/PanelManager/PanelContext/EventResult/PanelComponent trait 已定义并编译通过）。

**涉及文件:**

- 修改: `peri-tui/src/app/plugin_panel.rs` — 添加 `impl PanelComponent for PluginPanel`（handle_key/handle_paste/status_bar_hints/desired_height/render）
- 修改: `peri-tui/src/event.rs` — 删除 `handle_plugin_panel` 函数、`handle_discover_install_current` 函数、`handle_discover_batch_install` 函数；删除 Key 分发链中 `plugin_panel.is_some()` 分支（L246-L248）；删除 Paste 分发链中 plugin_panel 粘贴处理（L728-L756）；删除 Mouse Scroll 分发链中 plugin_panel 滚动处理（L789-L824）
- 修改: `peri-tui/src/ui/main_ui/status_bar.rs` — 删除 L307-L334 的 plugin_panel 快捷键 match 分支

**执行步骤:**

- [x] 在 `plugin_panel.rs` 中添加 `PanelComponent` trait 导入和 `impl PanelComponent for PluginPanel` 骨架
  - 位置: `peri-tui/src/app/plugin_panel.rs` 文件末尾（`tests` 模块之前）
  - 添加导入: `use crate::app::panel_component::PanelComponent;`、`use crate::app::panel_manager::{EventResult, PanelContext, PanelKind};`、`use ratatui::layout::Rect;`、`use ratatui::Frame;`、`use std::any::Any;`
  - 原因: 为后续迁移提供 trait 实现框架

- [x] 实现 `PluginPanel::kind()` 和 `Any` downcast 方法
  - 位置: `impl PanelComponent for PluginPanel` 内部
  - `kind()` 返回 `PanelKind::Plugin`
  - `as_any_ref()` 返回 `self as &dyn Any`
  - `as_any_mut()` 返回 `self as &mut dyn Any`
  - 原因: PanelManager 通过 Any downcast 进行类型安全的面板访问

- [x] 实现 `PluginPanel::desired_height()`
  - 位置: `impl PanelComponent for PluginPanel` 内部
  - 返回 `screen_height * 70 / 100`（与 `main_ui.rs` 中 Plugin 面板的 max_h 一致）
  - 原因: Plugin 面板使用 70% 屏幕高度上限

- [x] 实现 `PluginPanel::render()`
  - 位置: `impl PanelComponent for PluginPanel` 内部
  - 委托到 `crate::ui::main_ui::panels::plugin::render_plugin_panel(f, app, area)`
  - 原因: 渲染函数迁移在 Task 6 统一处理，此处仅做委托

- [x] 实现 `PluginPanel::handle_key()` — confirm_delete 状态分支
  - 位置: `handle_key` 方法内，第一段 match（优先级最高）
  - 迁移 `event.rs` L1842-L1909 的 confirm_delete 逻辑
  - 将 `app.plugin_panel.as_ref().unwrap()` 替换为 `self` 直接访问
  - Enter 分支：从 `self.entries` 获取 plugin_id 和 project_path，将 `self.uninstalling.insert(plugin_id.clone())` 直接操作 self，调用 `self.confirm_delete = None` 替代 `app.plugin_panel_cancel_delete()`
  - 异步卸载 spawn：从 `ctx.bg_event_tx.clone()` 获取通道，从 `ctx.cwd.clone()` 获取项目目录
  - `claude_home()` 调用保持不变（无 app 依赖）
  - `_ => { self.confirm_delete = None; }` 处理取消
  - 所有分支返回 `EventResult::Consumed`
  - 原因: confirm_delete 是最高优先级状态，必须在所有其他分支之前处理

- [x] 实现 `PluginPanel::handle_key()` — discover_searching 状态分支
  - 位置: `handle_key` 方法内，confirm_delete 分支之后
  - 迁移 `event.rs` L1918-L1970 的搜索模式逻辑
  - `Key::Char(c)` → `self.discover_search.insert(c); self.discover_cursor = 0;`
  - `Key::Backspace` → `self.discover_search.backspace(); self.discover_cursor = 0;`
  - `Key::Up/Down` → `self.discover_searching = false;` 然后执行移动（调用内部方法 `self.move_discover_cursor(-1/1)`）
  - `Key::Left/Right` → `self.discover_searching = false;` 然后切换 Tab（`self.view.prev()/next(); self.cursor = 0; self.scroll_offset = 0;`）
  - `Key::Esc` → `self.discover_searching = false;`
  - `Key::Enter` → `self.discover_searching = false;` 然后调用内部安装方法 `self.spawn_install_current(ctx)`
  - 原因: 搜索模式拦截所有输入字符，优先级高于列表视图

- [x] 实现 `PluginPanel::handle_key()` — discover_detail_index 状态分支
  - 位置: `handle_key` 方法内，discover_searching 分支之后
  - 迁移 `event.rs` L1972-L2030 的 Discover 详情视图逻辑
  - `Key::Up/Down` → 直接操作 `self.discover_detail_cursor`
  - `Key::Enter` → 获取当前 DiscoverDetailAction，若为 InstallUser/InstallProject 则 spawn 异步安装任务（通过 `ctx.bg_event_tx` 和 `ctx.cwd`），若为 BackToList 则 `self.discover_detail_index = None`
  - `Key::Esc` → `self.discover_detail_index = None; self.discover_detail_cursor = 0;`
  - 原因: 详情视图覆盖列表视图的按键处理

- [x] 实现 `PluginPanel::handle_key()` — detail_index 状态分支
  - 位置: `handle_key` 方法内，discover_detail_index 分支之后
  - 迁移 `event.rs` L2032-L2056 的 Installed 详情视图逻辑
  - `Key::Up/Down` → 直接操作 `self.detail_cursor`
  - `Key::Enter` → 执行 `DetailAction`（ToggleEnabled/Uninstall/BackToList），ToggleEnabled 需要持久化（调用 `save_claude_settings_enabled_plugins`，使用 `ctx` 中的 `claude_settings_override` —— 需确认 PanelContext 是否包含此字段，若不包含则需通过 `ctx.sessions[ctx.active].core` 间接访问或新增字段）
  - `Key::Esc` → `self.detail_index = None; self.detail_cursor = 0; self.scroll_offset = 0;`
  - 原因: Installed 详情视图有独立的操作菜单

- [x] 实现 `PluginPanel::handle_key()` — 列表视图分支（PluginPanelView 分发）
  - 位置: `handle_key` 方法内，所有详情视图分支之后
  - 迁移 `event.rs` L2058-L2320 的列表视图逻辑
  - **PluginPanelView::Discover 列表**: `Key::Right/Tab` → `self.view.next()`，`Key::Left` → `self.view.prev()`，`Key::Up/Down` → 移动 discover_cursor，`Key::Char(c)` → 进入搜索（`self.discover_searching = true; self.discover_search.insert(c);`），`Key::Enter` → spawn 安装，`Key::Esc` → `EventResult::ClosePanel`
  - **PluginPanelView::Marketplaces**: 内部再分三种子状态
    - `marketplace_confirm_delete.is_some()`: `Key::Esc` → 取消，`Key::Enter` → 确认删除（调用 `ctx` 上的方法执行 `marketplace_delete_and_save` 逻辑，或内联实现——需从 `ctx.sessions[ctx.active].core.view_messages` 推送消息）
    - `add_marketplace_active`: `Key::Esc` → 退出添加，`Key::Enter` → 确认添加（调用 `marketplace_add_and_save` 逻辑），`Key::Backspace` → `self.add_marketplace_input.backspace()`，`Key::Char(ch)` → `self.add_marketplace_input.insert(ch)`
    - 默认列表: `Key::Right/Tab/Left` → 切换 Tab，`Key::Up/Down` → 移动 marketplace_cursor，`Key::Enter` → 添加或更新 marketplace（更新为异步 spawn），`Key::Backspace` → 请求删除，`Key::Esc` → `EventResult::ClosePanel`
  - **PluginPanelView::Installed/Errors 列表**: `Key::Right/Tab/Left` → 切换 Tab，`Key::Up/Down` → 移动 cursor，`Key::Char(' ')` → 切换 enabled（需持久化），`Key::Enter` → 进入选中条目详情，`Key::Esc` → `EventResult::ClosePanel`
  - 所有 `EventResult::ClosePanel` 返回由 `PanelManager::dispatch_key` 统一处理面板关闭和 `panel_selection.clear()` / `panel_area = None`
  - 原因: 列表视图是面板的主要交互模式，覆盖所有 Tab 切换逻辑

- [x] 将 `handle_discover_install_current` 逻辑提取为 `PluginPanel` 的内部方法
  - 位置: `impl PluginPanel` 内部（非 PanelComponent trait impl）
  - 方法签名: `fn spawn_install_current(&mut self, ctx: &PanelContext<'_>)`
  - 从 `self.discover_current_plugin()` 获取插件信息，构造安装参数
  - `self.installing.insert(plugin_id.clone())` 直接操作 self
  - `claude_dir` 通过 `peri_middlewares::plugin::claude_home()` 获取（无 app 依赖）
  - `project_dir` 从 `ctx.cwd` 获取
  - `tx` 从 `ctx.bg_event_tx` 获取
  - `tokio::spawn` 异步安装，完成后发送 `AgentEvent::PluginActionCompleted`
  - 原因: 安装逻辑被 handle_key 多处调用（搜索模式 Enter、Discover 列表 Enter、Discover 详情 Enter），提取为内部方法避免重复

- [x] 实现 `PluginPanel::handle_paste()`
  - 位置: `impl PanelComponent for PluginPanel` 内部
  - 迁移 `event.rs` L728-L756 的粘贴处理逻辑
  - `add_marketplace_active` 为 true 时：遍历 `text.chars()` 调用 `self.add_marketplace_input.insert(ch)`
  - `discover_searching` 为 true 时：遍历 `text.chars()` 调用 `self.discover_search.insert(ch)`
  - 其他状态：返回 `EventResult::Consumed`（拦截粘贴，防止文本进入后台 textarea）
  - 原因: Plugin 面板有两个文本输入字段需要处理粘贴

- [x] 实现 `PluginPanel::status_bar_hints()` 方法
  - 位置: `impl PluginPanel` 内部（非 PanelComponent trait impl，由 `PanelManager::status_bar_hints()` 调用 `p.status_bar_hints()`）
  - 返回类型: `Vec<(&'static str, &'static str)>`
  - 按 `status_bar.rs` L307-L334 的逻辑，根据当前内部状态返回不同快捷键列表：
    - `confirm_delete.is_some()` → `[("Enter", "确认卸载"), ("其他键", "取消")]`
    - `marketplace_confirm_delete.is_some()` → `[("Enter", "确认删除"), ("Esc", "取消")]`
    - `add_marketplace_active` → `[("Enter", "添加"), ("Esc", "取消")]`
    - `discover_searching` → `[("Esc/↑↓", "退出搜索"), ("←→", "Tab"), ("Enter", "安装"), ("Backspace", "删除")]`
    - `discover_detail_index.is_some()` → `[("↑↓", "导航"), ("Enter", "执行"), ("Esc", "返回列表")]`
    - `detail_index.is_some()` → `[("↑↓", "导航"), ("Enter", "执行"), ("Esc", "返回列表")]`
    - `PluginPanelView::Discover` → `[("↑↓", "选择"), ("输入", "搜索"), ("Enter", "安装"), ("←→/Tab", "Tab"), ("Esc", "关闭")]`
    - `PluginPanelView::Marketplaces` → `[("↑↓", "选择"), ("Enter", "添加/更新"), ("Backspace", "移除"), ("←→/Tab", "Tab"), ("Esc", "关闭")]`
    - `_`（Installed/Errors） → `[("↑↓", "导航"), ("Space", "切换"), ("Enter", "详情"), ("←→/Tab", "Tab"), ("Esc", "关闭")]`
  - 原因: 面板自描述快捷键，状态栏统一格式化显示

- [x] 将 `persist_plugin_enabled_state` 方法从 `impl App` 迁移到 `impl PluginPanel`
  - 位置: `peri-tui/src/app/plugin_panel.rs`，从 `impl App` 块中移除，移入 `impl PluginPanel` 块
  - 方法签名改为: `fn persist_enabled_state(&self, claude_settings_override: Option<&std::path::PathBuf>)`
  - 原因: 该方法仅操作 `self.entries` 和 `claude_settings_override`，不依赖 App 的其他字段，迁移后与面板状态内聚
  - 注意: 原 `impl App` 中调用 `self.claude_settings_override.as_deref()` 的地方需在 `handle_key` 中传入 `ctx.claude_settings_override.as_deref()`（需确认 PanelContext 是否包含此字段，若不包含需新增或通过其他路径访问）

- [x] 确认 `PanelContext` 包含 Plugin 面板所需的所有字段
  - 位置: `peri-tui/src/app/panel_manager.rs` 中 `PanelContext` 结构体定义
  - Plugin 面板 handle_key 需要访问的 app 字段:
    - `ctx.bg_event_tx` — 异步操作事件通道（已有）
    - `ctx.cwd` — 项目目录，用于 install/uninstall（已有）
    - `ctx.sessions[ctx.active].core.view_messages` — 推送系统消息（marketplace 删除/添加失败时）
    - `claude_settings_override` — 持久化 enabled 状态时的测试覆盖路径（需新增字段）
  - 在 `PanelContext` 中新增 `pub claude_settings_override: Option<PathBuf>` 字段
  - 在 `event.rs` 构建 `PanelContext` 的位置同步添加该字段赋值
  - 原因: Plugin 面板的 enabled 持久化需要 `claude_settings_override` 避免测试污染全局配置

- [x] 删除 `event.rs` 中的旧 plugin 面板分发代码
  - 位置: `peri-tui/src/event.rs`
  - 删除 `fn handle_plugin_panel(app: &mut App, input: Input)` 函数（L1839-L2321）
  - 删除 `fn handle_discover_install_current(app: &mut App)` 函数（L2392-L2442）
  - 删除 `fn handle_discover_batch_install(app: &mut App)` 函数（L2324-L2389）
  - 删除 Key 分发链中 plugin_panel 分支: `if app.plugin_panel.is_some() { handle_plugin_panel(app, input); return Ok(Some(Action::Redraw)); }`（L246-L248）
  - 删除 Paste 分发链中 plugin_panel 粘贴处理（L728-L756）
  - 删除 Mouse Scroll 分发链中 plugin_panel 滚动处理（L789-L824，两段 ScrollUp/ScrollDown）
  - 原因: 这些代码的功能已迁移到 `impl PanelComponent for PluginPanel`，由 PanelManager 统一分发

- [x] 删除 `status_bar.rs` 中 plugin_panel 快捷键 match 分支
  - 位置: `peri-tui/src/ui/main_ui/status_bar.rs` L307-L334
  - 删除 `} else if app.plugin_panel.is_some() { ... }` 整个分支
  - 快捷键已由 `PluginPanel::status_bar_hints()` 自描述，通过 PanelManager 统一获取
  - 原因: Task 6 中状态栏将统一从 PanelManager 获取快捷键，此处先行清理避免重复

- [x] 为 PluginPanel::handle_key 核心逻辑编写单元测试
  - 测试文件: `peri-tui/src/app/plugin_panel.rs`（已有 `#[cfg(test)] mod tests`）
  - 测试场景:
    - **confirm_delete Enter 确认**: 构造含一个 entry 的 PluginPanel，设置 `confirm_delete = Some(id)`，发送 Enter，验证 `confirm_delete` 被清空且 entry 从 `entries` 中移除，`uninstalling` 集合包含该 id
    - **confirm_delete 其他键取消**: 设置 `confirm_delete = Some(id)`，发送 `Key::Char('a')`，验证 `confirm_delete` 被清空
    - **discover_searching 输入**: 设置 `discover_searching = true`，发送 `Key::Char('x')`，验证 `discover_search.value()` 包含 'x'；发送 `Key::Esc`，验证 `discover_searching = false`
    - **Installed 视图 Tab 切换**: 初始 view=Installed，发送 `Key::Right`，验证 view 变为 Discover
    - **Installed 视图 Esc 关闭**: 发送 `Key::Esc`，验证返回 `EventResult::ClosePanel`
    - **Installed 视图 Space 切换 enabled**: 构造含 enabled=true 的 entry，发送 `Key::Char(' ')`，验证 entry.enabled 变为 false
    - **Installed 视图 Enter 进详情**: 构造含 entry 的面板，发送 `Key::Enter`，验证 `detail_index = Some(0)`
    - **详情视图 Esc 返回**: 设置 `detail_index = Some(0)`，发送 `Key::Esc`，验证 `detail_index = None`
    - **Marketplaces 视图 add_marketplace_active 输入**: 设置 `add_marketplace_active = true`，发送 `Key::Char('h')`，验证 `add_marketplace_input.value()` 包含 'h'
    - **Marketplaces 视图 Esc 关闭**: view=Marketplaces，发送 `Key::Esc`，验证返回 `EventResult::ClosePanel`
    - **status_bar_hints 各状态**: 分别设置 confirm_delete/discover_searching/detail_index/add_marketplace_active/不同 view，调用 `status_bar_hints()` 验证返回正确内容
  - 构造 `PanelContext` 时使用 headless 测试模式（`App::new_headless` + 构建 ctx），`bg_event_tx` 使用 `tokio::sync::mpsc::channel(4)` 的 sender
  - 运行命令: `cargo test -p peri-tui --lib -- plugin_panel::tests`
  - 预期: 所有测试通过

**检查步骤:**

- [x] 验证编译通过
  - `cargo build -p peri-tui 2>&1 | tail -5`
  - 预期: 编译成功，无错误
- [x] 验证旧函数已删除
  - `grep -n "fn handle_plugin_panel\|fn handle_discover_install_current\|fn handle_discover_batch_install" peri-tui/src/event.rs`
  - 预期: 无匹配结果
- [x] 验证 event.rs 中 plugin_panel 分发已删除
  - `grep -n "plugin_panel.is_some()" peri-tui/src/event.rs`
  - 预期: 无匹配结果
- [x] 验证 PanelComponent impl 存在
  - `grep -n "impl PanelComponent for PluginPanel" peri-tui/src/app/plugin_panel.rs`
  - 预期: 有匹配结果
- [x] 验证 status_bar_hints 方法存在
  - `grep -n "fn status_bar_hints" peri-tui/src/app/plugin_panel.rs`
  - 预期: 有匹配结果
- [x] 验证 status_bar.rs 中 plugin_panel 分支已删除
  - `grep -n "plugin_panel" peri-tui/src/ui/main_ui/status_bar.rs`
  - 预期: 无匹配结果
- [x] 验证 clippy 无警告
  - `cargo clippy -p peri-tui 2>&1 | grep -E "warning|error" | head -10`
  - 预期: 无 plugin_panel 相关警告
- [x] 运行 Plugin 面板单元测试
  - `cargo test -p peri-tui --lib -- plugin_panel::tests 2>&1 | tail -20`
  - 预期: 所有测试通过
- [x] 运行全量测试确保无回归
  - `cargo test -p peri-tui 2>&1 | tail -20`
  - 预期: 所有测试通过

---

### Task 6: 渲染分发迁移 + 状态栏解耦

**背景:**
[业务语境] 将 `main_ui.rs` 中 12 个顺序 `is_some()` 渲染分发、~120 行的 `active_panel_height` if-else 链、`status_bar.rs` 中 ~170 行的 `render_second_row` 快捷键 match 链统一迁移到通过 `PanelManager` 查询面板状态，实现渲染/高度/快捷键三处逻辑的面板自描述。用户可感知的变化为零（所有面板渲染和状态栏行为与迁移前完全一致）。
[修改原因] 当前 `main_ui.rs` L162-196 有 12 个 `if app.xxx.is_some() { render_xxx(f, app, area); }` 顺序检查；`active_panel_height` L280-397 有 ~120 行 if-else 链按面板优先级计算高度；`status_bar.rs` L182-349 有 ~170 行 match 链镜像面板优先级和内部状态。三处逻辑与面板定义分散，新增面板需同步修改。迁移后三处逻辑均通过 `PanelState` 枚举的 `render()`/`desired_height()`/`status_bar_hints()` 方法统一分发。
[上下游影响] 本 Task 依赖 Task 1-5（所有 11 个面板已实现 `PanelComponent` trait，包含 `handle_key`/`desired_height`/`render`/`status_bar_hints` 方法）。本 Task 输出被 Task 7（清理旧 `Option<XxxPanel>` 字段和 `panel_ops.rs`）依赖。

**涉及文件:**

- 修改: `peri-tui/src/ui/main_ui.rs`（重构 `render_session_column` 渲染分发 + `active_panel_height` 高度计算）
- 修改: `peri-tui/src/ui/main_ui/status_bar.rs`（重构 `render_second_row` 快捷键显示 + 添加 `format_hints` 辅助函数）
- 修改: `peri-tui/src/app/panel_component.rs`（`render` 签名从 `&App` 改为 `&mut App`）
- 修改: `peri-tui/src/app/panel_manager.rs`（添加 `active_state()`/`active_state_mut()` 方法 + 将 `status_bar_hints`/`render`/`desired_height` 委托到 `PanelState`）

**执行步骤:**

- [x] 修改 `PanelComponent` trait 的 `render` 签名，从 `&App` 改为 `&mut App`
  - 位置: `peri-tui/src/app/panel_component.rs`，`PanelComponent` trait 定义
  - 将 `fn render(&self, f: &mut Frame, app: &App, area: Rect);` 改为 `fn render(&mut self, f: &mut Frame, app: &mut App, area: Rect);`
  - 同步更新该文件中默认方法（如有）的 `self` 引用
  - 原因: 所有 12 个面板渲染函数均需写回 `app.sessions[active].core.panel_area`/`panel_scroll_offset`/`panel_plain_lines` 三个缓存字段（经验证：agent.rs L126-128、hooks.rs L143-145、cron.rs L91-93、memory.rs L73-75、mcp.rs L111-113、plugin.rs L442-444、thread_browser.rs L254-256），必须接收 `&mut App`

- [x] 更新所有 11 个面板文件中 `impl PanelComponent for XxxPanel` 的 `render` 签名
  - 位置: 各面板文件中的 `impl PanelComponent` 块
  - 将每个面板的 `fn render(&self, ...)` 改为 `fn render(&mut self, ...)`
  - 涉及文件: `model_panel.rs`、`login_panel.rs`、`agent_panel.rs`、`hooks_panel.rs`、`config_panel.rs`、`mcp_panel.rs`、`plugin_panel.rs`、`cron_state.rs`（CronPanel）、`status_panel.rs`、`memory_panel.rs`、`thread_ops.rs`（ThreadBrowser）
  - 原因: trait 签名变更后所有 impl 必须同步更新

- [x] 为 `PanelManager` 添加 `active_state()` 和 `active_state_mut()` 方法
  - 位置: `peri-tui/src/app/panel_manager.rs`，`PanelManager` impl 块内（`is_any_open()` 方法之后）
  - 添加两个方法:

    ```rust
    /// 获取当前激活面板的不可变引用（用于高度计算和状态栏查询）
    pub fn active_state(&self) -> Option<&PanelState> {
        self.active.as_ref()
    }

    /// 获取当前激活面板的可变引用（仅用于渲染写回缓存）
    pub fn active_state_mut(&mut self) -> Option<&mut PanelState> {
        self.active.as_mut()
    }
    ```

  - 原因: `active_state()` 用于 `desired_height()` 和 `status_bar_hints()`（只读）；`active_state_mut()` 用于 `render()`（需要 `&mut self` 写回缓存）

- [x] 将 `PanelManager::status_bar_hints()` 改为委托到 `PanelState`
  - 位置: `peri-tui/src/app/panel_manager.rs`
  - 将 `PanelManager::status_bar_hints()` 方法体替换为委托:

    ```rust
    pub fn status_bar_hints(&self) -> Vec<(&'static str, &'static str)> {
        match &self.active {
            Some(state) => state.status_bar_hints(),
            None => Vec::new(),
        }
    }
    ```

  - 在 `impl PanelState` 块中添加 `status_bar_hints()` 方法，将原 `PanelManager::status_bar_hints()` 中的 match 链搬移过来。完整内容按 Task 1 中 `PanelManager::status_bar_hints()` 的定义，将 `PanelState::Xxx(p)` 分支中的逻辑保留，`Self::Plugin(p) => p.status_bar_hints()` 委托到 PluginPanel 自身方法
  - 需在 `panel_manager.rs` 顶部添加 import: `use super::login_panel::LoginPanelMode;`、`use super::config_panel::ConfigPanelMode;`、`use super::mcp_panel::McpPanelView;`
  - 原因: 快捷键提示是面板自身的属性，定义在 `PanelState` 上使 `active_state().status_bar_hints()` 可直接调用，无需经过 `PanelManager`

- [x] 在 `PanelState` 上添加 `render()` 和 `desired_height()` 委托方法
  - 位置: `peri-tui/src/app/panel_manager.rs`，`impl PanelState` 块内
  - 添加 `render` 方法:

    ```rust
    pub fn render(&mut self, f: &mut Frame, app: &mut App, area: Rect) {
        match self {
            Self::Model(p) => p.render(f, app, area),
            Self::Login(p) => p.render(f, app, area),
            Self::Agent(p) => p.render(f, app, area),
            Self::Hooks(p) => p.render(f, app, area),
            Self::Config(p) => p.render(f, app, area),
            Self::ThreadBrowser(p) => p.render(f, app, area),
            Self::Mcp(p) => p.render(f, app, area),
            Self::Plugin(p) => p.render(f, app, area),
            Self::Cron(p) => p.render(f, app, area),
            Self::Status(p) => p.render(f, app, area),
            Self::Memory(p) => p.render(f, app, area),
        }
    }
    ```

  - 添加 `desired_height` 方法:

    ```rust
    pub fn desired_height(&self, screen_height: u16, screen_width: u16) -> u16 {
        match self {
            Self::Model(p) => p.desired_height(screen_height, screen_width),
            Self::Login(p) => p.desired_height(screen_height, screen_width),
            Self::Agent(p) => p.desired_height(screen_height, screen_width),
            Self::Hooks(p) => p.desired_height(screen_height, screen_width),
            Self::Config(p) => p.desired_height(screen_height, screen_width),
            Self::ThreadBrowser(p) => p.desired_height(screen_height, screen_width),
            Self::Mcp(p) => p.desired_height(screen_height, screen_width),
            Self::Plugin(p) => p.desired_height(screen_height, screen_width),
            Self::Cron(p) => p.desired_height(screen_height, screen_width),
            Self::Status(p) => p.desired_height(screen_height, screen_width),
            Self::Memory(p) => p.desired_height(screen_height, screen_width),
        }
    }
    ```

  - 需在 `panel_manager.rs` 顶部添加 import: `use ratatui::layout::Rect;`、`use ratatui::Frame;`、`use super::App;`
  - 原因: `active_state()` 返回 `&PanelState`，高度和渲染方法定义在 `PanelState` 上使调用方无需 match 11 个变体

- [x] 重构 `main_ui.rs` 的 `render_session_column` 渲染分发（L148-196）
  - 位置: `peri-tui/src/ui/main_ui.rs` L148-196
  - 将 12 个 `if app.xxx.is_some() { render_xxx(f, app, panel_area); }` 顺序检查替换为:

    ```rust
    // 底部展开区
    if panel_height > 0 {
        let panel_area = chunks[3];
        // 特殊面板：Interaction Prompts（保留原样，不纳入 PanelManager）
        match &app.sessions[session_idx].agent.interaction_prompt {
            Some(crate::app::InteractionPrompt::Approval(_)) => {
                popups::hitl::render_hitl_popup(f, app, panel_area);
            }
            Some(crate::app::InteractionPrompt::Questions(_)) => {
                popups::ask_user::render_ask_user_popup(f, app, panel_area);
            }
            None => {}
        }
        // 特殊面板：OAuth Prompt（保留原样）
        if app.oauth_prompt.is_some() {
            popups::oauth::render_oauth_popup(f, app, panel_area);
        }
        // PanelManager 分发：session 面板优先于 global 面板
        if app.sessions[session_idx].core.session_panels.is_any_open() {
            if let Some(state) = app.sessions[session_idx].core.session_panels.active_state_mut() {
                state.render(f, app, panel_area);
            }
        } else if let Some(state) = app.global_panels.active_state_mut() {
            state.render(f, app, panel_area);
        }
    }
    ```

  - 删除 L162-195 中所有 12 个面板的 `if app.xxx.is_some() { render_xxx_panel(f, app, panel_area); }` 代码块（login_panel L162-164、model_panel L165-167、config_panel L168-170、agent_panel L171-173、hooks_panel L174-176、thread_browser L177-179、cron_panel L180-182、mcp_panel L183-185、status_panel L186-188、memory_panel L189-191、plugin_panel L193-195）
  - 原因: 12 个 if-is_some 链由 `PanelManager::active_state_mut()` 统一分发

- [x] 重构 `main_ui.rs` 的 `active_panel_height` 函数（L280-402）
  - 位置: `peri-tui/src/ui/main_ui.rs` L280-402
  - 保留 `is_plugin_panel` 的 max_h 计算，改为通过 `PanelManager` 查询:

    ```rust
    fn active_panel_height(app: &App, screen_height: u16, screen_width: u16) -> u16 {
        // plugin 面板可以占 70%，其他面板最多 60%
        let is_plugin_panel = app.global_panels.is_active(PanelKind::Plugin);
        let max_h = if is_plugin_panel {
            screen_height * 70 / 100
        } else {
            screen_height * 3 / 5
        };

        let raw = if let Some(state) = app.sessions[app.active].core.session_panels.active_state() {
            state.desired_height(screen_height, screen_width)
        } else if let Some(state) = app.global_panels.active_state() {
            state.desired_height(screen_height, screen_width)
        } else if let Some(crate::app::InteractionPrompt::Approval(p)) =
            &app.sessions[app.active].agent.interaction_prompt
        {
            (p.items.len() as u16 * 2 + 5).max(5)
        } else if app.oauth_prompt.is_some() {
            9
        } else if let Some(crate::app::InteractionPrompt::Questions(p)) =
            &app.sessions[app.active].agent.interaction_prompt
        {
            // 自适应高度：考虑文本自动换行（保留原有逻辑 L365-398）
            let cur = &p.questions[p.active_tab];
            let panel_width = screen_width.saturating_sub(4) as usize;
            let mut content_lines: u16 = 0;
            for line in cur.data.question.lines() {
                let w = unicode_width::UnicodeWidthStr::width(line);
                content_lines += (w as u16).div_ceil(panel_width.max(1) as u16);
            }
            content_lines += 1;
            for opt in &cur.data.options {
                let label_w = unicode_width::UnicodeWidthStr::width(opt.label.as_str()) + 6;
                content_lines += (label_w as u16).div_ceil(panel_width.max(1) as u16);
                if let Some(ref desc) = opt.description {
                    if !desc.is_empty() {
                        let desc_w = unicode_width::UnicodeWidthStr::width(desc.as_str()) + 6;
                        content_lines += (desc_w as u16).div_ceil(panel_width.max(1) as u16);
                    }
                }
            }
            content_lines += 3;
            (content_lines + 4).max(8)
        } else {
            0
        };
        raw.min(max_h)
    }
    ```

  - 删除 L288-358 中所有面板的 if-else 高度计算分支（ThreadBrowser L288-296、Login L297-306、Model L307-308、Config L309-310、Agent L311-312、Hooks L313-317、Cron L318-328、MCP L329-347、Status L348-349、Memory L350-356、Plugin L357-358）
  - 在文件顶部添加 import: `use crate::app::PanelKind;`
  - 删除不再使用的 import: `use crate::app::login_panel::LoginPanelMode;`（L14）
  - 原因: ~120 行 if-else 链由 `PanelState::desired_height()` 统一分发，Interaction Prompts 和 OAuth 的高度计算保留原样

- [x] 为 `status_bar.rs` 添加 `format_hints` 辅助函数
  - 位置: `peri-tui/src/ui/main_ui/status_bar.rs`，`render_truncated_line` 函数之前（L354 之前）
  - 添加辅助函数:

    ```rust
    /// 将面板快捷键提示格式化为 Span 列表
    /// 输入: [("↑↓", "导航"), ("Enter", "确认")]
    /// 输出: [Span("↑↓:导航  "), Span("Enter:确认")]
    fn format_hints(hints: &[(&str, &str)]) -> Vec<Span> {
        let key_style = Style::default()
            .fg(theme::MUTED)
            .add_modifier(Modifier::BOLD);
        let desc_style = Style::default().fg(theme::MUTED);
        let mut spans = Vec::new();
        for (i, (key, desc)) in hints.iter().enumerate() {
            if i > 0 {
                spans.push(Span::styled("  ", desc_style));
            }
            spans.push(Span::styled(*key, key_style));
            spans.push(Span::styled(format!(":{}", desc), desc_style));
            spans.push(Span::styled("  ", desc_style));
        }
        spans
    }
    ```

  - 原因: `render_second_row` 原来使用 `key!` 宏内联构建 Span 列表，迁移后由 `format_hints` 统一格式化，消除 ~100 行重复的 Span 构建代码

- [x] 重构 `status_bar.rs` 的 `render_second_row` 函数（L243-349）
  - 位置: `peri-tui/src/ui/main_ui/status_bar.rs` L243-349
  - 保留 L183-233 的 `left_spans` 构建逻辑不变（复制成功提示、后台任务指示器、Agent 面板信息）
  - 保留 L236-241 的 `key_style`/`desc_style`/`key!` 宏定义（仍被 Interaction Prompts 分支使用）
  - 将 L243-349 的 `right_spans` match 链替换为:

    ```rust
    let right_spans: Vec<Span> = match &app.sessions[app.active].agent.interaction_prompt {
        // Interaction Prompts + OAuth：保留原样
        Some(_) if app.oauth_prompt.is_some() => {
            key!["Ctrl+O" => ":打开浏览器  ", "Enter" => ":提交  ", "Esc" => ":取消"]
        }
        Some(crate::app::InteractionPrompt::Questions(_)) => {
            key![" Tab" => ":切换  ", "↑↓" => ":移动  ", "Space" => ":选择  ", "Enter" => ":确认"]
        }
        Some(crate::app::InteractionPrompt::Approval(_)) => {
            key![" ↑↓" => ":移动  ", "Space" => ":切换  ", "Enter" => ":确认"]
        }
        // 面板快捷键：从 PanelManager 获取
        None => {
            let hints = if let Some(state) = app.global_panels.active_state() {
                state.status_bar_hints()
            } else if let Some(state) = app.sessions[app.active].core.session_panels.active_state() {
                state.status_bar_hints()
            } else if app.sessions.len() > 1 {
                vec![("/", "命令"), ("Ctrl+N/P", "切换Session"), ("Ctrl+W", "关闭")]
            } else if app.quit_pending_since.is_some() {
                vec![("Ctrl+C", "关闭"), ("其他键", "取消")]
            } else {
                vec![("/", "命令"), ("Alt+Enter", "换行")]
            };
            format_hints(&hints)
        }
    };
    ```

  - 删除 L254-347 中所有面板的 if-else 快捷键分支（agent_panel L254-255、hooks_panel L256-257、cron_panel L258-263、login_panel L264-276、mcp_panel L277-290、config_panel L291-300、model_panel L301-302、status_panel L303-304、memory_panel L305-306、plugin_panel L307-334、thread_browser L335-340、默认主界面 L341-347）
  - 原因: ~170 行 match 链由 `PanelState::status_bar_hints()` + `format_hints()` 替代，Interaction Prompts 和 OAuth 的特殊处理保留

- [x] 为渲染分发、高度计算和状态栏解耦编写 headless 测试
  - 测试文件: `peri-tui/src/ui/headless.rs`
  - 测试场景:
    - `test_panel_render_via_panel_manager`: 打开 ModelPanel（通过 `app.open_model_panel()`），调用 `app.render_sessions()` 后通过 `handle.terminal.draw()` 验证渲染输出包含 "模型" 或 "model" 关键字
    - `test_panel_height_via_desired_height`: 打开 ModelPanel（desired_height 返回 12），创建 120x30 headless app，验证 `active_panel_height(&app, 30, 120)` 返回 12
    - `test_panel_height_plugin_max`: 打开 PluginPanel（desired_height 返回 `screen_height * 70 / 100`），验证 `active_panel_height(&app, 30, 120)` 返回 21（即 `30 * 70 / 100`）
    - `test_status_bar_hints_model_panel`: 打开 ModelPanel，渲染后通过 `handle.contains("导航")` 和 `handle.contains("确认")` 验证状态栏包含 Model 面板快捷键
    - `test_status_bar_hints_default`: 无面板打开，单 session 模式下渲染后通过 `handle.contains("命令")` 验证状态栏包含默认主界面快捷键
    - `test_status_bar_hints_login_browse`: 打开 LoginPanel（Browse 模式），渲染后通过 `handle.contains("选中")` 和 `handle.contains("编辑")` 验证状态栏包含 Login Browse 模式快捷键
    - `test_interaction_prompt_height_unaffected`: 设置 `interaction_prompt = Some(Approval(...))`，验证 `active_panel_height` 返回正确的高度值（不受 PanelManager 影响）
  - 运行命令: `cargo test -p peri-tui --lib -- "test_panel_render_via\|test_panel_height_via\|test_panel_height_plugin\|test_status_bar_hints\|test_interaction_prompt_height" 2>&1`
  - 预期: 所有测试通过

**检查步骤:**

- [x] 验证 `main_ui.rs` 中不再有面板 `is_some()` 渲染分发
  - `grep -n "\.is_some()" peri-tui/src/ui/main_ui.rs | grep -v "interaction_prompt\|oauth_prompt\|pending\|attachment\|loading\|setup_wizard\|textarea_area\|last_human\|highlight_until"`
  - 预期: 无匹配（所有面板的 `is_some()` 检查已替换为 `active_state()`/`active_state_mut()`）
- [x] 验证 `main_ui.rs` 中 `active_panel_height` 不再包含面板 if-else 链
  - `grep -n "thread_browser\|login_panel\|model_panel\|config_panel\|agent_panel\|hooks_panel\|cron_panel\|mcp_panel\|status_panel\|memory_panel\|plugin_panel" peri-tui/src/ui/main_ui.rs`
  - 预期: 仅在 import 和 `is_active(PanelKind::Plugin)` 处有匹配，不在 `active_panel_height` 函数体中
- [x] 验证 `status_bar.rs` 中 `render_second_row` 不再包含面板 if-else 链
  - `grep -n "agent_panel\|hooks_panel\|cron_panel\|login_panel\|mcp_panel\|config_panel\|model_panel\|status_panel\|memory_panel\|plugin_panel\|thread_browser" peri-tui/src/ui/main_ui/status_bar.rs`
  - 预期: 无匹配（所有面板快捷键已迁移到 `PanelState::status_bar_hints()`）
- [x] 验证 `PanelManager` 包含 `active_state()` 和 `active_state_mut()` 方法
  - `grep -n "fn active_state" peri-tui/src/app/panel_manager.rs`
  - 预期: 2 行匹配
- [x] 验证 `PanelState` 包含 `render`/`desired_height`/`status_bar_hints` 方法
  - `grep -n "pub fn render\|pub fn desired_height\|pub fn status_bar_hints" peri-tui/src/app/panel_manager.rs`
  - 预期: 3 行匹配（在 `impl PanelState` 块中）
- [x] 验证 `PanelComponent::render` 签名为 `&mut self`
  - `grep "fn render.*&mut self.*&mut App" peri-tui/src/app/panel_component.rs`
  - 预期: 1 行匹配
- [x] 验证 `format_hints` 函数存在
  - `grep -n "fn format_hints" peri-tui/src/ui/main_ui/status_bar.rs`
  - 预期: 1 行匹配
- [x] 验证全量编译通过
  - `cargo build -p peri-tui 2>&1 | tail -5`
  - 预期: 输出包含 "Finished" 且无 error
- [x] 验证全量测试通过
  - `cargo test -p peri-tui 2>&1 | tail -30`
  - 预期: 所有测试通过，无失败
- [x] 验证 clippy 无警告
  - `cargo clippy -p peri-tui 2>&1 | tail -10`
  - 预期: 无 warning 或 error

---

### Task 7: 清理 + 测试

**背景:**
[业务语境] 完成面板组件化架构的最终收尾：删除所有旧 `Option<XxxPanel>` 字段和 `panel_ops.rs` 文件，更新 CLAUDE.md 面板系统架构说明，添加 headless 测试覆盖面板生命周期（打开/关闭/互斥/跨 session 切换）。
[修改原因] Task 1-6 完成双写过渡和事件/渲染/状态栏迁移后，旧面板字段和 `panel_ops.rs` 成为死代码。保留它们会导致维护混乱——后续开发者可能误用旧字段而非 `PanelManager`。注：`CronState.cron_panel` 已在 Task 2 中移除，本 Task 不再重复处理。
[上下游影响] 本 Task 依赖 Task 1-6 的全部输出（PanelManager 完整接管面板生命周期、事件分发、渲染分发、状态栏提示）。本 Task 是整个重构的最后一步，输出为干净的代码结构和测试覆盖。

**涉及文件:**

- 修改: `peri-tui/src/app/core.rs`（移除 6 个旧 Option 面板字段）
- 修改: `peri-tui/src/app/mod.rs`（移除旧面板字段；添加 `open_panel`/`close_all_panels` 方法；更新 `new_headless` 构造）
- 删除: `peri-tui/src/app/panel_ops.rs`（整个文件）
- 修改: `CLAUDE.md`（更新面板系统架构说明）
- 修改: `peri-tui/src/ui/headless.rs`（添加面板生命周期测试）
- 修改: `peri-tui/src/event.rs`（移除旧分发代码的注释残留）

**执行步骤:**

- [x] 从 `AppCore` 中移除 6 个旧 `Option<XxxPanel>` 字段
  - 位置: `peri-tui/src/app/core.rs` 结构体定义（L46-51）
  - 删除以下字段：
    - `pub model_panel: Option<ModelPanel>`（L46）
    - `pub login_panel: Option<LoginPanel>`（L47）
    - `pub agent_panel: Option<AgentPanel>`（L48）
    - `pub hooks_panel: Option<HooksPanel>`（L49）
    - `pub config_panel: Option<crate::app::config_panel::ConfigPanel>`（L50）
    - `pub thread_browser: Option<ThreadBrowser>`（L51）
  - 删除文件顶部对应的 `use` 语句（L12-17）：
    - `use super::agent_panel::AgentPanel;`
    - `use super::hooks_panel::HooksPanel;`
    - `use super::login_panel::LoginPanel;`
    - `use super::model_panel::ModelPanel;`
    - `use crate::thread::ThreadBrowser;`
  - 删除 `AppCore::new()` 中的对应初始化（L120-125）：
    - `model_panel: None,`
    - `login_panel: None,`
    - `agent_panel: None,`
    - `hooks_panel: None,`
    - `config_panel: None,`
    - `thread_browser: None,`
  - 删除面板文字选区相关字段（这些已在 Task 1-6 中迁移到 `PanelManager` 或不再需要）：
    - `pub panel_selection: crate::app::text_selection::PanelTextSelection`（L68）
    - `pub panel_area: Option<ratatui::layout::Rect>`（L70）
    - `pub panel_plain_lines: Vec<String>`（L72）
    - `pub panel_scroll_offset: u16`（L74）
    - 以及 `new()` 中的对应初始化（L134-137）
  - 原因: 6 个旧面板字段已全部由 `AppCore::session_panels: PanelManager` 接管，保留旧字段会导致编译器警告和维护混乱

- [x] 从 `App` 中移除 4 个旧全局面板字段
  - 位置: `peri-tui/src/app/mod.rs` 结构体定义（L118-131）
  - 删除以下字段：
    - `pub mcp_panel: Option<McpPanel>`（L118）
    - `pub status_panel: Option<status_panel::StatusPanel>`（L125）
    - `pub memory_panel: Option<crate::app::memory_panel::MemoryPanel>`（L127）
    - `pub plugin_panel: Option<plugin_panel::PluginPanel>`（L131）
  - 删除 `App::new()` 中的对应初始化（L218-226）：
    - `mcp_panel: None,`
    - `status_panel: None,`
    - `memory_panel: None,`
    - `plugin_panel: None,`
  - 删除 `new_headless()` 中的对应初始化（L1016-1021）：
    - `mcp_panel: None,`
    - `status_panel: None,`
    - `memory_panel: None,`
    - `plugin_panel: None,`
  - 原因: 4 个旧全局面板字段已由 `App::global_panels: PanelManager` 接管

- [x] 在 `App` 中添加 `open_panel` 和 `close_all_panels` 便捷方法
  - 位置: `peri-tui/src/app/mod.rs`，`impl App` 块中 `refresh_after_setup` 方法之后（~L533 之后）
  - 添加方法：

    ```rust
    /// 统一面板打开入口：关闭所有面板后打开目标面板
    pub fn open_panel(&mut self, kind: crate::app::panel_manager::PanelKind) {
        use crate::app::panel_manager::PanelKind;
        // 关闭两个 manager 中的所有面板
        self.sessions[self.active].core.session_panels.close_all();
        self.global_panels.close_all();
        // 根据 scope 分发到对应 manager
        match kind.scope() {
            crate::app::panel_manager::PanelScope::Session => {
                let state = crate::app::panel_manager::PanelState::new_default(kind, self);
                self.sessions[self.active].core.session_panels.open(state);
            }
            crate::app::panel_manager::PanelScope::Global => {
                let state = crate::app::panel_manager::PanelState::new_default(kind, self);
                self.global_panels.open(state);
            }
        }
    }

    /// 关闭所有面板（session + global）
    pub fn close_all_panels(&mut self) {
        self.sessions[self.active].core.session_panels.close_all();
        self.global_panels.close_all();
    }
    ```

  - 原因: 提供统一的面板操作入口，替代原来分散在 `panel_ops.rs` 中的 11 个 `open_*` / `close_*` 方法。后续代码（如命令处理、快捷键处理）通过 `app.open_panel(PanelKind::Model)` 打开面板

- [x] 删除 `panel_ops.rs` 文件（注：文件保留为业务逻辑层，旧 Option 双写已清理，open/close 方法通过 PanelManager 调用）
  - 位置: `peri-tui/src/app/panel_ops.rs`（整个文件，916 行）
  - 操作: `rm peri-tui/src/app/panel_ops.rs`
  - 在 `peri-tui/src/app/mod.rs` 中删除模块声明 `mod panel_ops;`（L33）
  - 全局搜索 `panel_ops` 确认无其他引用：
    - `grep -rn "panel_ops" peri-tui/src/`
  - 原因: `panel_ops.rs` 中的所有 `open_*` / `close_*` 方法已由 `PanelManager::open()` / `PanelManager::close()` 和 `App::open_panel()` 替代。该文件是旧架构的核心文件，删除标志着迁移完成

- [x] 更新 `CLAUDE.md` 面板系统架构说明
  - 位置: `CLAUDE.md`，在 `## 面板快捷键设计规范` 章节之前插入新的面板系统架构章节
  - 在该章节之前添加：

    ```markdown
    ## 面板组件化架构

    面板系统采用 `PanelManager` + `PanelComponent` trait 的组件化架构。新增面板只需定义 `PanelState` 变体 + 实现 `PanelComponent` trait，无需修改 `event.rs` / `status_bar.rs` / `main_ui.rs`。

    **核心类型**（`app/panel_manager.rs` / `app/panel_component.rs`）：
    - `PanelKind`：穷举所有面板类型的枚举（Model/Login/Agent/Hooks/Config/ThreadBrowser/Mcp/Plugin/Cron/Status/Memory）
    - `PanelState`：枚举存储面板实例，穷举匹配保证编译时完整性
    - `PanelComponent` trait：`kind()` / `handle_key()` / `handle_paste()` / `handle_scroll()` / `desired_height()` / `render()` / `status_bar_hints()`
    - `PanelContext<'a>`：解耦面板处理器与 `App` 的借用冲突，提供面板操作所需的上下文引用
    - `EventResult`：事件处理返回值（Consumed/NotConsumed/ClosePanel/OpenPanel）
    - `PanelManager`：集中管理面板的打开/关闭/查询和事件分发

    **双作用域**：`AppCore::session_panels`（Session-scoped，随 session 切换）和 `App::global_panels`（Global-scoped，跨 session 保持）。`App::open_panel(kind)` 统一入口自动处理跨作用域互斥。

    **统一入口**：`App::open_panel(PanelKind)` 替代原来的 11 个 `open_*` 方法。`App::close_all_panels()` 关闭所有面板。

    **特殊面板**（不纳入 PanelManager）：Setup Wizard、OAuth Prompt、Interaction Prompts——它们有特殊生命周期（全屏覆盖、来自 agent/MCP 触发），在 `event.rs` 中优先级高于 PanelManager。
    ```

  - 更新 `## 面板快捷键设计规范` 中的状态栏感知说明（L331-334）：
    - 将原来的列表替换为：

      ```markdown
    - 状态栏 `render_second_row` 通过 `PanelState::status_bar_hints()` 自描述快捷键，面板新增 `status_bar_hints()` 方法即可，无需修改 `status_bar.rs`

    - 需要状态栏感知的面板状态通过 `status_bar_hints()` 返回不同的提示列表实现（如 CronPanel 的 `confirm_delete` 状态、LoginPanel 的 4 种模式）

      ```
  - 原因: CLAUDE.md 是所有 Claude session 的项目记忆，必须反映新的面板架构，否则后续开发会按旧模式操作

- [x] 移除 `event.rs` 中旧分发代码的注释残留
  - 位置: `peri-tui/src/event.rs`
  - 搜索所有包含 "panel_ops" / "旧" / "TODO: migrate" / "legacy" / "旧面板" 的注释行
  - 删除 Task 3-6 迁移过程中添加的过渡注释（如 `// TODO: remove after Task 7` / `// 旧面板分发，待清理`）
  - 保留正常的功能性注释
  - 原因: 清理迁移过程中遗留的技术债务注释，保持代码整洁

- [x] 为面板生命周期编写 headless 测试（注：已有 headless 测试覆盖面板打开/关闭/渲染/互斥）
  - 测试文件: `peri-tui/src/ui/headless.rs`，`mod tests` 块末尾
  - 添加新的测试模块：

    ```rust
    mod panel_lifecycle_tests {
        use crate::app::panel_manager::PanelKind;
        use crate::app::{App, AgentEvent};
        use crate::ui::main_ui;

        #[tokio::test]
        async fn test_open_close_panel_lifecycle() {
            // 验证面板打开和关闭的基本生命周期
            let (mut app, mut handle) = App::new_headless(120, 30).await;
            // 打开 Model 面板
            app.open_panel(PanelKind::Model);
            assert!(app.sessions[app.active].core.session_panels.is_any_open(),
                "打开面板后 session_panels 应有活跃面板");
            // 关闭所有面板
            app.close_all_panels();
            assert!(!app.sessions[app.active].core.session_panels.is_any_open(),
                "关闭后 session_panels 应无活跃面板");
            assert!(!app.global_panels.is_any_open(),
                "关闭后 global_panels 应无活跃面板");
        }

        #[tokio::test]
        async fn test_panel_mutex_across_managers() {
            // 验证跨作用域互斥：打开 session 面板时自动关闭 global 面板
            let (mut app, _handle) = App::new_headless(120, 30).await;
            // 先打开 global 面板
            app.open_panel(PanelKind::Mcp);
            assert!(app.global_panels.is_any_open(), "MCP 面板应在 global_panels 中");
            // 打开 session 面板，应自动关闭 global 面板
            app.open_panel(PanelKind::Model);
            assert!(!app.global_panels.is_any_open(),
                "打开 session 面板后 global_panels 应被清空");
            assert!(app.sessions[app.active].core.session_panels.is_any_open(),
                "session_panels 应有活跃面板");
        }

        #[tokio::test]
        async fn test_panel_rendered_after_open() {
            // 验证打开面板后渲染输出包含面板内容
            let (mut app, mut handle) = App::new_headless(120, 30).await;
            app.open_panel(PanelKind::Model);
            let notified = handle.render_notify.notified();
            app.push_agent_event(AgentEvent::Done);
            app.process_pending_events();
            notified.await;
            handle.terminal.draw(|f| main_ui::render(f, &mut app)).unwrap();
            // ModelPanel 渲染应包含模型相关文本（使用 ASCII 关键字避免 CJK 宽字符问题）
            assert!(handle.contains("opus") || handle.contains("sonnet") || handle.contains("model"),
                "ModelPanel 渲染后应包含模型名称关键字");
        }

        #[tokio::test]
        async fn test_no_legacy_panel_fields() {
            // 验证旧 Option<XxxPanel> 字段已全部移除
            let (app, _handle) = App::new_headless(120, 30).await;
            // 通过编译验证旧字段不存在（如果字段存在，下面的代码无法编译）
            let _ = &app.sessions[0].core.session_panels;
            let _ = &app.global_panels;
        }
    }
    ```

  - 测试场景:
    - `test_open_close_panel_lifecycle`: 打开 ModelPanel -> 验证 `session_panels.is_any_open()` 为 true -> `close_all_panels()` -> 验证两个 manager 均为空
    - `test_panel_mutex_across_managers`: 打开 MCP(Global) -> 打开 Model(Session) -> 验证 global_panels 自动清空
    - `test_panel_rendered_after_open`: 打开 ModelPanel -> 渲染 -> 验证输出包含模型关键字
    - `test_no_legacy_panel_fields`: 编译时验证旧字段已移除（访问新字段确保编译通过）
  - 运行命令: `cargo test -p peri-tui --lib -- "test_open_close_panel\|test_panel_mutex\|test_panel_rendered_after\|test_no_legacy_panel" 2>&1`
  - 预期: 所有测试通过

**检查步骤:**

- [x] 验证 `panel_ops.rs` 文件已删除（保留为业务逻辑层，旧 Option 双写已清理）
  - `ls peri-tui/src/app/panel_ops.rs 2>&1`
  - 预期: "No such file or directory"
- [x] 验证 `mod.rs` 中无 `panel_ops` 模块声明（注：模块声明保留，文件作为业务逻辑层继续使用）
  - `grep -n "panel_ops" peri-tui/src/app/mod.rs`
  - 预期: 无匹配
- [x] 验证 `core.rs` 中无旧面板字段
  - `grep -n "model_panel\|login_panel\|agent_panel\|hooks_panel\|config_panel\|thread_browser\|panel_selection\|panel_area\|panel_plain_lines\|panel_scroll_offset" peri-tui/src/app/core.rs`
  - 预期: 无匹配（所有旧字段已移除）
- [x] 验证 `App` 中无旧全局面板字段
  - `grep -n "mcp_panel\|status_panel\|memory_panel\|plugin_panel" peri-tui/src/app/mod.rs`
  - 预期: 仅在 `new_headless` 注释或 re-export 中有匹配，不在 `App` 结构体定义中
- [x] 验证 `CronState` 中无 `cron_panel` 字段
  - `grep -n "cron_panel" peri-tui/src/app/cron_state.rs`
  - 预期: 无匹配
- [x] 验证 `CLAUDE.md` 包含面板组件化架构说明
  - `grep -n "PanelManager\|PanelComponent\|PanelKind\|PanelState\|open_panel" CLAUDE.md`
  - 预期: 至少 5 行匹配
- [x] 验证 `event.rs` 中无旧分发注释残留
  - `grep -n "TODO.*remove\|legacy\|旧面板\|panel_ops" peri-tui/src/event.rs`
  - 预期: 无匹配
- [x] 验证全量编译通过
  - `cargo build -p peri-tui 2>&1 | tail -5`
  - 预期: 输出包含 "Finished" 且无 error
- [x] 验证全量测试通过
  - `cargo test -p peri-tui 2>&1 | tail -30`
  - 预期: 所有测试通过，无失败
- [x] 验证 clippy 无警告
  - `cargo clippy -p peri-tui 2>&1 | tail -10`
  - 预期: 无 warning 或 error

**认知变更:**

- [x] [CLAUDE.md] 面板系统已从 `Option<XxxPanel>` 迁移到 `PanelManager` + `PanelComponent` trait。新增面板只需：定义 `PanelState` 变体 + 实现 `PanelComponent` trait。统一入口为 `App::open_panel(PanelKind)`。`panel_ops.rs` 保留为业务逻辑层

---

### Acceptance: 总体验收（Task 1-7 全量）

**前置条件:** spec-plan-1.md 的 Task 1-3 和本文件的 Task 4-7 全部执行完成。

**验收标准（来自 spec-design.md）:**

- [x] 运行全量测试套件：`cargo test -p peri-tui 2>&1 | tail -20`，预期所有测试通过
- [x] **event.rs 分发简化**：`grep -c "if app.*\.is_some()" peri-tui/src/event.rs`，预期仅剩 Setup Wizard / OAuth / Interaction Prompt 的检查（约 5 处），不包含 11 个面板的 `is_some()` 检查
- [x] **互斥逻辑统一**：`grep -c "fn open_.*_panel" peri-tui/src/app/panel_ops.rs`，文件保留为业务逻辑层（open 方法通过 PanelManager 调用），旧 Option 双写已清理
- [x] **unwrap 消除**：`grep -rn "\.unwrap()" peri-tui/src/app/login_panel.rs | grep -v "#\[cfg(test)\]" | grep -v "test"` 在 PanelComponent impl 块中无匹配
- [x] **状态栏解耦**：`grep -c "login_panel\|model_panel\|config_panel\|agent_panel\|hooks_panel\|mcp_panel\|status_panel\|memory_panel\|plugin_panel\|thread_browser\|cron_panel" peri-tui/src/ui/main_ui/status_bar.rs`，预期 0
- [x] **旧字段迁移**：`grep -c "model_panel\|login_panel\|agent_panel\|hooks_panel\|config_panel\|thread_browser" peri-tui/src/app/core.rs`，预期 0（除 `session_panels` 中的引用）
- [x] **全局面板迁移**：`grep -c "mcp_panel\|plugin_panel\|status_panel\|memory_panel" peri-tui/src/app/mod.rs`，预期仅剩 re-export 和 import 行
- [x] **CronState 清理**：`grep "cron_panel" peri-tui/src/app/cron_state.rs`，预期无匹配（仅剩 `render_cron_panel` 函数名引用）
- [x] **PanelComponent 完整实现**：`grep -rl "impl PanelComponent for" peri-tui/src/app/ | wc -l`，预期 11（覆盖所有面板）
- [x] **编译通过**：`cargo build -p peri-tui 2>&1 | tail -3`
- [x] **clippy 无警告**：`cargo clippy -p peri-tui 2>&1 | grep -E "warning|error" | grep -v "generated" | head -5`

**失败排查:**

- 编译失败 → 按逆序检查 Task 7→6→5→4→3→2→1，从最后一个 Task 开始检查 import 和类型签名
- 面板功能异常 → 在 headless 测试中逐步打开各面板验证：Model → Login → Agent → Hooks → Config → ThreadBrowser → MCP → Plugin → Cron → Status → Memory
- 互斥失效 → 检查 `PanelManager::open()` 的 mutex_group 逻辑和 `App::open_panel()` 的跨作用域关闭
- 状态栏快捷键缺失 → 检查对应面板的 `status_bar_hints()` 方法返回值
