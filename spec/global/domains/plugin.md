# Plugin 领域

## 领域综述

Plugin 领域负责 Claude Code 插件生态的兼容实现，包括插件发现、安装、加载、hooks 执行、MCP 服务器注入等能力。

核心职责：

- 插件清单解析：兼容 Claude Code 的 `plugin.json` 格式，定义 commands/agents/skills/mcp_servers/hooks/channels 等资产类型
- Marketplace 发现：支持 GitHub 仓库、URL、本地路径、NPM 等 marketplace 来源，后台拉取并缓存插件列表
- 插件安装管理：安装/卸载/版本管理，支持 user/project/local 三种安装范围，配置合并（全局+项目）
- Hooks 系统：4 种执行类型（Command/Prompt/Http/Agent），13 个生命周期事件，exit code 控制流程（0=Allow,1=Warn,2=Block）
- MCP 环境变量展开：per-plugin 独立展开 `${CLAUDE_PLUGIN_ROOT}` / `${CLAUDE_PLUGIN_DATA}` / `${user_config.KEY}`，支持对象和数组格式
- 资产集成：插件 commands 注入到命令系统，skills 追加到搜索路径，MCP servers 合并到连接池，agents 追加到搜索路径

## 核心流程

### 插件发现与安装流程

```
应用启动
  → PluginManager::init()
  → 加载 known_marketplaces.json
  → 合并 settings.json 中的 extraKnownMarketplaces
  → 对每个 marketplace 源:
      GitHub → git clone --depth 1 / git pull --ff-only
      URL    → HTTP GET + If-Modified-Since
      本地   → 直接读取 marketplace.json
  → 解析 marketplace.json → 列出可用插件
  → 缓存 manifest 到内存

用户选择插件 → install_plugin(name, marketplace)
  → 从 marketplace manifest 查找 source 路径
  → 定位到缓存中的插件目录
  → 读取 plugin.json 验证清单完整性
  → 复制到 ~/.claude/plugins/cache/{marketplace}/{plugin}/{version}/
  → 追加到 installed_plugins.json
  → 更新 settings.json 的 enabledPlugins
```

### 插件加载与集成流程

```
PluginManager::load_plugins()
  → 遍历 installed_plugins.json 中已启用插件
  → 对每个插件:
      读取 .claude-plugin/plugin.json
      → 提取 commands → 注册到 PluginCommandProvider
      → 提取 skills  → 追加到 SkillsMiddleware 搜索路径
      → 提取 mcp_servers → 合并到 McpMiddleware（per-plugin env 展开）
      → 提取 agents  → 追加到 SubAgentMiddleware 搜索路径
      → 提取 hooks  → 注册到 HookMiddleware
      → 提取 settings → 合并到运行时配置
```

### Hooks 执行流程

```
HookMiddleware 拦截（位置 10：HITL 之后、SubAgent 之前）
  → fire_event(HookEvent, HookInput)
  → 遍历已注册 hooks:
      once 检查 → 已执行则跳过
      matcher 粗粒度匹配（工具名/正则）
      if 细粒度条件匹配（permission rule 语法）
      变量替换（${CLAUDE_PLUGIN_ROOT}, ${CLAUDE_PLUGIN_DATA}）
      执行 hook:
          Command → shell 调用 + stdin JSON + stdout JSON 解析
          Prompt → LLM 评估（30s 超时）
          Http → HTTP POST + SSRF 防护 + CRLF 注入防护
          Agent → 完整 agent 循环（50 轮上限，防递归）
      解析结果 → HookAction（Allow/Block/ModifyInput/...）
      Block/PreventContinuation 短路
      once 标记
  → 合并所有 HookAction 返回
```

### MCP 环境变量展开流程

```
load_merged_config()
  → step 2: 插件 MCP → 每个 server 先独立展开 env
      expand_server_config_with_context(
          config,
          install_path,    // ${CLAUDE_PLUGIN_ROOT}
          data_path,       // ${CLAUDE_PLUGIN_DATA}
          user_config,     // ${user_config.KEY}
      )
  → step 3: project config（不变）
  → step 4: 去重（基于展开后的值）
  → step 5: 合并 global → plugin → project
  → step 6: 仅 project/global 展开（插件已展开）
```

## 技术方案总结

| 维度 | 选型 |
|------|------|
| 插件清单 | Claude Code `plugin.json` 兼容格式（commands/agents/skills/mcp_servers/hooks/channels） |
| Marketplace 来源 | GitHub/URL/File/Directory/NPM 四种类型 |
| 配置合并 | 全局 `~/.peri/settings.json` + 项目 `{cwd}/.mcp.json`（同名覆盖） |
| 插件路径 | `~/.claude/plugins/`（缓存）、`~/.claude/settings.json`（enabledPlugins） |
| Hooks 执行 | 4 种类型（Command/Prompt/Http/Agent），13 个 Phase 1 事件 |
| Exit code 语义 | 0=Allow+JSON解析, 1=Warn, 2=Block |
| 环境变量 | `${CLAUDE_PLUGIN_ROOT}` / `${CLAUDE_PLUGIN_DATA}` / `${user_config.KEY}` / `${VAR}` |
| SSRF 防护 | IPv4/IPv6 私有地址阻止，loopback 允许 |
| CRLF 注入防护 | header 值过滤 `\r\n\0` |
| MCP env 展开 | per-plugin 独立展开，先展开后合并 |
| ClaudeSettings | extraKnownMarketplaces 和 enabledPlugins 双格式支持（对象/数组） |
| once 追踪 | `HashSet<String>` 存储已执行 hook key |
| async 语义 | tokio::spawn 后台执行 |
| asyncRewake | Phase 2 暂不实现 |
| userConfig | Phase 2 暂不实现 |

## Feature 附录

### feature_20260507_F002_plugin-hook-support

**摘要:** 插件 Hooks 系统：4 种执行类型（command/prompt/http/agent）+ 13 个生命周期事件
**关键决策:**

- HookMiddleware 拦截模式，插入中间件链位置 10（HITL 之后、SubAgent 之前）
- 4 种执行类型：Command（shell）、Prompt（LLM）、Http（HTTP POST）、Agent（完整 agent 循环）
- 13 个 Phase 1 事件：PreToolUse/PostToolUse/PostToolUseFailure/PermissionRequest/UserPromptSubmit/SessionStart/SessionEnd/Stop/StopFailure/SubagentStart/SubagentStop/PreCompact/PostCompact
- Exit code 语义：0=Allow+JSON解析, 1=Warn, 2=Block
- 双层匹配机制：matcher（粗粒度字符串/正则）+ if（细粒度 permission rule 语法）
- SSRF 防护：IPv4/IPv6 私有地址阻止，loopback 允许
- CRLF 注入防护：header 值过滤 `\r\n\0`
- Hook failure 不阻断：默认 Allow，仅显式 decision=block/continue=false/exit 2 阻断
- Agent hook 防递归：子 agent 不注册 HookMiddleware 和 Agent 工具
**归档:** [链接](../../archive/feature_20260507_F002_plugin-hook-support/)
**归档日期:** 2026-05-13

### feature_20260507_F001_plugin-mcp-injection

**摘要:** 插件 MCP 环境变量 per-plugin 展开，pluginSource 旁路表
**关键决策:**

- MCP env 展开时机：per-plugin 独立展开，先展开后合并（避免合并后同名 key 冲突）
- pluginSource 旁路表：`McpClientPool.plugin_sources: HashMap<String, String>` 记录 `"plugin@{marketplace}"` 来源
- 零 breaking change：不改 `McpServerConfig` / `ConfigSource` / `LoadedPlugin`
- load_merged_config() 重排：step 2 中每个 plugin server 先独立展开，step 6 仅处理 project/global
- 去重 hash 基于展开后的实际值（更准确）
- user_config 暂不接入（options 存储层 Phase 2）
**归档:** [链接](../../archive/feature_20260507_F001_plugin-mcp-injection/)
**归档日期:** 2026-05-13

### feature_20260506_F001_plugin-marketplace-compat

**摘要:** Claude Code 插件生态兼容：发现/安装/加载 commands/skills/MCP/agents
**关键决策:**

- Plugin 模块嵌入 peri-middlewares（仿 MCP 中间件组织方式）
- 兼容 Claude Code `plugin.json` 清单格式（commands/agents/skills/mcp_servers/hooks/lsp_servers/channels/options/settings）
- Marketplace 来源：GitHub/URL/File/Directory/NPM 四种类型
- 配置读取优先级：项目级 `.claude/settings.json` > 用户级 `~/.claude/settings.json` > managed-settings.json
- 文件布局：复用 `~/.claude/` 路径结构（plugins/cache/marketplaces/）
- 各系统集成：commands→PluginCommandProvider、skills→SkillsMiddleware、mcp_servers→McpMiddleware、agents→SubAgentMiddleware
- 命令名命名空间：`{plugin_name}:{command_name}` 格式
- 后台刷新：marketplace 拉取 tokio::spawn 异步执行
- 安全策略：内置官方 marketplace 白名单，非 marketplace 来源显示信任确认
**归档:** [链接](../../archive/feature_20260506_F001_plugin-marketplace-compat/)
**归档日期:** 2026-05-13

## Issue 经验附录

### issue_2026-05-18-claude-dir-missing-plugin-panel-empty

**摘要:** ~/.claude 目录不存在时插件面板 Discover/Marketplaces 视图无法使用
**状态:** Fixed
**归档日期:** 2026-05-18
**关键词:** plugin, marketplace, 首次使用, 目录初始化, 容错
**问题本质:** 当 ~/.claude 目录不存在时，marketplace 缓存目录不存在导致 try_load_cache() 返回 None，Discover 视图显示 "No plugins available"。虽然读取路径都返回默认值不崩溃，但终端用户首次使用时无法自然进入插件发现流程。
**通用模式:** 懒加载目录结构时，需要在首次访问时主动创建必要子目录并触发初始数据拉取，而不能仅依赖读取路径的 None 容错。用户可见的"空面板"在语义上是误导性的——它暗示"系统正常但无数据"，而实际是"系统尚未初始化"。
**架构影响:** 打开面板时自动创建 ~/.claude/plugins/ 必要子目录，为 official marketplace 触发首次后台刷新。
**技术决策:** panel_ops.rs 的 open_plugin_panel() 中在加载缓存前检查并创建目录结构；首次加载时对官方 marketplace 自动触发 refresh。
**涉及文件:** peri-middlewares/src/plugin/config.rs, peri-tui/src/app/panel_ops.rs, peri-tui/src/ui/main_ui/panels/plugin.rs, peri-middlewares/src/plugin/marketplace/manager.rs
**CLAUDE.md 链接:** false

### issue_2026-05-29-wsl-plugin-install-marketplace-uninstall-fail

**摘要:** Peri 插件系统依赖 Claude Code 目录结构，未安装 CC 时安装/卸载不可用
**状态:** Fixed
**归档日期:** 2026-05-29
**关键词:** 插件依赖 CC, marketplace git clone, ~/.claude 目录
**问题本质:** 插件系统隐式依赖 Claude Code 的目录结构（~/.claude/plugins/marketplaces/），marketplace refresh 的 git clone 在无缓存目录时失败，导致后续安装/卸载连锁失败
**通用模式:** 外部服务依赖必须显式检测并提供回退方案，不能假设上游目录结构已存在
**架构影响:** 插件系统应能在独立环境中运行，启动时需自动补建最小目录结构
**涉及文件:** peri-middlewares/src/plugin/config.rs, peri-middlewares/src/plugin/installer/install.rs, peri-tui/src/app/plugin_panel/handlers/plugin_handlers/persistence.rs
**CLAUDE.md 链接:** false

---

## 相关 Feature

- → [agent.md](./agent.md) — Agent 核心，插件 MCP servers 合并到中间件链
- → [mcp.md](./mcp.md) — MCP 中间件，插件 MCP 环境变量展开
- → [tui.md](./tui.md) — TUI 界面，/plugin 面板管理
- → [hitl-permissions.md](./hitl-permissions.md) — HITL 权限，Hook if 条件复用 permission rule 语法
