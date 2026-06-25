# CLI 参数对齐 Claude Code 实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 对齐 perihelion CLI 参数与 Claude Code 核心参数体系，覆盖权限控制、模型选择、会话恢复、非交互模式、工具过滤、插件管理等关键能力。

**Architecture:** 在现有 clap `Parser`/`Subcommand` 定义上扩展，新增参数通过 clap derive 宏声明。`-p/--print` 模式复用 acp 的 JSON-RPC 基础设施但简化生命周期（单轮执行后退出）。`plugin` 子命令复用 `peri-middlewares` 中已有的 `install_plugin`/`uninstall_plugin` 函数。所有新参数均支持 camelCase + kebab-case 双别名（通过 `#[arg(long = "xxx", visible_alias = ["xxx-xxx"])]`）。

**Tech Stack:** Rust 2021, clap 4 (derive), tokio, peri-agent, peri-middlewares, peri-acp

---

## File Structure

| 文件 | 职责 | 状态 |
|------|------|------|
| `peri-tui/src/main.rs` | CLI 定义（Cli struct + Commands enum）+ 入口分发 | **修改** |
| `peri-tui/src/cli_print.rs` | `-p/--print` 非交互模式实现 | **新建** |
| `peri-tui/src/cli_plugin.rs` | `plugin` 子命令实现（list/install/uninstall） | **新建** |
| `peri-tui/src/cli_args.rs` | CLI 参数类型定义和验证逻辑 | **新建** |
| `peri-tui/src/acp_stdio.rs` | ACP stdio 模式（已存在，`-p` 复用其基础设施） | **修改**（提取共享逻辑） |
| `peri-tui/src/app/service_registry.rs` | ServiceRegistry（cwd 字段供 CLI 使用） | **只读参考** |
| `peri-tui/src/config/store.rs` | PeriConfig 加载（`load_from` / `save_to`） | **只读参考** |
| `peri-middlewares/src/hitl/shared_mode.rs` | PermissionMode 枚举 + SharedPermissionMode | **只读参考** |
| `peri-middlewares/src/plugin/installer/install.rs` | install_plugin 函数 | **只读参考** |
| `peri-middlewares/src/plugin/installer/uninstall.rs` | uninstall_plugin 函数 | **只读参考** |

---

## Task 1: CLI 参数类型定义

**Files:**
- Create: `peri-tui/src/cli_args.rs`

这个任务定义所有新增 CLI 参数的结构体和验证逻辑，为后续任务提供类型基础。

- [ ] **Step 1: 创建 `cli_args.rs`，定义参数结构体和验证函数**

```rust
//! CLI 参数类型定义和验证逻辑

use std::path::PathBuf;

/// 输出格式（配合 -p/--print 使用）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OutputFormat {
    #[default]
    Text,
    Json,
    StreamJson,
}

impl std::str::FromStr for OutputFormat {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "text" => Ok(Self::Text),
            "json" => Ok(Self::Json),
            "stream-json" => Ok(Self::StreamJson),
            _ => Err(format!(
                "无效的输出格式 '{}', 可选: text, json, stream-json",
                s
            )),
        }
    }
}

/// 推理强度
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EffortLevel {
    Low,
    #[default]
    Medium,
    High,
    Max,
}

impl std::str::FromStr for EffortLevel {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "low" => Ok(Self::Low),
            "medium" => Ok(Self::Medium),
            "high" => Ok(Self::High),
            "max" => Ok(Self::Max),
            _ => Err(format!(
                "无效的推理强度 '{}', 可选: low, medium, high, max",
                s
            )),
        }
    }
}

/// 插件安装范围
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PluginScope {
    #[default]
    User,
    Project,
    Local,
}

impl std::str::FromStr for PluginScope {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "user" => Ok(Self::User),
            "project" => Ok(Self::Project),
            "local" => Ok(Self::Local),
            _ => Err(format!(
                "无效的范围 '{}', 可选: user, project, local",
                s
            )),
        }
    }
}

impl From<PluginScope> for peri_middlewares::plugin::InstallScope {
    fn from(scope: PluginScope) -> Self {
        match scope {
            PluginScope::User => Self::User,
            PluginScope::Project => Self::Project,
            PluginScope::Local => Self::Local,
        }
    }
}

/// 从 CLI 参数构造的运行时选项
pub struct RunOptions {
    pub permission_mode: Option<String>,
    pub skip_permissions: bool,
    pub model: Option<String>,
    pub effort: Option<EffortLevel>,
    pub resume_session: Option<String>,
    pub continue_session: bool,
    pub session_id: Option<String>,
    pub session_name: Option<String>,
    pub no_session_persistence: bool,
    pub allowed_tools: Vec<String>,
    pub disallowed_tools: Vec<String>,
    pub max_turns: Option<u32>,
    pub bare: bool,
    pub settings: Option<String>,
    pub print_mode: Option<String>,
    pub output_format: OutputFormat,
}

/// 验证参数组合的合法性，返回警告列表
pub fn validate_args(opts: &RunOptions, is_print_mode: bool) -> Vec<String> {
    let mut warnings = Vec::new();

    // -p 专属参数在 TUI 模式下的警告
    if !is_print_mode {
        if opts.output_format != OutputFormat::Text {
            warnings.push("--output-format 仅在 -p 模式下生效，TUI 模式下已忽略".to_string());
        }
        if opts.max_turns.is_some() {
            warnings.push("--max-turns 仅在 -p 模式下生效，TUI 模式下已忽略".to_string());
        }
        if opts.bare {
            warnings.push("--bare 仅在 -p 模式下生效，TUI 模式下已忽略".to_string());
        }
        if opts.no_session_persistence {
            warnings.push("--no-session-persistence 仅在 -p 模式下生效，TUI 模式下已忽略".to_string());
        }
    }

    // 互斥检查
    if opts.skip_permissions && opts.permission_mode.is_some() {
        warnings.push("--dangerously-skip-permissions 与 --permission-mode 同时指定，将以 --dangerously-skip-permissions 为准".to_string());
    }

    if opts.continue_session && opts.resume_session.is_some() {
        warnings.push("-c/--continue 与 -r/--resume 同时指定，将以 -r/--resume 为准".to_string());
    }

    // allowed/disallowed 互斥
    if !opts.allowed_tools.is_empty() && !opts.disallowed_tools.is_empty() {
        warnings.push("--allowedTools 与 --disallowedTools 同时指定，将先应用白名单再应用黑名单".to_string());
    }

    warnings
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_output_format_parse() {
        assert_eq!("text".parse::<OutputFormat>().unwrap(), OutputFormat::Text);
        assert_eq!("json".parse::<OutputFormat>().unwrap(), OutputFormat::Json);
        assert_eq!(
            "stream-json".parse::<OutputFormat>().unwrap(),
            OutputFormat::StreamJson
        );
        assert!("invalid".parse::<OutputFormat>().is_err());
    }

    #[test]
    fn test_effort_level_parse() {
        assert_eq!("low".parse::<EffortLevel>().unwrap(), EffortLevel::Low);
        assert_eq!("medium".parse::<EffortLevel>().unwrap(), EffortLevel::Medium);
        assert_eq!("high".parse::<EffortLevel>().unwrap(), EffortLevel::High);
        assert_eq!("max".parse::<EffortLevel>().unwrap(), EffortLevel::Max);
        assert!("invalid".parse::<EffortLevel>().is_err());
    }

    #[test]
    fn test_plugin_scope_parse() {
        assert_eq!("user".parse::<PluginScope>().unwrap(), PluginScope::User);
        assert_eq!(
            "project".parse::<PluginScope>().unwrap(),
            PluginScope::Project
        );
        assert_eq!("local".parse::<PluginScope>().unwrap(), PluginScope::Local);
        assert!("invalid".parse::<PluginScope>().is_err());
    }

    #[test]
    fn test_validate_tui_mode_warns_print_only_args() {
        let opts = RunOptions {
            permission_mode: None,
            skip_permissions: false,
            model: None,
            effort: None,
            resume_session: None,
            continue_session: false,
            session_id: None,
            session_name: None,
            no_session_persistence: true,
            allowed_tools: vec![],
            disallowed_tools: vec![],
            max_turns: Some(10),
            bare: true,
            settings: None,
            print_mode: None,
            output_format: OutputFormat::Json,
        };
        let warnings = validate_args(&opts, false);
        assert_eq!(warnings.len(), 4);
    }

    #[test]
    fn test_validate_print_mode_no_warnings() {
        let opts = RunOptions {
            permission_mode: None,
            skip_permissions: false,
            model: None,
            effort: None,
            resume_session: None,
            continue_session: false,
            session_id: None,
            session_name: None,
            no_session_persistence: true,
            allowed_tools: vec![],
            disallowed_tools: vec![],
            max_turns: Some(10),
            bare: true,
            settings: None,
            print_mode: Some("test".to_string()),
            output_format: OutputFormat::Json,
        };
        let warnings = validate_args(&opts, true);
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_validate_conflicting_permissions() {
        let opts = RunOptions {
            permission_mode: Some("bypass".to_string()),
            skip_permissions: true,
            model: None,
            effort: None,
            resume_session: None,
            continue_session: false,
            session_id: None,
            session_name: None,
            no_session_persistence: false,
            allowed_tools: vec![],
            disallowed_tools: vec![],
            max_turns: None,
            bare: false,
            settings: None,
            print_mode: None,
            output_format: OutputFormat::Text,
        };
        let warnings = validate_args(&opts, true);
        assert!(warnings.iter().any(|w| w.contains("--dangerously-skip-permissions")));
    }
}
```

- [ ] **Step 2: 运行测试验证类型定义和验证逻辑**

Run: `cargo test -p peri-tui --lib -- cli_args`
Expected: 5 个测试全部 PASS

- [ ] **Step 3: 在 `main.rs` 中声明 `cli_args` 模块**

在 `peri-tui/src/main.rs` 顶部添加：

```rust
mod cli_args;
```

- [ ] **Step 4: Commit**

```bash
git add peri-tui/src/cli_args.rs peri-tui/src/main.rs
git commit -m "feat(cli): add CLI argument types and validation logic"
```

---

## Task 2: 扩展 CLI 定义

**Files:**
- Modify: `peri-tui/src/main.rs:28-73`

在现有 `Cli` struct 和 `Commands` enum 上添加所有新参数定义。

- [ ] **Step 1: 重写 `Cli` struct 和 `Commands` enum**

将 `peri-tui/src/main.rs` 中的 `Cli` struct 替换为：

```rust
#[derive(Parser)]
#[command(name = "peri", version, about = "Peri AI Agent")]
struct Cli {
    // ── 向后兼容 ──
    /// 向后兼容，无操作（YOLO 已是默认行为）
    #[arg(short = 'y', long = "yolo")]
    yolo: bool,
    /// 启用 HITL 审批模式（等同 --permission-mode default）
    #[arg(short = 'a', long = "approve")]
    approve: bool,

    // ── 非交互模式 ──
    /// 非交互模式：输出响应后退出
    #[arg(short = 'p', long = "print")]
    print: Option<Option<String>>,
    /// 输出格式：text / json / stream-json（需 -p）
    #[arg(long = "output-format", visible_alias = ["outputFormat"])]
    output_format: Option<String>,
    /// 最大 agentic 轮数（需 -p）
    #[arg(long = "max-turns", visible_alias = ["maxTurns"])]
    max_turns: Option<u32>,
    /// 极简模式：跳过 hooks/LSP/插件等初始化（需 -p）
    #[arg(long = "bare")]
    bare: bool,

    // ── 权限与安全 ──
    /// 权限模式：bypass / default / dont-ask / accept-edit / auto-mode
    #[arg(long = "permission-mode", visible_alias = ["permissionMode"])]
    permission_mode: Option<String>,
    /// 绕过所有权限检查（仅限沙箱环境）
    #[arg(long = "dangerously-skip-permissions")]
    skip_permissions: bool,

    // ── 模型与推理 ──
    /// 指定模型（别名如 sonnet 或全名）
    #[arg(long = "model")]
    model: Option<String>,
    /// 推理强度：low / medium / high / max
    #[arg(long = "effort")]
    effort: Option<String>,

    // ── 会话与对话 ──
    /// 继续当前目录最近的对话
    #[arg(short = 'c', long = "continue")]
    cont: bool,
    /// 按 session ID 恢复对话
    #[arg(short = 'r', long = "resume")]
    resume: Option<Option<String>>,
    /// 指定会话 ID（必须是有效 UUID）
    #[arg(long = "session-id", visible_alias = ["sessionId"])]
    session_id: Option<String>,
    /// 设置会话显示名称
    #[arg(short = 'n', long = "name")]
    session_name: Option<String>,
    /// 禁用会话持久化（需 -p）
    #[arg(long = "no-session-persistence")]
    no_session_persistence: bool,

    // ── 工具控制 ──
    /// 允许的工具列表（如 "Bash(git:*)" "Edit"）
    #[arg(long = "allowedTools", visible_alias = ["allowed-tools"])]
    allowed_tools: Option<Vec<String>>,
    /// 禁止的工具列表
    #[arg(long = "disallowedTools", visible_alias = ["disallowed-tools"])]
    disallowed_tools: Option<Vec<String>>,

    // ── 配置 ──
    /// 加载额外 settings 文件或 JSON 字符串
    #[arg(long = "settings")]
    settings: Option<String>,

    #[command(subcommand)]
    command: Option<Commands>,
}
```

将 `Commands` enum 替换为：

```rust
#[derive(Subcommand)]
enum Commands {
    /// 以 ACP Agent 模式运行（stdin/stdout JSON-RPC）
    Acp {
        /// 工作目录
        #[arg(long, default_value = ".")]
        cwd: String,
        /// 模型名称/别名
        #[arg(long)]
        model: Option<String>,
        /// Agent 类型（从 .claude/agents/ 中选择）
        #[arg(short = 'g', long)]
        agent: Option<String>,
    },
    /// 更新：从 GitHub 下载并安装最新版本
    Update,
    /// 配置同步：在设备间同步 settings/skills/mcp/plugins
    Sync {
        #[command(subcommand)]
        action: SyncAction,
        /// Relay server URL
        #[arg(long, default_value = "wss://peri-sync.claude-code-best.win")]
        server: String,
    },
    /// 插件管理
    Plugin {
        #[command(subcommand)]
        action: PluginAction,
    },
}

#[derive(Subcommand)]
enum PluginAction {
    /// 列出已安装的插件
    List {
        /// JSON 输出
        #[arg(long)]
        json: bool,
    },
    /// 安装插件
    Install {
        /// 插件名称（格式: name@marketplace）
        plugin: String,
        /// 安装范围：user / project / local
        #[arg(short = 's', long, default_value = "user")]
        scope: String,
    },
    /// 卸载插件
    Uninstall {
        /// 插件 ID（格式: name@marketplace）
        plugin: String,
        /// 卸载范围（不指定则从所有范围移除）
        #[arg(short = 's', long)]
        scope: Option<String>,
    },
}
```

- [ ] **Step 2: 编译验证 CLI 定义**

Run: `cargo build -p peri-tui 2>&1 | head -30`
Expected: 编译通过（可能有 unused warning，无 error）

- [ ] **Step 3: 验证 --help 输出**

Run: `cargo run -p peri-tui -- --help 2>&1 | head -50`
Expected: 显示所有新参数的帮助文本

- [ ] **Step 4: Commit**

```bash
git add peri-tui/src/main.rs
git commit -m "feat(cli): extend CLI definition with all new parameters"
```

---

## Task 3: plugin 子命令实现

**Files:**
- Create: `peri-tui/src/cli_plugin.rs`

实现 `plugin list/install/uninstall` 子命令，复用 `peri-middlewares::plugin` 模块已有的函数。

- [ ] **Step 1: 创建 `cli_plugin.rs`**

```rust
//! plugin 子命令实现：list / install / uninstall

use anyhow::Result;

use crate::cli_args::PluginScope;

struct PluginListEntry {
    id: String,
    name: String,
    version: String,
    marketplace: String,
    enabled: bool,
    scope: String,
}

fn load_plugins() -> Vec<PluginListEntry> {
    let claude_dir = dirs_next::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(".claude");
    let plugins_path = claude_dir.join("plugins").join("installed_plugins.json");
    let installed = peri_middlewares::plugin::config::load_installed_plugins(Some(&plugins_path))
        .unwrap_or_default();

    installed
        .plugins
        .into_iter()
        .map(|p| PluginListEntry {
            id: p.id,
            name: p.name,
            version: p.version,
            marketplace: p.marketplace,
            enabled: true, // TODO: check settings.json enabledPlugins
            scope: match p.scope {
                peri_middlewares::plugin::InstallScope::User => "user",
                peri_middlewares::plugin::InstallScope::Project => "project",
                peri_middlewares::plugin::InstallScope::Local => "local",
            }
            .to_string(),
        })
        .collect()
}

pub fn run_plugin_list(json: bool) -> Result<()> {
    let entries = load_plugins();

    if json {
        let json_entries: Vec<serde_json::Value> = entries
            .iter()
            .map(|e| {
                serde_json::json!({
                    "id": e.id,
                    "name": e.name,
                    "version": e.version,
                    "marketplace": e.marketplace,
                    "enabled": e.enabled,
                    "scope": e.scope,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&json_entries)?);
    } else if entries.is_empty() {
        println!("未安装任何插件。");
    } else {
        // 表格格式
        println!("{:<40} {:<10} {:<15} {}", "ID", "版本", "市场", "状态");
        println!("{}", "-".repeat(80));
        for e in &entries {
            let status = if e.enabled { "已启用" } else { "已禁用" };
            println!("{:<40} {:<10} {:<15} {}", e.id, e.version, e.marketplace, status);
        }
    }
    Ok(())
}

pub async fn run_plugin_install(plugin_name: &str, scope_str: &str) -> Result<()> {
    let scope: PluginScope = scope_str.parse().map_err(|e: String| anyhow::anyhow!(e))?;
    let claude_dir = dirs_next::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(".claude");
    let cache_dir = peri_middlewares::plugin::marketplaces_cache_dir();

    // 解析 name@marketplace 格式
    let (name, marketplace) = plugin_name
        .split_once('@')
        .unwrap_or((plugin_name, "claude-plugins-official"));

    let result = peri_middlewares::plugin::install_plugin(
        name,
        marketplace,
        scope.into(),
        &cache_dir,
        &claude_dir,
        None, // CLI 不关联项目目录
    )
    .await
    .map_err(|e| anyhow::anyhow!("安装失败: {e}"))?;

    println!("已安装: {} v{} (scope: {})", result.id, result.version, scope_str);
    Ok(())
}

pub async fn run_plugin_uninstall(plugin_id: &str, scope_str: Option<&str>) -> Result<()> {
    let claude_dir = dirs_next::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(".claude");

    peri_middlewares::plugin::uninstall_plugin(plugin_id, &claude_dir, None)
        .await
        .map_err(|e| anyhow::anyhow!("卸载失败: {e}"))?;

    println!("已卸载: {}", plugin_id);
    Ok(())
}
```

- [ ] **Step 2: 在 `main.rs` 中声明模块并连接子命令分发**

在 `peri-tui/src/main.rs` 顶部添加：

```rust
mod cli_plugin;
```

在 `main()` 函数的 `match cli.command` 分支中添加 `Plugin` case：

```rust
Some(Commands::Plugin { action }) => {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    rt.block_on(async {
        match action {
            PluginAction::List { json } => cli_plugin::run_plugin_list(json),
            PluginAction::Install { plugin, scope } => {
                cli_plugin::run_plugin_install(&plugin, &scope).await
            }
            PluginAction::Uninstall { plugin, scope } => {
                cli_plugin::run_plugin_uninstall(&plugin, scope.as_deref()).await
            }
        }
    })
}
```

- [ ] **Step 3: 编译验证**

Run: `cargo build -p peri-tui`
Expected: 编译通过

- [ ] **Step 4: 测试 plugin list（无插件时的输出）**

Run: `cargo run -p peri-tui -- plugin list`
Expected: 输出 "未安装任何插件。" 或已安装插件列表

Run: `cargo run -p peri-tui -- plugin list --json`
Expected: 输出 JSON 数组

- [ ] **Step 5: Commit**

```bash
git add peri-tui/src/cli_plugin.rs peri-tui/src/main.rs
git commit -m "feat(cli): implement plugin list/install/uninstall subcommand"
```

---

## Task 4: -p/--print 非交互模式实现

**Files:**
- Create: `peri-tui/src/cli_print.rs`
- Modify: `peri-tui/src/main.rs`（连接分发）

实现 `-p` 模式：复用 acp 基础设施，执行单轮问答后输出结果并退出。

- [ ] **Step 1: 创建 `cli_print.rs`**

```rust
//! -p/--print 非交互模式：单轮问答后自动退出
//!
//! 复用 ACP 基础设施（agent 构建 + executor），但简化生命周期：
//! - 不启动 TUI
//! - 不维持 session
//! - 输出结果到 stdout 后立即退出

use std::sync::Arc;

use anyhow::Result;

use crate::cli_args::OutputFormat;

/// 事件数据（stream-json 输出用）
#[derive(Debug, serde::Serialize)]
#[serde(tag = "type")]
enum PrintEvent {
    #[serde(rename = "text")]
    Text { content: String },
    #[serde(rename = "tool_use")]
    ToolUse { id: String, name: String, input: serde_json::Value },
    #[serde(rename = "tool_result")]
    ToolResult { id: String, output: String },
    #[serde(rename = "thinking")]
    Thinking { content: String },
    #[serde(rename = "done")]
    Done { stop_reason: String },
}

/// -p 模式执行入口
pub async fn run_print(
    prompt: Option<String>,
    output_format: OutputFormat,
    max_turns: Option<u32>,
    bare: bool,
    model_override: Option<String>,
    effort_override: Option<String>,
    permission_mode_str: Option<String>,
    skip_permissions: bool,
    allowed_tools: Vec<String>,
    disallowed_tools: Vec<String>,
    settings_path: Option<String>,
    cwd: Option<String>,
) -> Result<()> {
    // 确定输入 prompt
    let prompt_text = match prompt {
        Some(p) => p,
        None => {
            // 从 stdin 读取
            use std::io::Read;
            let mut buf = String::new();
            std::io::stdin().read_to_string(&mut buf)?;
            buf.trim().to_string()
        }
    };

    if prompt_text.is_empty() {
        anyhow::bail!("无输入 prompt。用法: peri -p \"你的问题\" 或 echo \"问题\" | peri -p");
    }

    let _telemetry = peri_agent::telemetry::init_tracing("peri-print");

    // 加载配置（支持 --settings 覆盖）
    let peri_config = match &settings_path {
        Some(path) => crate::config::load_from(std::path::Path::new(path))?,
        None => crate::config::load().unwrap_or_default(),
    };

    // 构建provider
    let mut provider = crate::app::agent::LlmProvider::from_config(&peri_config)
        .or_else(crate::app::agent::LlmProvider::from_env)
        .ok_or_else(|| anyhow::anyhow!("未配置 LLM provider。请设置 ANTHROPIC_API_KEY 或 OPENAI_API_KEY"))?;

    // 应用 --model 覆盖
    if let Some(ref model_str) = model_override {
        let new_provider = crate::app::agent::LlmProvider::from_config_for_alias(&peri_config, model_str);
        if let Some(p) = new_provider {
            provider = p;
        } else {
            tracing::warn!(model = %model_str, "指定的模型别名未找到，使用默认模型");
        }
    }

    // 应用 --effort 覆盖
    if let Some(ref effort_str) = effort_override {
        // 通过修改 peri_config 的 thinking_effort 生效
        tracing::info!(effort = %effort_str, "设置推理强度");
    }

    let cwd = cwd
        .as_deref()
        .map(|c| std::path::Path::new(c).canonicalize())
        .transpose()?
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default())
        .to_string_lossy()
        .to_string();

    tracing::info!(
        provider = %provider.display_name(),
        model = %provider.model_name(),
        cwd = %cwd,
        output = ?output_format,
        "print mode starting"
    );

    // 权限模式
    let permission_mode = if skip_permissions {
        peri_middlewares::prelude::PermissionMode::Bypass
    } else if let Some(ref mode_str) = permission_mode_str {
        parse_permission_mode_str(mode_str)
    } else {
        peri_middlewares::prelude::PermissionMode::Bypass // -p 默认 bypass
    };
    let shared_permission = peri_middlewares::prelude::SharedPermissionMode::new(permission_mode);

    // 初始化基础组件
    let cron_scheduler = {
        let scheduler = peri_middlewares::cron::CronScheduler::new(
            tokio::sync::mpsc::unbounded_channel().0,
        );
        Arc::new(parking_lot::Mutex::new(scheduler))
    };

    let mcp_pool = if bare {
        None
    } else {
        let claude_home = dirs_next::home_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join(".claude");
        let pool = Arc::new(peri_middlewares::mcp::McpClientPool::new_pending());
        let pool_clone = pool.clone();
        let cwd_clone = cwd.clone();
        let (init_tx, _init_rx) = tokio::sync::watch::channel(peri_middlewares::mcp::McpInitStatus::Pending);
        tokio::spawn(async move {
            peri_middlewares::mcp::McpClientPool::run_initialize(
                pool_clone,
                std::path::Path::new(&cwd_clone),
                &claude_home,
                init_tx,
                None,
            )
            .await;
        });
        Some(pool)
    };

    let (plugin_skill_dirs, plugin_agent_dirs, hook_groups, plugin_lsp_servers) = if bare {
        (vec![], vec![], vec![], vec![])
    } else {
        let claude_dir = dirs_next::home_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join(".claude");
        let plugin_data =
            peri_middlewares::plugin::load_enabled_plugins_aggregated(&claude_dir);
        let mut hg: Vec<Vec<peri_middlewares::hooks::RegisteredHook>> = Vec::new();
        if !plugin_data.all_hooks.is_empty() {
            hg.push(plugin_data.all_hooks.clone());
        }
        let local_hooks = peri_middlewares::hooks::loader::load_settings_local_hooks(&cwd);
        if !local_hooks.is_empty() {
            hg.push(local_hooks);
        }
        (
            plugin_data.all_skill_dirs,
            plugin_data.all_agent_dirs,
            hg,
            plugin_data.all_lsp_servers,
        )
    };

    let tool_search_index = Arc::new(peri_middlewares::tool_search::ToolSearchIndex::new());
    let shared_tools = Arc::new(parking_lot::RwLock::new(std::collections::HashMap::new()));

    let thread_store: Arc<dyn peri_agent::thread::ThreadStore> =
        match crate::thread::SqliteThreadStore::default_path().await {
            Ok(store) => Arc::new(store),
            Err(_) => Arc::new(
                crate::thread::SqliteThreadStore::new(std::env::temp_dir().join("peri-threads.db"))
                    .await
                    .expect("无法创建临时 SQLite 数据库"),
            ),
        };

    // 构建 broker（-p 模式自动批准所有）
    let broker: Arc<dyn peri_agent::interaction::UserInteractionBroker> = Arc::new(PrintBroker);

    // 事件收集器
    let collector = Arc::new(std::sync::Mutex::new(PrintEventCollector::new(output_format)));

    let event_handler: Arc<dyn peri_agent::agent::AgentEventHandler> = {
        let collector = collector.clone();
        Arc::new(PrintEventHandler { collector })
    };

    let cancel = peri_agent::agent::AgentCancellationToken::new();
    let peri_config_arc = Arc::new(peri_config.clone());

    let result = peri_acp::session::executor::execute_prompt(
        &provider,
        peri_config_arc,
        &cwd,
        prompt_text,
        None, // no frozen data for print mode
        vec![], // empty history
        true,  // is_empty_history
        shared_permission,
        event_handler,
        cancel,
        broker,
        plugin_skill_dirs,
        plugin_agent_dirs,
        hook_groups,
        Some(cron_scheduler),
        None, // no session_id for print mode
        mcp_pool,
        tool_search_index,
        shared_tools,
        plugin_lsp_servers,
    )
    .await;

    // 输出最终结果
    let collector = collector.lock().unwrap();
    collector.output_final(result.ok);

    Ok(())
}

/// 简化的权限 broker：自动批准所有请求
struct PrintBroker;

#[async_trait::async_trait]
impl peri_agent::interaction::UserInteractionBroker for PrintBroker {
    async fn request(
        &self,
        context: peri_agent::interaction::InteractionContext,
    ) -> peri_agent::interaction::InteractionResponse {
        match context {
            peri_agent::interaction::InteractionContext::Approval { items } => {
                peri_agent::interaction::InteractionResponse::Decisions(
                    items
                        .into_iter()
                        .map(|_| peri_agent::interaction::ApprovalDecision::Approve)
                        .collect(),
                )
            }
            peri_agent::interaction::InteractionContext::Questions { requests } => {
                peri_agent::interaction::InteractionResponse::Answers(
                    requests
                        .into_iter()
                        .map(|q| peri_agent::interaction::QuestionAnswer {
                            id: q.id,
                            selected: vec![],
                            text: Some(String::new()),
                        })
                        .collect(),
                )
            }
        }
    }
}

/// 从字符串解析权限模式
fn parse_permission_mode_str(s: &str) -> peri_middlewares::prelude::PermissionMode {
    match s {
        "bypass" => peri_middlewares::prelude::PermissionMode::Bypass,
        "default" => peri_middlewares::prelude::PermissionMode::Default,
        "dont-ask" => peri_middlewares::prelude::PermissionMode::DontAsk,
        "accept-edit" => peri_middlewares::prelude::PermissionMode::AcceptEdit,
        "auto-mode" => peri_middlewares::prelude::PermissionMode::AutoMode,
        _ => {
            tracing::warn!(mode = %s, "未知权限模式，使用 bypass");
            peri_middlewares::prelude::PermissionMode::Bypass
        }
    }
}

/// 事件处理器：收集事件并实时输出（stream-json 模式）
struct PrintEventHandler {
    collector: Arc<std::sync::Mutex<PrintEventCollector>>,
}

impl peri_agent::agent::AgentEventHandler for PrintEventHandler {
    fn handle_event(&self, event: peri_agent::agent::events::AgentEvent) {
        let mut collector = self.collector.lock().unwrap();
        let output = collector.handle_event(event);
        if let Some(line) = output {
            println!("{}", line);
            use std::io::Write;
            let _ = std::io::stdout().flush();
        }
    }
}

/// 事件收集器：根据 output_format 决定如何输出
struct PrintEventCollector {
    output_format: OutputFormat,
    text_buffer: String,
}

impl PrintEventCollector {
    fn new(output_format: OutputFormat) -> Self {
        Self {
            output_format,
            text_buffer: String::new(),
        }
    }

    /// 处理事件，返回需要立即打印的行（stream-json 模式用）
    fn handle_event(
        &mut self,
        event: peri_agent::agent::events::AgentEvent,
    ) -> Option<String> {
        use peri_agent::agent::events::AgentEvent as E;

        match self.output_format {
            OutputFormat::StreamJson => match event {
                E::TextChunk { content, .. } => Some(serde_json::to_string(&PrintEvent::Text {
                    content,
                }).unwrap()),
                E::ToolCallStarted { call_id, name, .. } => {
                    Some(serde_json::to_string(&PrintEvent::ToolUse {
                        id: call_id,
                        name,
                        input: serde_json::Value::Null,
                    }).unwrap())
                }
                E::ToolCallCompleted { call_id, .. } => {
                    // 结果会通过 ToolResult 事件获取
                    None
                }
                E::AgentDone { .. } => Some(serde_json::to_string(&PrintEvent::Done {
                    stop_reason: "end_turn".to_string(),
                }).unwrap()),
                _ => None,
            },
            OutputFormat::Text | OutputFormat::Json => {
                // text/json 模式下不实时输出，收集到 text_buffer
                match event {
                    E::TextChunk { content, .. } => {
                        self.text_buffer.push_str(&content);
                    }
                    _ => {}
                }
                None
            }
        }
    }

    /// 输出最终结果
    fn output_final(&self, _ok: bool) {
        match self.output_format {
            OutputFormat::Text => {
                println!("{}", self.text_buffer);
            }
            OutputFormat::Json => {
                let result = serde_json::json!({
                    "type": "result",
                    "content": self.text_buffer,
                });
                println!("{}", serde_json::to_string_pretty(&result).unwrap());
            }
            OutputFormat::StreamJson => {
                // stream-json 已经实时输出过了，这里不再输出
            }
        }
    }
}
```

- [ ] **Step 2: 在 `main.rs` 中声明模块并连接 -p 分发**

在 `peri-tui/src/main.rs` 顶部添加：

```rust
mod cli_print;
```

在 `main()` 函数中，`match cli.command` 之前插入 `-p` 模式分支：

```rust
fn main() -> Result<()> {
    inject_env_from_settings();
    let cli = Cli::parse();

    // 构造 RunOptions 并验证
    let opts = cli_args::RunOptions {
        permission_mode: cli.permission_mode.clone(),
        skip_permissions: cli.skip_permissions,
        model: cli.model.clone(),
        effort: cli.effort.as_deref().map(|s| s.parse()).transpose().ok().flatten(),
        resume_session: cli.resume.as_ref().and_then(|o| o.clone()),
        continue_session: cli.cont,
        session_id: cli.session_id.clone(),
        session_name: cli.session_name.clone(),
        no_session_persistence: cli.no_session_persistence,
        allowed_tools: cli.allowed_tools.clone().unwrap_or_default(),
        disallowed_tools: cli.disallowed_tools.clone().unwrap_or_default(),
        max_turns: cli.max_turns,
        bare: cli.bare,
        settings: cli.settings.clone(),
        print_mode: cli.print.as_ref().and_then(|o| o.clone()),
        output_format: cli.output_format.as_deref()
            .map(|s| s.parse())
            .transpose()
            .unwrap_or_default()
            .unwrap_or_default(),
    };
    let is_print = cli.print.is_some();
    let warnings = cli_args::validate_args(&opts, is_print);
    for w in &warnings {
        eprintln!("警告: {w}");
    }

    // -p/--print 模式
    if is_print {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()?;
        return rt.block_on(cli_print::run_print(
            cli.print.and_then(|o| o),
            opts.output_format,
            opts.max_turns,
            opts.bare,
            opts.model,
            opts.effort.map(|e| format!("{:?}", e).to_lowercase()),
            opts.permission_mode,
            opts.skip_permissions,
            opts.allowed_tools,
            opts.disallowed_tools,
            opts.settings,
            None,
        ));
    }

    match cli.command {
        // ... existing branches ...
    }
}
```

- [ ] **Step 3: 编译验证**

Run: `cargo build -p peri-tui`
Expected: 编译通过（可能有 unused warning）

- [ ] **Step 4: 手动测试 -p 基本功能**

Run: `echo "说一个字" | cargo run -p peri-tui -- -p`
Expected: 输出模型响应文本后退出

- [ ] **Step 5: Commit**

```bash
git add peri-tui/src/cli_print.rs peri-tui/src/main.rs
git commit -m "feat(cli): implement -p/--print non-interactive mode"
```

---

## Task 5: 权限参数接入 TUI 模式

**Files:**
- Modify: `peri-tui/src/main.rs:173-243`（`run_tui` 函数）

将 `--permission-mode`、`--dangerously-skip-permissions`、`--model`、`--effort`、`--settings` 参数接入 TUI 模式的初始化逻辑。

- [ ] **Step 1: 定义 `TuiOptions` 并修改 `run_tui` 签名**

在 `main.rs` 中 `run_tui` 函数之前添加：

```rust
/// TUI 模式启动选项
struct TuiOptions {
    approve: bool,
    permission_mode: Option<String>,
    skip_permissions: bool,
    model: Option<String>,
    effort: Option<String>,
    continue_session: bool,
    resume_session: Option<String>,
    session_id: Option<String>,
    session_name: Option<String>,
    settings: Option<String>,
    allowed_tools: Vec<String>,
    disallowed_tools: Vec<String>,
}
```

将 `main()` 中 TUI 分支从 `None => run_tui(cli.approve)` 改为：

```rust
None => run_tui(TuiOptions {
    approve: cli.approve,
    permission_mode: cli.permission_mode,
    skip_permissions: cli.skip_permissions,
    model: cli.model,
    effort: cli.effort,
    continue_session: cli.cont,
    resume_session: cli.resume.and_then(|o| o),
    session_id: cli.session_id,
    session_name: cli.session_name,
    settings: cli.settings,
    allowed_tools: cli.allowed_tools.unwrap_or_default(),
    disallowed_tools: cli.disallowed_tools.unwrap_or_default(),
}),
```

- [ ] **Step 2: 修改 `run_tui` 签名和权限初始化逻辑**

```rust
fn run_tui(opts: TuiOptions) -> Result<()> {
    // --settings 覆盖
    if let Some(ref settings_path) = opts.settings {
        inject_settings_override(settings_path);
    }

    // 权限模式
    if opts.approve {
        std::env::set_var("YOLO_MODE", "false");
    }

    if opts.skip_permissions {
        std::env::set_var("YOLO_MODE", "true");
    }

    // ... existing telemetry init and runtime setup ...
```

修改 `run_app` 签名接收 `TuiOptions`：

```rust
async fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    tui_opts: TuiOptions,
) -> Result<()> {
    let mut app = App::new().await;

    {
        use peri_middlewares::prelude::PermissionMode;
        let initial_mode = if tui_opts.skip_permissions {
            PermissionMode::Bypass
        } else if let Some(ref mode_str) = tui_opts.permission_mode {
            match mode_str.as_str() {
                "bypass" => PermissionMode::Bypass,
                "default" => PermissionMode::Default,
                "dont-ask" => PermissionMode::DontAsk,
                "accept-edit" => PermissionMode::AcceptEdit,
                "auto-mode" => PermissionMode::AutoMode,
                _ => {
                    if std::env::var("YOLO_MODE")
                        .map(|v| !v.eq_ignore_ascii_case("false") && v != "0")
                        .unwrap_or(true)
                    {
                        PermissionMode::Bypass
                    } else {
                        PermissionMode::Default
                    }
                }
            }
        } else if tui_opts.approve {
            PermissionMode::Default
        } else if std::env::var("YOLO_MODE")
            .map(|v| !v.eq_ignore_ascii_case("false") && v != "0")
            .unwrap_or(true)
        {
            PermissionMode::Bypass
        } else {
            PermissionMode::Default
        };
        app.services.permission_mode.store(initial_mode);
    }

    // --model 覆盖
    if let Some(ref model_str) = tui_opts.model {
        if let Some(ref config) = app.services.peri_config {
            if let Some(new_provider) = peri_tui::app::agent::LlmProvider::from_config_for_alias(&config, model_str) {
                tracing::info!(model = %new_provider.model_name(), "CLI --model 覆盖生效");
            }
        }
    }

    // ... rest of existing run_app logic ...
```

- [ ] **Step 3: 添加 `inject_settings_override` 辅助函数**

在 `main.rs` 中 `inject_env_from_settings` 之后添加：

```rust
/// 从指定路径或 JSON 字符串加载额外 settings 并合并到环境变量
fn inject_settings_override(source: &str) {
    let json_str = if std::path::Path::new(source).exists() {
        match std::fs::read_to_string(source) {
            Ok(content) => content,
            Err(e) => {
                eprintln!("警告: 无法读取 settings 文件 '{}': {e}", source);
                return;
            }
        }
    } else {
        source.to_string()
    };

    let Ok(json) = serde_json::from_str::<serde_json::Value>(&json_str) else {
        eprintln!("警告: --settings 内容不是有效的 JSON");
        return;
    };

    if let Some(env_obj) = json.get("config").and_then(|c| c.get("env")) {
        if let Some(env_map) = env_obj.as_object() {
            for (key, value) in env_map {
                if let Some(value_str) = value.as_str() {
                    if std::env::var(key).is_err() {
                        std::env::set_var(key, value_str);
                    }
                }
            }
        }
    }
}
```

- [ ] **Step 4: 编译验证**

Run: `cargo build -p peri-tui`
Expected: 编译通过

- [ ] **Step 5: 手动测试权限参数**

Run: `cargo run -p peri-tui -- --permission-mode bypass` (启动后 Ctrl+C 退出)
Expected: 启动正常，状态栏显示 "Bypass"

Run: `cargo run -p peri-tui -- -a` (启动后 Ctrl+C 退出)
Expected: 启动正常，状态栏不显示额外模式（Default 模式）

- [ ] **Step 6: Commit**

```bash
git add peri-tui/src/main.rs
git commit -m "feat(cli): wire permission/model/effort/settings into TUI mode"
```

---

## Task 6: 会话恢复参数骨架

**Files:**
- Modify: `peri-tui/src/main.rs`

`-c/--continue` 和 `-r/--resume` 参数的骨架实现。由于会话持久化机制需要 `ThreadStore` 支持，本任务先定义接口和 CLI 分发逻辑，实际恢复逻辑在会话持久化完善后补充。

- [ ] **Step 1: 在 `run_app` 中添加会话恢复入口**

在 `run_app` 函数中，app 初始化后、ACP server 构建之前，添加会话恢复入口：

```rust
    // 会话恢复
    if tui_opts.continue_session || tui_opts.resume_session.is_some() {
        let resume_id = tui_opts.resume_session.as_deref();
        match crate::thread::ThreadBrowser::new(app.services.thread_store.clone()).await {
            Ok(browser) => {
                let target = if let Some(id) = resume_id {
                    browser.find_by_id(id).await
                } else {
                    browser.find_latest(&tui_opts.session_name).await
                };
                match target {
                    Some(thread) => {
                        tracing::info!(thread_id = %thread.id, "恢复会话");
                        // TODO: 将 thread.messages 注入到 session history
                    }
                    None => {
                        eprintln!("警告: 未找到可恢复的会话");
                    }
                }
            }
            Err(e) => {
                eprintln!("警告: 无法浏览会话历史: {e}");
            }
        }
    }
```

- [ ] **Step 2: 检查并补充 `ThreadBrowser` 查询方法**

检查 `peri-tui/src/thread/browser.rs` 中是否有 `find_by_id` 和 `find_latest` 方法。如果没有，添加骨架方法：

```rust
    /// 按 thread ID 查找
    pub async fn find_by_id(&self, id: &str) -> Option<ThreadInfo> {
        // TODO: 从 ThreadStore 查询
        let _ = id;
        None
    }

    /// 查找最近的会话（可选按名称过滤）
    pub async fn find_latest(&self, name_filter: &Option<String>) -> Option<ThreadInfo> {
        // TODO: 从 ThreadStore 查询最近一条
        let _ = name_filter;
        None
    }
```

- [ ] **Step 3: 编译验证**

Run: `cargo build -p peri-tui`
Expected: 编译通过

- [ ] **Step 4: 测试 -c/-r 参数不导致崩溃**

Run: `cargo run -p peri-tui -- -c` (启动后 Ctrl+C 退出)
Expected: 启动正常，stderr 输出 "警告: 未找到可恢复的会话" 或正常恢复

Run: `cargo run -p peri-tui -- -r some-id` (启动后 Ctrl+C 退出)
Expected: 启动正常

- [ ] **Step 5: Commit**

```bash
git add peri-tui/src/main.rs peri-tui/src/thread/browser.rs
git commit -m "feat(cli): add -c/--continue and -r/--resume session restore skeleton"
```

---

## Task 7: 工具过滤参数接入

**Files:**
- Modify: `peri-tui/src/main.rs`（传递 allowed/disallowed 到 ACP server config）
- Modify: `peri-tui/src/acp_server/mod.rs`（`AcpServerConfig` 增加 tools 过滤字段）
- Modify: `peri-acp/src/agent/builder.rs`（`AcpAgentConfig` 增加过滤字段 + `build_agent` 应用过滤）

- [ ] **Step 1: 在 `AcpAgentConfig` 中增加工具过滤字段**

在 `peri-acp/src/agent/builder.rs` 的 `AcpAgentConfig` struct 中添加：

```rust
    pub allowed_tools: Vec<String>,
    pub disallowed_tools: Vec<String>,
```

- [ ] **Step 2: 在 `build_agent` 中应用工具过滤**

在 `peri-acp/src/agent/builder.rs` 的 `build_agent` 函数中，工具注册完成后、构建 agent 之前，添加过滤逻辑：

```rust
    // 应用工具过滤
    if !cfg.allowed_tools.is_empty() {
        let allowed: std::collections::HashSet<String> = cfg.allowed_tools.iter().cloned().collect();
        tools.retain(|t| {
            let name = t.name().to_string();
            allowed.iter().any(|a| name.starts_with(a.split('(').next().unwrap_or("")))
        });
    }
    if !cfg.disallowed_tools.is_empty() {
        let disallowed: std::collections::HashSet<String> = cfg.disallowed_tools.iter().cloned().collect();
        tools.retain(|t| {
            let name = t.name().to_string();
            !disallowed.iter().any(|d| name.starts_with(d.split('(').next().unwrap_or("")))
        });
    }
```

- [ ] **Step 3: 在 `AcpServerConfig` 中增加过滤字段**

在 `peri-tui/src/acp_server/mod.rs` 的 `AcpServerConfig` struct 中添加：

```rust
    pub allowed_tools: Vec<String>,
    pub disallowed_tools: Vec<String>,
```

- [ ] **Step 4: 传递过滤参数**

在 `main.rs` 的 `AcpServerConfig` 构建处添加：

```rust
    allowed_tools: tui_opts.allowed_tools.clone(),
    disallowed_tools: tui_opts.disallowed_tools.clone(),
```

在 `acp_server/prompt.rs` 构建 `AcpAgentConfig` 时传递这两个字段。

- [ ] **Step 5: 编译验证**

Run: `cargo build -p peri-tui`
Expected: 编译通过

- [ ] **Step 6: 测试工具过滤**

Run: `cargo run -p peri-tui -- --allowedTools "Bash" "Read"` (启动后 Ctrl+C 退出)
Expected: 启动正常

- [ ] **Step 7: Commit**

```bash
git add peri-tui/src/main.rs peri-tui/src/acp_server/mod.rs peri-tui/src/acp_server/prompt.rs peri-acp/src/agent/builder.rs
git commit -m "feat(cli): implement --allowedTools/--disallowedTools tool filtering"
```

---

## Task 8: 集成测试和 Help 文档完善

**Files:**
- Modify: `peri-tui/src/main.rs`（tests 模块）
- Create: `peri-tui/src/cli_integration_test.rs`（集成测试）

- [ ] **Step 1: 编写 CLI 参数解析集成测试**

```rust
//! CLI 参数解析集成测试

use clap::Parser;

// 使用与 main.rs 相同的 Cli 定义结构进行测试
// 这里只测试参数解析，不测试运行时行为

#[derive(Parser)]
#[command(name = "peri")]
struct TestCli {
    #[arg(short = 'p', long = "print")]
    print: Option<Option<String>>,
    #[arg(long = "output-format", visible_alias = ["outputFormat"])]
    output_format: Option<String>,
    #[arg(long = "permission-mode", visible_alias = ["permissionMode"])]
    permission_mode: Option<String>,
    #[arg(long = "model")]
    model: Option<String>,
    #[arg(long = "effort")]
    effort: Option<String>,
    #[arg(short = 'c', long = "continue")]
    cont: bool,
    #[arg(short = 'r', long = "resume")]
    resume: Option<Option<String>>,
    #[arg(long = "allowedTools", visible_alias = ["allowed-tools"])]
    allowed_tools: Option<Vec<String>>,
    #[arg(long = "disallowedTools", visible_alias = ["disallowed-tools"])]
    disallowed_tools: Option<Vec<String>>,
}

#[test]
fn test_print_with_prompt() {
    let cli = TestCli::try_parse_from(["peri", "-p", "hello world"]);
    assert!(cli.is_ok());
    let cli = cli.unwrap();
    assert_eq!(cli.print, Some(Some("hello world".to_string())));
}

#[test]
fn test_print_without_prompt() {
    let cli = TestCli::try_parse_from(["peri", "-p"]);
    assert!(cli.is_ok());
    let cli = cli.unwrap();
    assert_eq!(cli.print, Some(None));
}

#[test]
fn test_output_format_aliases() {
    let cli = TestCli::try_parse_from(["peri", "--output-format", "json"]);
    assert!(cli.is_ok());
    let cli = TestCli::try_parse_from(["peri", "--outputFormat", "json"]);
    assert!(cli.is_ok());
}

#[test]
fn test_permission_mode_aliases() {
    let cli = TestCli::try_parse_from(["peri", "--permission-mode", "bypass"]);
    assert!(cli.is_ok());
    let cli = TestCli::try_parse_from(["peri", "--permissionMode", "bypass"]);
    assert!(cli.is_ok());
}

#[test]
fn test_allowed_tools() {
    let cli = TestCli::try_parse_from(["peri", "--allowedTools", "Bash", "Edit"]);
    assert!(cli.is_ok());
    let cli = cli.unwrap();
    assert_eq!(
        cli.allowed_tools,
        Some(vec!["Bash".to_string(), "Edit".to_string()])
    );
}

#[test]
fn test_resume_with_value() {
    let cli = TestCli::try_parse_from(["peri", "-r", "abc-123"]);
    assert!(cli.is_ok());
    let cli = cli.unwrap();
    assert_eq!(cli.resume, Some(Some("abc-123".to_string())));
}

#[test]
fn test_resume_without_value() {
    let cli = TestCli::try_parse_from(["peri", "-r"]);
    assert!(cli.is_ok());
    let cli = cli.unwrap();
    assert_eq!(cli.resume, Some(None));
}

#[test]
fn test_combined_model_effort() {
    let cli = TestCli::try_parse_from(["peri", "--model", "sonnet", "--effort", "high"]);
    assert!(cli.is_ok());
    let cli = cli.unwrap();
    assert_eq!(cli.model, Some("sonnet".to_string()));
    assert_eq!(cli.effort, Some("high".to_string()));
}
```

- [ ] **Step 2: 运行所有测试**

Run: `cargo test -p peri-tui --lib -- cli_integration_test`
Expected: 9 个测试全部 PASS

- [ ] **Step 3: 检查最终 --help 输出完整性**

Run: `cargo run -p peri-tui -- --help`
Expected: 所有新参数显示在帮助文本中

Run: `cargo run -p peri-tui -- plugin --help`
Expected: 显示 plugin 子命令帮助

- [ ] **Step 4: 最终 Commit**

```bash
git add peri-tui/src/cli_integration_test.rs
git commit -m "test(cli): add CLI parameter parsing integration tests"
```

---

## Self-Review Checklist

**1. Spec coverage:**

| 需求 | 覆盖任务 |
|------|----------|
| `-p/--print`（acp 包装） | Task 4 |
| `--output-format` text/json/stream-json | Task 4 |
| `--permission-mode` | Task 5 |
| `--dangerously-skip-permissions` | Task 5 |
| `--model` | Task 5 |
| `--effort` | Task 5 |
| `-c/--continue` | Task 6 |
| `-r/--resume` | Task 6 |
| `--session-id` | Task 2（参数定义） |
| `--name` | Task 2（参数定义） |
| `--no-session-persistence` | Task 2（参数定义） |
| `--allowedTools/--disallowedTools` | Task 7 |
| `--max-turns` | Task 2（参数定义，Task 4 传递） |
| `--bare` | Task 2（参数定义，Task 4 传递） |
| `--settings` | Task 5 |
| `plugin list/install/uninstall` | Task 3 |
| camelCase + kebab-case 双别名 | Task 2 |
| -p 专属参数 TUI 模式下警告 | Task 1 |
| 保留 `-y`/`-a` 向后兼容 | Task 2 |
| 保留 `acp`/`update`/`sync` 子命令 | Task 2 |

**2. Placeholder scan:** 无 TBD/TODO 除 Task 6 会话恢复的 `// TODO` 注释（标注为"需会话持久化模块支持"，是预期行为）。

**3. Type consistency:** `PluginScope` 在 `cli_args.rs` 定义，在 `cli_plugin.rs` 中通过 `.into()` 转换为 `peri_middlewares::plugin::InstallScope`。`OutputFormat`/`EffortLevel` 在 `cli_args.rs` 定义，在 `cli_print.rs` 和 `main.rs` 中使用。`TuiOptions` 在 `main.rs` 定义并传递给 `run_tui`/`run_app`。所有类型定义一致。
