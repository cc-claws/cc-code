# Feature: 20260506_F001 - plugin-marketplace-compat

## 需求背景

Claude Code 已经建立了完整的插件生态——通过 `.claude-plugin/plugin.json` 清单格式描述插件能力（命令、技能、Agent、MCP 服务器等），通过 marketplace（GitHub 仓库或 URL）分发和发现插件。Peri 作为兼容 Claude Code 配置的 Rust Agent 框架，需要能直接读取 `~/.claude/` 下的插件配置和缓存，安装和运行 Claude Code 生态的插件。

当前差距：Peri 已有 Skills、MCP、SubAgent、TUI 命令等能力，但缺少统一的插件发现、安装和加载机制。插件系统将这些能力串联起来，形成"从 marketplace 发现 → 安装到本地 → 加载到 agent"的完整链路。

## 目标

- 解析并兼容 Claude Code 的 `plugin.json` 清单格式
- 从 marketplace（GitHub 仓库/URL/本地路径/NPM）发现和拉取插件列表
- 加载插件的 commands（斜杠命令）和 skills 到现有命令/技能系统
- 加载插件的 MCP 服务器配置到现有 MCP 中间件
- 跳过 hooks（本次不实现，类型定义预留字段）
- 复用 `~/.claude/` 路径实现零迁移兼容

## 方案设计

### 选定方案：Plugin 模块嵌入 peri-middlewares

在 `peri-middlewares/src/plugin/` 下新建模块，仿照现有 MCP 中间件（`src/mcp/`）的组织方式。不新增 crate，不修改核心框架。

理由：插件系统和 MCP 中间件性质相同——都是外部资源的发现、桥接和注入。MCP 已验证"middlewares 内独立模块"模式可行。插件系统不涉及核心框架改动，放在 middlewares 层最自然。

### 模块结构

```
peri-middlewares/src/plugin/
├── mod.rs              # 模块入口，导出公共 API
├── types.rs            # Claude Code plugin.json 兼容类型
├── marketplace.rs      # Marketplace 拉取、缓存、更新
├── installer.rs        # 插件安装/卸载/版本管理
├── loader.rs           # 已安装插件加载（解析清单、提取资源）
├── middleware.rs        # Middleware trait 实现（settings 合并）
└── config.rs           # ~/.claude/ 配置读写
```

### 数据模型

基于 Claude Code `schemas.ts` 类型定义，设计 Rust 兼容类型。

**plugin.json 清单类型**（`types.rs`）：

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
    pub skills: Option<Vec<String>>,       // skills/ 子目录名列表
    pub hooks: Option<serde_json::Value>,  // 预留，本次不实现
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginAuthor {
    pub name: String,
    #[serde(default)]
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginCommand {
    pub path: String,          // commands/ 目录下的 .md 文件相对路径
    pub name: Option<String>,  // 显式命令名（默认取文件名）
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginAgent {
    pub path: String,          // agents/ 目录下的 .md 文件相对路径
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginLspServer {
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginChannel {
    pub name: String,
    #[serde(rename = "mcpServer")]
    pub mcp_server: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginOption {
    pub name: String,
    pub description: String,
    #[serde(rename = "type")]
    pub option_type: String,
    pub default: Option<serde_json::Value>,
}
```

**marketplace.json 类型**：

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketplaceManifest {
    pub name: String,
    pub plugins: Vec<MarketplacePlugin>,
    #[serde(rename = "allowCrossMarketplaceDependenciesOn")]
    pub allow_cross_marketplace: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketplacePlugin {
    pub name: String,
    pub description: String,
    pub source: String,        // 相对路径或子目录名
    pub version: String,
    pub sha: Option<String>,
    pub author: Option<PluginAuthor>,
}
```

**Marketplace 来源类型**：

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

**安装追踪类型**：

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledPlugins {
    pub version: u32,
    pub plugins: Vec<InstalledPlugin>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledPlugin {
    pub id: String,              // "pluginName@marketplace"
    pub name: String,
    pub version: String,
    pub marketplace: String,
    pub install_path: PathBuf,   // 缓存目录绝对路径
    pub scope: InstallScope,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum InstallScope { User, Project, Local }
```

**已知 Marketplace 追踪**（`known_marketplaces.json`）：

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnownMarketplace {
    pub source: MarketplaceSource,
    #[serde(rename = "installLocation")]
    pub install_location: Option<PathBuf>,
    #[serde(rename = "autoUpdate")]
    pub auto_update: bool,
    #[serde(rename = "lastUpdated")]
    pub last_updated: Option<String>,
}
```

### 文件布局

复用 `~/.claude/` 路径结构，与 Claude Code 零冲突：

```
~/.claude/
├── settings.json                              # enabledPlugins, extraKnownMarketplaces
├── plugins/
│   ├── known_marketplaces.json                 # 已知 marketplace 配置
│   ├── installed_plugins.json                  # V2 安装追踪
│   ├── marketplaces/                           # marketplace 缓存
│   │   ├── claude-plugins-official.json        # URL 来源的缓存文件
│   │   └── github-marketplace/                 # GitHub 来源的克隆仓库
│   │       └── .claude-plugin/
│   │           └── marketplace.json
│   └── cache/                                  # 插件版本缓存
│       └── {marketplace}/{plugin}/{version}/
│           ├── .claude-plugin/
│           │   └── plugin.json
│           ├── commands/
│           ├── skills/
│           └── agents/
└── .claude/                                    # 项目级
    └── settings.json                           # 项目级 enabledPlugins
```

**配置读取优先级**：项目级 `.claude/settings.json` > 用户级 `~/.claude/settings.json` > managed-settings.json。插件启用状态（`enabledPlugins`）按此优先级合并。

### Marketplace 发现与缓存

**核心流程**：

```
应用启动
  → PluginManager::init()
  → 加载 known_marketplaces.json
  → 合并 settings.json 中的 extraKnownMarketplaces
  → 对每个 marketplace 源:
      GitHub → git clone --depth 1 / git pull --ff-only 到缓存目录
      URL    → HTTP GET + If-Modified-Since 到缓存文件
      本地   → 直接读取 marketplace.json
      NPM    → npm pack + 解压到缓存目录
  → 解析 marketplace.json → 列出可用插件
  → 缓存 manifest 到内存
```

**GitHub 源拉取策略**：

- 首次 `git clone --depth 1`
- 后续 `git pull --ff-only`（auto_update=true 时）
- 失败时使用已有的缓存版本，不阻塞启动
- 超时 30 秒

**URL 源拉取策略**：

- HTTP GET 请求，带 If-Modified-Since 缓存头
- 超时 15 秒
- 失败时使用本地缓存

**官方 Marketplace**：

- 默认内置 `anthropics/claude-plugins-official`
- 首次启动自动注册并拉取
- `auto_update` 默认 true

**后台刷新**：marketplace 拉取在 `tokio::spawn` 中异步执行，不阻塞 agent 初始化。agent 启动时先使用缓存版本，后台刷新完成后通知 TUI 更新插件列表。

### 插件安装管理

**安装流程**：

```
用户选择插件 → install_plugin(name, marketplace)
  → 从 marketplace manifest 查找 source 路径
  → 定位到 marketplace 缓存中的插件目录
  → 读取 plugin.json 验证清单完整性
  → 计算版本（manifest.version 或 git SHA）
  → 复制到 ~/.claude/plugins/cache/{marketplace}/{plugin}/{version}/
  → 追加到 installed_plugins.json
  → 更新 settings.json 的 enabledPlugins
```

**卸载流程**：

```
uninstall_plugin(id)
  → 从 installed_plugins.json 移除条目
  → 删除缓存目录 ~/.claude/plugins/cache/{marketplace}/{plugin}/
  → 更新 settings.json 移除 enabledPlugins 对应条目
```

**版本管理**：

- 同一插件只保留一个版本（与 Claude Code 行为一致）
- 安装新版本时先卸载旧版本再安装新版本
- marketplace 刷新后检测版本更新，TUI 显示可更新标记

**安装范围**：

- `user`（默认）：`~/.claude/settings.json` 中的 `enabledPlugins`
- `project`：`.claude/settings.json` 中的 `enabledPlugins`

### 插件加载与集成

加载时机：Agent 初始化时（`agent_ops.rs`），在中间件注册之前。

**加载流程**：

```
PluginManager::load_plugins()
  → 遍历 installed_plugins.json 中已启用插件
  → 对每个插件:
      读取 .claude-plugin/plugin.json
      → 提取 commands → 注册到 PluginCommandProvider
      → 提取 skills  → 追加到 SkillsMiddleware 的搜索路径
      → 提取 mcp_servers → 合并到 McpMiddleware 的服务器池
      → 提取 agents  → 追加到 SubAgentMiddleware 的搜索路径
      → 提取 settings → 合并到运行时配置
```

**各系统集成点**：

| 插件能力 | 注入位置 | 机制 |
|----------|----------|------|
| Commands（斜杠命令） | TUI 命令系统 | `PluginCommandProvider` trait，命令浮层展示为 `{plugin}:{command}` 格式 |
| Skills | SkillsMiddleware | 追加 `plugin.install_path/skills/` 到搜索路径 |
| MCP Servers | McpMiddleware | 插件清单中的 `mcpServers` 合并到 McpConfig，统一走连接池 |
| Agents | SubAgentMiddleware | 追加 `plugin.install_path/agents/` 到 agent 搜索路径 |
| Settings | 运行时配置 | `settings` 字段合并到 AgentOverrides |

**PluginCommandProvider 设计**：

插件的命令本质是 Markdown 文件（`.md`），与现有 skill 的 `SKILL.md` 类似。加载时解析 frontmatter（shell/effort/model/args），注册为 slash command。

```rust
/// 命令提供者 trait——内置命令和插件命令都实现此接口
pub trait CommandProvider: Send + Sync {
    /// 返回命令名到命令定义的映射
    fn commands(&self) -> Vec<CommandEntry>;
}

pub struct CommandEntry {
    pub name: String,           // "plugin-name:command-name"
    pub description: String,
    pub source: CommandSource,  // Builtin / Plugin { path }
}
```

**命令名命名空间**：插件命令以 `{plugin_name}:{command_name}` 格式注册（如 `frontend-design:create-component`），避免与内置命令冲突。

### TUI 集成

**新增 `/plugin` 命令**：打开插件管理面板，包含三个视图：

| 视图 | 说明 |
|------|------|
| **Browse** | 浏览已安装插件列表，显示名称、版本、启用状态 |
| **Marketplace** | 浏览 marketplace 中可用插件，支持安装 |
| **Installed** | 管理已安装插件（启用/禁用/卸载/更新） |

**面板操作**：

| 按键 | 行为 |
|------|------|
| `↑/↓` | 列表导航 |
| `Space` | 启用/禁用切换 |
| `Enter` | 安装（Marketplace 视图）/ 展开详情（Installed 视图） |
| `d` | 卸载（需确认） |
| `u` | 检查更新 |
| `Tab` | 切换 Browse/Marketplace/Installed 视图 |
| `Esc` | 关闭面板 |

**状态栏集成**：`render_second_row` 新增 `plugin_panel` 分支，显示当前视图的快捷键提示。遵循面板快捷键设计规范——面板内部不渲染快捷键提示行，统一由状态栏第二行负责。

### 安全策略（简化版）

- 内置官方 marketplace 白名单（`anthropics/claude-plugins-official`）
- 非 marketplace 来源安装时显示一次信任确认弹窗
- 不实现 `strictKnownMarketplaces` / `blockedMarketplaces` 完整企业级策略
- 保留 `blockedMarketplaces` 字段在 settings 类型中（向前兼容），本次不读取

## 实现要点

### 关键技术决策

1. **`McpServerConfig` 复用**：插件清单中的 `mcpServers` 字段直接复用现有 `McpServerConfig` 类型（已有 command/args/env/url/headers 字段），无需定义新类型。

2. **Skills 搜索路径扩展**：现有 SkillsMiddleware 搜索顺序为 `~/.claude/skills/` → `skillsDir` → `./.claude/skills/`。插件 skills 追加到 `skillsDir` 之后，同名先到先得的优先级不变。插件 skill 名使用 `{plugin_name}:{skill_name}` 命名空间。

3. **MCP 配置合并时机**：插件的 `mcpServers` 在 `McpMiddleware` 初始化前合并到 `McpConfig`，走统一的连接池初始化流程。不额外创建连接池。

4. **异步安装**：安装/卸载操作在 `tokio::spawn_blocking` 中执行文件系统操作，避免阻塞 async runtime。

5. **`~/.claude/` 与 `~/.peri/` 双路径**：插件系统读写 `~/.claude/` 路径，现有配置继续使用 `~/.peri/settings.json`。两者独立管理，互不干扰。如需读取 `~/.claude/settings.json` 中的 `enabledPlugins` 字段，使用独立的文件读取而非替换现有配置系统。

6. **测试隔离**：插件安装/卸载涉及 `~/.claude/` 写操作，headless 测试需要通过 `config_path_override` 机制将写入重定向到临时目录，与现有测试隔离模式一致。

### 难点

1. **GitHub 源拉取**：需要 `git2` crate 或调用 `git` 命令行。推荐使用 `git` CLI（`tokio::process::Command`），避免 `git2` 的 OpenSSL 编译问题。

2. **NPM 源支持**：需要调用 `npm pack` 命令。考虑 NPM 源使用频率较低，可以延后实现或用简化方案（直接下载 tarball）。

3. **插件命令 Markdown 解析**：需要解析命令 `.md` 文件的 YAML frontmatter，提取 shell/effort/model/args 等字段。可复用 `pulldown-cmark`（已有依赖）或使用 `serde_yaml`。

### 依赖

| 新增依赖 | 用途 |
|----------|------|
| `reqwest`（已有） | HTTP 请求（URL marketplace） |
| `serde_yaml` | 解析命令 .md 文件的 frontmatter |
| `flate2` + `tar` | NPM tarball 解压（如实现 NPM 源） |

不需要新增 `git2`——使用 `git` CLI。

## 约束一致性

- **核心框架零内部依赖**：插件系统全部在 `peri-middlewares` 层实现，不修改 `peri-agent` 核心。✓
- **中间件模式**：`PluginMiddleware` 遵循 `Middleware` trait 接口。✓
- **字符串截断使用字符级操作**：插件名/描述在 TUI 显示时遵循此约束。✓
- **面板快捷键设计规范**：`/plugin` 面板遵循统一按键约定，状态栏负责快捷键提示。✓
- **测试隔离**：不写入全局配置，headless 测试通过 override 重定向。✓
- **Workspace resolver = "2"**：无新 crate，不违反依赖方向。✓
- **日志使用 tracing**：所有日志输出使用 tracing 宏。✓

## 验收标准

- [ ] 能解析 Claude Code 格式的 `plugin.json` 清单文件
- [ ] 能从 GitHub 仓库 marketplace 拉取插件列表
- [ ] 能从 URL marketplace 拉取插件列表
- [ ] 能从本地路径读取 marketplace
- [ ] 插件安装后写入 `~/.claude/plugins/installed_plugins.json`
- [ ] 已安装插件的 commands 在 `/` 浮层中可见（`plugin:command` 格式）
- [ ] 已安装插件的 skills 追加到 SkillsMiddleware 搜索路径
- [ ] 已安装插件的 mcpServers 合并到 McpMiddleware 连接池
- [ ] `/plugin` TUI 面板可浏览、安装、卸载插件
- [ ] 状态栏显示插件面板快捷键提示
- [ ] 官方 marketplace 首次启动自动注册并拉取
- [ ] 非 marketplace 来源安装时显示信任确认
- [ ] Headless 测试不写入真实 `~/.claude/` 目录
