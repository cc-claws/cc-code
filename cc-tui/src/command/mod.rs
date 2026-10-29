pub mod core;
pub mod panel;
pub mod session;

pub use panel::agents;

/// 注册所有内置命令，返回配置好的 CommandRegistry
pub fn default_registry() -> CommandRegistry {
    let mut r = CommandRegistry::new();
    r.register(Box::new(core::config::ConfigCommand));
    r.register(Box::new(core::clear::ClearCommand));
    r.register(Box::new(core::help::HelpCommand));
    r.register(Box::new(core::history::HistoryCommand));
    r.register(Box::new(core::resume::ResumeCommand));
    r.register(Box::new(core::doctor::DoctorCommand));
    r.register(Box::new(core::exit::ExitCommand));
    r.register(Box::new(core::gc::GcCommand));

    r.register(Box::new(panel::model::ModelCommand));
    r.register(Box::new(panel::theme::ThemeCommand));
    r.register(Box::new(panel::plugin::PluginCommand));
    r.register(Box::new(panel::mcp::McpCommand));
    r.register(Box::new(panel::hooks::HooksCommand));
    r.register(Box::new(panel::cron::CronCommand));
    r.register(Box::new(panel::ps::PsCommand));
    r.register(Box::new(panel::agents::AgentsCommand));
    r.register(Box::new(panel::agent_center::AgentCenterCommand));
    r.register(Box::new(panel::memory::MemoryCommand));
    r.register(Box::new(panel::login::LoginCommand));
    r.register(Box::new(session::rename::RenameCommand));
    r.register(Box::new(session::channel::ChannelCommand));
    r.register(Box::new(session::context_cmd::ContextCommand));
    r.register(Box::new(session::cost::CostCommand));
    r.register(Box::new(session::lang::LangCommand));
    r.register(Box::new(session::effort::EffortCommand));
    r.register(Box::new(session::loop_cmd::LoopCommand));
    r.register(Box::new(session::setup::SetupCommand));
    r.register(Box::new(session::init::InitCommand));
    r.register(Box::new(session::commit::CommitCommand));
    r.register(Box::new(session::review::ReviewCommand));
    r.register(Box::new(session::recap::RecapCommand));
    r.register(Box::new(session::rewind::RewindCommand));
    r.register(Box::new(session::streaming::StreamingCommand));
    r.register(Box::new(core::export_cmd::ExportCommand));
    r
}

use crate::app::App;

// ─── Command trait ────────────────────────────────────────────────────────────

pub trait Command: Send + Sync {
    /// 命令名，不含 /（如 "model"、"help"、"clear"）
    fn name(&self) -> &str;
    /// 单行描述，用于 /help 展示
    fn description(&self, lc: &crate::i18n::LcRegistry) -> String;
    /// 命令别名列表（不含 /），默认为空
    fn aliases(&self) -> Vec<&str> {
        vec![]
    }
    /// 该命令在 Agent 运行（`loading`）期间是否被阻塞。
    ///
    /// - 默认 `true`：命令需要 UI 处于稳定空闲态（开面板 / 弹选择器 / 改本地或全局状态 /
    ///   退出进程 / 向时间线追加通知）。运行中输入一律拒绝并保留输入，避免被误当成普通
    ///   消息发给模型。
    /// - `false`：命令只是把文本转发给 ACP / agent 执行，运行中可安全入队，本轮结束后再执行。
    ///
    /// 新增命令时若其 `execute` 仅调用 `App::submit_message`，应显式返回 `false`。
    fn blocked_while_busy(&self) -> bool {
        true
    }
    /// 该命令在 Agent 运行（`loading`）期间是否允许立即执行。
    ///
    /// 默认 `false`。纯只读状态查看类命令（如 `/ps` 查看后台任务与命令历史）
    /// 完全不影响 Agent 执行流程与上下文状态，可返回 `true` 允许在忙碌期间立即弹出。
    fn execute_while_busy(&self) -> bool {
        false
    }
    /// 执行命令，args 是命令名之后的参数字符串（已 trim）
    fn execute(&self, app: &mut App, args: &str);
}

// ─── CommandRegistry ──────────────────────────────────────────────────────────

#[derive(Default)]
pub struct CommandRegistry {
    commands: Vec<Box<dyn Command>>,
}

impl CommandRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, cmd: Box<dyn Command>) {
        self.commands.push(cmd);
    }

    /// 注册插件提供的命令（从 PluginLoadResult 获取）
    pub fn register_plugin_commands(
        &mut self,
        commands: Vec<cc_middlewares::plugin::CommandEntry>,
    ) {
        for entry in commands {
            self.register(Box::new(
                session::plugin_command::PluginCommandAdapter::new(entry),
            ));
        }
    }

    /// 解析命令名并返回命中的命令（精确名 → 别名精确 → 前缀唯一），**不执行**。
    ///
    /// 与 [`dispatch`](Self::dispatch) 共用同一份匹配规则：判定与执行分处两处时极易漂移，
    /// `dispatch` 已改为复用本函数。
    pub fn find(&self, name: &str) -> Option<&dyn Command> {
        // 1. 精确匹配
        if let Some(cmd) = self.commands.iter().find(|c| c.name() == name) {
            return Some(cmd.as_ref());
        }
        // 2. 别名精确匹配
        if let Some(cmd) = self.commands.iter().find(|c| c.aliases().contains(&name)) {
            return Some(cmd.as_ref());
        }
        // 3. 前缀唯一匹配（同时对 name 和 aliases）
        let matches: Vec<&Box<dyn Command>> = self
            .commands
            .iter()
            .filter(|c| {
                c.name().starts_with(name) || c.aliases().iter().any(|a| a.starts_with(name))
            })
            .collect();
        if matches.len() == 1 {
            return Some(matches[0].as_ref());
        }
        None
    }

    /// 前缀命中的命令名列表（同时匹配 name 与 aliases），用于歧义提示。
    /// 前缀为空时返回全部命令名。
    pub fn prefix_names(&self, prefix: &str) -> Vec<String> {
        self.commands
            .iter()
            .filter(|c| {
                c.name().starts_with(prefix) || c.aliases().iter().any(|a| a.starts_with(prefix))
            })
            .map(|c| c.name().to_string())
            .collect()
    }

    /// 解析并执行命令。
    /// 输入格式："/name args..."
    /// 匹配优先级：精确匹配 > 别名精确匹配 > 前缀唯一匹配（支持 /m → /model）
    /// 返回 true 表示找到命令并执行，false 表示未知命令或有歧义。
    pub fn dispatch(&self, app: &mut App, input: &str) -> bool {
        // dispatch 期间注册表会从 App 中移出，先用当前语言刷新 /help 的描述缓存。
        app.session_mgr.current_mut().commands.command_help_list = self.list(&app.services.lc);
        let input = input.trim_start_matches('/');
        let (name, args) = match input.split_once(' ') {
            Some((n, a)) => (n.trim(), a.trim()),
            None => (input.trim(), ""),
        };

        match self.find(name) {
            Some(cmd) => {
                cmd.execute(app, args);
                true
            }
            None => false,
        }
    }

    /// 返回所有已注册命令的 (name, description, aliases) 列表
    pub fn list(&self, lc: &crate::i18n::LcRegistry) -> Vec<(String, String, Vec<String>)> {
        self.commands
            .iter()
            .map(|c| {
                (
                    c.name().to_string(),
                    c.description(lc),
                    c.aliases().into_iter().map(String::from).collect(),
                )
            })
            .collect()
    }

    /// 按前缀匹配命令，返回匹配的 (name, description) 列表
    /// prefix 不含 /，如 "mo" 匹配 "model"
    /// 同时匹配 name 和 aliases
    pub fn match_prefix(
        &self,
        prefix: &str,
        lc: &crate::i18n::LcRegistry,
    ) -> Vec<(String, String)> {
        self.commands
            .iter()
            .filter(|c| {
                c.name().starts_with(prefix) || c.aliases().iter().any(|a| a.starts_with(prefix))
            })
            .map(|c| (c.name().to_string(), c.description(lc)))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };
    include!("mod_test.rs");
}
