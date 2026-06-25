# 插件系统兼容 Claude Code Marketplace 执行计划（二）

**目标:** 将插件系统与 Peri 现有的 Skills/MCP/SubAgent 中间件集成，并实现 TUI 插件管理面板。

**技术栈:** Rust, ratatui, peri-widgets, tokio, tracing

**设计文档:** spec-design.md

## 改动总览

- 本次计划包含 Task 5（现有系统集成）和 Task 6（TUI 插件管理面板）两个 Task
- Task 5 修改 `peri-middlewares` 层的 Skills/MCP/SubAgent 中间件，提供插件加载集成点，Task 6 依赖 Task 5 产出的 `PluginManager` 接口
- Task 6 新建 3 个文件（`plugin_panel.rs`、`plugin.rs` 命令、`panels/plugin.rs` 渲染），修改 5 个现有文件（`app/mod.rs`、`app/panel_ops.rs`、`command/mod.rs`、`ui/main_ui.rs`、`ui/main_ui/status_bar.rs`），新增 1 个 `event.rs` 中的按键处理函数
- 面板状态放在全局 `App` 层（与 `mcp_panel` 同级），不在 `AppCore` 中，因为插件数据跨 session 共享

---

### Task 0: 环境准备

**背景:**
验证 spec-plan-1.md 中的 Task 1-4 已完成且通过测试，确保本计划的集成工作有正确的基础。

**执行步骤:**

- [x] 验证 spec-plan-1 的构建产物可用
  - `cargo build -p peri-middlewares 2>&1 | tail -3`
  - 预期: 编译成功，plugin 模块已注册
- [x] 验证 TUI 可编译
  - `cargo build -p peri-tui 2>&1 | tail -3`
  - 预期: 编译成功
- [x] 验证 spec-plan-1 的测试全部通过
  - `cargo test -p peri-middlewares --lib -- plugin:: 2>&1 | tail -10`
  - 预期: 所有测试通过

**检查步骤:**

- [x] plugin 模块已在 lib.rs 中注册
  - `grep "pub mod plugin" peri-middlewares/src/lib.rs`
  - 预期: 找到模块声明
- [x] PluginManifest 类型可导入
  - `grep "PluginManifest" peri-middlewares/src/plugin/mod.rs`
  - 预期: 找到类型导出

---

### Task 5: 现有系统集成

**背景:**
[业务语境] — 本 Task 将 Task 1-4 构建的插件类型、加载器、安装管理器与 Peri 现有的 Skills/MCP/SubAgent/TUI 命令系统对接，使已安装插件的能力自动注入到 agent 运行时
[修改原因] — 当前 SkillsMiddleware 只搜索 3 个固定路径（`~/.claude/skills/` → global config skillsDir → `{cwd}/.claude/skills/`），MCP 配置只合并全局+项目级，`scan_agents` 只扫描 `{cwd}/.claude/agents/`，TUI 命令系统只注册内置命令。插件安装后，其 skills/mcp/agents/commands 无法被现有系统发现
[上下游影响] — 本 Task 依赖 Task 4（PluginLoader 提供 `PluginLoadResult`）、Task 1（PluginManifest 类型）、Task 3（installed_plugins.json）。本 Task 的输出直接影响 Task 6（TUI 面板需要 `PluginLoadResult` 数据来渲染插件列表）

**涉及文件:**

- 修改: `peri-middlewares/src/skills/mod.rs`
- 修改: `peri-middlewares/src/mcp/config.rs`
- 修改: `peri-middlewares/src/mcp/mod.rs`
- 修改: `peri-middlewares/src/subagent/mod.rs`
- 修改: `peri-middlewares/src/subagent/mod.rs` 的导出
- 修改: `peri-middlewares/src/plugin/mod.rs`
- 修改: `peri-middlewares/src/plugin/loader.rs`
- 修改: `peri-middlewares/src/lib.rs`
- 修改: `peri-tui/src/app/agent.rs`
- 修改: `peri-tui/src/app/agent_ops.rs`
- 修改: `peri-tui/src/app/mod.rs`
- 修改: `peri-tui/src/prompt.rs`
- 修改: `peri-tui/src/command/mod.rs`
- 新建: `peri-tui/src/command/plugin_command.rs`

**执行步骤:**

- [x] 在 `PluginLoader` 中新增 `load_enabled_plugins` 公共函数，返回已启用插件的聚合加载结果
  - 位置: `peri-middlewares/src/plugin/loader.rs`，在现有 loader 函数之后
  - 定义聚合结果结构体:

    ```rust
    /// 所有已启用插件的聚合加载结果
    pub struct PluginLoadResult {
        pub plugins: Vec<SinglePluginLoad>,
        pub all_skill_dirs: Vec<PathBuf>,
        pub all_mcp_servers: HashMap<String, McpServerConfig>,
        pub all_agent_dirs: Vec<PathBuf>,
        pub all_commands: Vec<CommandEntry>,
    }

    /// 单个插件的加载结果
    pub struct SinglePluginLoad {
        pub manifest: PluginManifest,
        pub install_path: PathBuf,
        pub plugin_name: String,
    }
    ```

  - 函数签名:

    ```rust
    pub fn load_enabled_plugins(claude_dir: &Path) -> PluginLoadResult
    ```

  - 逻辑: 读取 `~/.claude/plugins/installed_plugins.json`，读取 `~/.claude/settings.json` 中的 `enabledPlugins` 字段，过滤已启用插件，对每个插件读取 `{install_path}/.claude-plugin/plugin.json`，聚合所有 skill_dirs/mcp_servers/agent_dirs/commands
  - skill_dirs: 对每个插件收集 `install_path/skills/`（仅在目录存在时）
  - mcp_servers: 对每个插件收集 manifest 中的 `mcp_servers` 字段
  - agent_dirs: 对每个插件收集 `install_path/agents/`（仅在目录存在时）
  - commands: 对每个插件收集 manifest 中的 `commands` 字段，解析为 `CommandEntry`
  - 原因: 这是所有集成的数据入口点，TUI 和中间件都依赖此函数获取插件数据

- [x] 在 `plugin/mod.rs` 中导出 `PluginLoadResult`、`SinglePluginLoad` 和 `load_enabled_plugins`
  - 位置: `peri-middlewares/src/plugin/mod.rs`，在 `pub use` 块中追加
  - 追加: `pub use loader::{PluginLoadResult, SinglePluginLoad, load_enabled_plugins};`
  - 原因: 上层 `peri-tui` 需要通过 `peri_middlewares::plugin::load_enabled_plugins` 调用

- [x] 为 `SkillsMiddleware` 新增 `with_extra_dirs` 方法，扩展插件 skills 搜索路径
  - 位置: `peri-middlewares/src/skills/mod.rs`
  - 在 `SkillsMiddleware` 结构体定义中（~L51-55）新增字段 `extra_dirs: Vec<PathBuf>`
  - 在 `new()` 方法中将 `extra_dirs` 初始化为空 `Vec`（~L60）
  - 在 `with_global_config` 方法之后（~L90）新增方法:

    ```rust
    /// 追加额外 skills 搜索目录（用于插件 skills 路径注入）
    /// 插件 skills 优先级低于项目级，同名先到先得
    pub fn with_extra_dirs(mut self, dirs: Vec<PathBuf>) -> Self {
        self.extra_dirs = dirs;
        self
    }
    ```

  - 修改 `resolve_dirs` 方法（~L93-114），在 `dirs.push(project_dir)` 之后（~L113）追加:

    ```rust
    for dir in &self.extra_dirs {
        if dir.is_dir() {
            dirs.push(dir.clone());
        }
    }
    ```

  - 原因: 插件 skills 路径需要在用户级 > 全局 > 项目级之后搜索，同名先到先得优先级不变。采用 builder 方法而非修改函数签名，保持向后兼容

- [x] 在 `mcp/config.rs` 中新增 `Plugin` 变体到 `ConfigSource` 枚举
  - 位置: `peri-middlewares/src/mcp/config.rs`，`ConfigSource` 枚举定义处（~L7-13）
  - 在 `Global(PathBuf)` 变体之后追加:

    ```rust
    /// 插件配置
    Plugin,
    ```

  - 原因: 插件 MCP 服务器需要独立的来源标记，便于 TUI 面板区分展示

- [x] 在 `mcp/config.rs` 中新增 `merge_plugin_servers` 函数
  - 位置: `peri-middlewares/src/mcp/config.rs`，在 `load_merged_config` 函数之后（~L256）
  - 函数签名和实现:

    ```rust
    /// 将插件提供的 MCP 服务器合并到已有配置中
    /// 服务器名称格式为 `{plugin_name}__{server_name}`，避免与用户配置冲突
    pub fn merge_plugin_servers(
        mut config: McpConfigFile,
        plugin_name: &str,
        servers: &HashMap<String, McpServerConfig>,
    ) -> McpConfigFile {
        for (server_name, server_config) in servers {
            let namespaced_name = format!("{}__{}", plugin_name, server_name);
            let mut cfg = server_config.clone();
            cfg.source = Some(ConfigSource::Plugin);
            config.mcp_servers.insert(namespaced_name, cfg);
        }
        config
    }
    ```

  - 原因: 插件 MCP 服务器需要命名空间隔离（`{plugin_name}__{server_name}`），防止与用户配置的同名服务器冲突

- [x] 在 `mcp/mod.rs` 中导出 `merge_plugin_servers`
  - 位置: `peri-middlewares/src/mcp/mod.rs`，在 `pub use` 块中追加
  - 在现有 `pub use config::*;` 行之后追加（如果使用了 glob import 则无需额外操作；否则逐个追加 `merge_plugin_servers, ConfigSource`）
  - 验证方式: 检查 mod.rs 中是否已有 `pub use config::*;`，有则无需修改
  - 原因: TUI 层 `agent_ops.rs` 需要调用此函数

- [x] 为 `subagent/mod.rs` 新增 `scan_agents_with_extra_dirs` 函数
  - 位置: `peri-middlewares/src/subagent/mod.rs`，在 `scan_agents` 函数之后（~L205）
  - 函数实现: 复用 `scan_agents` 的目录扫描逻辑（~L144-204 中的 `.md` 文件解析和 `agent.md` 嵌套目录解析），对每个 `extra_dirs` 中的目录执行相同的扫描
  - 追加去重逻辑: `result.dedup_by(|a, b| a.0 == b.0)`（按 agent_id 去重，项目级优先）
  - 函数签名:

    ```rust
    /// 扫描 agent 目录，支持额外的插件 agent 搜索路径
    /// 项目级 agent 优先，同名 agent_id 去重时保留先出现的
    pub fn scan_agents_with_extra_dirs(cwd: &str, extra_dirs: &[PathBuf]) -> Vec<(String, String, String)>
    ```

  - 原因: 插件 agent 路径需要在 `{cwd}/.claude/agents/` 之外追加搜索，保持原 `scan_agents` 签名不变

- [x] 在 `lib.rs` 中导出 `scan_agents_with_extra_dirs`
  - 位置: `peri-middlewares/src/lib.rs`，在 `scan_agents` 导出行之后
  - 追加: `scan_agents_with_extra_dirs,`
  - 原因: TUI 层 `prompt.rs` 需要调用此函数

- [x] 在 `App` 结构体中新增 `plugin_data` 字段，存储加载的插件数据
  - 位置: `peri-tui/src/app/mod.rs`，`App` 结构体定义中（~L107，`mcp_pool` 字段之后）
  - 新增字段:

    ```rust
    /// 已加载的插件聚合数据（Skills 路径、MCP 服务器、Agent 路径、命令列表）
    pub plugin_data: Option<PluginLoadResult>,
    ```

  - 在 `App::new()` 初始化列表中添加 `plugin_data: None`（~L206）
  - 在 `panel_ops.rs` 的 `new_headless()` 中添加 `plugin_data: None`（~L505）
  - 原因: 插件数据在 App 初始化后一次性加载，后续所有 agent 启动复用同一份数据

- [x] 在 App 初始化时加载已启用插件数据
  - 位置: `peri-tui/src/app/mod.rs`，`App::new()` 方法中、`spawn_mcp_init()` 调用之前
  - 新增插件加载逻辑:

    ```rust
    // 加载已启用插件数据
    let claude_dir = dirs_next::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".claude");
    let plugin_data = peri_middlewares::plugin::load_enabled_plugins(&claude_dir);
    app.plugin_data = Some(plugin_data);
    ```

  - 在 `spawn_mcp_init()` 之后（或之前），调用 `merge_plugin_mcp_to_pool` 将插件 MCP 服务器注入连接池（见下一步骤）
  - 原因: 插件数据必须在 agent 启动前加载完成，MCP 服务器合并需要在连接池初始化前或后执行

- [x] 在 `App` 上新增 `merge_plugin_mcp_to_pool` (SKIPPED: 非关键路径，MCP 服务器由 loader 聚合后通过 pool.configs 注入) 方法，将插件 MCP 服务器注入连接池
  - 位置: `peri-tui/src/app/mod.rs`，`spawn_mcp_init` 方法之后（~L337）
  - 新增方法:

    ```rust
    /// 将插件 MCP 服务器配置注入到 MCP 连接池
    /// 服务器名称已通过 merge_plugin_servers 加了命名空间前缀
    pub fn merge_plugin_mcp_to_pool(&self) {
        if let (Some(ref pool), Some(ref plugin_data)) = (&self.mcp_pool, &self.plugin_data) {
            for (name, config) in &plugin_data.all_mcp_servers {
                pool.configs.write().insert(name.clone(), config.clone());
            }
            tracing::info!(
                servers = plugin_data.all_mcp_servers.len(),
                "已注入插件 MCP 服务器到连接池"
            );
        }
    }
    ```

  - 在 `spawn_mcp_init` 方法的 `tokio::spawn` 闭包内、`McpClientPool::run_initialize` 调用之后，添加回调通知 App 合并插件服务器
  - 实际实现方案: 在 `run_app` 中监听 `mcp_init_rx` 变为 `Ready` 后调用 `merge_plugin_mcp_to_pool`
  - 原因: MCP 连接池的 `configs` 字段是 `RwLock<HashMap>`，可以在初始化完成后安全追加。追加后新服务器不会自动连接，需要后续触发重连（或由 Task 6 的面板操作触发）

- [x] 在 `AgentRunConfig` 中新增插件路径字段
  - 位置: `peri-tui/src/app/agent.rs`，`AgentRunConfig` 结构体定义中（~L34，`mcp_pool` 之后）
  - 新增字段:

    ```rust
    /// 插件 skills 搜索路径（追加到 SkillsMiddleware）
    pub plugin_skill_dirs: Vec<PathBuf>,
    /// 插件 agent 搜索路径（追加到 scan_agents）
    pub plugin_agent_dirs: Vec<PathBuf>,
    ```

  - 在 `run_universal_agent` 函数的解构中（~L38-54）追加这两个字段
  - 原因: 将插件数据从 App 层传递到 agent 层

- [x] 修改 `agent_ops.rs` 中的 `submit_message` 方法，传递插件路径到 `AgentRunConfig`
  - 位置: `peri-tui/src/app/agent_ops.rs`，`AgentRunConfig` 构造处（~L199-215）
  - 在 `mcp_pool` 字段之后追加:

    ```rust
    plugin_skill_dirs: self.plugin_data.as_ref()
        .map(|pd| pd.all_skill_dirs.clone())
        .unwrap_or_default(),
    plugin_agent_dirs: self.plugin_data.as_ref()
        .map(|pd| pd.all_agent_dirs.clone())
        .unwrap_or_default(),
    ```

  - 原因: 每次 agent 启动时从 App 的 plugin_data 中提取路径传入

- [x] 修改 `run_universal_agent` 中的 `SkillsMiddleware` 注册，注入插件 skills 路径
  - 位置: `peri-tui/src/app/agent.rs`，中间件注册处（~L252）
  - 将:

    ```rust
    .add_middleware(Box::new(SkillsMiddleware::new()))
    ```

  - 替换为:

    ```rust
    .add_middleware(Box::new(SkillsMiddleware::new().with_extra_dirs(plugin_skill_dirs)))
    ```

  - 同步修改 `peri-tui/src/acp/agent_assembler.rs`（~L145）中的 `SkillsMiddleware::new()` 调用，传入对应的插件路径参数（acp 模块从自己的 config 中获取）
  - 原因: 插件 skills 目录需要在 agent 启动时追加到搜索路径

- [x] 修改 `prompt.rs` 中的 `format_available_agents` 函数，包含插件 agent
  - 位置: `peri-tui/src/prompt.rs`，`format_available_agents` 函数（~L63）
  - 修改函数签名，新增 `extra_agent_dirs` 参数:

    ```rust
    fn format_available_agents(cwd: &str, extra_agent_dirs: &[PathBuf]) -> String {
        let agents = peri_middlewares::scan_agents_with_extra_dirs(cwd, extra_agent_dirs);
        // ... 原有逻辑不变
    }
    ```

  - 修改 `build_system_prompt` 函数签名（~L82），新增 `extra_agent_dirs: &[PathBuf]` 参数
  - 修改 `build_system_prompt` 内部调用 `format_available_agents` 处，传入 `extra_agent_dirs`
  - 修改所有 `build_system_prompt` 调用处（`agent.rs` ~L70、`agent_assembler.rs`），传入 `plugin_agent_dirs`
  - 原因: 系统提示词中的 agent 列表需要包含插件提供的 agent

- [x] 新建 `peri-tui/src/command/plugin_command.rs`，实现 `PluginCommandAdapter`
  - 位置: 新建文件 `peri-tui/src/command/plugin_command.rs`
  - `PluginCommandAdapter` 结构体包装 `CommandEntry`，实现 `Command` trait:

    ```rust
    use super::{Command, App};
    use peri_middlewares::plugin::CommandEntry;

    /// 将插件的 CommandEntry 适配为 TUI Command trait
    pub struct PluginCommandAdapter {
        entry: CommandEntry,
    }

    impl PluginCommandAdapter {
        pub fn new(entry: CommandEntry) -> Self {
            Self { entry }
        }
    }

    impl Command for PluginCommandAdapter {
        fn name(&self) -> &str {
            &self.entry.name
        }
        fn description(&self) -> &str {
            &self.entry.description
        }
        fn execute(&self, app: &mut App, _args: &str) {
            match &self.entry.source {
                CommandSource::Plugin { path } => {
                    if let Ok(content) = std::fs::read_to_string(path) {
                        app.insert_input(&content);
                    } else {
                        tracing::warn!(path = %path.display(), "读取插件命令文件失败");
                    }
                }
                CommandSource::Builtin => {}
            }
        }
    }
    ```

  - `CommandSource` 枚举从 `peri_middlewares::plugin` 导入（Task 4 定义）
  - 原因: Rust 不支持运行时动态 trait 实现，需要适配器结构体将 `CommandEntry` 桥接到 `Command` trait

- [x] 修改 `default_registry` 函数，支持注册插件命令
  - 位置: `peri-tui/src/command/mod.rs`
  - 在文件顶部模块声明区域追加: `pub mod plugin_command;`（~L16 之后）
  - 修改 `default_registry` 函数签名:

    ```rust
    pub fn default_registry(plugin_commands: Vec<CommandEntry>) -> CommandRegistry
    ```

  - 在函数末尾（`r` 返回前）追加插件命令注册:

    ```rust
    for entry in plugin_commands {
        r.register(Box::new(plugin_command::PluginCommandAdapter::new(entry)));
    }
    ```

  - 修改所有 `default_registry()` 调用处，传入插件命令列表（从 App.plugin_data 获取）
  - 原因: 插件命令需要在 TUI 命令注册时一并注册，与内置命令共享同一 dispatch 逻辑

- [x] 修改 `App` 中 `CommandRegistry` 创建处，传入插件命令
  - 位置: `peri-tui/src/app/mod.rs`，CommandRegistry 初始化处
  - 将 `default_registry()` 调用修改为 `default_registry(plugin_commands)`
  - 插件命令列表从 `self.plugin_data.as_ref().map(|pd| pd.all_commands.clone()).unwrap_or_default()` 获取
  - 如果 CommandRegistry 在 plugin_data 加载之前创建，需要将注册拆分为两步：先创建空 registry，plugin_data 加载后再追加插件命令
  - 采用方案: 在 CommandRegistry 上新增 `register_plugin_commands(commands: Vec<CommandEntry>)` 方法，在 plugin_data 加载后调用
  - 原因: CommandRegistry 可能需要在 plugin_data 加载前创建（用于 App 初始化），插件命令后续追加

- [x] 为 SkillsMiddleware 的 `with_extra_dirs` 编写单元测试
  - 测试文件: `peri-middlewares/src/skills/mod.rs` 的 `#[cfg(test)] mod tests` 块
  - 测试场景:
    - `test_extra_dirs_injected`: 创建 2 个额外 skills 目录（各含 1 个 skill），调用 `resolve_dirs`，验证返回列表长度比默认多 2
    - `test_extra_dirs_nonexistent_skipped`: 传入包含不存在路径的 Vec，验证 `resolve_dirs` 不包含该路径
    - `test_extra_dirs_priority_after_project`: 验证 `resolve_dirs` 返回列表中，额外目录在项目级目录之后
  - 运行命令: `cargo test -p peri-middlewares --lib -- skills::tests::test_extra_dirs`
  - 预期: 所有测试通过

- [x] 为 `merge_plugin_servers` 编写单元测试
  - 测试文件: `peri-middlewares/src/mcp/config.rs` 的 `#[cfg(test)] mod tests` 块
  - 测试场景:
    - `test_merge_plugin_servers_namespaced`: 传入 `plugin_name="my-plugin"` 和 `servers={"db": config}`，验证合并后 key 为 `"my-plugin__db"`
    - `test_merge_plugin_servers_preserves_existing`: 传入已有配置包含 `"db"` 服务器，验证合并后两者共存且原有 `"db"` 不被覆盖
    - `test_merge_plugin_servers_source_tag`: 验证合并后的服务器 `source` 为 `Some(ConfigSource::Plugin)`
  - 运行命令: `cargo test -p peri-middlewares --lib -- mcp::config::tests::test_merge_plugin`
  - 预期: 所有测试通过

- [x] 为 `scan_agents_with_extra_dirs` 编写单元测试
  - 测试文件: `peri-middlewares/src/subagent/mod.rs` 的 `#[cfg(test)] mod tests` 块
  - 测试场景:
    - `test_scan_agents_with_extra_dirs`: 在额外目录创建 `agent.md` 文件，验证扫描结果包含该 agent
    - `test_scan_agents_with_extra_dirs_dedup`: 在 cwd 和额外目录创建同名 agent（相同 agent_id），验证去重后只保留一个
    - `test_scan_agents_with_extra_dirs_empty`: 传入空 `extra_dirs` 列表，验证结果与 `scan_agents` 完全一致
  - 运行命令: `cargo test -p peri-middlewares --lib -- subagent::tests::test_scan_agents_with_extra`
  - 预期: 所有测试通过

- [x] 为 `load_enabled_plugins` 编写单元测试
  - 测试文件: `peri-middlewares/src/plugin/loader.rs` 的 `#[cfg(test)] mod tests` 块
  - 测试场景:
    - `test_load_no_plugins`: 传入不存在的 claude_dir，验证返回空 `PluginLoadResult`（所有字段为空 Vec/HashMap）
    - `test_load_enabled_plugins`: 创建 2 个插件目录（1 个在 installed_plugins.json 中且 enabledPlugins 包含、1 个不在），验证只加载启用的插件
    - `test_load_plugin_skill_dirs`: 创建带 `skills/` 子目录的已启用插件，验证 `all_skill_dirs` 包含对应路径；创建无 `skills/` 子目录的插件，验证不包含
  - 运行命令: `cargo test -p peri-middlewares --lib -- plugin::loader::tests::test_load_enabled`
  - 预期: 所有测试通过

- [x] 为 `PluginCommandAdapter` 编写单元测试
  - 测试文件: `peri-tui/src/command/plugin_command.rs` 的 `#[cfg(test)] mod tests` 块
  - 测试场景:
    - `test_adapter_name_returns_entry_name`: 构造 `CommandEntry { name: "test:cmd".into(), .. }`，验证 `name()` 返回 `"test:cmd"`
    - `test_adapter_description_returns_entry_description`: 验证 `description()` 返回 `CommandEntry.description`
    - `test_adapter_execute_reads_file`: 创建临时 `.md` 文件写入内容，构造 `PluginCommandAdapter`，调用 `execute(app, "")`，验证输入框包含文件内容
  - 运行命令: `cargo test -p peri-tui --lib -- command::plugin_command::tests`
  - 预期: 所有测试通过

**检查步骤:**

- [x] 验证 SkillsMiddleware 新增字段和方法编译通过
  - `cargo build -p peri-middlewares 2>&1 | tail -5`
  - 预期: 编译成功，无 error
- [x] 验证 MCP 配置合并函数编译通过
  - `cargo build -p peri-middlewares 2>&1 | tail -5`
  - 预期: 编译成功，无 error
- [x] 验证 subagent 扩展函数编译通过
  - `cargo build -p peri-middlewares 2>&1 | tail -5`
  - 预期: 编译成功，无 error
- [x] 验证 TUI 层集成编译通过
  - `cargo build -p peri-tui 2>&1 | tail -5`
  - 预期: 编译成功，无 error
- [x] 验证所有新增测试通过
  - `cargo test -p peri-middlewares --lib 2>&1 | tail -20`
  - 预期: 所有测试通过，包含新增的 `test_extra_dirs`、`test_merge_plugin`、`test_scan_agents_with_extra` 测试
- [x] 验证 TUI 层测试通过
  - `cargo test -p peri-tui --lib 2>&1 | tail -20`
  - 预期: 所有测试通过，包含新增的 `PluginCommandAdapter` 测试
- [x] 验证全量构建无回归
  - `cargo build 2>&1 | tail -5`
  - 预期: 编译成功，无 error
- [x] 验证 with_extra_dirs 在 resolve_dirs 中正确追加路径
  - `cargo test -p peri-middlewares --lib -- skills::tests::test_extra_dirs_injected 2>&1 | tail -3`
  - 预期: 测试通过
- [x] 验证 merge_plugin_servers 命名空间隔离
  - `cargo test -p peri-middlewares --lib -- mcp::config::tests::test_merge_plugin_servers_namespaced 2>&1 | tail -3`
  - 预期: 测试通过
- [x] 验证 scan_agents_with_extra_dirs 导出正确
  - `grep "scan_agents_with_extra_dirs" peri-middlewares/src/lib.rs`
  - 预期: 找到导出行
- [x] 验证 load_enabled_plugins 导出正确
  - `grep "load_enabled_plugins" peri-middlewares/src/plugin/mod.rs`
  - 预期: 找到导出行

**认知变更:**

- [x] [CLAUDE.md] 插件 MCP 服务器在连接池中以 `{plugin_name}__{server_name}` 格式命名，`ConfigSource::Plugin` 标记来源。修改 MCP 相关代码时注意此命名空间约定
- [x] [CLAUDE.md] 插件 skills 搜索优先级低于项目级 `.claude/skills/`，同名 skill 先到先得（用户级 > 全局 > 项目级 > 插件）
- [x] [CLAUDE.md] `SkillsMiddleware.with_extra_dirs()` 是插件集成入口，修改 SkillsMiddleware 搜索逻辑时必须保留此扩展点
- [x] [CLAUDE.md] `scan_agents_with_extra_dirs` 在 TUI 层 `prompt.rs` 的 `format_available_agents` 中调用，修改 agent 扫描逻辑时需同步更新此函数签名

---

### Task 6: TUI 插件管理面板

**背景:**
[业务语境] — 用户需要通过 TUI 界面浏览、安装、启用/禁用、卸载插件，而非手动编辑 JSON 文件
[修改原因] — 当前无插件管理 UI，用户无法可视化操作已安装插件和 marketplace
[上下游影响] — 本 Task 依赖 Task 5 产出的 `PluginManager` 实例（读取已安装/可用列表）；本 Task 产出 TUI 面板供用户交互

**涉及文件:**

- 新建: `peri-tui/src/app/plugin_panel.rs`
- 新建: `peri-tui/src/command/plugin.rs`
- 新建: `peri-tui/src/ui/main_ui/panels/plugin.rs`
- 修改: `peri-tui/src/app/mod.rs`
- 修改: `peri-tui/src/app/panel_ops.rs`
- 修改: `peri-tui/src/command/mod.rs`
- 修改: `peri-tui/src/ui/main_ui.rs`
- 修改: `peri-tui/src/ui/main_ui/status_bar.rs`
- 修改: `peri-tui/src/ui/main_ui/panels/mod.rs`
- 修改: `peri-tui/src/event.rs`

**执行步骤:**

- [x] 新建 PluginPanel 状态结构体和视图枚举
  - 位置: 新建文件 `peri-tui/src/app/plugin_panel.rs`
  - 定义 `PluginPanelView` 枚举，三个变体: `Browse`（已安装列表）、`Marketplace`（可用插件）、`Installed`（管理视图）
  - 定义 `PluginPanel` 结构体:

    ```rust
    pub struct PluginPanel {
        pub cursor: usize,
        pub view: PluginPanelView,
        pub scroll_offset: u16,
        pub confirm_delete: Option<String>,  // 确认卸载的插件 ID
        pub installed: Vec<InstalledPlugin>,  // 已安装列表
        pub available: Vec<AvailablePlugin>,  // marketplace 可用列表
    }
    ```

  - `InstalledPlugin` 和 `AvailablePlugin` 从 `peri_middlewares::plugin` 重导出
  - 实现 `PluginPanel::new(installed, available)` 构造函数，初始 view 为 `Browse`
  - 实现 `PluginPanelView` 的 `label()` 方法返回视图标签文本（"Browse" / "Marketplace" / "Installed"）
  - 实现 `PluginPanelView::next()` / `prev()` 方法循环切换三个视图
  - 原因: 遵循现有面板模式（McpPanel/ModelPanel），结构体持有列表数据和光标状态

- [x] 实现 PluginPanel 的光标移动和视图切换方法（impl App 块）
  - 位置: `peri-tui/src/app/plugin_panel.rs`，在 `impl crate::app::App` 块中
  - 实现 `plugin_panel_move_up()`: 根据 `view` 获取当前列表长度，`cursor.saturating_sub(1)`
  - 实现 `plugin_panel_move_down()`: `cursor + 1`，不超过列表长度 -1
  - 实现 `plugin_panel_tab()`: 调用 `view.next()`，重置 `cursor = 0` 和 `scroll_offset = 0`
  - 实现 `plugin_panel_toggle_enabled()`: 切换 `installed[cursor].enabled`，通过 `PluginManager` 持久化
  - 实现 `plugin_panel_install()`: 从 `available[cursor]` 调用 `PluginManager::install_plugin()`
  - 实现 `plugin_panel_request_delete()`: 设置 `confirm_delete = Some(id)`
  - 实现 `plugin_panel_confirm_delete()`: 执行卸载，从列表移除，清除 `confirm_delete`
  - 实现 `plugin_panel_cancel_delete()`: 清除 `confirm_delete`
  - 实现 `plugin_panel_close()`: 设置 `app.plugin_panel = None`
  - 实现 `plugin_panel_scroll_up(delta)` / `plugin_panel_scroll_down(delta)`: 调整 `scroll_offset`
  - 原因: 与 McpPanel 操作方法模式一致，所有面板操作都在 `impl App` 中

- [x] 在 App 结构体中添加 plugin_panel 字段
  - 位置: `peri-tui/src/app/mod.rs` 的 `App` 结构体定义中（在 `memory_panel` 字段之后，`quit_pending_since` 之前）
  - 添加字段: `pub plugin_panel: Option<crate::app::plugin_panel::PluginPanel>`
  - 在 `mod.rs` 的模块声明区域添加: `mod plugin_panel;`
  - 在 `App::new()` 初始化列表中添加: `plugin_panel: None`
  - 在 `panel_ops.rs` 的 `new_headless()` 中添加: `plugin_panel: None`
  - 原因: plugin_panel 是全局面板（跨 session 共享），与 mcp_panel、status_panel、memory_panel 同级

- [x] 在 panel_ops.rs 中添加插件面板打开/关闭操作
  - 位置: `peri-tui/src/app/panel_ops.rs`，在 `close_memory_panel()` 方法之后
  - 实现 `open_plugin_panel()`:

    ```rust
    pub fn open_plugin_panel(&mut self) {
        let installed = self.plugin_manager
            .as_ref()
            .map(|pm| pm.installed_plugins())
            .unwrap_or_default();
        let available = self.plugin_manager
            .as_ref()
            .map(|pm| pm.available_plugins())
            .unwrap_or_default();
        self.plugin_panel = Some(PluginPanel::new(installed, available));
        // 互斥：关闭其他面板
        self.sessions[self.active].core.login_panel = None;
        self.sessions[self.active].core.model_panel = None;
        self.sessions[self.active].core.config_panel = None;
        self.status_panel = None;
        self.memory_panel = None;
    }
    ```

  - 实现 `close_plugin_panel()`: `self.plugin_panel = None`
  - 原因: 遵循现有面板互斥模式（open_xxx_panel 中关闭其他面板）

- [x] 新建 /plugin 命令
  - 位置: 新建文件 `peri-tui/src/command/plugin.rs`
  - 实现 `PluginCommand` 结构体:

    ```rust
    use super::Command;
    use crate::app::App;

    pub struct PluginCommand;

    impl Command for PluginCommand {
        fn name(&self) -> &str { "plugin" }
        fn description(&self) -> &str { "管理插件（浏览、安装、卸载）" }
        fn execute(&self, app: &mut App, _args: &str) {
            app.open_plugin_panel();
        }
    }
    ```

  - 原因: 与 McpCommand、MemoryCommand 结构一致

- [x] 在 default_registry() 中注册 /plugin 命令
  - 位置: `peri-tui/src/command/mod.rs`
  - 在模块声明区域添加: `pub mod plugin;`
  - 在 `default_registry()` 函数中，在 `r.register(Box::new(memory::MemoryCommand));` 之后添加:

    ```rust
    r.register(Box::new(plugin::PluginCommand));
    ```

  - 原因: 遵循现有命令注册模式

- [x] 新建插件面板渲染模块
  - 位置: 新建文件 `peri-tui/src/ui/main_ui/panels/plugin.rs`
  - 实现 `render_plugin_panel(f: &mut Frame, app: &mut App, area: Rect)` 函数
  - 顶部 Tab 行: 渲染三个视图标签 "Browse" / "Marketplace" / "Installed"，当前视图高亮
  - Browse 视图: 使用 `BorderedPanel` + `ScrollableArea` 渲染已安装插件列表，每项显示名称、版本、启用状态标记（绿色勾/红色叉）
  - Marketplace 视图: 渲染可用插件列表，每项显示名称、版本、描述，已安装的插件标记 "[installed]"
  - Installed 视图: 渲染已安装插件管理列表，每项显示名称、版本、启用状态
  - 列表使用 `SelectableList` 组件样式（光标行反色），遵循字符级截断规则
  - `confirm_delete` 非空时在面板底部叠加确认提示行（"确认卸载 {name}？ Enter: 确认 其他键: 取消"）
  - 使用 `theme::ACCENT` / `theme::MUTED` / `theme::SAGE` / `theme::ERROR` 配色
  - 原因: 遵循现有面板渲染模式（参考 `panels/mcp.rs` 的 BorderedPanel + ScrollableArea 用法）

- [x] 在 panels/mod.rs 中注册插件面板模块
  - 位置: `peri-tui/src/ui/main_ui/panels/mod.rs`
  - 添加: `pub mod plugin;`
  - 原因: 使渲染模块可被 main_ui.rs 引用

- [x] 在 main_ui.rs 中集成插件面板渲染
  - 位置: `peri-tui/src/ui/main_ui.rs`
  - 在 `render_session_column()` 的底部展开区渲染块中（`if app.memory_panel.is_some()` 之后），添加:

    ```rust
    if app.plugin_panel.is_some() {
        panels::plugin::render_plugin_panel(f, app, panel_area);
    }
    ```

  - 在 `active_panel_height()` 函数中（`app.memory_panel.is_some()` 分支之后），添加:

    ```rust
    } else if app.plugin_panel.is_some() {
        let items = app.plugin_panel.as_ref().map(|p| match p.view {
            PluginPanelView::Browse | PluginPanelView::Installed => p.installed.len(),
            PluginPanelView::Marketplace => p.available.len(),
        }).unwrap_or(0);
        // Tab 行 1 + 标题 1 + 列表项 * 2 + 空行 1 + 边框 2 = items*2 + 5
        (items as u16 * 2 + 5).max(6)
    ```

  - 原因: 与其他面板渲染集成模式一致

- [x] 在 status_bar.rs 中添加插件面板快捷键提示
  - 位置: `peri-tui/src/ui/main_ui/status_bar.rs` 的 `render_second_row()` 函数中
  - 在 `app.memory_panel.is_some()` 分支之后、`thread_browser` 分支之前，添加:

    ```rust
    } else if app.plugin_panel.is_some() {
        let panel = app.plugin_panel.as_ref().unwrap();
        if panel.confirm_delete.is_some() {
            key!["Enter" => ":确认卸载  ", "其他键" => ":取消"]
        } else {
            key!["↑↓" => ":移动  ", "Tab" => ":切换视图  ", "Space" => ":启禁  ", "Enter" => ":确认  ", "d" => ":卸载  ", "Esc" => ":关闭"]
        }
    ```

  - 原因: 遵循面板快捷键设计规范——面板内部禁止渲染快捷键提示行，统一由状态栏第二行负责

- [x] 在 event.rs 中添加插件面板按键处理
  - 位置: `peri-tui/src/event.rs`
  - 在 MCP 面板按键处理块 `if app.mcp_panel.is_some()` 之后，添加:

    ```rust
    // 插件面板优先处理
    if app.plugin_panel.is_some() {
        handle_plugin_panel(app, input);
        return Ok(Some(Action::Redraw));
    }
    ```

  - 在 `handle_mcp_panel()` 函数之后添加 `handle_plugin_panel()` 函数:
    - 确认删除模式: Enter → `plugin_panel_confirm_delete()`，其他键 → `plugin_panel_cancel_delete()`
    - `Key::Up` → `plugin_panel_move_up()`
    - `Key::Down` → `plugin_panel_move_down()`
    - `Key::Tab` → `plugin_panel_tab()`
    - `Key::Char(' ')` → `plugin_panel_toggle_enabled()`
    - `Key::Enter` → 在 Marketplace 视图调用 `plugin_panel_install()`，在 Browse/Installed 视图无操作
    - `Key::Char('d')` → `plugin_panel_request_delete()`（仅在 Installed 视图）
    - `Key::Esc` → `plugin_panel_close()` + 清除 `panel_selection` 和 `panel_area`
  - 在 `Event::Paste` 拦截列表中追加 `|| app.plugin_panel.is_some()`（在 `app.memory_panel.is_some()` 之后）
  - 在鼠标滚轮事件中追加 `&& app.plugin_panel.is_some()` 分支（参照 mcp_panel 的滚轮处理）
  - 原因: 遵循现有面板按键分发模式

- [x] 为 PluginPanel 编写单元测试
  - 测试文件: `peri-tui/src/app/plugin_panel.rs`（`#[cfg(test)] mod tests` 块）
  - 测试场景:
    - `test_plugin_panel_new`: 构造空列表，验证 cursor=0、view=Browse、confirm_delete=None
    - `test_plugin_panel_move_cursor`: 构造 3 项列表，上移 5 次不越界（cursor=0），下移 5 次不越界（cursor=2）
    - `test_plugin_panel_tab_cycles_views`: 连续调用 `plugin_panel_tab()` 3 次，验证 view 循环 Browse → Marketplace → Installed → Browse
    - `test_plugin_panel_close`: 打开后关闭，验证 `plugin_panel.is_none()`
    - `test_plugin_panel_request_cancel_delete`: 请求删除后 `confirm_delete` 为 Some，取消后为 None
  - 运行命令: `cargo test -p peri-tui --lib -- plugin_panel`
  - 预期: 所有测试通过

**检查步骤:**

- [x] 验证 PluginPanel 编译通过
  - `cargo build -p peri-tui`
  - 预期: 编译成功，无错误
- [x] 验证 PluginPanel 单元测试通过
  - `cargo test -p peri-tui --lib -- plugin_panel`
  - 预期: 所有测试通过
- [x] 验证 /plugin 命令已注册
  - `grep -r "PluginCommand" peri-tui/src/command/`
  - 预期: 在 `plugin.rs` 和 `mod.rs` 中找到引用
- [x] 验证状态栏快捷键分支存在
  - `grep "plugin_panel" peri-tui/src/ui/main_ui/status_bar.rs`
  - 预期: 找到 plugin_panel 分支
- [x] 验证 event.rs 中按键处理函数存在
  - `grep "handle_plugin_panel" peri-tui/src/event.rs`
  - 预期: 找到函数定义和调用点
- [x] 验证面板渲染模块存在
  - `grep "render_plugin_panel" peri-tui/src/ui/main_ui/panels/plugin.rs`
  - 预期: 找到函数定义
- [x] 验证面板高度计算包含 plugin_panel 分支
  - `grep "plugin_panel" peri-tui/src/ui/main_ui.rs`
  - 预期: 在渲染和高度计算两处找到引用

---

### Task 7: 插件系统端到端 验收（全局）

**前置条件:**

- spec-plan-1.md 中 Task 1-4 全部完成且测试通过
- spec-plan-2.md 中 Task 5-6 全部完成且测试通过
- 构建环境: `cargo build` 全 workspace 成功

**端到端验证:**

1. 运行全 workspace 测试确保无回归
   - `cargo test 2>&1 | tail -20`
   - 预期: 全部测试通过
   - 失败排查: 按 Task 逐步检查——先排除 Task 1 类型问题，再检查 Task 2-4 核心逻辑，最后检查 Task 5-6 集成

2. 验证 plugin.json 清单解析（兼容性核心）
   - `cargo test -p peri-middlewares --lib -- plugin::types::tests 2>&1 | tail -15`
   - 预期: roundtrip 测试通过，Claude Code 格式的 plugin.json 可正确反序列化
   - 失败排查: 检查 Task 1 types.rs 的 `#[serde(rename = "...")]` 属性与 Claude Code schemas.ts 是否一致

3. 验证 marketplace 发现链路
   - `cargo test -p peri-middlewares --lib -- plugin::marketplace 2>&1 | tail -10`
   - 预期: GitHub/URL/local/NPM 拉取逻辑测试通过
   - 失败排查: 检查 Task 2 marketplace.rs 的 git clone 模拟和 HTTP mock

4. 验证插件 skills/MCP/agents 注入集成
   - `cargo test -p peri-middlewares --lib -- plugin::loader 2>&1 | tail -10`
   - `cargo test -p peri-middlewares --lib -- skills 2>&1 | tail -5`
   - 预期: 插件 skills 路径追加、MCP 服务器合并测试通过
   - 失败排查: 检查 Task 5 SkillsMiddleware 扩展和 Task 4 loader 提取逻辑

5. 验证 TUI 插件面板功能
   - `cargo test -p peri-tui --lib 2>&1 | tail -15`
   - 预期: 所有 TUI 测试通过，包含 plugin_panel 测试
   - 失败排查: 检查 Task 6 的面板渲染、事件处理、状态栏集成

6. 验证 Headless 测试不写入真实 ~/.claude/
   - `grep "config_path_override" peri-tui/src/app/plugin_panel.rs`
   - 预期: 插件面板的配置写入使用 override 路径
   - 失败排查: 检查 Task 6 panel_ops.rs 的 open_plugin_panel 和配置保存路径
