//! # rust-agent-middlewares
//!
//! Rust middleware implementations aligned with `@langgraph-js/agent-middlewares` (TypeScript).
//!
//! ## 文件系统与终端（原 rust-agent-middlewares）

#![allow(
    clippy::type_complexity,
    clippy::empty_line_after_doc_comments,
    clippy::useless_conversion
)]
//! - [`middleware::FilesystemMiddleware`]：文件系统操作
//! - [`middleware::TerminalMiddleware`]：终端命令执行
//!
//! ## 认知增强与安全（原 rust-standard-middlewares）
//! - [`AgentsMdMiddleware`]：注入 AGENTS.md / CLAUDE.md 项目指引
//! - [`SkillsMiddleware`]：渐进式 Skills 摘要注入
//! - [`HumanInTheLoopMiddleware`]：敏感工具调用前需用户确认

pub mod agent_define;
pub mod agents_md;
pub mod claude_agent_parser;
pub mod subagent;
pub use claude_agent_parser::{
    format_agent_id, parse_agent_file, ClaudeAgent, ClaudeAgentFrontmatter, ToolsValue,
};
pub mod ask_user;
pub mod cron;
pub mod hitl;
pub mod hooks;
pub mod mcp;
pub mod middleware;
pub mod plugin;
pub use plugin::{
    AvailablePlugin, ClaudeSettings, CommandEntry, CommandProvider, CommandSource, InstallScope,
    InstalledPlugin, InstalledPlugins, KnownMarketplace, LoadedPlugin, LoaderError,
    MarketplaceEntry, MarketplaceError, MarketplaceManager, MarketplaceManifest, MarketplacePlugin,
    MarketplaceRefreshEvent, MarketplaceSource, PluginAgent, PluginAuthor, PluginChannel,
    PluginCommand, PluginCommandProvider, PluginConfigError, PluginLspServer, PluginManifest,
    PluginMiddleware, PluginOption,
};
pub mod skills;
pub mod tool_search;
pub mod tools;

pub use agent_define::{AgentDefineMiddleware, AgentOverrides};
pub use agents_md::AgentsMdMiddleware;
pub use ask_user::{
    ask_user_tool_definition, parse_ask_user, AskUserBatchRequest, AskUserOption,
    AskUserQuestionData,
};
pub use cron::{CronMiddleware, CronScheduler, CronTask, CronTrigger};
pub use hitl::{
    default_requires_approval, effective_tool_name, is_yolo_mode, AutoClassifier, BatchItem,
    Classification, HitlDecision, HumanInTheLoopMiddleware, LlmAutoClassifier, PermissionMode,
    SharedPermissionMode,
};
#[allow(deprecated)]
pub use middleware::PrependSystemMiddleware;
pub use skills::{
    list_skills, load_global_skills_dir, load_skill_metadata, SkillMetadata, SkillsMiddleware,
};
pub use subagent::{
    scan_agents, scan_agents_with_extra_dirs, BackgroundTask, BackgroundTaskRegistry,
    BackgroundTaskStatus, SkillPreloadMiddleware, SubAgentMiddleware, SubAgentTool,
};
pub use tool_search::{is_deferred_tool, ToolSearchMiddleware, CORE_TOOLS, META_TOOLS};
pub use tools::{ArcToolWrapper, AskUserTool, BoxToolWrapper};

/// Prelude - 常用类型一次性导入
pub mod prelude {
    pub use crate::agent_define::AgentDefineMiddleware;
    pub use crate::agents_md::AgentsMdMiddleware;
    pub use crate::ask_user::{
        ask_user_tool_definition, parse_ask_user, AskUserBatchRequest, AskUserOption,
        AskUserQuestionData,
    };
    pub use crate::cron::{CronMiddleware, CronScheduler, CronTask, CronTrigger};
    pub use crate::hitl::{
        default_requires_approval, is_yolo_mode, AutoClassifier, BatchItem, Classification,
        HitlDecision, HumanInTheLoopMiddleware, LlmAutoClassifier, PermissionMode,
        SharedPermissionMode,
    };
    pub use crate::hooks::{HookMiddleware, RegisteredHook};
    #[allow(deprecated)]
    pub use crate::middleware::PrependSystemMiddleware;
    pub use crate::middleware::{
        FilesystemMiddleware, TerminalMiddleware, TodoMiddleware, WebMiddleware,
    };
    pub use crate::plugin::{
        AvailablePlugin, ClaudeSettings, CommandEntry, CommandProvider, CommandSource,
        InstallScope, InstalledPlugin, InstalledPlugins, KnownMarketplace, LoadedPlugin,
        LoaderError, MarketplaceEntry, MarketplaceError, MarketplaceManager, MarketplaceManifest,
        MarketplacePlugin, MarketplaceRefreshEvent, MarketplaceSource, PluginAgent, PluginAuthor,
        PluginChannel, PluginCommand, PluginCommandProvider, PluginConfigError, PluginLspServer,
        PluginManifest, PluginMiddleware, PluginOption,
    };
    pub use crate::skills::{SkillMetadata, SkillsMiddleware};
    pub use crate::subagent::{SkillPreloadMiddleware, SubAgentMiddleware, SubAgentTool};
    pub use crate::tools::{
        ArcToolWrapper, AskUserTool, BoxToolWrapper, EditFileTool, FolderOperationsTool,
        GlobFilesTool, GrepTool, ReadFileTool, TodoItem, TodoStatus, TodoWriteTool, WriteFileTool,
    };
    pub use rust_create_agent::tools::ToolProvider;

    // 重导出 rust-create-agent 核心类型
    pub use rust_create_agent::prelude::*;
}
