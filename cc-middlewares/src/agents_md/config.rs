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
    /// 单文件 UTF-8 字节上限；超限按「头 70% + 尾 20%」截断并加标记。
    pub max_source_bytes: usize,
    /// 渲染后总量上限；超限停止追加后续文件并加标记。
    pub max_bytes: usize,
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

/// 把绝对路径渲染为 `~/...` 形式（不在 home 下则原样返回）。
pub(crate) fn display_with_home(path: &Path) -> String {
    if let Some(home) = dirs_next::home_dir() {
        if let Ok(rel) = path.strip_prefix(&home) {
            let rel = rel.to_string_lossy().replace('\\', "/");
            if rel.is_empty() {
                return "~".to_string();
            }
            return format!("~/{rel}");
        }
    }
    path.display().to_string()
}
