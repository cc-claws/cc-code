# App 分层重构 执行计划（第一阶段）

**目标:** 将 App 26 字段 + AppCore 38 字段逐步提取为 ServiceRegistry / SessionManager / UiState / MessageState 子结构体

**技术栈:** Rust 2021, tokio async/await, ratatui, 字段投影拆分借用策略

**设计文档:** spec-design.md

## 改动总览

- 涉及 peri-tui/src/app/ 下 6 个新文件（service_registry.rs, session_manager.rs, ui_state.rs, message_state.rs, command_system.rs, session_metadata.rs）和 18+ 个现有文件的字段访问路径迁移
- Task 1-2 重构 App 级字段（services + sessions），Task 3-4 重构 AppCore 字段（ui + messages）
- 依赖链：Task 1→Task 2→Task 3→Task 4，每 Task 独立可编译可测试
- 关键决策：采用字段投影拆分（`let App { services, .. } = &mut *app`）替代 `std::mem::take` workaround

---

### Task 0: 环境准备

**背景:**
确保构建和测试工具链在当前开发环境中可用，避免后续 Task 因环境问题阻塞。

**执行步骤:**

- [x] 验证构建工具可用
  - `cargo build -p peri-tui 2>&1 | tail -3`
  - 预期: 编译成功（可能有一个旧的 deprecation warning，但无 error）
- [x] 验证测试工具可用
  - `cargo test -p peri-tui 2>&1 | tail -5`
  - 预期: 全部测试通过

**检查步骤:**

- [x] 构建成功
  - `cargo build -p peri-tui 2>&1 | grep -c "error"`
  - 预期: 0
- [x] 测试通过
  - `cargo test -p peri-tui 2>&1 | grep -c "test result: ok"`
  - 预期: ≥ 1

---

### Task 1: 提取 ServiceRegistry

**背景:**
App 结构体当前 26 字段混合 6 种职责，其中 14 个服务字段（peri_config, cwd, provider_name, model_name, permission_mode, thread_store, mcp_pool, mcp_init_rx, cron, plugin_data, bg_event_tx, bg_event_rx, config_path_override, claude_settings_override）+ 6 个特殊生命周期字段（setup_wizard, oauth_prompt, mode_highlight_until, model_highlight_until, mcp_ready_shown_until, quit_pending_since）属于全局服务/状态，跨 session 共享。本 Task 将这 20 个字段提取到 ServiceRegistry 子结构体，App 通过 `services: ServiceRegistry` 单字段持有。提取后 App 字段数从 26 降至 7（sessions, active, session_areas, services, global_panels + 2 个暂未迁移字段），为 Task 2 SessionManager 提取铺路。本 Task 无前置依赖，其输出被 Task 2-9 全部后续 Task 依赖。

**涉及文件:**

- 新建: `peri-tui/src/app/service_registry.rs`
- 修改: `peri-tui/src/app/mod.rs`, `peri-tui/src/app/panel_ops.rs`, `peri-tui/src/event.rs`, `peri-tui/src/app/agent_ops.rs`, `peri-tui/src/app/thread_ops.rs`, `peri-tui/src/app/cron_ops.rs`, `peri-tui/src/app/message_pipeline.rs`, `peri-tui/src/app/mcp_panel.rs`, `peri-tui/src/app/plugin_panel.rs`, `peri-tui/src/app/panel_manager.rs`, `peri-tui/src/ui/main_ui/status_bar.rs`, `peri-tui/src/ui/main_ui/panels/status.rs`, `peri-tui/src/ui/main_ui/popups/setup_wizard.rs`, `peri-tui/src/ui/main_ui/popups/oauth.rs`, `peri-tui/src/ui/main_ui.rs`, `peri-tui/src/main.rs`, `peri-tui/src/command/model.rs`, `peri-tui/src/command/agents.rs`, `peri-tui/src/ui/welcome.rs`, `peri-tui/src/ui/headless.rs`

**执行步骤:**

- [x] 创建 ServiceRegistry 结构体定义
  - 位置: 新建 `peri-tui/src/app/service_registry.rs`
  - 定义 `ServiceRegistry` 结构体，包含以下 20 个 pub 字段（按 spec-design.md）：

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
        pub setup_wizard: Option<SetupWizardPanel>,
        pub oauth_prompt: Option<OAuthPrompt>,
        pub mode_highlight_until: Option<std::time::Instant>,
        pub model_highlight_until: Option<std::time::Instant>,
        pub mcp_ready_shown_until: std::cell::Cell<Option<std::time::Instant>>,
        pub quit_pending_since: Option<std::time::Instant>,
    }
    ```

  - 原因: 将 20 个散落在 App 中的服务/全局状态字段聚合成单一子结构体

- [x] 在 mod.rs 中注册模块并添加 App.services 字段（双写期）
  - 位置: `peri-tui/src/app/mod.rs` 模块声明区（~L1-36），追加 `mod service_registry;` 和 `pub use service_registry::ServiceRegistry;`
  - 在 App 结构体中（~L93-134）新增 `pub services: ServiceRegistry` 字段，位于 `session_areas` 之后、旧字段之前。旧字段暂不删除（双写期）
  - 实际执行: 跳过双写期，直接添加 services 字段并删除旧字段（因 mpsc::Receiver 不可 Clone）
  - 原因: 先建立新字段，确保新旧路径均可编译，降低迁移风险

- [x] 修改 App::new() 构造 ServiceRegistry
  - 位置: `peri-tui/src/app/mod.rs` 的 `App::new()` 方法（~L136-226）
  - 在 `Self { ... }` 构造处，将 20 个服务字段包裹进 `services: ServiceRegistry { peri_config, cwd, provider_name, model_name, permission_mode, thread_store, mcp_pool, mcp_init_rx, cron, plugin_data, bg_event_tx, bg_event_rx, config_path_override, claude_settings_override, setup_wizard, mode_highlight_until, model_highlight_until, mcp_ready_shown_until, quit_pending_since, oauth_prompt }`
  - 实际执行: 直接删除旧字段，App::new() 仅返回 sessions/active/session_areas/services/global_panels
  - 原因: 双写期新旧字段共存，后续逐文件迁移后删除旧字段

- [x] 迁移 mod.rs 中 impl App 方法内的 self.xxx 访问路径
  - 位置: `peri-tui/src/app/mod.rs` 的 `impl App` 块（~L136-556）
  - 逐方法替换 `self.peri_config` → `self.services.peri_config`，`self.cwd` → `self.services.cwd`，`self.provider_name` → `self.services.provider_name`，`self.model_name` → `self.services.model_name`，`self.thread_store` → `self.services.thread_store`，`self.mcp_pool` → `self.services.mcp_pool`，`self.mcp_init_rx` → `self.services.mcp_init_rx`，`self.permission_mode` → `self.services.permission_mode`，`self.config_path_override` → `self.services.config_path_override`
  - 涉及方法: `spawn_mcp_init()`（~L322-364，使用 cwd/bg_event_tx/mcp_pool/mcp_init_rx），`refresh_after_setup()`（~L538-545，使用 peri_config/provider_name/model_name），`get_compact_config()`（~L547-555，使用 peri_config），`new_session()`（~L251-281，使用 cwd/plugin_data）
  - 原因: impl 块内通过 `self.` 访问的字段需统一迁移到 `self.services.`

- [x] 迁移 panel_ops.rs 中 self.xxx 访问路径
  - 位置: `peri-tui/src/app/panel_ops.rs` 的 `impl App` 块
  - 逐方法替换: `self.peri_config` → `self.services.peri_config`，`self.provider_name` → `self.services.provider_name`，`self.model_name` → `self.services.model_name`，`self.config_path_override` → `self.services.config_path_override`，`self.cwd` → `self.services.cwd`，`self.plugin_data` → `self.services.plugin_data`，`self.mcp_pool` → `self.services.mcp_pool`，`self.cron` → `self.services.cron`，`self.bg_event_tx` → `self.services.bg_event_tx`，`self.thread_store` → `self.services.thread_store`，`self.claude_settings_override` → `self.services.claude_settings_override`
  - 关键方法: `open_model_panel()`（peri_config），`model_panel_confirm()`（peri_config/config_path_override/provider_name/model_name），`open_login_panel()`（peri_config），`login_panel_confirm()`（peri_config/config_path_override/provider_name/model_name/cwd），`open_mcp_panel()`（mcp_pool/cron），`mcp_panel_refresh()`（mcp_pool/cron），`open_plugin_panel()`（plugin_data），`plugin_panel_refresh()`（plugin_data），`open_cron_panel()`（cron），`cron_panel_confirm()`（cron），`new_headless()`（全部 20 个字段）
  - 原因: panel_ops.rs 是 service 字段使用最密集的文件之一（26 处），需完整迁移

- [x] 迁移 agent_ops.rs 中 self.xxx 访问路径
  - 位置: `peri-tui/src/app/agent_ops.rs` 的 `impl App` 块
  - 替换: `self.cwd` → `self.services.cwd`（~L98），`self.thread_store` → `self.services.thread_store`（~L169,1196,1211），`self.peri_config` → `self.services.peri_config`（~L171），`self.cron` → `self.services.cron`（~L172），`self.permission_mode` → `self.services.permission_mode`（~L173），`self.mcp_pool` → `self.services.mcp_pool`（~L175），`self.mcp_init_rx` → `self.services.mcp_init_rx`（~L219,225），`self.bg_event_rx` → `self.services.bg_event_rx`（~L1539,1547），`self.bg_event_tx` → `self.services.bg_event_tx`
  - 原因: agent_ops.rs 包含 agent 启动和后台事件轮询逻辑，需要 cwd/peri_config/mcp_pool/mcp_init_rx/bg_event_rx 等

- [x] 迁移 thread_ops.rs 中 self.xxx 访问路径
  - 位置: `peri-tui/src/app/thread_ops.rs` 的 `impl App` 块
  - 替换: `self.cwd` → `self.services.cwd`（~L10,104,167,263,337,348），`self.thread_store` → `self.services.thread_store`（~L11,92,336,355），`self.provider_name` → `self.services.provider_name`（~L168,312）
  - 原因: thread_ops.rs 操作线程存储和线程浏览器，需要 cwd/thread_store/provider_name

- [x] 迁移 cron_ops.rs 中 self.xxx 访问路径
  - 位置: `peri-tui/src/app/cron_ops.rs` 的 `impl App` 块
  - 替换 `self.cron` → `self.services.cron`，`self.peri_config` → `self.services.peri_config`（共 4 处）
  - 原因: cron 操作需要 cron 调度器和 peri_config

- [x] 迁移 message_pipeline.rs 中 self.xxx 访问路径
  - 位置: `peri-tui/src/app/message_pipeline.rs`
  - 替换 `self.cwd` → `self.services.cwd`（共 6 处）
  - 原因: 消息管线使用 cwd 解析相对路径

- [x] 迁移 mcp_panel.rs 中 App 字段访问路径
  - 位置: `peri-tui/src/app/mcp_panel.rs` 的 `impl McpPanel` render 方法
  - 检查 render 方法中直接引用 App 的 `app.mcp_pool` / `app.mcp_init_rx` 等字段，替换为 `app.services.mcp_pool` / `app.services.mcp_init_rx`
  - 原因: mcp_panel 面板渲染通过 `&mut App` 访问 MCP 相关字段

- [x] 迁移 plugin_panel.rs 中 self.xxx 访问路径
  - 位置: `peri-tui/src/app/plugin_panel.rs`
  - 替换 `self.plugin_data` → `self.services.plugin_data`（2 处）
  - 原因: 插件面板读取 plugin_data

- [x] 迁移 event.rs 中 app.xxx 访问路径（66 处，分 3 组处理）
  - 位置: `peri-tui/src/event.rs`
  - 第 1 组——全局弹窗拦截（~L81-434）: 替换 `app.quit_pending_since` → `app.services.quit_pending_since`（~L81,83,423,424,430,434,723），`app.permission_mode` → `app.services.permission_mode`（~L112），`app.mode_highlight_until` → `app.services.mode_highlight_until`（~L113），`app.peri_config` → `app.services.peri_config`（~L124,130），`app.config_path_override` → `app.services.config_path_override`（~L130），`app.provider_name` → `app.services.provider_name`（~L137），`app.model_name` → `app.services.model_name`（~L138），`app.model_highlight_until` → `app.services.model_highlight_until`（~L140），`app.setup_wizard` → `app.services.setup_wizard`（~L188,190,215,733）
  - 第 2 组——PanelContext 构造（~L244-258, 296-308, 757-802 三处重复模式）: 替换 `app.cwd.clone()` → `app.services.cwd.clone()`，`&mut app.peri_config` → `&mut app.services.peri_config`，`app.config_path_override.clone()` → `app.services.config_path_override.clone()`，`app.claude_settings_override.as_ref()` → `app.services.claude_settings_override.as_ref()`，`&mut app.provider_name` → `&mut app.services.provider_name`，`&mut app.model_name` → `&mut app.services.model_name`，`&mut app.mcp_pool` → `&mut app.services.mcp_pool`，`&mut app.cron` → `&mut app.services.cron`，`&mut app.plugin_data` → `&mut app.services.plugin_data`，`&app.bg_event_tx` → `&app.services.bg_event_tx`，`&app.thread_store` → `&app.services.thread_store`
  - 第 3 组——OAuth 弹窗（~L1025-1051）: 替换 `app.oauth_prompt` → `app.services.oauth_prompt`（~L1025,1034,1051）
  - 原因: event.rs 是最大的消费者（66 处），包含 3 个重复的 PanelContext 构造模式

- [x] 迁移 UI 层文件中的 app.xxx 访问路径
  - 位置: `peri-tui/src/ui/main_ui/status_bar.rs`（~L122,135,136,140 替换 `app.mcp_init_rx` → `app.services.mcp_init_rx`，`app.mcp_ready_shown_until` → `app.services.mcp_ready_shown_until`）
  - 位置: `peri-tui/src/ui/main_ui/panels/status.rs`（1 处，替换 `app.peri_config` → `app.services.peri_config`）
  - 位置: `peri-tui/src/ui/main_ui/popups/setup_wizard.rs`（2 处，替换 `app.setup_wizard` → `app.services.setup_wizard`）
  - 位置: `peri-tui/src/ui/main_ui/popups/oauth.rs`（3 处，替换 `app.oauth_prompt` → `app.services.oauth_prompt`）
  - 位置: `peri-tui/src/ui/main_ui.rs`（4 处，替换 `app.setup_wizard` / `app.oauth_prompt` / `app.mcp_pool` 等）
  - 位置: `peri-tui/src/ui/welcome.rs`（2 处，替换 `app.provider_name` / `app.model_name`）
  - 原因: UI 渲染层通过 `&mut App` 参数访问服务字段

- [x] 迁移 headless.rs 测试文件中的 app.xxx 访问路径
  - 位置: `peri-tui/src/ui/headless.rs` 测试模块
  - headless 测试通过 `App::new_headless()` 创建 App，测试代码中直接 `app.` 访问 service 字段的地方逐一替换为 `app.services.xxx`。绝大多数测试通过 `push_agent_event` / `process_pending_events` 间接访问，直接访问较少
  - 原因: headless 测试必须随生产代码同步迁移

- [x] 迁移 main.rs 和 command/ 模块
  - 位置: `peri-tui/src/main.rs`（~L180,184,186,190,201,287 替换 `app.permission_mode` → `app.services.permission_mode`，`app.peri_config` → `app.services.peri_config`，`app.setup_wizard` → `app.services.setup_wizard`，`app.plugin_data` → `app.services.plugin_data`，`app.mcp_pool` → `app.services.mcp_pool`）
  - 位置: `peri-tui/src/command/model.rs`（~L19,21,28,29 替换 `app.peri_config` → `app.services.peri_config`，`app.config_path_override` → `app.services.config_path_override`，`app.provider_name` → `app.services.provider_name`，`app.model_name` → `app.services.model_name`）
  - 位置: `peri-tui/src/command/agents.rs`（~L21 替换 `app.cwd` → `app.services.cwd`）
  - 原因: main.rs 和 command 模块是 App 外部的消费者

- [x] 删除 App 中 20 个旧字段（结束双写期）
  - 位置: `peri-tui/src/app/mod.rs` 的 App 结构体（~L93-134）
  - 删除 `cwd`, `provider_name`, `model_name`, `peri_config`, `thread_store`, `cron`, `setup_wizard`, `permission_mode`, `mode_highlight_until`, `model_highlight_until`, `config_path_override`, `claude_settings_override`, `mcp_pool`, `mcp_init_rx`, `oauth_prompt`, `bg_event_tx`, `bg_event_rx`, `mcp_ready_shown_until`, `plugin_data`, `quit_pending_since` 共 20 个字段
  - 同步删除 App::new() 中对应的旧字段赋值
  - 原因: 双写期结束后清理旧字段，App 从 26 字段降至 7 字段

- [x] 为 ServiceRegistry 编写单元测试
  - 测试文件: `peri-tui/src/app/service_registry.rs` 底部 `#[cfg(test)] mod tests`
  - 测试场景:
    - `test_service_registry_from_app`: 从 `App::new_headless()` 创建 App，验证 `app.services.cwd` 非空、`app.services.provider_name` 可访问、`app.services.setup_wizard` 为 None、`app.services.oauth_prompt` 为 None
    - `test_service_registry_defaults`: 验证 `ServiceRegistry` 中 Option 字段默认为 None（setup_wizard, oauth_prompt, mode_highlight_until, model_highlight_until, quit_pending_since）
    - `test_app_no_old_fields`: 编译期验证——尝试访问 `app.peri_config` 应产生编译错误（通过 `#[cfg(test)]` 注释代码标记）
  - 运行命令: `cargo test -p peri-tui --lib -- service_registry`
  - 预期: 所有测试通过

**检查步骤:**

- [x] 验证 App 旧字段已删除
  - `grep -n "pub cwd:" peri-tui/src/app/mod.rs | head -5`
  - 预期: 无匹配结果（cwd 已移入 ServiceRegistry）
- [x] 验证 App 仅保留 7 个字段
  - `grep -E "^\s+pub [a-z_]+:" peri-tui/src/app/mod.rs | wc -l`
  - 预期: 输出 7（sessions, active, session_areas, services, global_panels，加上可能保留的 2 个）
- [x] 验证无残留的 app.xxx 旧路径访问
  - `grep -rn "app\.peri_config\|app\.cwd\b\|app\.provider_name\|app\.model_name\|app\.permission_mode\|app\.thread_store\|app\.mcp_pool\|app\.mcp_init_rx\|app\.cron\b\|app\.plugin_data\|app\.bg_event_tx\|app\.bg_event_rx\|app\.config_path_override\|app\.claude_settings_override\|app\.setup_wizard\|app\.oauth_prompt\|app\.mode_highlight\|app\.model_highlight\|app\.mcp_ready_shown\|app\.quit_pending" peri-tui/src/ | grep -v "app\.services\." | grep -v "spec-plan"`
  - 预期: 无匹配结果（所有旧路径已迁移到 app.services.xxx）
- [x] 验证无残留的 self.xxx 旧路径访问（app/ 目录内）
  - `grep -rn "self\.peri_config\|self\.cwd\b\|self\.provider_name\|self\.model_name\|self\.permission_mode\|self\.thread_store\|self\.mcp_pool\|self\.mcp_init_rx\|self\.cron\b\|self\.plugin_data\|self\.bg_event_tx\|self\.bg_event_rx\|self\.config_path_override\|self\.claude_settings_override\|self\.setup_wizard\|self\.oauth_prompt\|self\.mode_highlight\|self\.model_highlight\|self\.mcp_ready_shown\|self\.quit_pending" peri-tui/src/app/ | grep -v "self\.services\."`
  - 预期: 无匹配结果
- [x] 验证编译通过
  - `cargo build -p peri-tui 2>&1 | tail -5`
  - 预期: 编译成功，无错误
- [x] 验证 headless 测试通过
  - `cargo test -p peri-tui --lib -- headless 2>&1 | tail -10`
  - 预期: 所有 headless 测试通过
- [x] 验证 ServiceRegistry 单元测试通过
  - `cargo test -p peri-tui --lib -- service_registry 2>&1 | tail -10`
  - 预期: 所有 service_registry 测试通过
- [x] 验证 clippy 无警告
  - `cargo clippy -p peri-tui 2>&1 | grep -E "warning|error" | head -10`
  - 预期: 无新增 warning 或 error

---

### Task 2: 提取 SessionManager

**背景:**
App 中 `sessions`、`active`、`session_areas` 三个字段高度内聚，共同管理多会话切换与渲染。当前 27 个文件中有 421 处 `app.sessions` / 390 处 `app.active` / 2 处 `app.session_areas` 的直接访问，且 `main_ui.rs:render_session_column()` 存在临时切换 `app.active` 的 workaround。将这三个字段提取为 `SessionManager` 结构体，提供 `current()` / `current_mut()` 辅助方法，为后续 Task 3-7 消除 `session.core.xxx` 长路径奠定基础。
依赖 Task 1（ServiceRegistry 提取）完成，本 Task 的输出被 Task 3（UiState 提取）和 Task 8（消除 AppCore）依赖。

**涉及文件:**

- 新建: `peri-tui/src/app/session_manager.rs`
- 修改: `peri-tui/src/app/mod.rs`, `peri-tui/src/event.rs`, `peri-tui/src/ui/main_ui.rs`, `peri-tui/src/ui/headless.rs`, `peri-tui/src/main.rs`, `peri-tui/src/app/agent_ops.rs`, `peri-tui/src/app/hint_ops.rs`, `peri-tui/src/app/panel_ops.rs`, `peri-tui/src/command/model.rs`, `peri-tui/src/command/agent.rs`, `peri-tui/src/command/help.rs`, `peri-tui/src/command/history.rs`, `peri-tui/src/command/loop_cmd.rs`, `peri-tui/src/command/compact.rs`, `peri-tui/src/ui/welcome.rs`, `peri-tui/src/ui/main_ui/popups/hints.rs`, `peri-tui/src/ui/main_ui/popups/hitl.rs`, `peri-tui/src/ui/main_ui/popups/ask_user.rs`, `peri-tui/src/ui/main_ui/status_bar.rs`, `peri-tui/src/ui/main_ui/sticky_header.rs`, `peri-tui/src/ui/main_ui/panels/mcp.rs`, `peri-tui/src/ui/main_ui/panels/agent.rs`, `peri-tui/src/ui/main_ui/panels/hooks.rs`, `peri-tui/src/ui/main_ui/panels/status.rs`, `peri-tui/src/ui/main_ui/panels/thread_browser.rs`, `peri-tui/src/ui/main_ui/panels/cron.rs`, `peri-tui/src/ui/main_ui/panels/plugin.rs`, `peri-tui/src/ui/main_ui/panels/model.rs`, `peri-tui/src/ui/main_ui/panels/memory.rs`, `peri-tui/src/ui/main_ui/panels/login.rs`

**执行步骤:**

- [x] 创建 `SessionManager` 结构体，定义字段 + 辅助方法
  - 位置: 新建 `peri-tui/src/app/session_manager.rs`
  - 包含 3 个字段: `sessions: Vec<ChatSession>`, `active: usize`, `session_areas: Vec<Rect>`
  - 辅助方法:

    ```rust
    impl SessionManager {
        pub fn new(initial_session: ChatSession) -> Self {
            Self { sessions: vec![initial_session], active: 0, session_areas: Vec::new() }
        }
        pub fn current(&self) -> &ChatSession { &self.sessions[self.active] }
        pub fn current_mut(&mut self) -> &mut ChatSession { &mut self.sessions[self.active] }
        pub fn session_at(&self, idx: usize) -> Option<&ChatSession> { self.sessions.get(idx) }
        pub fn session_at_mut(&mut self, idx: usize) -> Option<&mut ChatSession> { self.sessions.get_mut(idx) }
        pub fn len(&self) -> usize { self.sessions.len() }
        pub fn is_empty(&self) -> bool { self.sessions.is_empty() }
    }
    ```

  - 原因: 将 3 个内聚字段封装为一个语义清晰的结构体，`current()` 消除 `app.sessions[app.active]` 散弹式访问

- [x] 在 `app/mod.rs` 中注册模块、添加 `session_mgr` 字段（双写期）
  - 位置: `peri-tui/src/app/mod.rs` — 文件顶部 `mod` 声明区（~L18 后）追加 `mod session_manager;`
  - 位置: `peri-tui/src/app/mod.rs` — `App` 结构体（~L93），在 `session_areas` 字段后追加:

    ```rust
    pub session_mgr: session_manager::SessionManager,
    ```

  - 位置: `peri-tui/src/app/mod.rs` — `App::new()` 返回值（~L198），在 `Self { ... }` 中追加:

    ```rust
    session_mgr: session_manager::SessionManager::new(initial_session.clone()),
    ```

    注意: `initial_session` 已被 `sessions` 字段 move，需在 `sessions` 初始化前 clone 或在 `session_mgr` 中使用同一实例。正确做法: 先构建 `session_mgr`，再让 `sessions` 引用同一数据（双写期两份独立副本，后续步骤删除旧字段）。
  - 位置: `peri-tui/src/app/mod.rs` — re-export 区（~L80）追加:

    ```rust
    pub use session_manager::SessionManager;
    ```

  - 原因: 双写期新旧字段共存，确保编译通过后再逐文件迁移

- [x] 迁移 `app/mod.rs` 中 `App` 的 session 管理方法
  - 位置: `peri-tui/src/app/mod.rs` — `active()` (~L231), `active_mut()` (~L236), `session_at()` (~L241), `session_at_mut()` (~L246)
  - 改为委托到 `session_mgr`:

    ```rust
    pub fn active(&self) -> &ChatSession { self.session_mgr.current() }
    pub fn active_mut(&mut self) -> &mut ChatSession { self.session_mgr.current_mut() }
    pub fn session_at(&self, idx: usize) -> Option<&ChatSession> { self.session_mgr.session_at(idx) }
    pub fn session_at_mut(&mut self, idx: usize) -> Option<&mut ChatSession> { self.session_mgr.session_at_mut(idx) }
    ```

  - 位置: `peri-tui/src/app/mod.rs` — `new_session()` (~L251), `close_session()` (~L284), `switch_next_session()` (~L302), `switch_prev_session()` (~L309)
  - 将这些方法内部的 `self.sessions` / `self.active` 全部替换为 `self.session_mgr.sessions` / `self.session_mgr.active`，保持旧字段同步（双写期）
  - 位置: `peri-tui/src/app/mod.rs` — `interrupt()` (~L380) 中所有 `self.sessions[self.active]` 替换为 `self.session_mgr.sessions[self.session_mgr.active]`，旧字段同步写入
  - 原因: 方法迁移先行，确保 `App` 的公共 API 通过委托保持一致，外部调用者无需改动

- [x] 迁移 `agent_ops.rs` 中 31 处 `self.sessions[self.active]` 访问
  - 位置: `peri-tui/src/app/agent_ops.rs` — 全文件
  - 替换规则: `self.sessions[self.active]` → `self.session_mgr.sessions[self.session_mgr.active]`，同时保持旧字段 `self.sessions[self.active]` 同步写入（双写期仅在可变操作处需要同步）
  - 由于 `agent_ops.rs` 是 `impl App` 的方法，`self` 是 `&mut App`，直接通过 `self.session_mgr.current_mut()` 访问更简洁
  - 关键替换: `self.sessions[self.active].core.xxx` → `self.session_mgr.current_mut().core.xxx`
  - 关键替换: `self.sessions[self.active].agent.xxx` → `self.session_mgr.current_mut().agent.xxx`
  - 原因: `agent_ops.rs` 是 `app.sessions[app.active]` 的高频使用文件，全部通过 `session_mgr` 路由

- [x] 迁移 `hint_ops.rs` 中 28 处 `self.sessions[self.active]` 访问
  - 位置: `peri-tui/src/app/hint_ops.rs` — `impl App` 内全部方法
  - 替换规则同上: `self.sessions[self.active].core.textarea` → `self.session_mgr.current_mut().core.textarea`
  - 原因: hint_ops 是 `app.active` 第二高频使用文件

- [x] 迁移 `event.rs` 中 118 处 `app.sessions` / `app.active` 访问
  - 位置: `peri-tui/src/event.rs` — 全文件
  - 替换规则:
    - `app.sessions[app.active].core.xxx` → `app.session_mgr.current_mut().core.xxx`
    - `app.sessions[app.active].agent.xxx` → `app.session_mgr.current_mut().agent.xxx`
    - `app.active = idx` → `app.session_mgr.active = idx; app.active = idx;`（双写期同步）
    - `app.sessions.len()` → `app.session_mgr.len()`（只读操作无需同步）
    - `app.sessions.iter()` / `app.sessions.iter_mut()` → `app.session_mgr.sessions.iter()` / `.iter_mut()`
    - `app.sessions.push(x)` → `app.session_mgr.sessions.push(x.clone()); app.sessions.push(x);`
    - `app.sessions.remove(idx)` → 同步操作
  - 对于 `&mut App` 参数的函数: 优先使用 `app.session_mgr.current_mut()` 获取可变引用
  - 对于 `&App` 参数的函数: 使用 `app.session_mgr.current()` 获取不可变引用
  - 原因: event.rs 是访问量最大的文件（118 处），是本次重构的核心战场

- [x] 迁移 `main_ui.rs` 中 48 处 `app.sessions` / `app.active` 访问，消除临时 active 交换 workaround
  - 位置: `peri-tui/src/ui/main_ui.rs` — `render_session_column()` (~L63)
  - **核心改动**: 将 `render_session_column` 签名从 `app: &mut App` 改为同时传入 `session_mgr: &SessionManager` 和 `session_idx: usize`，函数内所有 `app.sessions[app.active]` 改为 `session_mgr.sessions[session_idx]`
  - 删除 L71-72 的 `prev_active = app.active; app.active = session_idx;` 及恢复代码
  - 位置: `peri-tui/src/ui/main_ui.rs` — `render_messages()` (~L335), `render_attachment_bar()` (~L559), `active_panel_height()` (~L270)
  - 这些函数当前通过 `app.sessions[app.active]` 访问活跃 session，改为接收 `(app: &mut App, session_mgr: &SessionManager, session_idx: usize)` 参数，内部用 `session_mgr.sessions[session_idx]`
  - 位置: `peri-tui/src/ui/main_ui.rs` — `render()` (~L19) 中 `app.session_areas = cols...` 改为 `app.session_mgr.session_areas = cols...`
  - 原因: 消除 `main_ui.rs` 中最危险的 workaround（临时切换全局 `active` 索引），这是本 Task 的重要设计目标

- [x] 迁移 `headless.rs` 中 109 处 `app.sessions` / `app.active` 访问
  - 位置: `peri-tui/src/ui/headless.rs` — 测试函数中所有 `app.sessions[app.active]`
  - 替换规则: `app.sessions[app.active].core.xxx` → `app.session_mgr.current_mut().core.xxx`
  - 只读场景用 `app.session_mgr.current().core.xxx`
  - 原因: 测试代码同样需要迁移以保持编译通过

- [x] 迁移 `main.rs` 中 10 处 `app.sessions` / `app.active` 访问
  - 位置: `peri-tui/src/main.rs` — 主循环中 spinner tick、render poll 等
  - 替换规则: `app.sessions[app.active]` → `app.session_mgr.current_mut()`，遍历场景用 `app.session_mgr.sessions.iter_mut()`
  - 原因: main.rs 中也存在临时 active 交换（~L247），同样需要消除

- [x] 迁移 `panel_ops.rs` 中 `new_headless()` 的字段初始化
  - 位置: `peri-tui/src/app/panel_ops.rs` — `new_headless()` (~L1088)
  - 在 `App { ... }` 初始化中追加 `session_mgr: super::SessionManager::new(session.clone())`（注意 `session` 已被 `sessions` 字段 move，需先构建 `session_mgr`）
  - 正确做法: `let session_mgr = super::SessionManager::new(/* 使用同一 ChatSession 实例 */);` 然后 `sessions: session_mgr.sessions.clone()`
  - 原因: headless 测试工厂必须同步创建 `session_mgr`

- [x] 迁移 command/ 目录下 6 个文件中的 `app.sessions[app.active]` 访问
  - 位置: `peri-tui/src/command/model.rs`, `agent.rs`, `help.rs`, `history.rs`, `loop_cmd.rs`, `compact.rs`
  - 替换规则: `app.sessions[app.active]` → `app.session_mgr.current()`（只读）或 `app.session_mgr.current_mut()`（可变）
  - 原因: command 文件通过 `&mut App` 访问 session，迁移路径一致

- [x] 迁移 ui/main_ui/panels/ 下 11 个面板文件中的 `app.sessions[app.active]` 访问
  - 位置: `peri-tui/src/ui/main_ui/panels/` 下所有 `*.rs` 文件
  - 替换规则: 面板 render 函数中 `app.sessions[app.active].core.xxx` → `app.session_mgr.current().core.xxx`
  - 原因: 面板文件统一使用 `&App` 参数，迁移为只读访问

- [x] 迁移 ui/main_ui/popups/ 和 ui/main_ui/status_bar.rs, sticky_header.rs, welcome.rs
  - 位置: `peri-tui/src/ui/main_ui/popups/{hints,hitl,ask_user}.rs`, `status_bar.rs`, `sticky_header.rs`, `ui/welcome.rs`
  - 替换规则同上
  - 原因: 覆盖所有使用 `app.sessions[app.active]` 的 UI 文件

- [x] 删除 App 中旧字段 `sessions`, `active`, `session_areas`
  - 位置: `peri-tui/src/app/mod.rs` — `App` 结构体定义（~L95-L99）
  - 删除 `pub sessions: Vec<ChatSession>`, `pub active: usize`, `pub session_areas: Vec<Rect>` 三行
  - 删除所有双写期同步代码（步骤 3-12 中添加的旧字段同步写入）
  - 全项目搜索确认无残留: `app.sessions` / `app.active` / `app.session_areas`（不含 `session_mgr.` 前缀的）
  - 原因: 双写期结束，旧字段彻底移除

- [x] 为 SessionManager 核心逻辑编写单元测试
  - 测试文件: `peri-tui/src/app/session_manager.rs`（模块内 `#[cfg(test)] mod tests`）
  - 测试场景:
    - `new()`: 初始状态 → `sessions.len() == 1`, `active == 0`
    - `current()`: 返回 `sessions[active]` 的引用 → 验证地址一致
    - `current_mut()`: 可变访问 → 修改后通过 `current()` 可见
    - `session_at()`: 有效索引 → `Some`；越界索引 → `None`
    - `session_at_mut()`: 同上可变版本
  - 运行命令: `cargo test -p peri-tui --lib -- session_manager`
  - 预期: 所有测试通过

**检查步骤:**

- [x] 验证编译通过
  - `cargo build -p peri-tui 2>&1 | tail -5`
  - 预期: 编译成功，无错误
- [x] 验证无残留旧路径访问（不含 session_mgr 前缀）
  - `grep -rn 'app\.sessions\b' peri-tui/src/ | grep -v 'session_mgr' | grep -v '//.*app\.sessions' | wc -l`
  - 预期: 输出 0
- [x] 验证无残留 `app.active` 直接赋值（不含 session_mgr 前缀）
  - `grep -rn 'app\.active\s*=' peri-tui/src/ | grep -v 'session_mgr' | wc -l`
  - 预期: 输出 0
- [x] 验证无残留 `app.session_areas` 直接访问
  - `grep -rn 'app\.session_areas' peri-tui/src/ | grep -v 'session_mgr' | wc -l`
  - 预期: 输出 0
- [x] 验证 SessionManager 单元测试通过
  - `cargo test -p peri-tui --lib -- session_manager`
  - 预期: 所有测试通过
- [x] 验证 headless 测试通过
  - `cargo test -p peri-tui --lib -- ui::headless`
  - 预期: 所有测试通过
- [x] 验证 clippy 无警告
  - `cargo clippy -p peri-tui 2>&1 | grep -E 'warning|error' | head -10`
  - 预期: 无新增 warning

---

### Task 3: 提取 UiState

**背景:**
[业务语境] — 将 AppCore 中 18 个 UI 相关字段（textarea/scroll/selection/copy/history 等）提取到独立的 `UiState` 结构体，每个 ChatSession 拥有独立的 UiState 实例
[修改原因] — 当前 AppCore 混合了 UI 状态、消息管线、命令系统、会话元数据四种职责，textarea/scroll_offset 等 UI 字段与 view_messages/pipeline 等消息字段耦合在同一结构体中，导致 `event.rs` 中借用冲突频发
[上下游影响] — 依赖 Task 2（SessionManager）完成。UiState 提取后，Task 4（MessageState）/ Task 5（CommandSystem）/ Task 6（SessionMetadata）将处理 AppCore 剩余字段

**涉及文件:**

- 新建: `peri-tui/src/app/ui_state.rs`
- 修改: `peri-tui/src/app/core.rs`, `peri-tui/src/app/chat_session.rs`, `peri-tui/src/app/mod.rs`, `peri-tui/src/event.rs`, `peri-tui/src/ui/main_ui.rs`, `peri-tui/src/ui/headless.rs`, `peri-tui/src/app/panel_ops.rs`, `peri-tui/src/app/agent_ops.rs`, `peri-tui/src/app/hint_ops.rs`, `peri-tui/src/app/history_ops.rs`, `peri-tui/src/app/thread_ops.rs`, `peri-tui/src/app/mcp_panel.rs`, `peri-tui/src/app/agent_panel.rs`, `peri-tui/src/app/hooks_panel.rs`, `peri-tui/src/app/plugin_panel.rs`, `peri-tui/src/app/ask_user_ops.rs`, `peri-tui/src/app/cron_ops.rs`, `peri-tui/src/command/history.rs`, `peri-tui/src/command/plugin_command.rs`, `peri-tui/src/command/compact.rs`, `peri-tui/src/main.rs`, `peri-tui/src/ui/main_ui/status_bar.rs`, `peri-tui/src/ui/main_ui/popups/hints.rs`, `peri-tui/src/ui/main_ui/popups/ask_user.rs`, `peri-tui/src/ui/main_ui/panels/mcp.rs`, `peri-tui/src/ui/main_ui/panels/agent.rs`, `peri-tui/src/ui/main_ui/panels/hooks.rs`, `peri-tui/src/ui/main_ui/panels/memory.rs`, `peri-tui/src/ui/main_ui/panels/thread_browser.rs`, `peri-tui/src/ui/main_ui/panels/cron.rs`, `peri-tui/src/ui/main_ui/panels/plugin.rs`, `peri-tui/src/ui/render_thread.rs`

**执行步骤:**

- [x] 创建 `peri-tui/src/app/ui_state.rs`，定义 UiState 结构体及构造方法
  - 位置: 新建 `peri-tui/src/app/ui_state.rs`
  - 包含 18 个字段（从 AppCore 提取）:

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
        pub text_selection: crate::app::text_selection::TextSelection,
        pub messages_area: Option<ratatui::layout::Rect>,
        pub textarea_area: Option<ratatui::layout::Rect>,
        pub copy_message_until: Option<std::time::Instant>,
        pub copy_char_count: usize,
        pub panel_selection: crate::app::text_selection::PanelTextSelection,
        pub panel_area: Option<ratatui::layout::Rect>,
        pub panel_plain_lines: Vec<String>,
        pub panel_scroll_offset: u16,
    }
    ```

  - 实现 `UiState::new(textarea: TextArea<'static>) -> Self`，默认值: loading=false, scroll_offset=u16::MAX, scroll_follow=true, show_tool_messages=false, hint_cursor=None, input_history=Vec::new(), history_index=None, draft_input=None, text_selection=TextSelection::new(), messages_area=None, textarea_area=None, copy_message_until=None, copy_char_count=0, panel_selection=PanelTextSelection::new(), panel_area=None, panel_plain_lines=Vec::new(), panel_scroll_offset=0
  - 原因: 18 个 UI 字段属于同一职责域（会话级 UI 交互状态），集中后 event.rs 中对 textarea 和 scroll 的操作可独立于 view_messages/pipeline 借用

- [x] 在 `peri-tui/src/app/mod.rs` 中注册模块
  - 位置: `peri-tui/src/app/mod.rs` 模块声明区域（~L15 后），在 `pub mod text_selection;` 之后
  - 添加: `pub mod ui_state;`
  - 原因: ui_state.rs 需作为 app 子模块可见

- [x] 在 ChatSession 中新增 `ui: UiState` 字段（双写阶段）
  - 位置: `peri-tui/src/app/chat_session.rs` ChatSession 结构体（~L11）
  - 在 `pub core: AppCore,` 之前添加 `pub ui: UiState,`
  - 在 `ChatSession::new()` 中，`core: AppCore::new(...)` 之前添加 `ui: UiState::new(super::build_textarea(false)),`
  - 原因: 双写过渡——先添加新字段，旧字段暂不删除，保证编译不中断

- [x] 在 `panel_ops.rs` 的 `new_headless()` 中初始化 `ui` 字段
  - 位置: `peri-tui/src/app/panel_ops.rs` ChatSession 构造（~L1076）
  - 在 `core,` 之前添加 `ui: super::UiState::new(super::build_textarea(false)),`
  - 原因: headless 测试工厂必须同步创建 ui 字段

- [x] 迁移 `event.rs` 中所有 UiState 字段访问
  - 位置: `peri-tui/src/event.rs` 全文件
  - 替换规则（Task 2 完成后路径为 session_mgr 路由）:
    - `.core.textarea` → `.ui.textarea`
    - `.core.loading` → `.ui.loading`
    - `.core.scroll_offset` → `.ui.scroll_offset`
    - `.core.scroll_follow` → `.ui.scroll_follow`
    - `.core.show_tool_messages` → `.ui.show_tool_messages`
    - `.core.hint_cursor` → `.ui.hint_cursor`
    - `.core.input_history` → `.ui.input_history`
    - `.core.history_index` → `.ui.history_index`
    - `.core.draft_input` → `.ui.draft_input`
    - `.core.text_selection` → `.ui.text_selection`
    - `.core.messages_area` → `.ui.messages_area`
    - `.core.textarea_area` → `.ui.textarea_area`
    - `.core.copy_message_until` → `.ui.copy_message_until`
    - `.core.copy_char_count` → `.ui.copy_char_count`
    - `.core.panel_selection` → `.ui.panel_selection`
    - `.core.panel_area` → `.ui.panel_area`
    - `.core.panel_plain_lines` → `.ui.panel_plain_lines`
    - `.core.panel_scroll_offset` → `.ui.panel_scroll_offset`
  - 注意: 仅替换以上 18 个字段，不动 `core.view_messages`, `core.session_panels`, `core.command_registry`, `core.skills`, `core.last_human_message`, `core.round_start_vm_idx`, `core.pipeline`, `core.render_cache`, `core.pending_messages`, `core.last_submitted_text`, `core.pre_submit_state_len`, `core.pending_attachments` 等（属于后续 Task 4-6）
  - 原因: event.rs 是 UiState 字段最高频访问文件，textarea/scroll/selection/copy/history 操作全部在此

- [x] 迁移 `main_ui.rs` 中 16 处 UiState 字段访问
  - 位置: `peri-tui/src/ui/main_ui.rs` 全文件
  - 替换所有 `.core.textarea` / `.core.messages_area` / `.core.textarea_area` / `.core.scroll_offset` / `.core.scroll_follow` / `.core.loading` / `.core.panel_area` / `.core.panel_plain_lines` / `.core.panel_scroll_offset` / `.core.panel_selection` / `.core.show_tool_messages` 为 `.ui.` 前缀
  - 原因: main_ui.rs 渲染管道需要读取 textarea 内容、渲染区域 Rect、滚动偏移等 UI 状态

- [x] 迁移 `headless.rs` 中 UiState 字段访问
  - 位置: `peri-tui/src/ui/headless.rs` 测试函数
  - 仅替换 UiState 字段: `core.textarea` → `ui.textarea`, `core.loading` → `ui.loading`
  - 不动: `core.view_messages`, `core.session_panels`, `core.command_registry`, `core.skills`, `core.last_human_message`, `core.round_start_vm_idx`, `core.pipeline`, `core.render_cache` 等（属于后续 Task 4-6 的字段）
  - 原因: headless 测试大量直接设置 textarea 和 loading 状态

- [x] 迁移 `panel_ops.rs` 中 8 处 UiState 字段访问
  - 位置: `peri-tui/src/app/panel_ops.rs` 全文件
  - 替换所有 `core.textarea` / `core.loading` / `core.scroll_offset` / `core.scroll_follow` / `core.show_tool_messages` / `core.hint_cursor` 为 `ui.` 前缀
  - 原因: panel_ops.rs 包含面板操作中对 UI 状态的访问

- [x] 迁移 app/ 子模块中 UiState 字段访问（agent_ops, hint_ops, history_ops, thread_ops, mcp_panel, agent_panel, hooks_panel, plugin_panel, ask_user_ops, cron_ops）
  - 位置: 各文件中所有 `core.<ui_field>` 访问
  - agent_ops.rs（5 处）: `core.textarea` → `ui.textarea`
  - hint_ops.rs（26 处）: `core.hint_cursor` / `core.textarea` → `ui.` 前缀
  - history_ops.rs（24 处）: `core.input_history` / `core.history_index` / `core.draft_input` / `core.textarea` → `ui.` 前缀
  - thread_ops.rs（9 处）: `core.textarea` / `core.loading` → `ui.` 前缀
  - mcp_panel.rs（8 处）: `core.panel_selection` / `core.panel_area` / `core.panel_plain_lines` / `core.panel_scroll_offset` → `ui.` 前缀
  - agent_panel.rs（4 处）: 同上 panel_selection/panel_area/panel_plain_lines/panel_scroll_offset → `ui.` 前缀
  - hooks_panel.rs（4 处）: 同上 → `ui.` 前缀
  - plugin_panel.rs（16 处）: 同上 → `ui.` 前缀
  - ask_user_ops.rs（1 处）: `core.textarea` → `ui.textarea`
  - cron_ops.rs（2 处）: `core.textarea` → `ui.textarea`
  - 原因: 各子模块操作不同子集的 UI 状态字段，需逐一确认并替换

- [x] 迁移 command/ 子模块和 main.rs 中 UiState 字段访问
  - 位置:
    - `peri-tui/src/command/history.rs`（1 处）: `core.textarea` → `ui.textarea`
    - `peri-tui/src/command/plugin_command.rs`（1 处）: `core.textarea` → `ui.textarea`
    - `peri-tui/src/command/compact.rs`（1 处）: `core.textarea` → `ui.textarea`
    - `peri-tui/src/main.rs`（1 处）: `core.textarea` → `ui.textarea`
  - 原因: command 子模块和 main.rs 中少量直接操作 textarea

- [x] 迁移 ui/main_ui/ 子目录中 UiState 字段访问（status_bar, popups, panels）
  - 位置:
    - `peri-tui/src/ui/main_ui/status_bar.rs`（3 处）: `core.loading` / `core.show_tool_messages` → `ui.` 前缀
    - `peri-tui/src/ui/main_ui/popups/hints.rs`（3 处）: `core.hint_cursor` / `core.textarea` → `ui.` 前缀
    - `peri-tui/src/ui/main_ui/popups/ask_user.rs`（1 处）: `core.textarea` → `ui.textarea`
    - `peri-tui/src/ui/main_ui/panels/mcp.rs`（10 处）: `core.panel_selection` / `core.panel_area` / `core.panel_plain_lines` / `core.panel_scroll_offset` → `ui.` 前缀
    - `peri-tui/src/ui/main_ui/panels/agent.rs`（6 处）: 同上 → `ui.` 前缀
    - `peri-tui/src/ui/main_ui/panels/hooks.rs`（4 处）: 同上 → `ui.` 前缀
    - `peri-tui/src/ui/main_ui/panels/memory.rs`（4 处）: 同上 → `ui.` 前缀
    - `peri-tui/src/ui/main_ui/panels/thread_browser.rs`（8 处）: 同上 → `ui.` 前缀
    - `peri-tui/src/ui/main_ui/panels/cron.rs`（7 处）: 同上 → `ui.` 前缀
    - `peri-tui/src/ui/main_ui/panels/plugin.rs`（17 处）: 同上 → `ui.` 前缀
  - 原因: UI 渲染子模块中对面板选区和 textarea 的访问需统一迁移

- [x] 检查并迁移 `render_thread.rs` 中可能存在的 UiState 字段访问
  - 位置: `peri-tui/src/ui/render_thread.rs`（1 处）
  - 确认该处访问是否为 UiState 字段，仅替换 UiState 相关字段
  - 原因: 渲染线程可能涉及 textarea 区域计算

- [x] 删除 AppCore 中的 18 个 UiState 字段及其初始化
  - 位置: `peri-tui/src/app/core.rs` AppCore 结构体定义（~L16-64）
  - 从 AppCore 中删除以下字段: textarea, loading, scroll_offset, scroll_follow, show_tool_messages, hint_cursor, input_history, history_index, draft_input, text_selection, messages_area, textarea_area, copy_message_until, copy_char_count, panel_selection, panel_area, panel_plain_lines, panel_scroll_offset
  - 从 AppCore::new() 的 `Self { ... }` 初始化块中删除对应 18 行
  - AppCore::new() 参数中 `cwd: String` 保留（pipeline 仍需使用）
  - 清理 AppCore::new() 中不再需要的 `super::build_textarea(false)` 调用及 `tui_textarea::TextArea` 相关 import
  - 原因: 双写阶段结束后，AppCore 中这些字段已无引用，必须删除以避免数据不一致

- [x] 为 UiState 编写单元测试
  - 测试文件: `peri-tui/src/app/ui_state.rs`（模块内 `#[cfg(test)] mod tests`）
  - 测试场景:
    - 默认值测试: `UiState::new(textarea)` 构造后 loading=false, scroll_offset=u16::MAX, scroll_follow=true, hint_cursor=None, input_history 为空, history_index=None, draft_input=None, messages_area=None, textarea_area=None, copy_message_until=None, copy_char_count=0, panel_plain_lines 为空, panel_scroll_offset=0
    - textarea 初始状态: 构造后 textarea.lines() 为空
    - text_selection 初始状态: 构造后 text_selection.is_active() 为 false, panel_selection.is_active() 为 false
  - 运行命令: `cargo test -p peri-tui --lib -- ui_state::tests`
  - 预期: 所有测试通过

**检查步骤:**

- [x] 验证 UiState 结构体存在且包含 18 个字段
  - `grep -c "pub " peri-tui/src/app/ui_state.rs`
  - 预期: 大于等于 19（18 个字段 + 1 个 new 方法）
- [x] 验证 AppCore 不再包含 UiState 字段
  - `grep -E "pub (textarea|loading|scroll_offset|scroll_follow|show_tool_messages|hint_cursor|input_history|history_index|draft_input|text_selection|messages_area|textarea_area|copy_message_until|copy_char_count|panel_selection|panel_area|panel_plain_lines|panel_scroll_offset)" peri-tui/src/app/core.rs`
  - 预期: 0 行输出
- [x] 验证 event.rs 中无 `core.` 前缀的 UiState 字段访问
  - `grep -E "core\.(textarea|loading|scroll_offset|scroll_follow|show_tool_messages|hint_cursor|input_history|history_index|draft_input|text_selection|messages_area|textarea_area|copy_message_until|copy_char_count|panel_selection|panel_area|panel_plain_lines|panel_scroll_offset)" peri-tui/src/event.rs`
  - 预期: 0 行输出
- [x] 验证全项目无残留的 UiState 字段通过 core 访问
  - `grep -rn "core\.\(textarea\|loading\|scroll_offset\|scroll_follow\|show_tool_messages\|hint_cursor\|input_history\|history_index\|draft_input\|text_selection\|messages_area\|textarea_area\|copy_message_until\|copy_char_count\|panel_selection\|panel_area\|panel_plain_lines\|panel_scroll_offset\)" peri-tui/src/`
  - 预期: 0 行输出
- [x] 验证编译通过
  - `cargo build -p peri-tui 2>&1 | tail -5`
  - 预期: 构建成功，无错误
- [x] 验证 clippy 无新增警告
  - `cargo clippy -p peri-tui 2>&1 | grep -E 'warning|error' | head -10`
  - 预期: 无新增 warning
- [x] 验证全部 headless 测试通过
  - `cargo test -p peri-tui --lib -- ui::headless::tests 2>&1 | tail -20`
  - 预期: 所有测试通过
- [x] 验证 UiState 单元测试通过
  - `cargo test -p peri-tui --lib -- ui_state::tests 2>&1 | tail -10`
  - 预期: 所有测试通过

---

### Task 4: 提取 MessageState

**背景:**
[业务语境] — 将 AppCore 中 9 个消息相关字段（view_messages/pipeline/render_tx/render_cache/render_notify/last_render_version/pending_messages/last_submitted_text/round_start_vm_idx）提取到独立的 `MessageState` 结构体，每个 ChatSession 拥有独立的 MessageState 实例
[修改原因] — 当前 AppCore 中消息管线字段（view_messages/pipeline）与 UI 字段（textarea/scroll）耦合在同一结构体中，导致 agent_ops.rs 中 submit_message() 和 poll_agent() 对 view_messages/pipeline 的高频读写与 event.rs 中 textarea 操作产生不必要的借用冲突。提取后 view_messages/pipeline 可独立于 UiState 借用
[上下游影响] — 依赖 Task 3（UiState 提取）完成。MessageState 提取后，Task 5（CommandSystem）/ Task 6（SessionMetadata）将处理 AppCore 剩余字段，最终 Task 7（消除 AppCore）删除 AppCore 结构体

**涉及文件:**

- 新建: `peri-tui/src/app/message_state.rs`
- 修改: `peri-tui/src/app/core.rs`, `peri-tui/src/app/chat_session.rs`, `peri-tui/src/app/mod.rs`, `peri-tui/src/app/agent_ops.rs`, `peri-tui/src/app/thread_ops.rs`, `peri-tui/src/app/panel_ops.rs`, `peri-tui/src/event.rs`, `peri-tui/src/ui/main_ui.rs`, `peri-tui/src/ui/headless.rs`, `peri-tui/src/main.rs`, `peri-tui/src/command/loop_cmd.rs`, `peri-tui/src/command/agent.rs`, `peri-tui/src/command/help.rs`, `peri-tui/src/app/login_panel.rs`, `peri-tui/src/app/plugin_panel.rs`, `peri-tui/src/app/agent_panel.rs`, `peri-tui/src/app/config_panel.rs`, `peri-tui/src/app/cron_state.rs`, `peri-tui/src/app/cron_ops.rs`, `peri-tui/src/thread/browser.rs`

**执行步骤:**

- [x] 创建 `peri-tui/src/app/message_state.rs`，定义 MessageState 结构体及构造方法
  - 位置: 新建 `peri-tui/src/app/message_state.rs`
  - 包含 9 个字段（从 AppCore 提取）:

    ```rust
    use std::sync::Arc;
    use parking_lot::RwLock;
    use tokio::sync::{mpsc, Notify};
    use super::message_pipeline::MessagePipeline;
    use crate::ui::message_view::MessageViewModel;
    use crate::ui::render_thread::{RenderCache, RenderEvent};

    pub struct MessageState {
        pub view_messages: Vec<MessageViewModel>,
        pub round_start_vm_idx: usize,
        pub pipeline: MessagePipeline,
        pub render_tx: mpsc::UnboundedSender<RenderEvent>,
        pub render_cache: Arc<RwLock<RenderCache>>,
        pub render_notify: Arc<Notify>,
        pub last_render_version: u64,
        pub pending_messages: Vec<String>,
        /// 最近一次提交的用户文本（用于 Ctrl+C 中断时恢复到输入框）
        pub last_submitted_text: Option<String>,
    }
    ```

  - 实现 `MessageState::new(cwd, render_tx, render_cache, render_notify) -> Self`，默认值: view_messages=Vec::new(), round_start_vm_idx=0, pipeline=MessagePipeline::new(cwd), last_render_version=0, pending_messages=Vec::new(), last_submitted_text=None
  - 原因: 9 个消息管线字段属于同一职责域（会话级消息渲染和管线状态），集中后 agent_ops.rs 中 submit_message/poll_agent 可独立于 UiState 操作 view_messages/pipeline

- [x] 在 `peri-tui/src/app/mod.rs` 中注册模块
  - 位置: `peri-tui/src/app/mod.rs` 模块声明区域，在 `pub mod ui_state;` 之后
  - 添加: `pub mod message_state;` 和 `pub use message_state::MessageState;`
  - 原因: message_state.rs 需作为 app 子模块可见

- [x] 在 ChatSession 中新增 `messages: MessageState` 字段（双写阶段）
  - 位置: `peri-tui/src/app/chat_session.rs` ChatSession 结构体（~L11）
  - 在 `pub core: AppCore,` 之前添加 `pub messages: MessageState,`
  - 位置: `peri-tui/src/app/chat_session.rs` ChatSession::new()（~L23）
  - 修改构造逻辑: 将 `crate::ui::render_thread::spawn_render_thread(80)` 返回的 render_tx/render_cache/render_notify 先用于创建 MessageState，再传入 AppCore::new()
  - 具体改动:

    ```rust
    let (render_tx, render_cache, render_notify) =
        crate::ui::render_thread::spawn_render_thread(80);
    let messages = MessageState::new(
        cwd.clone(),
        render_tx.clone(),
        Arc::clone(&render_cache),
        Arc::clone(&render_notify),
    );
    Self {
        messages,
        core: AppCore::new(cwd, render_tx, render_cache, render_notify, command_registry, skills),
        // ... 其余字段不变
    }
    ```

  - 注意: render_tx 是 `mpsc::UnboundedSender`，需要 `.clone()`；render_cache 是 `Arc<RwLock<>>`，需要 `Arc::clone()`；render_notify 是 `Arc<Notify>`，需要 `Arc::clone()`
  - 原因: 双写过渡——先添加新字段，旧字段暂不删除。render 相关字段需要 clone 共享所有权（UnboundedSender 支持克隆）

- [x] 在 `panel_ops.rs` 的 `new_headless()` 中初始化 `messages` 字段（双写阶段）
  - 位置: `peri-tui/src/app/panel_ops.rs` ChatSession 构造（~L1076）
  - 在 `core,` 之前添加:

    ```rust
    messages: super::MessageState::new(
        "/tmp".to_string(),
        render_tx.clone(),
        Arc::clone(&render_cache),
        Arc::clone(&render_notify),
    ),
    ```

  - 注意: render_tx/render_cache/render_notify 变量在 ~L1048 创建，~L1065 传入 AppCore::new()，此处需要在其之前 clone
  - 原因: headless 测试工厂必须同步创建 messages 字段

- [x] 迁移 `agent_ops.rs` 中 51 处 MessageState 字段访问（最高频文件）
  - 位置: `peri-tui/src/app/agent_ops.rs` 全文件
  - 替换规则:
    - `.core.view_messages` → `.messages.view_messages`
    - `.core.round_start_vm_idx` → `.messages.round_start_vm_idx`
    - `.core.pipeline` → `.messages.pipeline`
    - `.core.render_tx` → `.messages.render_tx`
    - `.core.pending_messages` → `.messages.pending_messages`
    - `.core.last_submitted_text` → `.messages.last_submitted_text`
  - 关键方法:
    - `submit_message()`（~L42-47）: round_start_vm_idx 赋值、view_messages.len()、last_submitted_text 赋值
    - `poll_agent()`（~L280-430）: pending_messages、view_messages.last_mut()、pipeline.handle_event()
    - `interrupt()`（~L827-930）: round_start_vm_idx、view_messages.clone()、render_tx.send()、pending_messages.clear()、pipeline.done()
    - `clear_session()`（~L87-107）: last_submitted_text=None、view_messages.clear()、pipeline.clear()
    - `switch_thread()`（~L99-210）: view_messages 操作、pipeline.clear()
  - 仅替换以上 6 个字段，不动 `core.render_cache`/`core.render_notify`/`core.last_render_version`（这些由 main_ui.rs/main.rs 管理）
  - 原因: agent_ops.rs 是 MessageState 字段最高频访问文件（51 处），submit_message 和 poll_agent 是核心热路径

- [x] 迁移 `headless.rs` 中 29 处 MessageState 字段访问
  - 位置: `peri-tui/src/ui/headless.rs` 测试函数
  - 替换所有 `.core.view_messages` → `.messages.view_messages`，`.core.round_start_vm_idx` → `.messages.round_start_vm_idx`，`.core.pipeline` → `.messages.pipeline`，`.core.render_cache` → `.messages.render_cache`
  - 测试中通过 `app.sessions[app.active].core.view_messages` 访问的改为 `app.sessions[app.active].messages.view_messages`
  - 原因: headless 测试大量断言 view_messages 内容和 pipeline 状态

- [x] 迁移 `main_ui.rs` 中 6 处 MessageState 字段访问
  - 位置: `peri-tui/src/ui/main_ui.rs`
  - 替换:
    - `core.pending_messages` → `messages.pending_messages`（~L96, 191）
    - `core.view_messages` → `messages.view_messages`（~L337）
    - `core.render_cache` → `messages.render_cache`（~L395, 490）
    - `core.last_render_version` → `messages.last_render_version`（~L432）
  - 原因: main_ui.rs 渲染管道需要读取 pending_messages 显示加载指示器、读取 render_cache 渲染消息、写入 last_render_version 跟踪版本

- [x] 迁移 `event.rs` 中 4 处 MessageState 字段访问
  - 位置: `peri-tui/src/event.rs`
  - 替换:
    - `core.pending_messages` → `messages.pending_messages`（~L440, 441）
    - `core.pending_messages` → `messages.pending_messages`（~L588）
    - `core.render_cache` → `messages.render_cache`（~L999）
  - 原因: event.rs 中消息操作相关分支需要访问 pending_messages 和 render_cache

- [x] 迁移 `app/mod.rs` 中 5 处 MessageState 字段访问
  - 位置: `peri-tui/src/app/mod.rs` 全文件
  - 替换所有 `.core.view_messages`/`.core.pipeline`/`.core.last_submitted_text`/`.core.round_start_vm_idx`/`.core.pending_messages` 为 `.messages.` 前缀
  - 原因: mod.rs 中 App 的公共方法可能涉及消息状态操作

- [x] 迁移 `thread_ops.rs` 中 9 处 MessageState 字段访问
  - 位置: `peri-tui/src/app/thread_ops.rs` 全文件
  - 替换:
    - `core.last_submitted_text` → `messages.last_submitted_text`（~L396）
    - `core.round_start_vm_idx` → `messages.round_start_vm_idx`（~L397）
    - `core.view_messages` → `messages.view_messages`（~L403）
    - `core.pipeline` → `messages.pipeline`（~L416）
    - `core.pending_messages` → `messages.pending_messages`（~L428）
    - 其余 `core.view_messages` / `core.pipeline` 同样替换
  - 原因: thread_ops.rs 中线程切换和加载操作需要重建 view_messages 和 pipeline

- [x] 迁移 `main.rs` 中 2 处 MessageState 字段访问
  - 位置: `peri-tui/src/main.rs`（~L272, 274）
  - 替换 `core.render_cache` → `messages.render_cache`，`core.last_render_version` → `messages.last_render_version`
  - 原因: main.rs 主循环中检测 render_cache 版本变化以决定是否重绘

- [x] 迁移 command/ 目录下 3 个文件中的 MessageState 字段访问
  - 位置:
    - `peri-tui/src/command/loop_cmd.rs`（~L24, 27, 59, 60, 73, 74, 81, 86, 90）: `core.view_messages` → `messages.view_messages`，`core.render_tx` → `messages.render_tx`
    - `peri-tui/src/command/agent.rs`（~L47, 50）: `core.view_messages` → `messages.view_messages`，`core.render_tx` → `messages.render_tx`
    - `peri-tui/src/command/help.rs`（~L20, 26）: `core.view_messages` → `messages.view_messages`
  - 原因: command 文件中推送系统消息到 view_messages 或发送渲染事件

- [x] 迁移 app/ 子模块中 6 个文件的 MessageState 字段访问
  - 位置:
    - `peri-tui/src/app/login_panel.rs`（6 处，~L430, 441, 562, 578, 588, 627, 638）: `ctx.sessions[ctx.active].core.view_messages` → `ctx.sessions[ctx.active].messages.view_messages`
    - `peri-tui/src/app/plugin_panel.rs`（3 处，~L129, 136, 162）: 同上替换
    - `peri-tui/src/app/agent_panel.rs`（1 处，~L160）: `ctx.sessions[ctx.active].core.view_messages` → `ctx.sessions[ctx.active].messages.view_messages`
    - `peri-tui/src/app/config_panel.rs`（2 处，~L331, 338）: 同上替换
    - `peri-tui/src/app/cron_state.rs`（1 处，~L1184）: `ctx.sessions[ctx.active].core.view_messages` → `ctx.sessions[ctx.active].messages.view_messages`（此处通过 `.view_messages` 访问，需确认完整路径）
    - `peri-tui/src/app/cron_ops.rs`（1 处，~L49）: `core.view_messages` → `messages.view_messages`
  - 原因: 面板和 cron 模块中向 view_messages 推送系统消息

- [x] 迁移 `thread/browser.rs` 中 1 处 MessageState 字段访问
  - 位置: `peri-tui/src/thread/browser.rs`（~L160）
  - 替换 `ctx.sessions[ctx.active].core.view_messages` → `ctx.sessions[ctx.active].messages.view_messages`
  - 原因: 浏览器模块推送消息到 view_messages

- [x] 删除 AppCore 中的 9 个 MessageState 字段及其初始化
  - 位置: `peri-tui/src/app/core.rs` AppCore 结构体定义（~L16-64）
  - 从 AppCore 中删除以下字段: view_messages, round_start_vm_idx, pipeline, render_tx, render_cache, render_notify, last_render_version, pending_messages, last_submitted_text
  - 从 AppCore::new() 的 `Self { ... }` 初始化块中删除对应 9 行
  - AppCore::new() 参数中移除: `render_tx`, `render_cache`, `render_notify`（这三个参数现在由 ChatSession 传递给 MessageState::new()）
  - 同步修改 ChatSession::new() 中对 AppCore::new() 的调用: 移除 render_tx/render_cache/render_notify 参数
  - 清理 AppCore::new() 中不再需要的 import: `mpsc`, `Notify`, `RwLock`, `RenderCache`, `RenderEvent`, `MessagePipeline`, `MessageViewModel`
  - 清理 AppCore::new() 中 `pipeline: MessagePipeline::new(cwd)` 初始化及 `cwd` 参数（pipeline 已移至 MessageState）
  - 原因: 双写阶段结束后，AppCore 中这些字段已无引用，必须删除以避免数据不一致

- [x] 为 MessageState 编写单元测试
  - 测试文件: `peri-tui/src/app/message_state.rs`（模块内 `#[cfg(test)] mod tests`）
  - 测试场景:
    - 默认值测试: `MessageState::new(cwd, render_tx, render_cache, render_notify)` 构造后 view_messages 为空, round_start_vm_idx=0, last_render_version=0, pending_messages 为空, last_submitted_text=None
    - pipeline 初始化: 构造后 `state.pipeline.cwd()` 返回传入的 cwd
    - view_messages 读写: push 一个 MessageViewModel 后 len() 为 1，可通过索引访问
    - pending_messages 操作: push 文本后 len() 增加，remove(0) 后减少
  - 运行命令: `cargo test -p peri-tui --lib -- message_state::tests`
  - 预期: 所有测试通过

**检查步骤:**

- [x] 验证 MessageState 结构体存在且包含 9 个字段
  - `grep -c "pub " peri-tui/src/app/message_state.rs`
  - 预期: 大于等于 10（9 个字段 + 1 个 new 方法）
- [x] 验证 AppCore 不再包含 MessageState 字段
  - `grep -E "pub (view_messages|round_start_vm_idx|pipeline|render_tx|render_cache|render_notify|last_render_version|pending_messages|last_submitted_text)" peri-tui/src/app/core.rs`
  - 预期: 0 行输出
- [x] 验证 AppCore::new() 不再接受 render 相关参数
  - `grep -E "render_tx|render_cache|render_notify" peri-tui/src/app/core.rs`
  - 预期: 0 行输出
- [x] 验证全项目无残留的 MessageState 字段通过 core 访问
  - `grep -rn "core\.\(view_messages\|round_start_vm_idx\|pipeline\|render_tx\|render_cache\|render_notify\|last_render_version\|pending_messages\|last_submitted_text\)" peri-tui/src/ | grep -v "spec-plan"`
  - 预期: 0 行输出
- [x] 验证编译通过
  - `cargo build -p peri-tui 2>&1 | tail -5`
  - 预期: 构建成功，无错误
- [x] 验证 clippy 无新增警告
  - `cargo clippy -p peri-tui 2>&1 | grep -E 'warning|error' | head -10`
  - 预期: 无新增 warning
- [x] 验证全部 headless 测试通过
  - `cargo test -p peri-tui --lib -- ui::headless::tests 2>&1 | tail -20`
  - 预期: 所有测试通过
- [x] 验证 MessageState 单元测试通过
  - `cargo test -p peri-tui --lib -- message_state::tests 2>&1 | tail -10`
  - 预期: 所有测试通过
- [x] 验证 AppCore 现有测试仍通过（core.rs 中 test_appcore_pipeline_initialized 需删除或改写）
  - `cargo test -p peri-tui --lib -- core::tests 2>&1 | tail -10`
  - 预期: 测试通过（如果 test_appcore_pipeline_initialized 已迁移或删除则无输出；如仍存在则需确认 pipeline 引用已更新为 MessageState 路径）

---

### Task 验收: 第一阶段局部验收（Task 1-4）

**前置条件:**

- Task 1-4 全部执行完成
- 无编译错误

**端到端验证:**

1. 运行完整测试套件确保无回归
   - `cargo test -p peri-tui 2>&1 | tail -10`
   - 预期: 全部测试通过
   - 失败排查: 检查各 Task 的检查步骤，逐 Task 二分定位

2. 验证 App 字段数已减少
   - `grep -c "pub [a-z_]*:" peri-tui/src/app/mod.rs | head -1`（粗略检查）
   - 预期: App 字段数 < 26（ServiceRegistry + SessionManager 已提取）

3. 验证 ServiceRegistry 和 SessionManager 新文件存在
   - `ls peri-tui/src/app/service_registry.rs peri-tui/src/app/session_manager.rs peri-tui/src/app/ui_state.rs peri-tui/src/app/message_state.rs`
   - 预期: 4 个文件均存在

4. 验证无旧路径残留（ServiceRegistry 字段）
   - `grep -r "app\.cwd\b" peri-tui/src/ --include="*.rs" | grep -v "services\.cwd" | grep -v "test" | wc -l`
   - 预期: 0（所有 app.cwd 已迁移为 app.services.cwd）

5. 验证 clippy 无警告
   - `cargo clippy -p peri-tui 2>&1 | grep "warning:" | wc -l`
   - 预期: 0

6. 验证 headless 测试通过
   - `cargo test -p peri-tui --lib -- headless 2>&1 | grep "test result"`
   - 预期: 全部通过
