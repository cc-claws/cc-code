//! Core Tools 白名单定义与延迟加载判定逻辑

use std::collections::HashSet;
use std::sync::LazyLock;

/// 核心工具白名单（始终发送给 LLM，共 12 个）
///
/// - 文件操作 (6): Read, Write, Edit, Glob, Grep, folder_operations
/// - 执行 (1): Bash
/// - Web (2): WebFetch, WebSearch
/// - 交互 (2): Agent, AskUserQuestion
/// - 管理 (1): TodoWrite
pub static CORE_TOOLS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    [
        // 文件操作
        "Read",
        "Write",
        "Edit",
        "Glob",
        "Grep",
        "folder_operations",
        // 执行
        "Bash",
        // Web
        "WebFetch",
        "WebSearch",
        // 交互
        "Agent",
        "AskUserQuestion",
        // 管理
        "TodoWrite",
    ]
    .into_iter()
    .collect()
});

/// 元工具集合（Tool Search 延迟加载机制的工具，始终发送给 LLM）
pub static META_TOOLS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    ["SearchExtraTools", "ExecuteExtraTool"]
        .into_iter()
        .collect()
});

/// 判定工具是否为延迟加载工具（Deferred Tool）
///
/// 返回 `true` 表示该工具应从 LLM 可见工具列表中移除，
/// 通过 SearchExtraTools 按需发现，ExecuteExtraTool 代理执行。
///
/// # Examples
///
/// ```ignore
/// assert_eq!(is_deferred_tool("Read"), false);           // Core Tool
/// assert_eq!(is_deferred_tool("SearchExtraTools"), false); // Meta Tool
/// assert_eq!(is_deferred_tool("CronRegister"), true);     // Deferred Tool
/// assert_eq!(is_deferred_tool("mcp__slack__send_message"), true); // MCP Tool
/// ```
pub fn is_deferred_tool(tool_name: &str) -> bool {
    !CORE_TOOLS.contains(tool_name) && !META_TOOLS.contains(tool_name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_core_tool_not_deferred() {
        assert!(!is_deferred_tool("Read"));
    }

    #[test]
    fn test_meta_tool_not_deferred() {
        assert!(!is_deferred_tool("SearchExtraTools"));
        assert!(!is_deferred_tool("ExecuteExtraTool"));
    }

    #[test]
    fn test_deferred_tool() {
        assert!(is_deferred_tool("CronRegister"));
        assert!(is_deferred_tool("CronList"));
        assert!(is_deferred_tool("CronRemove"));
    }

    #[test]
    fn test_mcp_tool_deferred() {
        assert!(is_deferred_tool("mcp__slack__send_message"));
        assert!(is_deferred_tool("mcp__read_resource"));
    }

    #[test]
    fn test_unknown_tool_deferred() {
        assert!(is_deferred_tool("UnknownTool"));
        assert!(is_deferred_tool(""));
    }
}
