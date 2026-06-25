# 插件系统兼容 Claude Code Marketplace 执行计划（一）

**目标:** 在 peri-middlewares/src/plugin/ 下实现兼容 Claude Code 的插件核心基础设施——类型定义、配置读写、marketplace 发现与缓存、插件安装管理、插件加载器与中间件。

**技术栈:** Rust, serde/serde_json, tokio (async), reqwest (HTTP), git CLI, tracing

**设计文档:** spec-design.md

## 改动总览

- Task 1 创建插件核心类型（`types.rs`）和配置读写模块（`config.rs`），在 `lib.rs` 注册 plugin 模块，为后续 Task 2-6 提供数据模型基础
- Task 2 的 marketplace 发现模块依赖 Task 1 中 `types.rs` 定义的 `MarketplaceSource`/`MarketplaceManifest`/`KnownMarketplace` 类型
- Task 3 的安装管理模块依赖 Task 1 中 `types.rs` 定义的 `InstalledPlugins`/`InstalledPlugin`/`InstallScope` 类型，以及 `config.rs` 提供的 `~/.claude/` 路径函数
- 关键设计决策：`PluginManifest.mcp_servers` 字段直接复用现有 `McpServerConfig` 类型（`peri-middlewares/src/mcp/config.rs:17`），通过 `use crate::mcp::McpServerConfig` 引用

---

### Task 0: 环境准备

**背景:**
确保构建和测试工具链可用，验证 plugin 模块的新增依赖不会破坏现有编译。

**执行步骤:**
- [x] 验证 cargo build 可用
  - `cargo build -p peri-middlewares`
  - 预期: 编译成功
- [x] 验证 cargo test 可用
  - `cargo test -p peri-middlewares --lib 2>&1 | tail -5`
  - 预期: 测试框架可用，无配置错误
- [x] 检查现有依赖中是否包含 `serde_yaml` 或 `gray_matter`
  - `grep -E "(serde_yaml|gray_matter)" peri-middlewares/Cargo.toml`
  - 预期: 至少有一个已存在（Task 4 使用 gray_matter 解析 frontmatter）
- [x] 检查 `dirs_next` 依赖已存在
  - `grep "dirs_next" peri-middlewares/Cargo.toml`
  - 预期: 找到依赖声明

**检查步骤:**
- [x] 构建命令执行成功
  - `cargo build -p peri-middlewares 2>&1 | tail -3`
  - 预期: 输出包含 "Finished" 且无 error
- [x] 测试命令执行成功
  - `cargo test -p peri-middlewares --lib 2>&1 | tail -5`
  - 预期: 输出包含 "test result" 且无 failures

---

### Task 1: 插件核心类型与模块引导

**背景:**
[业务语境] 本 Task 建立插件系统的数据模型层——定义兼容 Claude Code `plugin.json` 的清单类型、marketplace 类型、安装追踪类型，以及 `~/.claude/` 目录下的配置读写函数。这是整个插件链路（发现→安装→加载→集成）的数据基础。
[修改原因] 当前代码中不存在任何插件相关类型和模块，需要从零创建。现有 `mcp/` 模块（`src/mcp/config.rs`）已提供可复用的 `McpServerConfig` 类型，插件清单的 `mcpServers` 字段直接引用它。
[上下游影响] 本 Task 的输出（`types.rs` 中的所有类型、`config.rs` 中的路径函数）被 Task 2（marketplace 发现）、Task 3（安装管理）、Task 4（加载器与中间件）全部依赖。本 Task 无前置依赖。

**涉及文件:**
- 新建: `peri-middlewares/src/plugin/types.rs`
- 新建: `peri-middlewares/src/plugin/config.rs`
- 新建: `peri-middlewares/src/plugin/mod.rs`
- 修改: `peri-middlewares/src/lib.rs`

**执行步骤:**
- [x] 创建 `peri-middlewares/src/plugin/` 目录
  - `mkdir -p peri-middlewares/src/plugin/`

- [x] 编写插件核心类型定义文件 `types.rs`
  - 位置: `peri-middlewares/src/plugin/types.rs`（新建文件）
  - 文件头部引入依赖：
    ```rust
    use crate::mcp::McpServerConfig;
    use serde::{Deserialize, Serialize};
    use std::collections::HashMap;
    use std::path::PathBuf;
    ```
  - 定义 `PluginAuthor` 结构体：
    ```rust
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct PluginAuthor {
        pub name: String,
        #[serde(default)]
        pub url: Option<String>,
    }
    ```
  - 定义 `PluginCommand` 结构体：
    ```rust
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct PluginCommand {
        pub path: String,
        pub name: Option<String>,
        pub description: Option<String>,
    }
    ```
  - 定义 `PluginAgent` 结构体：
    ```rust
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct PluginAgent {
        pub path: String,
        pub name: String,
    }
    ```
  - 定义 `PluginLspServer` 结构体：
    ```rust
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct PluginLspServer {
        pub name: String,
        pub command: String,
        #[serde(default)]
        pub args: Vec<String>,
    }
    ```
  - 定义 `PluginChannel` 结构体：
    ```rust
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct PluginChannel {
        pub name: String,
        #[serde(rename = "mcpServer")]
        pub mcp_server: String,
    }
    ```
  - 定义 `PluginOption` 结构体：
    ```rust
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct PluginOption {
        pub name: String,
        pub description: String,
        #[serde(rename = "type")]
        pub option_type: String,
        pub default: Option<serde_json::Value>,
    }
    ```
  - 定义 `PluginManifest` 结构体——核心清单类型，所有字段除 `name`/`version` 外均 Option，兼容 Claude Code `plugin.json` 格式：
    ```rust
    /// 兼容 Claude Code 的插件清单
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct PluginManifest {
        pub name: String,
        pub version: String,
        #[serde(default)]
        pub description: String,
        pub author: Option<PluginAuthor>,
        pub commands: Option<Vec<PluginCommand>>,
        pub agents: Option<Vec<PluginAgent>>,
        pub skills: Option<Vec<String>>,
        /// 预留字段，本次不实现
        pub hooks: Option<serde_json::Value>,
        #[serde(rename = "mcpServers")]
        pub mcp_servers: Option<HashMap<String, McpServerConfig>>,
        #[serde(rename = "lspServers")]
        pub lsp_servers: Option<Vec<PluginLspServer>>,
        #[serde(rename = "outputStyles")]
        pub output_styles: Option<Vec<String>>,
        pub channels: Option<Vec<PluginChannel>>,
        pub options: Option<Vec<PluginOption>>,
        pub settings: Option<serde_json::Value>,
    }
    ```
  - 定义 `MarketplacePlugin` 结构体：
    ```rust
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct MarketplacePlugin {
        pub name: String,
        pub description: String,
        pub source: String,
        pub version: String,
        pub sha: Option<String>,
        pub author: Option<PluginAuthor>,
    }
    ```
  - 定义 `MarketplaceManifest` 结构体：
    ```rust
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct MarketplaceManifest {
        pub name: String,
        pub plugins: Vec<MarketplacePlugin>,
        #[serde(rename = "allowCrossMarketplaceDependenciesOn")]
        pub allow_cross_marketplace: Option<Vec<String>>,
    }
    ```
  - 定义 `MarketplaceSource` 枚举——带 serde tag 的枚举，JSON 序列化格式为 `{"source": "github", "repo": "..."}` ：
    ```rust
    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(tag = "source")]
    pub enum MarketplaceSource {
        #[serde(rename = "github")]
        GitHub { repo: String },
        #[serde(rename = "url")]
        Url { url: String },
        #[serde(rename = "file")]
        File { path: String },
        #[serde(rename = "directory")]
        Directory { path: String },
        #[serde(rename = "npm")]
        Npm { package: String },
    }
    ```
  - 定义 `InstallScope` 枚举：
    ```rust
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    pub enum InstallScope {
        User,
        Project,
        Local,
    }

    impl Default for InstallScope {
        fn default() -> Self {
            InstallScope::User
        }
    }
    ```
  - 定义 `InstalledPlugin` 结构体：
    ```rust
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct InstalledPlugin {
        pub id: String,
        pub name: String,
        pub version: String,
        pub marketplace: String,
        pub install_path: PathBuf,
        #[serde(default)]
        pub scope: InstallScope,
    }
    ```
  - 定义 `InstalledPlugins` 结构体——安装追踪文件的顶层类型，version 字段为格式版本号（当前为 2）：
    ```rust
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct InstalledPlugins {
        pub version: u32,
        #[serde(default)]
        pub plugins: Vec<InstalledPlugin>,
    }

    impl Default for InstalledPlugins {
        fn default() -> Self {
            Self { version: 2, plugins: Vec::new() }
        }
    }
    ```
  - 定义 `KnownMarketplace` 结构体：
    ```rust
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct KnownMarketplace {
        pub source: MarketplaceSource,
        #[serde(rename = "installLocation")]
        pub install_location: Option<PathBuf>,
        #[serde(default)]
        #[serde(rename = "autoUpdate")]
        pub auto_update: bool,
        #[serde(rename = "lastUpdated")]
        pub last_updated: Option<String>,
    }
    ```
  - 原因: 所有后续 Task 的数据模型均来自此文件，必须与 Claude Code 的 `schemas.ts` 类型定义保持字段级兼容

- [x] 编写配置读写模块 `config.rs`
  - 位置: `peri-middlewares/src/plugin/config.rs`（新建文件）
  - 文件头部引入依赖：
    ```rust
    use crate::plugin::types::{InstalledPlugins, KnownMarketplace, PluginManifest};
    use serde::{Deserialize, Serialize};
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};
    use thiserror::Error;
    ```
  - 定义 `ClaudeSettings` 结构体——用于读取 `~/.claude/settings.json` 中插件相关字段（`enabledPlugins`、`extraKnownMarketplaces`）：
    ```rust
    #[derive(Debug, Clone, Serialize, Deserialize, Default)]
    pub struct ClaudeSettings {
        #[serde(default)]
        #[serde(rename = "enabledPlugins")]
        pub enabled_plugins: Vec<String>,
        #[serde(default)]
        #[serde(rename = "extraKnownMarketplaces")]
        pub extra_known_marketplaces: Vec<KnownMarketplace>,
    }
    ```
  - 定义 `PluginConfigError` 错误类型：
    ```rust
    #[derive(Debug, Error)]
    pub enum PluginConfigError {
        #[error("插件配置文件解析失败: {path}: {source}")]
        ParseError {
            path: String,
            #[source]
            source: serde_json::Error,
        },
        #[error("插件配置文件读取失败: {path}: {source}")]
        ReadError {
            path: String,
            #[source]
            source: std::io::Error,
        },
        #[error("插件配置文件写入失败: {path}: {source}")]
        WriteError {
            path: String,
            #[source]
            source: std::io::Error,
        },
        #[error("插件清单缺少必需字段: {field}")]
        MissingField { field: String },
    }
    ```
  - 定义路径常量和获取函数：
    ```rust
    /// 返回 `~/.claude/` 根目录，不存在时返回 fallback（当前目录）
    pub fn claude_home() -> PathBuf {
        dirs_next::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".claude")
    }

    /// 返回 `~/.claude/plugins/` 目录
    pub fn plugins_dir() -> PathBuf {
        claude_home().join("plugins")
    }

    /// 返回 `~/.claude/plugins/known_marketplaces.json` 路径
    pub fn known_marketplaces_path() -> PathBuf {
        plugins_dir().join("known_marketplaces.json")
    }

    /// 返回 `~/.claude/plugins/installed_plugins.json` 路径
    pub fn installed_plugins_path() -> PathBuf {
        plugins_dir().join("installed_plugins.json")
    }

    /// 返回 `~/.claude/plugins/marketplaces/` 缓存目录
    pub fn marketplaces_cache_dir() -> PathBuf {
        plugins_dir().join("marketplaces")
    }

    /// 返回 `~/.claude/plugins/cache/` 插件版本缓存目录
    pub fn plugin_cache_dir() -> PathBuf {
        plugins_dir().join("cache")
    }

    /// 返回 `~/.claude/settings.json` 路径
    pub fn claude_settings_path() -> PathBuf {
        claude_home().join("settings.json")
    }
    ```
  - 定义 `load_installed_plugins(override_path: Option<&Path>) -> Result<InstalledPlugins, PluginConfigError>` 函数：
    - 参数 `override_path` 用于测试时重定向路径（headless 测试隔离机制）
    - 当 `override_path` 为 `None` 时使用 `installed_plugins_path()` 的默认路径
    - 文件不存在时返回 `InstalledPlugins::default()`（空列表，version=2）
    - 文件存在时读取并反序列化为 `InstalledPlugins`
    - 原因: 与现有 `mcp::config::load_from_path()` 保持一致的"文件不存在返回空默认值"模式
  - 定义 `save_installed_plugins(plugins: &InstalledPlugins, override_path: Option<&Path>) -> Result<(), PluginConfigError>` 函数：
    - 使用原子写入（先写 `.tmp` 再 `rename`，复用 `mcp::config` 中的 `atomic_write_json` 模式）
    - 写入前确保父目录存在（`std::fs::create_dir_all`）
  - 定义 `load_known_marketplaces(override_path: Option<&Path>) -> Result<Vec<KnownMarketplace>, PluginConfigError>` 函数：
    - 文件不存在时返回空 `Vec`
  - 定义 `save_known_marketplaces(marketplaces: &[KnownMarketplace], override_path: Option<&Path>) -> Result<(), PluginConfigError>` 函数：
    - 原子写入
  - 定义 `load_claude_settings(override_path: Option<&Path>) -> Result<ClaudeSettings, PluginConfigError>` 函数：
    - 读取 `~/.claude/settings.json`，提取 `enabledPlugins` 和 `extraKnownMarketplaces` 字段
    - 文件不存在时返回 `ClaudeSettings::default()`
    - 其他字段（非插件相关）忽略不报错
  - 定义 `load_plugin_manifest(plugin_dir: &Path) -> Result<PluginManifest, PluginConfigError>` 函数：
    - 读取 `{plugin_dir}/.claude-plugin/plugin.json`
    - 反序列化为 `PluginManifest`
    - 验证 `name` 和 `version` 非空（缺失时返回 `MissingField` 错误）
  - 原因: 这些函数是 Task 2-6 的基础设施工具，统一在此定义避免后续重复实现

- [x] 编写模块入口文件 `mod.rs`
  - 位置: `peri-middlewares/src/plugin/mod.rs`（新建文件）
  - 内容：
    ```rust
    pub mod config;
    pub mod types;

    pub use config::{
        claude_home, claude_settings_path, installed_plugins_path, known_marketplaces_path,
        load_claude_settings, load_installed_plugins, load_known_marketplaces, marketplaces_cache_dir,
        plugin_cache_dir, plugins_dir, save_installed_plugins, save_known_marketplaces,
        ClaudeSettings, PluginConfigError,
    };
    pub use types::{
        InstallScope, InstalledPlugin, InstalledPlugins, KnownMarketplace, MarketplaceManifest,
        MarketplacePlugin, MarketplaceSource, PluginAgent, PluginAuthor, PluginChannel,
        PluginCommand, PluginLspServer, PluginManifest, PluginOption,
    };
    ```
  - 原因: 与现有 `mcp/mod.rs`（`peri-middlewares/src/mcp/mod.rs`）结构一致——子模块声明 + 公共类型重导出

- [x] 在 `lib.rs` 中注册 plugin 模块
  - 位置: `peri-middlewares/src/lib.rs`，在 `pub mod skills;`（L32）之后、`pub mod tools;`（L33）之前插入
  - 插入内容：
    ```rust
    pub mod plugin;
    pub use plugin::{
        ClaudeSettings, InstallScope, InstalledPlugin, InstalledPlugins, KnownMarketplace,
        MarketplaceManifest, MarketplacePlugin, MarketplaceSource, PluginAgent, PluginAuthor,
        PluginChannel, PluginCommand, PluginConfigError, PluginLspServer, PluginManifest,
        PluginOption,
    };
    ```
  - 在 prelude 模块（L59 的 `pub mod prelude {` 内部）中，在 `pub use crate::skills::` 之前插入：
    ```rust
    pub use crate::plugin::{
        ClaudeSettings, InstallScope, InstalledPlugin, InstalledPlugins, KnownMarketplace,
        MarketplaceManifest, MarketplacePlugin, MarketplaceSource, PluginAgent, PluginAuthor,
        PluginChannel, PluginCommand, PluginConfigError, PluginLspServer, PluginManifest,
        PluginOption,
    };
    ```
  - 原因: 遵循现有模块注册模式（`pub mod mcp;` + `pub use mcp::{...};`），prelude 导出确保外部 crate 可通过 `use peri_middlewares::prelude::*;` 获取插件类型

- [x] 为 `types.rs` 核心类型编写单元测试
  - 测试文件: `peri-middlewares/src/plugin/types.rs`（文件底部 `#[cfg(test)] mod tests { ... }` 块）
  - 测试场景:
    - `test_plugin_manifest_minimal`: 仅含 `name` + `version` 的 JSON 反序列化为 `PluginManifest`，其余字段均为 `None`
    - `test_plugin_manifest_full`: 包含所有字段的完整 JSON 反序列化，验证 `mcp_servers` 中的 `McpServerConfig` 正确解析
    - `test_plugin_manifest_mcp_servers_rename`: 验证 JSON 中 `"mcpServers"` 键正确映射到 `mcp_servers` 字段（`#[serde(rename = "mcpServers")]`）
    - `test_marketplace_source_github`: `{"source":"github","repo":"anthropics/claude-plugins-official"}` 反序列化为 `MarketplaceSource::GitHub { repo: "..." }`
    - `test_marketplace_source_url`: `{"source":"url","url":"https://example.com/marketplace.json"}` 反序列化为 `MarketplaceSource::Url { url: "..." }`
    - `test_installed_plugins_default`: `InstalledPlugins::default()` 的 `version` 为 2、`plugins` 为空
    - `test_install_scope_default`: `InstallScope::default()` 为 `InstallScope::User`
    - `test_known_marketplace_deserialize`: 包含所有字段的完整 JSON 反序列化
    - `test_plugin_manifest_serialization_roundtrip`: 完整清单序列化后再反序列化，字段一致
  - 运行命令: `cargo test -p peri-middlewares --lib -- plugin::types::tests`
  - 预期: 所有测试通过

- [x] 为 `config.rs` 配置读写函数编写单元测试
  - 测试文件: `peri-middlewares/src/plugin/config.rs`（文件底部 `#[cfg(test)] mod tests { ... }` 块）
  - 测试场景:
    - `test_load_installed_plugins_nonexistent`: 传入不存在的临时目录路径，返回 `InstalledPlugins::default()`
    - `test_save_and_load_installed_plugins`: 先 `save_installed_plugins` 写入临时文件，再 `load_installed_plugins` 读回，验证数据一致
    - `test_load_known_marketplaces_nonexistent`: 传入不存在的路径，返回空 `Vec`
    - `test_save_and_load_known_marketplaces`: 写入后读回，验证 `KnownMarketplace` 数据一致
    - `test_load_claude_settings_nonexistent`: 传入不存在的路径，返回 `ClaudeSettings::default()`
    - `test_load_claude_settings_with_plugins`: 构造含 `enabledPlugins` 和 `extraKnownMarketplaces` 的 JSON 文件，验证正确解析
    - `test_load_claude_settings_ignores_unknown_fields`: JSON 中包含非插件字段（如 `"otherKey": 42`），不报错，仅提取插件相关字段
    - `test_load_plugin_manifest_success`: 构造 `{temp}/.claude-plugin/plugin.json` 文件，验证正确解析
    - `test_load_plugin_manifest_missing_name`: 构造不含 `name` 字段的 JSON，返回 `PluginConfigError::MissingField`
    - `test_load_plugin_manifest_missing_version`: 构造不含 `version` 字段的 JSON，返回 `PluginConfigError::MissingField`
    - `test_plugins_dir_uses_claude_home`: 验证 `plugins_dir()` 返回路径以 `.claude/plugins` 结尾
    - 所有文件操作使用 `tempfile::tempdir()` 创建临时目录，通过 `override_path` 参数传入
  - 运行命令: `cargo test -p peri-middlewares --lib -- plugin::config::tests`
  - 预期: 所有测试通过

**检查步骤:**
- [x] 验证 plugin 模块编译通过
  - `cargo build -p peri-middlewares 2>&1 | tail -5`
  - 预期: 输出 `Finished` 且无编译错误
- [x] 验证 types.rs 单元测试全部通过
  - `cargo test -p peri-middlewares --lib -- plugin::types::tests 2>&1 | tail -10`
  - 预期: 所有 test 以 `ok` 结尾，无 failure
- [x] 验证 config.rs 单元测试全部通过
  - `cargo test -p peri-middlewares --lib -- plugin::config::tests 2>&1 | tail -10`
  - 预期: 所有 test 以 `ok` 结尾，无 failure
- [x] 验证 plugin 模块已注册到 lib.rs
  - `grep -n "pub mod plugin" peri-middlewares/src/lib.rs`
  - 预期: 输出包含 `pub mod plugin;` 的行号
- [x] 验证 prelude 导出包含插件类型
  - `grep -c "PluginManifest" peri-middlewares/src/lib.rs`
  - 预期: 输出 >= 2（mod 级导出 + prelude 导出）
- [x] 验证 McpServerConfig 复用关系正确
  - `grep "use crate::mcp::McpServerConfig" peri-middlewares/src/plugin/types.rs`
  - 预期: 输出该行，确认引用路径正确

---

### Task 2: Marketplace 发现与缓存

**背景:**
[业务语境] — 实现从多种来源（GitHub/URL/本地/NPM）拉取 marketplace.json 并缓存到 `~/.claude/plugins/marketplaces/`，支持后台异步刷新，为 TUI 面板和插件安装器提供可用插件列表。
[修改原因] — 当前系统无 marketplace 发现能力，用户无法浏览和安装 Claude Code 生态的插件。Task 1 已定义了 `MarketplaceSource`、`MarketplaceManifest`、`KnownMarketplace` 等类型和 `marketplaces_cache_dir()` 路径函数，但缺少实际的拉取、解析和缓存逻辑。
[上下游影响] — 本 Task 输出的 `marketplace.rs` 被 Task 3（installer.rs，安装时从 marketplace manifest 查找插件 source 路径）和 Task 6（TUI PluginPanel，展示可用插件列表）依赖。本 Task 依赖 Task 1 的 `types.rs`（MarketplaceSource、MarketplaceManifest、KnownMarketplace）和 `config.rs`（路径函数、ClaudeSettings）。

**涉及文件:**
- 新建: `peri-middlewares/src/plugin/marketplace.rs`
- 修改: `peri-middlewares/src/plugin/mod.rs`
- 修改: `peri-middlewares/Cargo.toml`（新增 flate2 + tar 依赖）

**执行步骤:**
- [x] 在 `Cargo.toml` 中新增 NPM tarball 解压依赖
  - 位置: `peri-middlewares/Cargo.toml`，在 `regex = "1"`（L38）行之后插入
  - 插入内容：
    ```toml
    flate2 = "1"
    tar = "0.4"
    ```
  - 原因: NPM 源拉取需要解压 `.tgz` tarball；`tokio` workspace 依赖已包含 `full` features（含 `process`），无需额外添加

- [x] 实现 `MarketplaceError` 错误类型和状态枚举
  - 位置: `peri-middlewares/src/plugin/marketplace.rs`（新建文件）
  - 文件头部引入：
    ```rust
    use crate::plugin::config::{self, load_claude_settings, marketplaces_cache_dir};
    use crate::plugin::types::{KnownMarketplace, MarketplaceManifest, MarketplacePlugin, MarketplaceSource, PluginAuthor};
    use chrono::{DateTime, Utc};
    use serde::{Deserialize, Serialize};
    use std::path::{Path, PathBuf};
    use thiserror::Error;
    use tokio::sync::mpsc;
    use tracing::{debug, warn};
    ```
  - 定义 `MarketplaceError`：
    ```rust
    #[derive(Debug, Error)]
    pub enum MarketplaceError {
        #[error("Git 操作失败: {0}")]
        GitFailed(String),
        #[error("HTTP 请求失败: {0}")]
        HttpFailed(String),
        #[error("JSON 解析失败: {0}")]
        ParseFailed(String),
        #[error("marketplace.json 未找到: {path}")]
        ManifestNotFound { path: String },
        #[error("NPM 操作失败: {0}")]
        NpmFailed(String),
        #[error("IO 错误: {0}")]
        Io(#[from] std::io::Error),
    }
    ```
  - 定义 `MarketplaceStatus`：
    ```rust
    #[derive(Debug, Clone, PartialEq)]
    pub enum MarketplaceStatus {
        /// 从缓存加载成功（未执行网络请求）
        Cached,
        /// 正在拉取
        Fetching,
        /// 拉取成功
        Fresh,
        /// 拉取失败（保留缓存可用）
        Stale(String),
        /// 从未拉取
        NotFetched,
    }
    ```
  - 定义 `MarketplaceEntry` — 单个 marketplace 的运行时状态：
    ```rust
    pub struct MarketplaceEntry {
        pub name: String,
        pub source: MarketplaceSource,
        pub manifest: Option<MarketplaceManifest>,
        pub status: MarketplaceStatus,
        pub last_updated: Option<DateTime<Utc>>,
        pub auto_update: bool,
    }
    ```
  - 定义 `AvailablePlugin` — 聚合视图，供 TUI 和安装器使用：
    ```rust
    pub struct AvailablePlugin {
        pub name: String,
        pub description: String,
        pub version: String,
        pub marketplace: String,
        pub source: String,
        pub author: Option<PluginAuthor>,
    }
    ```
  - 定义 `MarketplaceRefreshEvent` — 后台刷新通知事件：
    ```rust
    #[derive(Debug, Clone)]
    pub enum MarketplaceRefreshEvent {
        Updated { index: usize, name: String },
        Failed { index: usize, name: String, error: String },
    }
    ```
  - 原因: 错误类型和状态枚举是 marketplace 发现的核心抽象，供 MarketplaceManager 和调用方使用

- [x] 实现 `find_marketplace_json` 和 `read_manifest_from_path` — 通用辅助函数
  - 位置: `peri-middlewares/src/plugin/marketplace.rs`，在类型定义之后
  - 实现 `fn find_marketplace_json(dir: &Path) -> Option<PathBuf>`：
    - 按优先级依次检查: `{dir}/marketplace.json` → `{dir}/.claude-plugin/marketplace.json`
    - 第一个存在的路径返回 `Some`，都不存在返回 `None`
  - 实现 `fn read_manifest_from_path(path: &Path) -> Result<MarketplaceManifest, MarketplaceError>`：
    - `std::fs::read_to_string(path)` 读取文件内容
    - `serde_json::from_str::<MarketplaceManifest>(&content)` 解析
    - IO 错误映射为 `MarketplaceError::Io`
    - JSON 解析错误映射为 `MarketplaceError::ParseFailed`
  - 原因: GitHub/Directory/NPM 三种源都需要在目录中查找 marketplace.json，提取为公共函数避免重复

- [x] 实现 `fetch_github` — GitHub 源拉取
  - 位置: `peri-middlewares/src/plugin/marketplace.rs`，在辅助函数之后
  - 实现函数签名: `async fn fetch_github(name: &str, repo: &str, cache_base: &Path, auto_update: bool) -> Result<MarketplaceManifest, MarketplaceError>`
  - 逻辑:
    1. 拼接 `cache_dir = cache_base.join(name)`（GitHub 源缓存为目录）
    2. 判断 `cache_dir.exists()`:
       - 不存在 → `tokio::process::Command::new("git")` 执行 `["clone", "--depth", "1", format!("https://github.com/{repo}.git"), cache_dir.display().to_string()]`，用 `tokio::time::timeout(Duration::from_secs(30), cmd.output()).await` 控制超时
       - 已存在且 `auto_update` 为 true → `tokio::process::Command::new("git")` 执行 `["-C", cache_dir.display().to_string(), "pull", "--ff-only"]`，同样 30 秒超时
       - 已存在且 `auto_update` 为 false → 跳过网络请求，直接读缓存
    3. git 命令失败时: 若缓存目录存在且有 marketplace.json，记录 `warn!` 日志并回退到缓存读取；否则返回 `MarketplaceError::GitFailed(stderr)`
    4. 调用 `find_marketplace_json(&cache_dir)` 定位 manifest 文件
    5. 找不到 → 返回 `MarketplaceError::ManifestNotFound`
    6. 调用 `read_manifest_from_path` 解析并返回
  - 原因: GitHub 是官方 marketplace 的默认来源，clone/pull 双路径逻辑是核心

- [x] 实现 `fetch_url` — URL 源拉取
  - 位置: `peri-middlewares/src/plugin/marketplace.rs`，在 `fetch_github` 之后
  - 实现函数签名: `async fn fetch_url(name: &str, url: &str, cache_base: &Path) -> Result<MarketplaceManifest, MarketplaceError>`
  - 逻辑:
    1. 拼接 `cache_file = cache_base.join(format!("{name}.json"))`
    2. 读取缓存文件元数据获取 `last_modified`（`std::fs::metadata(&cache_file).ok().and_then(|m| m.modified().ok())`），转为 HTTP date 格式（使用 `chrono` 的 `DateTime::from(last_modified).format("%a, %d %b %Y %H:%M:%S GMT")`）
    3. 构造 `reqwest::Client::builder().timeout(Duration::from_secs(15)).build()?`
    4. 构造请求: `let mut req = client.get(url);`，若 `last_modified` 非空则 `req = req.header("If-Modified-Since", last_modified_str)`
    5. `let response = req.send().await.map_err(|e| { /* 若缓存文件存在则尝试回退 */ ... })?`
    6. 匹配 `response.status().as_u16()`:
       - 304 → 从缓存文件调用 `read_manifest_from_path(&cache_file)` 读取并返回
       - 200 → 读取 `response.text().await?`，写入缓存文件（`std::fs::write(&cache_file, &body)`），解析为 `MarketplaceManifest`
       - 其他 → 返回 `MarketplaceError::HttpFailed(format!("HTTP {}", status))`
    7. 请求本身失败（网络错误、超时等）→ 若缓存文件存在则调用 `read_manifest_from_path(&cache_file)` 返回，记录 `warn!` 日志；否则返回 `MarketplaceError::HttpFailed(e.to_string())`
  - 原因: URL 源是轻量级分发方式，HTTP 条件请求减少带宽消耗

- [x] 实现 `read_file` 和 `read_directory` — 本地源读取
  - 位置: `peri-middlewares/src/plugin/marketplace.rs`，在 `fetch_url` 之后
  - 实现 `fn read_file(path: &Path) -> Result<MarketplaceManifest, MarketplaceError>`:
    - 直接调用 `read_manifest_from_path(path)`
  - 实现 `fn read_directory(path: &Path) -> Result<MarketplaceManifest, MarketplaceError>`:
    - 调用 `find_marketplace_json(path)`
    - `None` → 返回 `MarketplaceError::ManifestNotFound { path: path.display().to_string() }`
    - `Some(p)` → 调用 `read_manifest_from_path(&p)`
  - 原因: 本地源用于开发调试和离线场景，逻辑最简单

- [x] 实现 `fetch_npm` — NPM 源拉取
  - 位置: `peri-middlewares/src/plugin/marketplace.rs`，在本地源函数之后
  - 实现函数签名: `async fn fetch_npm(name: &str, package: &str, cache_base: &Path) -> Result<MarketplaceManifest, MarketplaceError>`
  - 逻辑:
    1. 拼接 `cache_dir = cache_base.join(name)`
    2. 调用 `find_marketplace_json(&cache_dir)` 检查缓存是否已有 manifest
    3. 若有缓存 → 直接读取并返回（NPM 源不做自动更新）
    4. 若无缓存 → 创建临时目录 `tmp_dir = tempfile::tempdir()?`
    5. `tokio::process::Command::new("npm")` 执行 `["pack", package, "--pack-destination", tmp_dir.display().to_string()]`，超时 60 秒
    6. 命令失败 → 返回 `MarketplaceError::NpmFailed(stderr)`
    7. 在 `tmp_dir` 中查找 `.tgz` 文件: `std::fs::read_dir(&tmp_dir)?.find(|e| e.as_ref().map(|f| f.path().extension().map(|ext| ext == "tgz").unwrap_or(false)).unwrap_or(false))`
    8. 使用 `std::fs::File::open(tgz_path)?` → `flate2::read::GzDecoder::new(file)` → `tar::Archive::new(decoder)` → `.unpack(&cache_dir)` 解压
    9. 调用 `find_marketplace_json(&cache_dir)` 定位 manifest
    10. 找不到 → 返回 `MarketplaceError::ManifestNotFound`
    11. 调用 `read_manifest_from_path` 解析并返回
  - 原因: NPM 源覆盖 Node.js 生态，使用 `npm pack` + tarball 解压方案

- [x] 实现 `MarketplaceManager` 结构体和 `new` 方法
  - 位置: `peri-middlewares/src/plugin/marketplace.rs`，在所有拉取函数之后
  - 定义结构体:
    ```rust
    pub struct MarketplaceManager {
        entries: Vec<MarketplaceEntry>,
        override_dir: Option<PathBuf>,
    }
    ```
  - 实现 `pub fn new(override_dir: Option<PathBuf>) -> Self`:
    - 返回 `MarketplaceManager { entries: Vec::new(), override_dir }`
  - 原因: MarketplaceManager 是 marketplace 子系统的统一入口

- [x] 实现 `MarketplaceManager::init` — 初始化入口
  - 位置: `peri-middlewares/src/plugin/marketplace.rs`，在 `new` 之后
  - 实现方法: `pub async fn init(&mut self, tx: mpsc::Sender<MarketplaceRefreshEvent>) -> Vec<tokio::task::JoinHandle<()>>`
  - 逻辑:
    1. 调用 `config::load_known_marketplaces(self.override_dir.as_deref())` 获取已知 marketplace 列表
    2. 调用 `load_claude_settings(self.override_dir.as_deref())` 获取 `extra_known_marketplaces`
    3. 合并: 将 `extra_known_marketplaces` 中不存在于已知列表的条目追加（按 `source` 字段的 JSON 序列化结果去重）
    4. 检查官方 marketplace 是否存在: 遍历列表查找 `MarketplaceSource::GitHub { repo }` 中 `repo == "anthropics/claude-plugins-official"`
    5. 不存在 → 创建 `KnownMarketplace { source: MarketplaceSource::GitHub { repo: "anthropics/claude-plugins-official".into() }, install_location: None, auto_update: true, last_updated: None }`，追加到列表，调用 `config::save_known_marketplaces` 持久化
    6. 对每个 `KnownMarketplace`:
       a. 从 `source` 变体提取名称: `GitHub { repo }` → repo 的 `/` 后面部分（如 `claude-plugins-official`）；`Url { url }` → URL 最后路径段去掉 `.json` 后缀（使用 `url::Url::parse(url).unwrap().path_segments().unwrap().last().unwrap()`）；`File { path }` → 文件名去掉 `.json`；`Directory { path }` → 目录名；`Npm { package }` → package 名
       b. 调用 `try_load_cache(&source, &name)` 尝试加载缓存
       c. 创建 `MarketplaceEntry { name, source: km.source.clone(), manifest: cached_manifest, status: if cached_manifest.is_some() { MarketplaceStatus::Cached } else { MarketplaceStatus::NotFetched }, last_updated: km.last_updated.as_ref().and_then(|s| DateTime::parse_from_rfc3339(s).ok()).map(|dt| dt.with_timezone(&Utc)), auto_update: km.auto_update }`
    7. 对每个条目调用 `spawn_refresh(index, tx)` 创建后台任务，收集 JoinHandle
    8. 返回 JoinHandle 列表
  - 原因: `init()` 是 marketplace 子系统的入口函数，负责加载已知列表、自动注册官方源、从缓存预热、启动后台刷新

- [x] 实现 `MarketplaceManager::try_load_cache` — 缓存加载分发
  - 位置: `peri-middlewares/src/plugin/marketplace.rs`，在 `init` 之后
  - 实现私有方法: `fn try_load_cache(&self, source: &MarketplaceSource, name: &str) -> Option<MarketplaceManifest>`
  - 逻辑:
    - `cache_base` = 若 `self.override_dir` 非空则 `self.override_dir.join("marketplaces")`，否则调用 `marketplaces_cache_dir()`
    - 匹配 `source`:
      - `GitHub { .. }` → `find_marketplace_json(&cache_base.join(name))`
      - `Url { .. }` → 若 `cache_base.join(format!("{name}.json")).exists()` 则返回 `Some(path)`
      - `File { path }` → 若 `PathBuf::from(path).exists()` 则返回 `Some(PathBuf::from(path))`
      - `Directory { path }` → `find_marketplace_json(Path::new(path))`
      - `Npm { .. }` → `find_marketplace_json(&cache_base.join(name))`
    - 对返回的 `Option<PathBuf>` 调用 `read_manifest_from_path`，`Ok` 则 `Some(manifest)`，`Err` 则记录 `debug!` 日志并返回 `None`
  - 原因: 将缓存查找逻辑集中到一处，`init()` 和拉取函数都复用它

- [x] 实现 `MarketplaceManager::spawn_refresh` — 后台异步刷新
  - 位置: `peri-middlewares/src/plugin/marketplace.rs`，在 `try_load_cache` 之后
  - 实现方法: `pub fn spawn_refresh(&self, index: usize, tx: mpsc::Sender<MarketplaceRefreshEvent>) -> tokio::task::JoinHandle<()>`
  - 逻辑:
    1. 克隆 `self.entries[index]` 的 `name`、`source`、`auto_update`、`override_dir`
    2. `tokio::spawn(async move { ... })`:
       a. 设置 `cache_base`（同 `try_load_cache` 的逻辑）
       b. 根据 `source` 变体调用对应的 fetch 函数:
          - `GitHub { repo }` → `fetch_github(&name, &repo, &cache_base, auto_update).await`
          - `Url { url }` → `fetch_url(&name, &url, &cache_base).await`
          - `File { path }` → `tokio::task::spawn_blocking(move || read_file(Path::new(&path))).await.expect("spawn_blocking panicked")`
          - `Directory { path }` → `tokio::task::spawn_blocking(move || read_directory(Path::new(&path))).await.expect("spawn_blocking panicked")`
          - `Npm { package }` → `fetch_npm(&name, &package, &cache_base).await`
       c. `Ok(manifest)` → `let _ = tx.send(MarketplaceRefreshEvent::Updated { index, name: name.clone() }).await;`
       d. `Err(e)` → `let _ = tx.send(MarketplaceRefreshEvent::Failed { index, name: name.clone(), error: e.to_string() }).await; warn!("Marketplace '{}' 刷新失败: {}", name, e);`
    3. 返回 JoinHandle
  - 原因: 后台刷新不阻塞 agent 启动，TUI 通过 channel 接收更新通知，与 MCP 连接池的 `tokio::spawn` 模式一致

- [x] 实现 `MarketplaceManager` 的查询方法
  - 位置: `peri-middlewares/src/plugin/marketplace.rs`，在 `spawn_refresh` 之后
  - 实现 `pub fn entries(&self) -> &[MarketplaceEntry]` — 返回只读切片
  - 实现 `pub fn update_entry(&mut self, index: usize, manifest: MarketplaceManifest, status: MarketplaceStatus)` — 后台刷新完成后由调用方更新 entries 中的 manifest 和 status，同时更新 `last_updated` 为 `Utc::now()`
  - 实现 `pub fn find_plugin(&self, plugin_name: &str) -> Option<(&MarketplacePlugin, &str)>`:
    - 遍历 `self.entries`，过滤 `status == Cached || status == Fresh`
    - 对每个有效 entry 的 `manifest.plugins` 查找 `plugin.name == plugin_name`
    - 找到则返回 `Some((&plugin, &entry.name))`
  - 原因: 安装器（Task 3）通过此方法查找插件的 source 路径
  - 实现 `pub fn available_plugins(&self) -> Vec<AvailablePlugin>`:
    - 遍历 `self.entries`，过滤 `Cached || Fresh`
    - 对每个有效 entry 的 `manifest.plugins`，映射为 `AvailablePlugin { name: p.name.clone(), description: p.description.clone(), version: p.version.clone(), marketplace: entry.name.clone(), source: p.source.clone(), author: p.author.clone() }`
    - 收集为 `Vec<AvailablePlugin>`
  - 原因: TUI PluginPanel（Task 6）通过此方法获取可用插件列表展示

- [x] 在 `mod.rs` 中注册 marketplace 模块并导出公共 API
  - 位置: `peri-middlewares/src/plugin/mod.rs`，在 `pub mod types;` 之后插入
  - 插入: `pub mod marketplace;`
  - 在 `pub use types::{ ... };` 之后追加:
    ```rust
    pub use marketplace::{
        AvailablePlugin, MarketplaceEntry, MarketplaceError, MarketplaceManager, MarketplaceRefreshEvent,
        MarketplaceStatus,
    };
    ```
  - 在 `lib.rs` 的 `pub use plugin::{ ... };` 列表末尾追加: `AvailablePlugin, MarketplaceEntry, MarketplaceError, MarketplaceManager, MarketplaceRefreshEvent, MarketplaceStatus`
  - 在 `lib.rs` 的 `pub mod prelude { ... }` 的 `pub use crate::plugin::{ ... };` 列表末尾同样追加
  - 原因: 遵循现有模块注册和导出模式

- [x] 为 GitHub 拉取和辅助函数编写单元测试
  - 测试文件: `peri-middlewares/src/plugin/marketplace.rs`（文件底部 `#[cfg(test)] mod tests { ... }` 块）
  - 测试场景:
    - `test_find_marketplace_json_root`: 临时目录下直接有 `marketplace.json` → 返回 `Some` 指向该文件
    - `test_find_marketplace_json_subdir`: 临时目录下仅 `.claude-plugin/marketplace.json` → 返回 `Some` 指向子目录文件
    - `test_find_marketplace_json_not_found`: 空临时目录 → 返回 `None`
    - `test_find_marketplace_json_priority`: 根目录和子目录都有 marketplace.json → 优先返回根目录的
    - `test_read_manifest_from_path_success`: 构造有效 JSON 文件（含 `name` 和 `plugins` 数组） → 返回正确的 `MarketplaceManifest`
    - `test_read_manifest_from_path_invalid_json`: 构造无效 JSON 文件 → 返回 `MarketplaceError::ParseFailed`
    - `test_read_manifest_from_path_not_found`: 传入不存在的路径 → 返回 `MarketplaceError::Io`
    - `test_fetch_github_cache_hit`: 构造缓存目录并放置 `.claude-plugin/marketplace.json`，`auto_update=false` → 直接返回缓存 manifest，不调用 git
  - 使用 `tempfile::tempdir()` 创建临时目录，手动放置测试文件
  - 运行命令: `cargo test -p peri-middlewares --lib -- plugin::marketplace::tests::test_find`
  - 预期: 所有测试通过

- [x] 为本地源读取编写单元测试
  - 测试文件: `peri-middlewares/src/plugin/marketplace.rs`（内联测试模块）
  - 测试场景:
    - `test_read_file_success`: 构造有效 marketplace.json 临时文件 → 返回正确的 `MarketplaceManifest`
    - `test_read_file_not_found`: 传入不存在的路径 → 返回 IO 错误
    - `test_read_directory_root`: 目录下有 marketplace.json → 正确解析
    - `test_read_directory_subdir`: 仅 `.claude-plugin/marketplace.json` → 正确解析
    - `test_read_directory_not_found`: 空目录 → 返回 `MarketplaceError::ManifestNotFound`
  - 使用 `tempfile::tempdir()` 创建临时目录和文件
  - 运行命令: `cargo test -p peri-middlewares --lib -- plugin::marketplace::tests::test_read`
  - 预期: 所有测试通过

- [x] 为 URL 拉取逻辑编写单元测试
  - 测试文件: `peri-middlewares/src/plugin/marketplace.rs`（内联测试模块）
  - 测试场景:
    - `test_fetch_url_cache_fallback`: 构造缓存文件，不启动 HTTP 服务器 → `fetch_url` 应返回缓存中的 manifest（通过使用不可达端口触发网络错误，测试回退逻辑）
    - `test_fetch_url_no_cache_no_server`: 无缓存、无服务器 → 返回 `MarketplaceError::HttpFailed`
  - 网络相关测试标记为: `#[tokio::test] #[cfg_attr(not(feature = "integration"), ignore)]`，仅在有 `integration` feature 时运行
  - 运行命令: `cargo test -p peri-middlewares --lib -- plugin::marketplace::tests::test_url`
  - 预期: 缓存回退测试通过；integration test 在默认配置下正确跳过

- [x] 为 MarketplaceManager 编写单元测试
  - 测试文件: `peri-middlewares/src/plugin/marketplace.rs`（内联测试模块）
  - 测试场景:
    - `test_manager_auto_register_official`: 创建 `MarketplaceManager` 并 `init()`，使用临时 override_dir → 验证 `known_marketplaces.json` 中自动追加了 `anthropics/claude-plugins-official`，且 `auto_update` 为 true
    - `test_manager_merge_extra_known_marketplaces`: 在 override_dir 中放置含 `extraKnownMarketplaces` 的 settings.json（路径为 `{override_dir}/settings.json`） → `init()` 后 entries 包含合并后的 marketplace 列表
    - `test_manager_cache_loading`: 在 override_dir 的 `marketplaces` 子目录放置有效的 `test-marketplace.json`（URL 源缓存格式），并在 `known_marketplaces.json` 中注册该 URL 源 → `init()` 后对应 entry 状态为 `Cached`，manifest 非空
    - `test_manager_find_plugin`: 手动构造 entries（含一个 Cached 状态的 entry，其 manifest 包含目标 plugin） → `find_plugin` 返回 `Some`
    - `test_manager_find_plugin_not_found`: entries 中无匹配 plugin → `find_plugin` 返回 `None`
    - `test_manager_available_plugins`: 手动构造 entries（含多个 Cached 状态的 entry，各含插件） → `available_plugins` 返回所有插件的聚合列表
    - `test_manager_update_entry`: 调用 `update_entry` 后 → entry 的 manifest 和 status 更新为指定值，`last_updated` 非 None
  - 使用 `tempfile::tempdir()` 作为 override_dir，手动构造 `known_marketplaces.json`、`settings.json`、缓存文件
  - `init()` 需要传入 `mpsc::channel(16).0` 作为 tx 参数
  - 对不需要真实网络请求的测试，使用 `MarketplaceSource::File` 或 `MarketplaceSource::Directory` 类型的已知 marketplace，指向测试临时目录中的文件
  - 运行命令: `cargo test -p peri-middlewares --lib -- plugin::marketplace::tests::test_manager`
  - 预期: 所有测试通过

**检查步骤:**
- [x] 验证 marketplace 模块编译通过
  - `cargo build -p peri-middlewares 2>&1 | tail -5`
  - 预期: 输出 `Finished` 且无编译错误
- [x] 验证所有 marketplace 单元测试通过（非 integration）
  - `cargo test -p peri-middlewares --lib -- plugin::marketplace::tests 2>&1 | tail -15`
  - 预期: 所有 test 以 `ok` 结尾，标记 `ignore` 的 integration test 显示为 `ignored`
- [x] 验证新增依赖声明存在
  - `grep -E "^(flate2|tar)" peri-middlewares/Cargo.toml`
  - 预期: 输出包含 `flate2 = "1"` 和 `tar = "0.4"` 两行
- [x] 验证 mod.rs 导出 marketplace 类型
  - `grep "MarketplaceManager" peri-middlewares/src/plugin/mod.rs`
  - 预期: 输出包含 `MarketplaceManager` 的行
- [x] 验证 lib.rs 和 prelude 导出更新
  - `grep -c "MarketplaceManager" peri-middlewares/src/lib.rs`
  - 预期: 输出 >= 2（mod 级导出 + prelude 导出）
- [x] 验证 Task 1 测试无回归
  - `cargo test -p peri-middlewares --lib -- plugin::types::tests 2>&1 | tail -5`
  - 预期: 所有测试通过
- [x] 验证 Task 1 config 测试无回归
  - `cargo test -p peri-middlewares --lib -- plugin::config::tests 2>&1 | tail -5`
  - 预期: 所有测试通过
- [x] 验证全量 middlewares 测试无回归
  - `cargo test -p peri-middlewares --lib 2>&1 | tail -10`
  - 预期: 所有测试通过，无回归

---

### Task 3: 插件安装管理

**背景:**
[业务语境] — 用户从 marketplace 浏览插件后，需要一键安装到本地，安装后的插件才能被加载器解析并注入到 agent 运行时。同时提供卸载、更新能力以管理插件生命周期。
[修改原因] — 当前不存在任何插件安装/卸载逻辑，Task 1 定义了 InstalledPlugins 等追踪类型和 config.rs 读写函数，Task 2 提供了 marketplace manifest 缓存，本 Task 在此基础上实现安装/卸载/更新的完整流程。
[上下游影响] — 本 Task 的输出（installed_plugins.json 写入、settings.json 的 enabledPlugins 更新）被 Task 4（插件加载器）和 Task 6（TUI 插件面板）依赖。本 Task 依赖 Task 1 的 types.rs/config.rs 和 Task 2 的 marketplace.rs。

**涉及文件:**
- 新建: `peri-middlewares/src/plugin/installer.rs`
- 修改: `peri-middlewares/src/plugin/mod.rs`（导出 installer 模块公共 API）

**执行步骤:**

- [x] 创建 installer.rs 并定义 InstallerError 错误类型
  - 位置: `peri-middlewares/src/plugin/installer.rs` 文件顶部
  - 定义 `InstallerError` 枚举，变体包含：
    - `PluginNotFound { name: String, marketplace: String }` — marketplace manifest 中无此插件
    - `ManifestInvalid { path: PathBuf, source: serde_json::Error }` — plugin.json 解析失败
    - `CopyFailed { src: PathBuf, dst: PathBuf, source: std::io::Error }` — 目录复制失败
    - `ConfigError(#[from] PluginConfigError)` — 包装 config.rs 读写错误
    - `SettingsError(String)` — settings.json enabledPlugins 更新失败
  - 使用 `thiserror::Error` 派生宏自动实现
  - 原因: 插件安装涉及文件系统、JSON 解析、配置读写多个失败点，需要精确的错误类型供上层处理

- [x] 实现 `install_plugin` 核心函数
  - 位置: `peri-middlewares/src/plugin/installer.rs`
  - 函数签名: `pub async fn install_plugin(name: &str, marketplace: &str, scope: InstallScope, marketplace_cache_dir: &Path, claude_dir: &Path) -> Result<InstalledPlugin, InstallerError>`
  - 关键逻辑:
    1. 调用 `config::load_installed_plugins(Some(claude_dir))` 加载当前已安装列表
    2. 调用 `marketplace::get_marketplace_manifest(marketplace, marketplace_cache_dir)` 获取 marketplace 的 `MarketplaceManifest`（从 Task 2 的内存缓存或磁盘缓存读取）
    3. 在 `manifest.plugins` 中查找 `name` 匹配的 `MarketplacePlugin`，未找到则返回 `PluginNotFound`
    4. 根据 `plugin.source` 路径和 `marketplace_cache_dir` 拼接出源目录: `{marketplace_cache_dir}/{marketplace}/{plugin.source}/`
    5. 调用 `config::load_plugin_manifest(&source_dir)` 验证清单完整性（读取 `.claude-plugin/plugin.json`），失败返回 `ManifestInvalid`
    6. 计算版本: 优先使用 `marketplace_plugin.sha`（有值时取前 7 位），其次使用 `marketplace_plugin.version`，最后使用 `PluginManifest.version`
    7. 构造目标缓存路径: `{claude_dir}/plugins/cache/{marketplace}/{name}/{version}/`
    8. 在 `tokio::task::spawn_blocking` 中执行:
       a. 如果目标路径已存在（同插件重装），先递归删除旧缓存目录 `fs::remove_dir_all`
       b. 使用 `fs::create_dir_all` 创建目标路径
       c. 调用 `copy_dir_recursive(&source_dir, &target_dir)` 递归复制插件目录
    9. 构造 `InstalledPlugin { id: "{name}@{marketplace}", name: name.into(), version, marketplace: marketplace.into(), install_path: target_dir, scope }`
    10. 如果 installed_plugins 中已存在同 id 条目，先移除（同插件只保留一个版本）
    11. 将新 `InstalledPlugin` 追加到 `installed.plugins`
    12. 调用 `config::save_installed_plugins(&installed, Some(claude_dir))` 原子写入
    13. 调用 `update_enabled_plugins(&installed_plugin.id, scope, claude_dir, None)` 将插件 id 追加到对应 scope 的 settings.json enabledPlugins 数组
    14. 返回 `Ok(installed_plugin)`
  - 原因: 安装是插件系统的核心写操作，必须保证幂等性（重装时先清理旧版本）和原子性（配置写入使用 rename 模式）

- [x] 实现 `uninstall_plugin` 函数
  - 位置: `peri-middlewares/src/plugin/installer.rs`，紧跟 `install_plugin` 之后
  - 函数签名: `pub async fn uninstall_plugin(plugin_id: &str, claude_dir: &Path, project_dir: Option<&Path>) -> Result<(), InstallerError>`
  - 关键逻辑:
    1. 解析 `plugin_id` 中的 `@` 分隔符，提取 `name` 和 `marketplace` 部分。解析失败时按整个 plugin_id 作为 name、marketplace 为空处理
    2. 调用 `config::load_installed_plugins(Some(claude_dir))` 加载当前已安装列表
    3. 在 `installed.plugins` 中查找 `id` 完全匹配的条目，记录其 `install_path` 和 `scope`
    4. 未找到则返回 `PluginNotFound { name, marketplace }`
    5. 从 `installed.plugins` 中移除该条目（`retain` 过滤掉匹配 id 的项）
    6. 调用 `config::save_installed_plugins(&installed, Some(claude_dir))` 原子写入
    7. 在 `tokio::task::spawn_blocking` 中执行 `fs::remove_dir_all(&install_path)` 删除缓存目录（`fs::remove_dir_all` 在目录不存在时也返回错误，使用 `if install_path.exists()` 守卫）
    8. 调用 `remove_from_enabled_plugins(plugin_id, &scope, claude_dir, project_dir)` 从 settings.json 的 enabledPlugins 数组中移除该 id
  - 原因: 卸载必须清理所有痕迹——追踪文件、缓存目录、启用配置

- [x] 实现 `update_plugin` 函数
  - 位置: `peri-middlewares/src/plugin/installer.rs`，紧跟 `uninstall_plugin` 之后
  - 函数签名: `pub async fn update_plugin(plugin_id: &str, marketplace_cache_dir: &Path, claude_dir: &Path) -> Result<InstalledPlugin, InstallerError>`
  - 关键逻辑:
    1. 解析 `plugin_id` 提取 `name` 和 `marketplace`
    2. 调用 `config::load_installed_plugins(Some(claude_dir))` 找到当前安装记录
    3. 调用 `marketplace::get_marketplace_manifest(marketplace, marketplace_cache_dir)` 获取最新 manifest
    4. 在 manifest.plugins 中查找匹配的 `MarketplacePlugin`，获取最新版本
    5. 对比最新版本与当前 `installed.version`，相同则直接返回当前 `InstalledPlugin`（无需更新）
    6. 版本不同时，调用 `uninstall_plugin(plugin_id, claude_dir, None)` 卸载旧版本（忽略 enabledPlugins 移除，因为马上会重新安装）
    7. 调用 `install_plugin(name, marketplace, installed.scope, marketplace_cache_dir, claude_dir)` 安装新版本
    8. 返回新版本的 `InstalledPlugin`
  - 原因: 更新是卸载+安装的组合操作，先卸载保证缓存干净，再安装保证配置一致

- [x] 实现 `check_updates` 批量检查函数
  - 位置: `peri-middlewares/src/plugin/installer.rs`，紧跟 `update_plugin` 之后
  - 定义返回类型:
    ```rust
    #[derive(Debug, Clone)]
    pub struct PluginUpdateInfo {
        pub plugin_id: String,
        pub current_version: String,
        pub latest_version: String,
    }
    ```
  - 函数签名: `pub async fn check_updates(installed: &InstalledPlugins, marketplace_cache_dir: &Path) -> Vec<PluginUpdateInfo>`
  - 关键逻辑:
    1. 使用 `HashMap<String, &MarketplaceManifest>` 按 marketplace 名称去重，避免同一 marketplace 重复获取 manifest
    2. 遍历 `installed.plugins`，对每个插件按 `marketplace` 字段查找或获取 manifest
    3. 在 manifest.plugins 中查找同名插件，获取最新版本
    4. 对比最新版本与 `installed_plugin.version`，不同则收集到结果列表
    5. 返回 `Vec<PluginUpdateInfo>`
  - 原因: TUI 面板需要批量展示哪些插件有可用更新，按 marketplace 去重避免重复请求

- [x] 实现 `update_enabled_plugins` 和 `remove_from_enabled_plugins` 辅助函数
  - 位置: `peri-middlewares/src/plugin/installer.rs`，在公共函数之后、单元测试之前
  - `update_enabled_plugins` 签名: `fn update_enabled_plugins(plugin_id: &str, scope: InstallScope, claude_dir: &Path, project_dir: Option<&Path>) -> Result<(), InstallerError>`
  - `remove_from_enabled_plugins` 签名: `fn remove_from_enabled_plugins(plugin_id: &str, scope: &InstallScope, claude_dir: &Path, project_dir: Option<&Path>) -> Result<(), InstallerError>`
  - 关键逻辑（两个函数共享 settings.json 读写逻辑）:
    - 根据 scope 确定 settings.json 路径: `User` → `{claude_dir}/settings.json`，`Project` → `{project_dir}/.claude/settings.json`（project_dir 为 None 时回退到 User）
    - 使用 `std::fs::read_to_string` 读取 settings.json，文件不存在时初始化为空 `serde_json::Value::Object`
    - 在 `value["enabledPlugins"]` 数组中追加（或移除）`plugin_id` 字符串。追加前检查去重（已存在则跳过）
    - 如果 `enabledPlugins` 字段不存在则初始化为空数组 `serde_json::Value::Array(vec![])`
    - 使用 `atomic_write_json` 模式写入: 先写 `{path}.tmp.{uuid}` 临时文件，再 `std::fs::rename` 替换原文件
    - 写入前确保父目录存在: `std::fs::create_dir_all(path.parent().unwrap())`
  - 原因: settings.json 是 Claude Code 的配置入口，enabledPlugins 字段控制哪些插件被启用，必须原子写入避免损坏

- [x] 实现 `copy_dir_recursive` 目录递归复制辅助函数
  - 位置: `peri-middlewares/src/plugin/installer.rs`，在辅助函数区域
  - 函数签名: `fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()>`
  - 关键逻辑:
    - 使用 `fs::create_dir_all(dst)` 创建目标目录
    - 遍历 `fs::read_dir(src)?` 的每个条目
    - 对 `DirEntry::file_type()` 判断: `is_file()` 调用 `fs::copy(entry.path(), dst.join(entry.file_name()))`，`is_dir()` 递归调用 `copy_dir_recursive(&entry.path(), &dst.join(entry.file_name()))`
    - 跳过条目名称为 `.git` 的目录（marketplace 缓存中的 git 元数据不需要复制到插件缓存）
  - 原因: 标准库没有递归复制目录的功能，需要自行实现

- [x] 在 mod.rs 中导出 installer 模块的公共 API
  - 位置: `peri-middlewares/src/plugin/mod.rs`
  - 在现有 `pub mod config;` 和 `pub mod types;` 声明之后追加: `pub mod installer;`
  - 在 `pub use` 重导出区域追加:
    ```rust
    pub use installer::{
        check_updates, install_plugin, uninstall_plugin, update_plugin,
        InstallerError, PluginUpdateInfo,
    };
    ```
  - 原因: Task 4 和 Task 6 通过 mod.rs 的 re-export 调用安装管理功能

- [x] 为 installer 核心逻辑编写单元测试
  - 测试文件: `peri-middlewares/src/plugin/installer.rs` 底部 `#[cfg(test)] mod tests`
  - 测试场景:
    - `test_install_plugin_success`: 在临时目录构造 marketplace 缓存（含 marketplace.json 和一个插件的 plugin.json），调用 `install_plugin`，断言 `installed_plugins.json` 写入正确（id 格式为 `{name}@{marketplace}`）、缓存目录复制完整（`.claude-plugin/plugin.json` 存在）、settings.json 的 enabledPlugins 包含插件 id
    - `test_install_plugin_not_found`: marketplace manifest 中不包含目标插件名，断言返回 `InstallerError::PluginNotFound`
    - `test_install_plugin_invalid_manifest`: plugin.json 内容为非法 JSON，断言返回 `InstallerError::ManifestInvalid`
    - `test_install_plugin_reinstall`: 先安装一次，再安装同插件（同 marketplace），断言 installed_plugins 中只有一条记录（旧版本被替换），缓存目录内容为最新版本
    - `test_uninstall_plugin`: 先安装再卸载，断言 `installed_plugins.json` 中条目已移除、缓存目录已删除、settings.json 中 enabledPlugins 已移除该 id
    - `test_uninstall_plugin_not_found`: 卸载不存在的插件 id，断言返回 `InstallerError::PluginNotFound`
    - `test_update_plugin_same_version`: 当前版本与 marketplace 最新版本相同，断言直接返回当前 InstalledPlugin，不执行卸载/安装
    - `test_check_updates`: 构造两个已安装插件（一个有更新、一个无更新），断言 `check_updates` 返回长度为 1 的列表且包含正确的 plugin_id 和版本信息
    - `test_copy_dir_recursive`: 构造含嵌套文件和子目录的源目录（含 `.git` 目录），复制后断言目标目录结构一致、`.git` 目录被跳过、文件内容相同
    - `test_update_enabled_plugins_append`: settings.json 初始无 enabledPlugins 字段，追加后断言字段创建且包含正确 id
    - `test_update_enabled_plugins_dedup`: settings.json 的 enabledPlugins 已包含目标 id，再次追加后不重复
    - `test_remove_from_enabled_plugins`: settings.json 的 enabledPlugins 含两个 id，移除一个后断言只剩另一个
  - 每个测试使用 `tempfile::tempdir()` 创建临时 claude_dir 和 marketplace_cache_dir，通过参数传入实现测试隔离
  - 运行命令: `cargo test -p peri-middlewares --lib -- plugin::installer::tests`
  - 预期: 所有测试通过

**检查步骤:**
- [x] 验证 installer.rs 文件存在且包含所有公共函数
  - `grep -c "pub async fn" peri-middlewares/src/plugin/installer.rs`
  - 预期: 输出 >= 4（install_plugin, uninstall_plugin, update_plugin, check_updates）
- [x] 验证 InstallerError 使用 thiserror 派生
  - `grep "thiserror" peri-middlewares/src/plugin/installer.rs`
  - 预期: 匹配到 `#[derive(thiserror::Error)]` 或 `#[derive(Error)]` 配合 `use thiserror::Error`
- [x] 验证 mod.rs 正确 re-export installer 公共 API
  - `grep "installer" peri-middlewares/src/plugin/mod.rs`
  - 预期: 包含 `pub mod installer;` 和 `pub use installer::`
- [x] 验证单元测试编译通过
  - `cargo test -p peri-middlewares --lib -- plugin::installer::tests --no-run 2>&1 | tail -3`
  - 预期: 输出包含 "Finished" 且无编译错误
- [x] 验证所有单元测试通过
  - `cargo test -p peri-middlewares --lib -- plugin::installer::tests 2>&1 | tail -5`
  - 预期: 所有测试通过，无失败

---

### Task 4: 插件加载器与中间件

**背景:**
[业务语境] — 实现已安装插件的运行时加载：解析每个插件的 `plugin.json` 清单，提取 commands（斜杠命令）、skills、agents、MCP 服务器等能力，通过 `PluginMiddleware` 注入到 ReAct 循环，使插件能力对 agent 透明可用。
[修改原因] — Task 1-3 完成了类型定义、配置读写和安装管理，但缺少"从已安装插件提取资源并注入 agent 运行时"的核心加载逻辑。当前 `run_universal_agent`（`peri-tui/src/app/agent.rs`）中的中间件链（L246-L268）硬编码了固定的 SkillsMiddleware、McpMiddleware、SubAgentMiddleware，未纳入插件提供的额外资源。
[上下游影响] — 本 Task 的输出（`loader.rs` 的加载函数、`middleware.rs` 的 PluginMiddleware 实现、`CommandProvider` trait）被 Task 5（现有系统集成，修改 `agent.rs` 中的中间件链注册）和 Task 6（TUI PluginPanel，通过 CommandProvider 展示插件命令）依赖。本 Task 依赖 Task 1 的 types.rs/config.rs（InstalledPlugins、PluginManifest 类型）、Task 2 的 marketplace.rs（无直接依赖，但 loader 的输入来自 installer 的输出）、Task 3 的 installer.rs（InstalledPlugin.install_path 提供插件根目录）。

**涉及文件:**
- 新建: `peri-middlewares/src/plugin/loader.rs`
- 新建: `peri-middlewares/src/plugin/middleware.rs`
- 修改: `peri-middlewares/src/plugin/mod.rs`（导出新模块公共 API）
- 修改: `peri-middlewares/src/lib.rs`（prelude 导出新增类型）

**执行步骤:**

- [x] 实现 `CommandProvider` trait 和 `CommandEntry`/`CommandSource` 类型
  - 位置: `peri-middlewares/src/plugin/loader.rs`（新建文件）
  - 文件头部引入：
    ```rust
    use crate::plugin::config::{load_plugin_manifest, load_installed_plugins, ClaudeSettings};
    use crate::plugin::types::{InstalledPlugin, InstalledPlugins, PluginManifest, PluginCommand, PluginAgent};
    use crate::mcp::McpServerConfig;
    use gray_matter::{engine::YAML, Matter};
    use serde::Deserialize;
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};
    use tracing::{debug, warn};
    ```
  - 定义 `CommandSource` 枚举：
    ```rust
    #[derive(Debug, Clone)]
    pub enum CommandSource {
        Builtin,
        Plugin { path: PathBuf },
    }
    ```
  - 定义 `CommandEntry` 结构体：
    ```rust
    #[derive(Debug, Clone)]
    pub struct CommandEntry {
        pub name: String,
        pub description: String,
        pub source: CommandSource,
    }
    ```
  - 定义 `CommandProvider` trait：
    ```rust
    pub trait CommandProvider: Send + Sync {
        fn commands(&self) -> Vec<CommandEntry>;
    }
    ```
  - 原因: CommandProvider 是 TUI 命令浮层的统一抽象（Task 6 使用），内置命令和插件命令都实现此接口。CommandSource 标记命令来源，Plugin 变体携带命令 .md 文件路径供后续执行

- [x] 实现 `CommandFrontmatter` 和 `parse_command_md` — 插件命令 Markdown 解析
  - 位置: `peri-middlewares/src/plugin/loader.rs`，在 CommandProvider 定义之后
  - 定义 frontmatter 反序列化结构：
    ```rust
    #[derive(Debug, Deserialize)]
    struct CommandFrontmatter {
        #[serde(default)]
        shell: Option<String>,
        #[serde(default)]
        effort: Option<String>,
        #[serde(default)]
        model: Option<String>,
        #[serde(default)]
        description: Option<String>,
        #[serde(default)]
        args: Option<serde_yaml::Value>,
    }
    ```
  - 实现 `fn parse_command_md(path: &Path) -> Option<(CommandFrontmatter, String)>`:
    - 使用 `std::fs::read_to_string(path).ok()?` 读取文件
    - 使用 `Matter::<YAML>::new()` + `matter.parse(&content)` 解析 frontmatter（与 `skills/loader.rs:24` 使用相同的 `gray_matter` 模式）
    - 反序列化 frontmatter 为 `CommandFrontmatter`
    - `result.content` 为正文部分
    - 返回 `Some((fm, body))`
  - 原因: 插件命令是 `.md` 文件，与现有 skill 的 `SKILL.md` 格式一致（YAML frontmatter + Markdown body）。复用 `gray_matter` crate（已在 Cargo.toml 中）避免引入额外依赖，`serde_yaml` 也已存在

- [x] 实现 `LoadedPlugin` 结构体 — 插件加载结果
  - 位置: `peri-middlewares/src/plugin/loader.rs`，在 parse_command_md 之后
  - 定义结构体：
    ```rust
    pub struct LoadedPlugin {
        pub name: String,
        pub version: String,
        pub install_path: PathBuf,
        pub manifest: PluginManifest,
        pub commands: Vec<CommandEntry>,
        pub skills_dirs: Vec<PathBuf>,
        pub agents_dirs: Vec<PathBuf>,
        pub mcp_servers: HashMap<String, McpServerConfig>,
    }
    ```
  - 原因: LoadedPlugin 是一次完整的加载结果，包含解析后的所有资源。Task 5 和 Task 6 从此结构体获取具体资源

- [x] 实现 `load_manifest` — 单个插件清单加载
  - 位置: `peri-middlewares/src/plugin/loader.rs`，在 LoadedPlugin 之后
  - 实现函数签名: `pub fn load_manifest(plugin_dir: &Path) -> Result<PluginManifest, LoaderError>`
  - 关键逻辑:
    1. 拼接 manifest 路径: `{plugin_dir}/.claude-plugin/plugin.json`
    2. 调用 `load_plugin_manifest(plugin_dir)`（复用 Task 1 config.rs 中的函数）
    3. 包装错误为 `LoaderError::ManifestLoadFailed`
  - 定义 `LoaderError` 错误类型（文件顶部）：
    ```rust
    #[derive(Debug, thiserror::Error)]
    pub enum LoaderError {
        #[error("插件清单加载失败: {0}")]
        ManifestLoadFailed(String),
        #[error("插件配置读取失败: {0}")]
        ConfigError(#[from] crate::plugin::PluginConfigError),
        #[error("IO 错误: {0}")]
        Io(#[from] std::io::Error),
    }
    ```
  - 原因: 复用 config.rs 中的 `load_plugin_manifest` 避免重复实现

- [x] 实现 `extract_commands` — 从清单中提取命令列表
  - 位置: `peri-middlewares/src/plugin/loader.rs`，在 load_manifest 之后
  - 实现函数签名: `pub fn extract_commands(manifest: &PluginManifest, base_dir: &Path, plugin_name: &str) -> Vec<CommandEntry>`
  - 关键逻辑:
    1. 若 `manifest.commands` 为 `None` 或空，返回空 `Vec`
    2. 遍历 `manifest.commands.as_ref().unwrap()` 中的每个 `PluginCommand`:
       a. 拼接命令文件路径: `{base_dir}/{cmd.path}`（`cmd.path` 是相对于插件根目录的路径，如 `commands/my-cmd.md`）
       b. 若文件不存在，记录 `warn!` 日志并跳过
       c. 调用 `parse_command_md(&cmd_file_path)` 解析 frontmatter
       d. 解析失败时记录 `warn!` 日志并跳过
       e. 命令名: 优先使用 `cmd.name`（Option<String>），其次使用文件名（去掉 `.md` 后缀，使用 `path.file_stem().and_then(|s| s.to_str()).unwrap_or("unknown")`）
       f. 命令描述: 优先使用 frontmatter 的 `description`，其次使用 `cmd.description`，最后使用空字符串
       g. 完整命令名格式: `{plugin_name}:{command_name}`（命名空间隔离，如 `frontend-design:create-component`）
       h. 构造 `CommandEntry { name, description, source: CommandSource::Plugin { path: cmd_file_path } }`
    3. 收集为 `Vec<CommandEntry>` 返回
  - 原因: 命令提取是插件加载的核心功能，`plugin:command` 命名空间格式避免与内置命令冲突

- [x] 实现 `extract_skills_paths` — 从清单中提取 skills 目录路径
  - 位置: `peri-middlewares/src/plugin/loader.rs`，在 extract_commands 之后
  - 实现函数签名: `pub fn extract_skills_paths(manifest: &PluginManifest, base_dir: &Path) -> Vec<PathBuf>`
  - 关键逻辑:
    1. 若 `manifest.skills` 为 `None` 或空，返回空 `Vec`
    2. 遍历 `manifest.skills.as_ref().unwrap()` 中的每个 skill 名称字符串（如 `"my-skill"`）
    3. 拼接路径: `{base_dir}/skills/{skill_name}`
    4. 若路径不存在（非目录），记录 `debug!` 日志并跳过
    5. 收集为 `Vec<PathBuf>` 返回
  - 原因: skills 路径列表将追加到 SkillsMiddleware 的搜索目录（Task 5 实现）

- [x] 实现 `extract_agents_paths` — 从清单中提取 agents 目录路径
  - 位置: `peri-middlewares/src/plugin/loader.rs`，在 extract_skills_paths 之后
  - 实现函数签名: `pub fn extract_agents_paths(manifest: &PluginManifest, base_dir: &Path) -> Vec<PathBuf>`
  - 关键逻辑:
    1. 若 `manifest.agents` 为 `None` 或空，返回空 `Vec`
    2. 遍历 `manifest.agents.as_ref().unwrap()` 中的每个 `PluginAgent`:
       a. 拼接路径: `{base_dir}/{agent.path}`（`agent.path` 是相对于插件根目录的路径，如 `agents/code-review.md`）
       b. 若路径不存在，记录 `debug!` 日志并跳过
    3. 收集为 `Vec<PathBuf>` 返回
  - 原因: agents 路径列表将追加到 SubAgentMiddleware 的搜索路径（Task 5 实现）

- [x] 实现 `extract_mcp_servers` — 从清单中提取 MCP 服务器配置
  - 位置: `peri-middlewares/src/plugin/loader.rs`，在 extract_agents_paths 之后
  - 实现函数签名: `pub fn extract_mcp_servers(manifest: &PluginManifest) -> HashMap<String, McpServerConfig>`
  - 关键逻辑:
    1. 若 `manifest.mcp_servers` 为 `None`，返回空 `HashMap`
    2. 克隆 `manifest.mcp_servers.as_ref().unwrap()` 为新 `HashMap` 返回
  - 原因: MCP 服务器配置直接复用 `McpServerConfig` 类型，合并到 McpConfig 时按命名空间加前缀避免冲突

- [x] 实现 `load_plugins` — 批量加载已安装插件
  - 位置: `peri-middlewares/src/plugin/loader.rs`，在 extract 函数之后
  - 实现函数签名: `pub fn load_plugins(installed: &InstalledPlugins) -> Result<Vec<LoadedPlugin>, LoaderError>`
  - 关键逻辑:
    1. 初始化空 `Vec<LoadedPlugin>` 和 `HashSet<String>` 用于去重
    2. 遍历 `installed.plugins`:
       a. 调用 `load_manifest(&plugin.install_path)` 加载清单，失败时记录 `warn!` 日志并 `continue`（不中断其他插件加载）
       b. 调用 `extract_commands(&manifest, &plugin.install_path, &plugin.name)`
       c. 调用 `extract_skills_paths(&manifest, &plugin.install_path)`
       d. 调用 `extract_agents_paths(&manifest, &plugin.install_path)`
       e. 调用 `extract_mcp_servers(&manifest)`
       f. 构造 `LoadedPlugin { name: plugin.name.clone(), version: plugin.version.clone(), install_path: plugin.install_path.clone(), manifest, commands, skills_dirs, agents_dirs, mcp_servers }`
       g. 追加到结果列表
    3. 记录 `debug!(count = result.len(), "已加载插件")` 日志
    4. 返回 `Ok(result)`
  - 原因: 批量加载是 agent 初始化的入口，单个插件加载失败不阻塞其他插件（容错设计）

- [x] 实现 `load_enabled_plugins` — 仅加载已启用插件（读取 enabledPlugins 列表）
  - 位置: `peri-middlewares/src/plugin/loader.rs`，在 load_plugins 之后
  - 实现函数签名: `pub fn load_enabled_plugins(override_dir: Option<&Path>) -> Result<Vec<LoadedPlugin>, LoaderError>`
  - 关键逻辑:
    1. 调用 `load_installed_plugins(override_dir)` 获取已安装列表
    2. 调用 `load_claude_settings(override_dir)` 获取 `enabledPlugins` 列表
    3. 将 `installed.plugins` 过滤为 `enabled_plugins.contains(&plugin.id)` 的子集
    4. 构造过滤后的 `InstalledPlugins { version: installed.version, plugins: filtered }`
    5. 调用 `load_plugins(&filtered)` 加载
    6. 返回结果
  - 原因: agent 只需加载已启用的插件，`enabledPlugins` 在 `~/.claude/settings.json` 中管理

- [x] 实现 `PluginCommandProvider` — CommandProvider 的插件实现
  - 位置: `peri-middlewares/src/plugin/loader.rs`，在 load_enabled_plugins 之后
  - 定义结构体:
    ```rust
    pub struct PluginCommandProvider {
        entries: Vec<CommandEntry>,
    }
    ```
  - 实现 `PluginCommandProvider::new(plugins: &[LoadedPlugin]) -> Self`:
    - 收集所有 `plugins` 的 `commands` 字段为扁平 `Vec<CommandEntry>`
    - 返回 `Self { entries }`
  - 实现 `CommandProvider for PluginCommandProvider`:
    ```rust
    impl CommandProvider for PluginCommandProvider {
        fn commands(&self) -> Vec<CommandEntry> {
            self.entries.clone()
        }
    }
    ```
  - 原因: TUI 层（Task 6）通过 `CommandProvider` trait 获取所有可用命令（内置+插件），PluginCommandProvider 是插件命令的统一视图

- [x] 实现 `merge_plugin_mcp_servers` — 将插件 MCP 服务器合并到现有配置
  - 位置: `peri-middlewares/src/plugin/loader.rs`，在 PluginCommandProvider 之后
  - 实现函数签名: `pub fn merge_plugin_mcp_servers(plugins: &[LoadedPlugin]) -> HashMap<String, McpServerConfig>`
  - 关键逻辑:
    1. 初始化空 `HashMap<String, McpServerConfig>`
    2. 遍历 `plugins`:
       a. 遍历 `plugin.mcp_servers` 中的每个 `(name, config)` 条目
       b. 使用 `{plugin_name}__{name}` 格式作为合并后的服务器名称（双下划线分隔，与现有 MCP 中间件的 `mcp__{server_name}__{tool_name}` 命名约定一致）
       c. 插入到合并 map 中
    3. 返回合并后的 `HashMap`
  - 原因: 不同插件可能声明同名 MCP 服务器，使用 `{plugin_name}__{server_name}` 前缀避免冲突

- [x] 实现 `PluginMiddleware` — Middleware trait 实现
  - 位置: `peri-middlewares/src/plugin/middleware.rs`（新建文件）
  - 文件头部引入：
    ```rust
    use crate::plugin::loader::LoadedPlugin;
    use peri_agent::agent::state::State;
    use peri_agent::middleware::r#trait::Middleware;
    use std::sync::Arc;
    ```
  - 定义结构体:
    ```rust
    pub struct PluginMiddleware {
        plugins: Arc<Vec<LoadedPlugin>>,
    }
    ```
  - 实现 `PluginMiddleware::new(plugins: Vec<LoadedPlugin>) -> Self`:
    - 返回 `Self { plugins: Arc::new(plugins) }`
  - 实现 `Middleware<S> for PluginMiddleware`:
    ```rust
    #[async_trait::async_trait]
    impl<S: State> Middleware<S> for PluginMiddleware {
        fn name(&self) -> &str {
            "PluginMiddleware"
        }

        async fn before_agent(&self, state: &mut S) -> AgentResult<()> {
            let _ = (state, self.plugins);
            // 插件资源（skills/mcp/agents）的注入在 Task 5 中通过修改
            // agent.rs 的中间件注册流程实现，不在 before_agent 中动态注入。
            // PluginMiddleware 作为占位中间件，确保插件系统在中间件链中有一席之地，
            // 并提供 plugins 的只读访问。
            Ok(())
        }
    }
    ```
  - 实现 `PluginMiddleware::plugins(&self) -> &[LoadedPlugin]`:
    - 返回 `&self.plugins`
  - 原因: PluginMiddleware 在中间件链中占据一个位置，提供 `plugins()` 只读访问供 Task 5 集成时使用。实际的 skills/MCP/agents 注入在 Task 5 中通过修改 `agent.rs` 的初始化流程实现（在中间件注册之前预加载插件资源并传入对应的中间件构造器）

- [x] 在 mod.rs 中注册 loader 和 middleware 模块并导出公共 API
  - 位置: `peri-middlewares/src/plugin/mod.rs`
  - 在现有模块声明之后追加: `pub mod loader;` 和 `pub mod middleware;`
  - 在 `pub use` 重导出区域追加:
    ```rust
    pub use loader::{
        load_enabled_plugins, load_manifest, load_plugins, merge_plugin_mcp_servers,
        CommandEntry, CommandProvider, CommandSource, LoadedPlugin, LoaderError,
        PluginCommandProvider,
    };
    pub use middleware::PluginMiddleware;
    ```
  - 在 `peri-middlewares/src/lib.rs` 的 `pub use plugin::{ ... };` 列表末尾追加: `CommandEntry, CommandProvider, CommandSource, LoadedPlugin, LoaderError, PluginCommandProvider, PluginMiddleware`
  - 在 `peri-middlewares/src/lib.rs` 的 `pub mod prelude { ... }` 的 `pub use crate::plugin::{ ... };` 列表末尾同样追加
  - 原因: 遵循现有模块注册和导出模式，确保 Task 5 和 Task 6 可通过 `peri_middlewares::prelude::*` 获取类型

- [x] 为 `parse_command_md` 和 frontmatter 解析编写单元测试
  - 测试文件: `peri-middlewares/src/plugin/loader.rs` 底部 `#[cfg(test)] mod tests`
  - 测试场景:
    - `test_parse_command_md_with_shell`: 构造含 `shell: "echo hello"` frontmatter 的 .md 文件 → `parse_command_md` 返回 `Some((fm, body))`，`fm.shell` 为 `Some("echo hello".to_string())`
    - `test_parse_command_md_with_all_fields`: 构造含 `shell`/`effort`/`model`/`description`/`args` 全部字段的 .md 文件 → 所有字段正确解析
    - `test_parse_command_md_no_frontmatter`: 构造无 frontmatter（直接 Markdown）的 .md 文件 → 返回 `Some((fm, body))`，fm 所有字段为 `None`，body 为完整内容
    - `test_parse_command_md_file_not_found`: 传入不存在的路径 → 返回 `None`
  - 使用 `tempfile::tempdir()` 创建临时目录和 .md 文件
  - 运行命令: `cargo test -p peri-middlewares --lib -- plugin::loader::tests::test_parse`
  - 预期: 所有测试通过

- [x] 为 `extract_commands` 编写单元测试
  - 测试文件: `peri-middlewares/src/plugin/loader.rs`（内联测试模块）
  - 测试场景:
    - `test_extract_commands_single`: 构造含一个 PluginCommand 的 manifest 和对应的 commands/xxx.md 文件 → 返回长度为 1 的列表，命令名格式为 `{plugin}:{cmd_name}`
    - `test_extract_commands_multiple`: manifest 含多个 PluginCommand → 返回多个 CommandEntry，每个命令名含命名空间前缀
    - `test_extract_commands_missing_file`: manifest 引用了不存在的 .md 文件 → 跳过该命令，返回空列表（无 panic）
    - `test_extract_commands_explicit_name`: PluginCommand.name 为 `Some("my-cmd")` → 使用显式名称而非文件名
    - `test_extract_commands_none`: manifest.commands 为 None → 返回空列表
    - `test_extract_commands_frontmatter_description`: .md 文件 frontmatter 含 `description` 字段 → CommandEntry.description 使用 frontmatter 中的值
  - 构造 manifest 时使用 `serde_json::json!` 宏，构造 .md 文件使用 `std::fs::write`
  - 运行命令: `cargo test -p peri-middlewares --lib -- plugin::loader::tests::test_extract_commands`
  - 预期: 所有测试通过

- [x] 为 `extract_skills_paths` 和 `extract_agents_paths` 编写单元测试
  - 测试文件: `peri-middlewares/src/plugin/loader.rs`（内联测试模块）
  - 测试场景:
    - `test_extract_skills_paths`: manifest.skills 为 `Some(vec!["code-review".into()])`，临时目录下有 `skills/code-review/SKILL.md` → 返回含一个路径的列表
    - `test_extract_skills_paths_missing_dir`: manifest.skills 引用不存在的 skill 目录 → 跳过，返回空列表
    - `test_extract_skills_paths_none`: manifest.skills 为 None → 返回空列表
    - `test_extract_agents_paths`: manifest.agents 为 `Some(vec![PluginCommand { path: "agents/reviewer.md".into(), name: Some("reviewer".into()), description: None }])`，临时目录下有 `agents/reviewer.md` → 返回含一个路径的列表
    - `test_extract_agents_paths_missing`: manifest.agents 引用不存在的 agent 文件 → 跳过，返回空列表
    - `test_extract_agents_paths_none`: manifest.agents 为 None → 返回空列表
  - 运行命令: `cargo test -p peri-middlewares --lib -- plugin::loader::tests::test_extract_skills`
  - 预期: 所有测试通过

- [x] 为 `extract_mcp_servers` 和 `merge_plugin_mcp_servers` 编写单元测试
  - 测试文件: `peri-middlewares/src/plugin/loader.rs`（内联测试模块）
  - 测试场景:
    - `test_extract_mcp_servers`: manifest.mcp_servers 为 `Some(HashMap::from([("server1".into(), McpServerConfig { command: "node".into(), ..Default::default() })]))` → 返回含一个条目的 HashMap
    - `test_extract_mcp_servers_none`: manifest.mcp_servers 为 None → 返回空 HashMap
    - `test_merge_plugin_mcp_servers`: 两个 LoadedPlugin 各含一个 MCP server → 合并后 server 名称格式为 `{plugin_name}__{server_name}`，无冲突
    - `test_merge_plugin_mcp_servers_conflict`: 两个插件声明同名 MCP server（如都叫 `"my-server"`） → 合并后各自带插件名前缀，不冲突
  - 运行命令: `cargo test -p peri-middlewares --lib -- plugin::loader::tests::test_extract_mcp`
  - 预期: 所有测试通过

- [x] 为 `load_plugins` 和 `load_enabled_plugins` 编写单元测试
  - 测试文件: `peri-middlewares/src/plugin/loader.rs`（内联测试模块）
  - 测试场景:
    - `test_load_plugins_success`: 构造含一个有效 InstalledPlugin 的 InstalledPlugins（临时目录中有完整的 `.claude-plugin/plugin.json`） → 返回长度为 1 的 LoadedPlugin 列表
    - `test_load_plugins_empty`: InstalledPlugins.plugins 为空 → 返回空列表
    - `test_load_plugins_invalid_manifest`: install_path 指向不含 plugin.json 的目录 → 记录 warn 日志，跳过该插件，返回空列表（不 panic）
    - `test_load_enabled_plugins`: 在 override_dir 中构造 `installed_plugins.json`（含一个插件）和 `settings.json`（`enabledPlugins` 包含该插件 id） → 返回长度为 1 的列表
    - `test_load_enabled_plugins_disabled`: settings.json 的 `enabledPlugins` 不包含已安装插件 id → 返回空列表
  - 每个测试使用 `tempfile::tempdir()` 作为 override_dir，手动构造 `installed_plugins.json`、`settings.json`、插件目录结构
  - 运行命令: `cargo test -p peri-middlewares --lib -- plugin::loader::tests::test_load`
  - 预期: 所有测试通过

- [x] 为 `PluginCommandProvider` 编写单元测试
  - 测试文件: `peri-middlewares/src/plugin/loader.rs`（内联测试模块）
  - 测试场景:
    - `test_plugin_command_provider_empty`: 传入空 LoadedPlugin 列表 → `commands()` 返回空列表
    - `test_plugin_command_provider_multiple`: 两个 LoadedPlugin 各含两个命令 → `commands()` 返回 4 个 CommandEntry
  - 运行命令: `cargo test -p peri-middlewares --lib -- plugin::loader::tests::test_plugin_command_provider`
  - 预期: 所有测试通过

- [x] 为 `PluginMiddleware` 编写单元测试
  - 测试文件: `peri-middlewares/src/plugin/middleware.rs` 底部 `#[cfg(test)] mod tests`
  - 测试场景:
    - `test_middleware_name`: 创建 `PluginMiddleware::new(vec![])` → `name()` 返回 `"PluginMiddleware"`
    - `test_middleware_before_agent_noop`: 创建 `PluginMiddleware::new(vec![])`，传入 `AgentState::new("/tmp")` → `before_agent()` 返回 `Ok(())`
    - `test_middleware_plugins_accessor`: 创建 `PluginMiddleware` 传入含一个 LoadedPlugin 的列表 → `plugins()` 返回长度为 1 的切片
  - 运行命令: `cargo test -p peri-middlewares --lib -- plugin::middleware::tests`
  - 预期: 所有测试通过

**检查步骤:**
- [x] 验证 loader.rs 和 middleware.rs 编译通过
  - `cargo build -p peri-middlewares 2>&1 | tail -5`
  - 预期: 输出 `Finished` 且无编译错误
- [x] 验证 loader.rs 所有单元测试通过
  - `cargo test -p peri-middlewares --lib -- plugin::loader::tests 2>&1 | tail -15`
  - 预期: 所有 test 以 `ok` 结尾，无 failure
- [x] 验证 middleware.rs 所有单元测试通过
  - `cargo test -p peri-middlewares --lib -- plugin::middleware::tests 2>&1 | tail -10`
  - 预期: 所有 test 以 `ok` 结尾，无 failure
- [x] 验证 mod.rs 正确 re-export loader 和 middleware 公共 API
  - `grep -E "(loader|middleware|CommandProvider|PluginMiddleware)" peri-middlewares/src/plugin/mod.rs`
  - 预期: 输出包含 `pub mod loader;`、`pub mod middleware;`、`pub use loader::`、`pub use middleware::PluginMiddleware`
- [x] 验证 lib.rs 和 prelude 导出更新
  - `grep -c "PluginMiddleware" peri-middlewares/src/lib.rs`
  - 预期: 输出 >= 2（mod 级导出 + prelude 导出）
  - `grep -c "CommandProvider" peri-middlewares/src/lib.rs`
  - 预期: 输出 >= 2
- [x] 验证 gray_matter 被复用（无新增 serde_yaml 以外依赖）
  - `grep "gray_matter" peri-middlewares/src/plugin/loader.rs`
  - 预期: 输出包含 `use gray_matter`，确认复用现有依赖
- [x] 验证 McpServerConfig 复用关系正确
  - `grep "use crate::mcp::McpServerConfig" peri-middlewares/src/plugin/loader.rs`
  - 预期: 输出该行，确认引用路径正确
- [x] 验证 Task 1-3 测试无回归
  - `cargo test -p peri-middlewares --lib -- plugin:: 2>&1 | tail -20`
  - 预期: 所有测试通过，无回归
- [x] 验证全量 middlewares 测试无回归
  - `cargo test -p peri-middlewares --lib 2>&1 | tail -10`
  - 预期: 所有测试通过，无回归

**认知变更:**
- [x] [CLAUDE.md] 插件命令的 Markdown 文件使用 `gray_matter` crate（YAML engine）解析 frontmatter，与现有 skills/loader.rs 使用的解析方式一致。新增命令解析时必须复用 `Matter::<YAML>::new()` 模式，不手动解析 `---` 分隔符
- [x] [CLAUDE.md] 插件 MCP 服务器使用 `{plugin_name}__{server_name}` 双下划线前缀命名空间，与现有 MCP 中间件的 `mcp__{server_name}__{tool_name}` 命名约定对齐。新增 MCP 配置合并逻辑时必须遵循此前缀规则

---

### Task 5: 插件核心基础设施 验收（本地）

**前置条件:**
- 构建环境: `cargo build -p peri-middlewares` 成功
- 临时目录: 使用 `tempfile::tempdir()` 创建隔离的 `~/.claude/plugins/` 目录

**端到端验证:**

1. 运行 middlewares 全量测试确保无回归
   - `cargo test -p peri-middlewares --lib 2>&1 | tail -10`
   - 预期: 所有测试通过
   - 失败排查: 检查 Task 1（类型）、Task 2（marketplace）、Task 3（installer）、Task 4（loader）的测试步骤

2. 验证 plugin.json 清单解析兼容性
   - `cargo test -p peri-middlewares --lib -- plugin::types::tests 2>&1 | tail -15`
   - 预期: 9 个序列化/反序列化测试全部通过
   - 失败排查: 检查 Task 1 types.rs 的 serde 属性是否与 Claude Code schemas.ts 字段名一致

3. 验证 marketplace 拉取和缓存
   - `cargo test -p peri-middlewares --lib -- plugin::marketplace 2>&1 | tail -15`
   - 预期: GitHub/URL/local helper 测试通过
   - 失败排查: 检查 Task 2 marketplace.rs 的缓存目录创建和 manifest 解析

4. 验证安装/卸载流程
   - `cargo test -p peri-middlewares --lib -- plugin::installer 2>&1 | tail -15`
   - 预期: 12 个安装/卸载/更新测试全部通过
   - 失败排查: 检查 Task 3 installer.rs 的文件复制和 installed_plugins.json 写入

5. 验证插件加载和资源提取
   - `cargo test -p peri-middlewares --lib -- plugin::loader 2>&1 | tail -15`
   - 预期: 7 组加载/提取测试全部通过
   - 失败排查: 检查 Task 4 loader.rs 的 manifest 解析和命令/skills/MCP 提取逻辑