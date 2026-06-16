use anyhow::Result;

use clap::{Parser, Subcommand};
use ratatui::{
    crossterm::{
        event::{
            DisableBracketedPaste, DisableFocusChange, EnableBracketedPaste, EnableFocusChange,
        },
        execute,
        terminal::{disable_raw_mode, enable_raw_mode, size as terminal_size},
    },
    prelude::*,
    text::Text,
    widgets::{Paragraph, Widget, Wrap},
    TerminalOptions, Viewport,
};
use std::io;
use std::time::{Duration, Instant};

use peri_acp::transport::mpsc::mpsc_transport_pair;
use peri_tui::{
    acp_client::AcpTuiClient,
    acp_server::{run_acp_server, AcpServerConfig},
    app::App,
    event, ui,
};
use std::sync::Arc;

#[cfg(not(target_os = "windows"))]
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

mod acp_stdio;
mod cli_args;
mod cli_plugin;
mod cli_print;

// ─── CLI 定义 ──────────────────────────────────────────────────────────────

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
    #[arg(long = "output-format", visible_alias = "outputFormat")]
    output_format: Option<String>,
    /// 最大 agentic 轮数（需 -p）
    #[arg(long = "max-turns", visible_alias = "maxTurns")]
    max_turns: Option<u32>,
    /// 极简模式：跳过 hooks/LSP/插件等初始化（需 -p）
    #[arg(long = "bare")]
    bare: bool,

    // ── 权限与安全 ──
    /// 权限模式：bypass / default / dont-ask / accept-edit / auto-mode
    #[arg(long = "permission-mode", visible_alias = "permissionMode")]
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
    #[arg(long = "session-id", visible_alias = "sessionId")]
    session_id: Option<String>,
    /// 设置会话显示名称
    #[arg(short = 'n', long = "name")]
    session_name: Option<String>,
    /// 禁用会话持久化（需 -p）
    #[arg(long = "no-session-persistence")]
    no_session_persistence: bool,

    // ── 工具控制 ──
    /// 允许的工具列表（如 "Bash(git:*)" "Edit"）
    #[arg(long = "allowedTools", visible_alias = "allowed-tools")]
    allowed_tools: Option<Vec<String>>,
    /// 禁止的工具列表
    #[arg(long = "disallowedTools", visible_alias = "disallowed-tools")]
    disallowed_tools: Option<Vec<String>>,

    // ── 配置 ──
    /// 加载额外 settings 文件或 JSON 字符串
    #[arg(long = "settings")]
    settings: Option<String>,

    #[command(subcommand)]
    command: Option<Commands>,
}

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
enum SyncAction {
    /// 发送本地配置到远端设备
    Sender,
    /// 从远端设备接收配置
    Receiver,
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

// ─── 环境变量注入 ──────────────────────────────────────────────────────────

/// 从 settings.json 读取 env 字段并注入进程环境变量
/// 仅在进程环境变量不存在时设置（进程环境优先）
fn inject_env_from_settings() {
    let path = dirs_next::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(".peri")
        .join("settings.json");

    if !path.exists() {
        return;
    }

    // 读取并解析 JSON
    let Ok(content) = std::fs::read_to_string(&path) else {
        return;
    };

    // 提取 config.env 字段
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) else {
        return;
    };

    let Some(env_obj) = json.get("config").and_then(|c| c.get("env")) else {
        return;
    };

    let Some(env_map) = env_obj.as_object() else {
        return;
    };

    // 遍历键值对，仅在进程环境变量不存在时设置
    for (key, value) in env_map {
        if let Some(value_str) = value.as_str() {
            if std::env::var(key).is_err() {
                std::env::set_var(key, value_str);
            }
        }
    }
}

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

// ─── 入口 ──────────────────────────────────────────────────────────────────

fn main() -> Result<()> {
    // Set jemalloc MALLOC_CONF env vars BEFORE any allocation.
    // Must be the very first line — jemalloc reads these during init.
    peri_tui::alloc_config::init_alloc_conf();

    // 最先注入环境变量（进程环境变量优先）
    inject_env_from_settings();

    let cli = Cli::parse();

    // -p/--print 模式（优先级高于子命令）
    if cli.print.is_some() {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(4) // 限制 worker 数（默认=CPU 核数，18 核=72MB 栈空间浪费）
            .thread_stack_size(4 * 1024 * 1024) // 4 MB (default: 8 MB)
            .enable_all()
            .build()?;
        return rt.block_on(cli_print::run_print(
            cli.print.and_then(|o| o),
            cli.output_format,
            cli.max_turns,
            cli.bare,
            cli.model,
            cli.effort,
            cli.permission_mode,
            cli.skip_permissions,
            cli.allowed_tools.unwrap_or_default(),
            cli.disallowed_tools.unwrap_or_default(),
            cli.settings,
            None,
        ));
    }

    match cli.command {
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
        Some(Commands::Acp {
            cwd,
            model: _,
            agent: _,
        }) => {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(4) // 限制 worker 数（默认=CPU 核数，18 核=72MB 栈空间浪费）
                .thread_stack_size(4 * 1024 * 1024) // 4 MB (default: 8 MB)
                .enable_all()
                .build()?;
            rt.block_on(acp_stdio::run_acp_stdio(cwd))
        }
        Some(Commands::Update) => {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(4) // 限制 worker 数（默认=CPU 核数，18 核=72MB 栈空间浪费）
                .thread_stack_size(4 * 1024 * 1024) // 4 MB (default: 8 MB)
                .enable_all()
                .build()?;
            rt.block_on(async {
                match peri_tui::update::run_update().await {
                    Ok(tag) => println!("Updated to {tag}"),
                    Err(e) => {
                        eprintln!("Update failed: {e:#}");
                        std::process::exit(1);
                    }
                }
                Ok(())
            })
        }
        Some(Commands::Sync { action, server }) => {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(4) // 限制 worker 数（默认=CPU 核数，18 核=72MB 栈空间浪费）
                .thread_stack_size(4 * 1024 * 1024) // 4 MB (default: 8 MB)
                .enable_all()
                .build()?;
            rt.block_on(async {
                match action {
                    SyncAction::Sender => peri_tui::sync::run_sync_sender(&server).await,
                    SyncAction::Receiver => peri_tui::sync::run_sync_receiver(&server).await,
                }
            })
            .map(|_| println!("Sync complete"))
            .map_err(|e| {
                eprintln!("Sync failed: {e:#}");
                std::process::exit(1);
            })
        }
        Some(Commands::Plugin { action }) => {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(4) // 限制 worker 数（默认=CPU 核数，18 核=72MB 栈空间浪费）
                .thread_stack_size(4 * 1024 * 1024) // 4 MB (default: 8 MB)
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
    }
}

// ─── TUI 模式 ──────────────────────────────────────────────────────────────

/// TUI 模式启动选项
#[allow(dead_code)] // 部分 CLI 桥接字段尚未接入
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

fn run_tui(opts: TuiOptions) -> Result<()> {
    // Windows: 拦截 Ctrl+C / Ctrl+Break，防止进程被 OS 直接终止。
    // crossterm raw mode 在部分 Windows 终端下无法完全拦截 CTRL_C_EVENT，
    // 需要显式注册 handler 返回 TRUE，让 Ctrl+C 作为 KeyEvent 传递给应用层。
    #[cfg(target_os = "windows")]
    {
        unsafe extern "system" fn ctrl_handler(ctrl_type: u32) -> i32 {
            if ctrl_type == windows_sys::Win32::System::Console::CTRL_C_EVENT
                || ctrl_type == windows_sys::Win32::System::Console::CTRL_BREAK_EVENT
            {
                return 1;
            }
            0
        }
        unsafe {
            windows_sys::Win32::System::Console::SetConsoleCtrlHandler(Some(ctrl_handler), 1);
        }
    }

    // --settings 覆盖
    if let Some(ref settings_path) = opts.settings {
        inject_settings_override(settings_path);
    }

    if opts.approve {
        std::env::set_var("YOLO_MODE", "false");
    }

    if opts.skip_permissions {
        std::env::set_var("YOLO_MODE", "true");
    }

    // 在创建 tokio runtime 之前初始化 tracing，确保 reqwest::blocking::Client
    // 的内部 runtime 与应用 runtime 完全隔离，避免嵌套 runtime drop panic。
    let _telemetry = peri_agent::telemetry::init_tracing("agent-tui");

    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4) // 限制 worker 数（默认=CPU 核数，18 核=72MB 栈空间浪费）
        .thread_stack_size(4 * 1024 * 1024) // 4 MB (default: 8 MB)
        .enable_all()
        .build()?;

    let result = rt.block_on(async {
        // 初始化终端
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnableBracketedPaste, EnableFocusChange)?;
        let backend = CrosstermBackend::new(stdout);
        let (_, terminal_rows) = terminal_size()?;
        let mut terminal = Terminal::with_options(
            backend,
            TerminalOptions {
                viewport: Viewport::Inline(inline_viewport_height(terminal_rows)),
            },
        )?;

        // 运行应用
        let result = run_app(&mut terminal, &opts).await;

        // 恢复终端（不用 ? —— 恢复失败不应阻止 session ID 打印）
        if let Err(e) = disable_raw_mode() {
            tracing::warn!(error = %e, "disable_raw_mode 失败");
        }
        if let Err(e) = execute!(
            terminal.backend_mut(),
            DisableBracketedPaste,
            DisableFocusChange
        ) {
            tracing::warn!(error = %e, "DisableBracketedPaste/FocusChange 失败");
        }
        if let Err(e) = terminal.show_cursor() {
            tracing::warn!(error = %e, "show_cursor 失败");
        }

        // 退出时显示 session ID，方便用户恢复会话
        match &result {
            Ok(Some(msg)) => {
                tracing::info!(session_msg = %msg, "退出: 打印 session ID");
                eprintln!("{}", msg);
            }
            Ok(None) => {
                tracing::warn!("退出: run_app 返回 Ok(None)，无 session ID 可打印");
                eprintln!("\n  (no session ID available)");
            }
            Err(e) => {
                tracing::error!(error = %e, "退出: run_app 返回错误");
            }
        }

        result.map(|_| ())
    });

    // 先 drop rt（关闭所有 tokio 任务），再 drop _telemetry
    drop(rt);
    drop(_telemetry);

    if let Err(e) = result {
        eprintln!("Error: {e}");
    }

    Ok(())
}

fn inline_viewport_height(terminal_rows: u16) -> u16 {
    terminal_rows.saturating_sub(1).max(1)
}

async fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    tui_opts: &TuiOptions,
) -> Result<Option<String>> {
    let mut app = App::new().await;

    // 根据环境变量/CLI 参数设置初始权限模式
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
            if let Some(new_provider) =
                peri_tui::app::agent::LlmProvider::from_config_for_alias(config, model_str)
            {
                tracing::info!(model = %new_provider.model_name(), "CLI --model 覆盖生效");
            }
        }
    }

    // 会话恢复：-c 恢复当前目录最近会话，-r <id> 恢复指定会话
    if let Some(ref session_id) = tui_opts.resume_session {
        tracing::info!(session_id = %session_id, "-r: 恢复指定会话");
        app.open_thread(session_id.clone());
    } else if tui_opts.continue_session {
        let store = app.services.thread_store.clone();
        let cwd = app.services.cwd.clone();
        let thread_id = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let threads = store.list_threads().await.ok()?;
                threads.into_iter().find(|t| t.cwd == cwd).map(|t| t.id)
            })
        });
        if let Some(tid) = thread_id {
            tracing::info!(thread_id = %tid, "-c: 恢复最近会话");
            app.open_thread(tid);
        } else {
            tracing::info!("-c: 当前目录无历史会话，创建新会话");
        }
    }

    // 检测是否需要 Setup 向导
    if let Some(ref cfg) = app.services.peri_config {
        if peri_tui::app::setup_wizard::needs_setup(&cfg.config) {
            app.global_ui.setup_wizard = Some(peri_tui::app::SetupWizardPanel::new());
        }
    } else {
        // 无配置文件 → 必然需要 setup
        app.global_ui.setup_wizard = Some(peri_tui::app::SetupWizardPanel::new());
    }

    // 后台初始化 MCP 连接池（不阻塞 UI）
    app.spawn_mcp_init();

    // 加载已启用插件数据
    {
        let claude_dir = dirs_next::home_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join(".claude");
        app.services.plugin_data = Some(peri_middlewares::plugin::load_enabled_plugins_aggregated(
            &claude_dir,
        ));
        // 将插件命令注册到所有 session 的 CommandRegistry
        let plugin_commands = app
            .services
            .plugin_data
            .as_ref()
            .map(|pd| pd.all_commands.clone())
            .unwrap_or_default();
        // 将插件 skills 追加到所有 session 的 skill 列表
        let plugin_skill_dirs = app
            .services
            .plugin_data
            .as_ref()
            .map(|pd| pd.all_skill_dirs.clone())
            .unwrap_or_default();
        let plugin_skills = peri_middlewares::skills::list_skills(&plugin_skill_dirs);
        app.session_mgr
            .current_mut()
            .commands
            .command_registry
            .register_plugin_commands(plugin_commands.clone());
        let session = app.session_mgr.current_mut();
        let existing_names: std::collections::HashSet<String> = session
            .commands
            .skills
            .iter()
            .map(|s| s.name.clone())
            .collect();
        for skill in &plugin_skills {
            if !existing_names.contains(&skill.name) {
                session.commands.skills.push(skill.clone());
            }
        }
    }

    // ── Step 6-a: Setup ACP Server + Client ──────────────────────────────
    {
        let provider = app
            .services
            .peri_config
            .as_ref()
            .and_then(peri_tui::app::LlmProvider::from_config)
            .or_else(peri_tui::app::LlmProvider::from_env);

        if let Some(provider) = provider {
            // Gather plugin configs
            let plugin_skill_dirs = app
                .services
                .plugin_data
                .as_ref()
                .map(|pd| pd.all_skill_dirs.clone())
                .unwrap_or_default();
            let plugin_agent_dirs = app
                .services
                .plugin_data
                .as_ref()
                .map(|pd| pd.all_agent_dirs.clone())
                .unwrap_or_default();
            let plugin_lsp_servers = app
                .services
                .plugin_data
                .as_ref()
                .map(|pd| pd.all_lsp_servers.clone())
                .unwrap_or_default();
            let plugin_hooks = app
                .services
                .plugin_data
                .as_ref()
                .map(|pd| pd.all_hooks.clone())
                .unwrap_or_default();

            // Build hook groups from plugin hooks + global hooks + local hooks
            let mut hook_groups: Vec<Vec<peri_middlewares::hooks::RegisteredHook>> = Vec::new();
            if !plugin_hooks.is_empty() {
                hook_groups.push(plugin_hooks);
            }
            let global_hooks = peri_middlewares::hooks::loader::load_global_settings_hooks();
            if !global_hooks.is_empty() {
                hook_groups.push(global_hooks);
            }
            let local_hooks =
                peri_middlewares::hooks::loader::load_settings_local_hooks(&app.services.cwd);
            if !local_hooks.is_empty() {
                hook_groups.push(local_hooks);
            }

            let flat_hooks: Vec<peri_middlewares::hooks::RegisteredHook> =
                hook_groups.iter().flatten().cloned().collect();

            // Create session-level tool_search_index and shared_tools
            let tool_search_index = Arc::new(peri_middlewares::tool_search::ToolSearchIndex::new());
            let shared_tools = Arc::new(parking_lot::RwLock::new(std::collections::HashMap::new()));

            let server_config = AcpServerConfig {
                provider: Arc::new(parking_lot::RwLock::new(provider.clone())),
                peri_config: Arc::new(parking_lot::RwLock::new(
                    app.services.peri_config.clone().unwrap_or_default(),
                )),
                permission_mode: app.services.permission_mode.clone(),
                cron_scheduler: Some(app.services.cron.scheduler.clone()),
                mcp_pool: app.services.mcp_pool.clone(),
                channel_state: app.services.channel_state.clone(),
                plugin_skill_dirs,
                plugin_agent_dirs,
                plugin_hooks: flat_hooks,
                hook_groups,
                plugin_lsp_servers,
                tool_search_index: tool_search_index.clone(),
                shared_tools: shared_tools.clone(),
                thread_store: app.services.thread_store.clone(),
                langfuse_session: {
                    if let Some(config) = peri_acp::langfuse::LangfuseConfig::from_env() {
                        tracing::info!("Langfuse tracing enabled (TUI mode)");
                        peri_acp::langfuse::LangfuseSession::new(config)
                            .await
                            .map(Arc::new)
                    } else {
                        None
                    }
                },
                config_path: peri_tui::config::config_path(),
            };

            let (client_transport, server_transport) = mpsc_transport_pair();
            tokio::spawn(async move {
                run_acp_server(Arc::new(server_transport), server_config).await;
            });

            let (acp_client, notification_rx) = AcpTuiClient::new(client_transport);
            // Spawn notification pump
            acp_client.spawn_pump();
            // Wire notification receiver to active session's AgentComm
            app.session_mgr.current_mut().agent.acp_notification_rx = Some(notification_rx);
            app.acp_client = Some(acp_client);
        }
    }

    // Spinner tick 驱动：每次渲染前推进一帧
    app.session_mgr.current_mut().spinner_state.advance_tick();

    // 初始全量绘制一次
    if let Err(e) = draw_app(terminal, &mut app) {
        tracing::error!(error = %e, "初始绘制失败");
    }
    let mut last_render = Instant::now();

    /// loading 动画帧率限制间隔（约 30 FPS）。
    /// 仅在 loading=true 且无用户事件的 poll 超时路径生效，
    /// 用户交互（键盘/鼠标/resize）始终立即渲染。
    const TARGET_FRAME_INTERVAL: Duration = Duration::from_millis(33);

    'event_loop: loop {
        // 推进 Spinner 动画帧
        app.session_mgr.current_mut().spinner_state.advance_tick();
        // 推进光标闪烁（返回 true 表示可见性切换，需要重绘）
        let cursor_blinked = app.session_mgr.current_mut().ui.advance_cursor_tick();
        // 轮询 agent 结果
        let mut agent_updated = false;
        agent_updated |= app.poll_agent();
        agent_updated |= app.poll_at_mention();
        // 轮询后台事件（MCP OAuth 等）
        let bg_updated = app.poll_background_events();
        // 检查 cron 定时触发
        app.poll_cron_triggers();

        // NOTE: 不对 next_event/draw_app 使用 ?，避免 terminal 错误导致跳过 session ID 打印
        let next = match event::next_event(&mut app).await {
            Ok(ev) => ev,
            Err(e) => {
                tracing::error!(error = %e, "event loop: next_event 错误，退出循环");
                break 'event_loop;
            }
        };
        match next {
            Some(action) => match action {
                event::Action::Quit => break 'event_loop,
                event::Action::Submit(input) => {
                    app.submit_message(input);
                    if let Err(e) = draw_app(terminal, &mut app) {
                        tracing::error!(error = %e, "event loop: draw_app 错误");
                    }
                    last_render = Instant::now();
                }
                event::Action::RunShellCommand(command) => {
                    app.run_shell_command(command);
                    if let Err(e) = draw_app(terminal, &mut app) {
                        tracing::error!(error = %e, "event loop: draw_app 错误");
                    }
                    last_render = Instant::now();
                }
                event::Action::Redraw => {
                    // 有用户交互（键盘/鼠标/resize）→ 始终重绘
                    if let Err(e) = draw_app(terminal, &mut app) {
                        tracing::error!(error = %e, "event loop: draw_app 错误");
                    }
                    last_render = Instant::now();
                }
            },
            None => {
                // 无用户事件（poll 超时）：在阻塞结束后重新读取缓存版本
                // 这样能捕获渲染线程在等待期间发出的更新
                let cache_version = app
                    .session_mgr
                    .current_mut()
                    .messages
                    .render_cache
                    .read()
                    .version;
                let cache_updated =
                    cache_version != app.session_mgr.current_mut().messages.last_render_version;
                let loading = app.session_mgr.current_mut().ui.loading;
                let should_render = cache_updated || agent_updated || bg_updated || loading || cursor_blinked;
                if should_render {
                    let now = Instant::now();
                    // loading 路径：限制帧率到 TARGET_FRAME_INTERVAL，降低 CPU 开销
                    // 非 loading 路径（cache_updated/agent_updated/bg_updated）始终立即渲染
                    if !loading || now.duration_since(last_render) >= TARGET_FRAME_INTERVAL {
                        if let Err(e) = draw_app(terminal, &mut app) {
                            tracing::error!(error = %e, "event loop: draw_app 错误");
                        }
                        last_render = now;
                    }
                }
            }
        }
        // /exit 或 /quit 命令设置的退出标志
        if app.global_ui.quit_requested {
            break 'event_loop;
        }
    }

    // Fire SessionEnd hooks before shutdown
    {
        let mut hooks = app
            .services
            .plugin_data
            .as_ref()
            .map(|pd| pd.all_hooks.clone())
            .unwrap_or_default();
        hooks.extend(peri_middlewares::hooks::loader::load_global_settings_hooks());
        hooks.extend(peri_middlewares::hooks::loader::load_settings_local_hooks(
            &app.services.cwd,
        ));
        if !hooks.is_empty() {
            let cwd = app.services.cwd.clone();
            let provider_name = app.services.provider_name.clone();
            tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current().block_on(async {
                    peri_middlewares::hooks::middleware::fire_standalone_lifecycle_hooks(
                        &hooks,
                        peri_middlewares::hooks::types::HookEvent::SessionEnd,
                        &cwd,
                        "",
                        "",
                        &provider_name,
                        None,
                    )
                    .await;
                })
            });
        }
    }

    // 尽早读取 session ID（避免后续清理逻辑影响）
    // 优先从 current_thread_id 读取，兜底从 ACP client 读取
    let thread_id = app.session_mgr.current().current_thread_id.clone();
    let acp_sid = app.acp_client.as_ref().and_then(|c| c.session_id());
    tracing::info!(
        current_thread_id = ?thread_id,
        acp_session_id = ?acp_sid,
        "退出: session ID 读取"
    );
    let session_id = select_exit_session_id(
        thread_id,
        acp_sid,
        tui_opts.resume_session.clone(),
        tui_opts.session_id.clone(),
    );

    // 关闭 MCP 连接池（断开所有 MCP 服务器连接，清理子进程）
    if let Some(pool) = app.services.mcp_pool.take() {
        tracing::info!("正在关闭 MCP 连接池...");
        tokio::task::block_in_place(|| tokio::runtime::Handle::current().block_on(pool.shutdown()));
        tracing::info!("MCP 连接池已关闭");
    }

    // 等待最后一次 Langfuse flush 完成，防止 runtime drop 前 batcher 数据丢失
    if let Some(handle) = app
        .session_mgr
        .current_mut()
        .langfuse
        .langfuse_flush_handle
        .take()
    {
        let _ = handle.await;
    }

    // 返回 session ID 供退出时显示
    let exit_msg = if let Some(ref sid) = session_id {
        if !sid.is_empty() {
            let lc = &app.services.lc;
            Some(format!(
                "\n  Session ID: {}\n  {}\n",
                sid,
                lc.tr_args("app-exit-resume", &[("id".into(), sid.to_string().into())])
            ))
        } else {
            None
        }
    } else {
        None
    };
    Ok(exit_msg)
}

fn draw_app(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, app: &mut App) -> Result<()> {
    // [临时验证-控制变量] 禁用 scrollback flush（terminal.insert_before）。
    // 目的：若禁用后渲染残留消失 → 根因确认在 insert_before/inline viewport；
    //       若残留仍在 → 根因在 viewport_clip 等其他路径。验证后必须回退此注释。
    // flush_scrollback_history(terminal, app)?;
    terminal.draw(|f| ui::main_ui::render(f, app))?;
    Ok(())
}

#[allow(dead_code)]
fn flush_scrollback_history(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
) -> Result<()> {
    let session = app.session_mgr.current();
    if session.ui.loading {
        return Ok(());
    }
    let Some(messages_area) = session.ui.messages_area else {
        return Ok(());
    };
    if messages_area.width == 0 || messages_area.height == 0 {
        return Ok(());
    }

    let (end, height, lines) = {
        let messages = &session.messages;
        let cache = messages.render_cache.read();
        if cache.width != messages_area.width {
            return Ok(());
        }
        let start = messages.scrollback_committed_lines.min(cache.lines.len());
        let target_end =
            scrollback_commit_end(cache.total_lines, messages_area.height, &cache.wrap_map);
        let end = target_end.min(cache.lines.len());
        if start >= end {
            return Ok(());
        }

        let start_visual =
            committed_visual_start(start, cache.lines.len(), cache.total_lines, &cache.wrap_map);
        let mut capped_end = start;
        let mut capped_height = 0usize;
        for info in &cache.wrap_map[start..end] {
            let next_height = info.visual_row_end.saturating_sub(start_visual);
            if next_height > u16::MAX as usize {
                break;
            }
            capped_end = info.line_idx + 1;
            capped_height = next_height;
        }
        if capped_end <= start || capped_height == 0 {
            return Ok(());
        }

        (
            capped_end,
            capped_height as u16,
            cache.lines[start..capped_end].to_vec(),
        )
    };

    terminal.insert_before(height, move |buf| {
        Paragraph::new(Text::from(lines))
            .wrap(Wrap { trim: false })
            .render(buf.area, buf);
    })?;
    app.session_mgr
        .current_mut()
        .messages
        .scrollback_committed_lines = end;
    Ok(())
}

#[allow(dead_code)]
fn scrollback_commit_end(
    total_visual_rows: usize,
    retained_height: u16,
    wrap_map: &[peri_tui::ui::render_thread::WrappedLineInfo],
) -> usize {
    let retained_height = retained_height as usize;
    if retained_height == 0 || total_visual_rows <= retained_height {
        return 0;
    }
    let retain_from_visual = total_visual_rows - retained_height;
    wrap_map.partition_point(|info| info.visual_row_end <= retain_from_visual)
}

#[allow(dead_code)]
fn committed_visual_start(
    committed_lines: usize,
    line_count: usize,
    total_visual_rows: usize,
    wrap_map: &[peri_tui::ui::render_thread::WrappedLineInfo],
) -> usize {
    if committed_lines == 0 {
        return 0;
    }
    if committed_lines >= line_count {
        return total_visual_rows;
    }
    wrap_map
        .get(committed_lines)
        .map(|info| info.visual_row_start)
        .unwrap_or(total_visual_rows)
}

fn select_exit_session_id(
    thread_id: Option<String>,
    acp_session_id: Option<String>,
    resume_session: Option<String>,
    cli_session_id: Option<String>,
) -> Option<String> {
    thread_id
        .or(acp_session_id)
        .or(resume_session)
        .or(cli_session_id)
}

#[cfg(test)]
mod cli_integration_test;

#[cfg(test)]
mod tests {
    use super::*;
    use peri_tui::ui::render_thread::WrappedLineInfo;

    fn wrapped_line(line_idx: usize, start: usize, end: usize) -> WrappedLineInfo {
        WrappedLineInfo {
            line_idx,
            visual_row_start: start,
            visual_row_end: end,
            plain_text: String::new(),
            char_widths: Vec::new(),
        }
    }

    #[test]
    fn test_env_priority_process_over_settings() {
        // 测试进程环境变量优先于 settings.json
        // 设置一个测试环境变量
        std::env::set_var("TEST_ENV_PRIORITY_VAR", "from_process");

        // 调用注入函数（即使 settings.json 存在该变量也不应覆盖）
        inject_env_from_settings();

        // 验证进程环境变量未被覆盖
        assert_eq!(
            std::env::var("TEST_ENV_PRIORITY_VAR").unwrap(),
            "from_process"
        );

        // 清理
        std::env::remove_var("TEST_ENV_PRIORITY_VAR");
    }

    #[test]
    fn test_inline_viewport_height_leaves_native_scrollback_row() {
        assert_eq!(inline_viewport_height(24), 23);
        assert_eq!(inline_viewport_height(1), 1);
    }

    #[test]
    fn test_scrollback_commit_end_retains_last_viewport() {
        let wrap_map = (0..10)
            .map(|idx| wrapped_line(idx, idx, idx + 1))
            .collect::<Vec<_>>();
        let result = scrollback_commit_end(10, 4, &wrap_map);
        assert_eq!(result, 6);
    }

    #[test]
    fn test_scrollback_commit_end_does_not_commit_when_content_fits() {
        let wrap_map = (0..4)
            .map(|idx| wrapped_line(idx, idx, idx + 1))
            .collect::<Vec<_>>();
        let result = scrollback_commit_end(4, 4, &wrap_map);
        assert_eq!(result, 0);
    }

    #[test]
    fn test_scrollback_commit_end_respects_wrapped_visual_rows() {
        let wrap_map = vec![
            wrapped_line(0, 0, 1),
            wrapped_line(1, 1, 4),
            wrapped_line(2, 4, 5),
            wrapped_line(3, 5, 7),
        ];
        let result = scrollback_commit_end(7, 3, &wrap_map);
        assert_eq!(result, 2);
    }

    #[test]
    fn test_select_exit_session_id_prefers_live_thread_id() {
        let result = select_exit_session_id(
            Some("thread-live".to_string()),
            Some("acp".to_string()),
            Some("resume".to_string()),
            Some("cli".to_string()),
        );
        assert_eq!(result.as_deref(), Some("thread-live"));
    }

    #[test]
    fn test_select_exit_session_id_falls_back_to_resume_arg() {
        let result = select_exit_session_id(
            None,
            None,
            Some("resume-id".to_string()),
            Some("cli".to_string()),
        );
        assert_eq!(result.as_deref(), Some("resume-id"));
    }
}
// test
