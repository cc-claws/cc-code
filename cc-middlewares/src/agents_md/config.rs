//! 指引文件加载配置。
//!
//! 对齐 dsh（deepseek-harness）加载模型：同目录候选**全部加载并合并**（内容去重），
//! 跨目录从项目根逐级向下拼接到 cwd。

use std::path::{Path, PathBuf};

/// 单文件 UTF-8 字节上限默认值（1 MiB）。
pub const DEFAULT_MAX_SOURCE_BYTES: usize = 1024 * 1024;
/// 渲染后总量上限默认值（256 KiB）。
pub const DEFAULT_MAX_BYTES: usize = 256 * 1024;
/// 用户全局指引文件名。
pub const USER_GLOBAL_FILE_NAME: &str = "AGENTS.md";

/// `@import` 允许读取的范围。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ImportScope {
    /// **默认（安全）**：只允许 import 指引文件**所属项目根**内的文件；
    /// 该文件不在任何项目根下时（如 `~/.cc-code/AGENTS.md`），范围收窄为它**所在目录**。
    ///
    /// 目的是阻断 `<!-- @import ../../../../etc/passwd -->` 这类路径穿越：`git clone` 一个
    /// 恶意仓库后，它的 `AGENTS.md` 就能把项目外文件内容带进 System 消息，且不经过 HITL。
    #[default]
    ProjectRoot,
    /// 不限制范围（旧行为）：相对路径可向上穿越、绝对路径直接生效。
    /// 需要跨仓库共享规则文件时才用。
    Unrestricted,
}

/// 指引文件（`AGENTS.md` / `CLAUDE.md` 及变体）的发现与限额配置。
#[derive(Debug, Clone)]
pub struct AgentsMdConfig {
    /// 项目根标记（默认 `[".git"]`）：从 cwd 向上找到第一个含标记的目录即项目根。
    pub project_root_markers: Vec<String>,
    /// 同目录基础候选（有序，默认 `["AGENTS.md", "CLAUDE.md", ".claude/AGENTS.md"]`）。
    /// 第三项是 cc-code 历史位置，保留兼容（dsh 无此项）。
    pub instruction_file_candidates: Vec<String>,
    /// 同目录本地覆盖候选（有序，排在基础层之后，通常被 gitignore）。
    pub local_instruction_file_candidates: Vec<String>,
    /// 单文件读取及导入展开的 UTF-8 字节上限；超限有界读取头尾并加标记。
    pub max_source_bytes: usize,
    /// 渲染后总量上限；超限停止追加后续文件并加标记。
    pub max_bytes: usize,
    /// 排除 glob（匹配路径字符串）。**在发现阶段早期生效**（去重之前），
    /// 因此被排除的候选不会占用去重槽位、也不会挤掉同内容但未排除的候选。
    pub excludes: Vec<String>,
    /// `@import` 允许读取的范围（默认 [`ImportScope::ProjectRoot`]）。
    pub import_scope: ImportScope,
    /// 用户全局指引文件（链首，最宽）。默认 `{APP_HOME}/AGENTS.md`（`~/.cc-code/AGENTS.md`）。
    pub user_global_file: PathBuf,
}

impl Default for AgentsMdConfig {
    fn default() -> Self {
        Self {
            project_root_markers: vec![".git".to_string()],
            instruction_file_candidates: vec![
                "AGENTS.md".to_string(),
                "CLAUDE.md".to_string(),
                ".claude/AGENTS.md".to_string(),
            ],
            local_instruction_file_candidates: vec![
                "AGENTS.local.md".to_string(),
                "CLAUDE.local.md".to_string(),
            ],
            max_source_bytes: DEFAULT_MAX_SOURCE_BYTES,
            max_bytes: DEFAULT_MAX_BYTES,
            excludes: Vec::new(),
            import_scope: ImportScope::default(),
            user_global_file: default_user_global_file(),
        }
    }
}

impl AgentsMdConfig {
    /// 同目录候选全序（基础层 → 本地覆盖层）。
    pub(crate) fn all_candidates(&self) -> impl Iterator<Item = &str> {
        self.instruction_file_candidates
            .iter()
            .chain(self.local_instruction_file_candidates.iter())
            .map(String::as_str)
    }

    /// 用户全局文件的 provenance 展示文本（形如 `~/.cc-code/AGENTS.md`；不在 home 下则用绝对路径）。
    pub(crate) fn user_global_display(&self) -> String {
        display_with_home(&self.user_global_file)
    }
}

fn default_user_global_file() -> PathBuf {
    cc_agent::app_home::app_home_dir().join(USER_GLOBAL_FILE_NAME)
}

/// 把路径渲染为 `/` 分隔的展示串。
///
/// Windows 上 `\` 会让 provenance 头与 `~/...` 展示变味（也让跨平台断言无法复用），
/// 故统一归一为 `/`。
pub(crate) fn slash_display(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// 把绝对路径渲染为 `~/...` 形式（不在 home 下则原样返回）。
pub(crate) fn display_with_home(path: &Path) -> String {
    if let Some(home) = dirs_next::home_dir() {
        if let Ok(rel) = path.strip_prefix(&home) {
            let rel = slash_display(rel);
            if rel.is_empty() {
                return "~".to_string();
            }
            return format!("~/{rel}");
        }
    }
    slash_display(path)
}
