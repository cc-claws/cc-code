use crate::ui::theme;
use ratatui::style::Color;

/// 只读工具分类，用于折叠聚合
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ToolCategory {
    /// 搜索类（Glob/Grep）
    Search,
    /// Glob 搜索
    Glob,
    /// Grep 搜索
    Grep,
    /// Read 读取
    Read,
    /// Write/Edit 写入
    Write,
    /// AskUserQuestion 提问
    AskUser,
}

impl ToolCategory {
    pub fn from_tool_name(name: &str) -> Option<Self> {
        match name {
            "Glob" => Some(Self::Glob),
            "Grep" => Some(Self::Search),
            "Read" => Some(Self::Read),
            "AskUserQuestion" => Some(Self::AskUser),
            _ => None,
        }
    }
}

/// 工具条目（聚合组内）
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ToolEntry {
    pub tool_name: String,
    pub display_name: String,
    pub args_display: Option<String>,
    pub content: String,
    pub is_error: bool,
}

/// SubAgent 批次聚合的摘要信息
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AgentSummary {
    pub agent_id: String,
    pub task_preview: String,
    /// 工具调用数
    pub tool_count: usize,
    /// 是否以错误结束
    pub is_error: bool,
    /// 最终结果（仅第一行）
    pub final_result: Option<String>,
}

/// 从 SubAgent 结果内容中解析工具调用次数。
///
/// 支持英文格式 `[Sub-agent executed N tool calls: ...]`
/// 或中文版 `[子 agent 执行了 N 个工具调用: ...]`。
/// 解析失败时返回 0（优雅降级）。
pub(crate) fn parse_subagent_tool_count(content: &str) -> usize {
    if let Some(rest) = content.strip_prefix("[Sub-agent executed ") {
        if let Some(n_str) = rest.split(' ').next() {
            if let Ok(n) = n_str.parse::<usize>() {
                return n;
            }
        }
    }
    if let Some(rest) = content
        .strip_prefix("[子 agent 执行了 ")
        .or_else(|| content.strip_prefix("[子agent 执行了 "))
    {
        if let Some(n_str) = rest.split(' ').next() {
            if let Ok(n) = n_str.parse::<usize>() {
                return n;
            }
        }
    }
    0
}

/// 按工具名分配颜色（按操作类型分色）
pub fn tool_color(name: &str) -> Color {
    match name {
        "Read" | "Glob" | "Grep" => theme::SAGE,
        "Write" | "Edit" | "folder_operations" | "delete_file" | "delete_folder" | "rm"
        | "rm_rf" => theme::WARNING,
        "Bash" => theme::BASH_BORDER,
        "Agent" | "AskUserQuestion" | "TodoWrite" => theme::THINKING,
        _ if name.contains("error") => theme::ERROR,
        _ => theme::MUTED,
    }
}
