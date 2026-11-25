//! Fork semantics: tool filtering, fork directive construction, agent override extraction.
//!
//! Pure computation functions for sub-agent inheritance from parent agent.
//! No async, no external state mutation — safe for unit testing without mocks.

use std::sync::Arc;

use cc_agent::kernel::canonical_tool_name;
use cc_agent::tools::BaseTool;

use crate::tool_search::core_tools::TOOL_AGENT;
use crate::{agent_define::AgentOverrides, claude_agent_parser::ToolsValue, tools::ArcToolWrapper};

/// 把工具名归一到规范名再比较：subagent 的 `disallowedTools` 可能用任一内核的
/// 方言名（如 pi 的 `edit`/`write`），不归一会让只读 agent 拿到写权限。
fn canonical_lower(name: &str) -> String {
    canonical_tool_name(name).unwrap_or(name).to_lowercase()
}

/// Pattern 命中判定：支持整表 `*`、精确名、以及尾部 `*` 的前缀通配。
///
/// 前缀通配是为 `mcp__server__*` 这类「按 server 批量授权」准备的：MCP 工具名不由
/// 任一内核管辖（`should_hide` 对它们一律放行），只能靠白名单前缀表达范围。
/// `mcp__srv__*` 去掉尾部 `*` 后是 `mcp__srv__`，因此不会命中裸名 `mcp__srv`。
pub(crate) fn pattern_matches(pattern: &str, name_key: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    match pattern.strip_suffix('*') {
        Some(prefix) => name_key.starts_with(&canonical_lower(prefix)),
        None => canonical_lower(pattern) == name_key,
    }
}

/// 白名单整体判定：空列表 = 不限；否则任一条 pattern 命中即可。
pub(crate) fn allowed_matches(allowed: &[String], name_key: &str) -> bool {
    allowed.is_empty() || allowed.iter().any(|p| pattern_matches(p, name_key))
}

/// 禁用列表判定：任一条 pattern 命中即禁用。
pub(crate) fn disallowed_matches(disallowed: &[String], name_key: &str) -> bool {
    disallowed.iter().any(|p| pattern_matches(p, name_key))
}

/// Filter tools from parent set based on agent definition's tools/disallowedTools fields.
///
/// Rules:
/// - `tools` is `Empty` -> inherit all parent tools (but always exclude `Agent` itself to prevent recursion)
/// - `tools` has value -> only keep tools in the list (also exclude `Agent`)
/// - then remove tools listed in `disallowed_tools` from the result
///
/// Matching is case-insensitive (users often write PascalCase in agent.md), and
/// a trailing `*` grants prefix wildcard matching (e.g. `mcp__server__*`).
pub fn filter_tools(
    parent_tools: &[Arc<dyn BaseTool>],
    allowed: &ToolsValue,
    disallowed: &ToolsValue,
) -> Vec<Box<dyn BaseTool>> {
    let allowed_list = allowed.to_vec();
    let disallowed_list = disallowed.to_vec();

    parent_tools
        .iter()
        .filter(|tool| {
            let name = tool.name();
            let name_key = canonical_lower(name);
            if name == TOOL_AGENT {
                return false;
            }
            if !allowed_matches(&allowed_list, &name_key) {
                return false;
            }
            if disallowed_matches(&disallowed_list, &name_key) {
                return false;
            }
            true
        })
        .map(|tool| Box::new(ArcToolWrapper(Arc::clone(tool))) as Box<dyn BaseTool>)
        .collect()
}

/// Build fork directive message for fork mode.
///
/// The directive instructs the forked agent to continue from the parent conversation
/// with specific rules to prevent recursion and maintain scope.
pub fn build_fork_directive(prompt: &str) -> String {
    format!(
        "<fork_directive>\n\
         You are a forked agent continuing from the parent conversation.\n\
         Use the inherited context above and the assignment below.\n\
         \n\
         RULES:\n\
         1. Execute directly using your available tools; do not spawn sub-agents\n\
         2. Resolve routine choices from the available evidence. If essential information or authorization is missing, report the blocker to the caller and continue independent work\n\
         3. Stay within the assigned scope and authorization; do not invent facts or expand the task\n\
         4. Report results, supporting evidence, and any remaining uncertainty, then stop\n\
         5. Keep your response under 500 words unless specified otherwise\n\
         \n\
         Output format:\n\
           Scope: <your assigned scope in one sentence>\n\
           Result: <the answer or key findings>\n\
           Key files: <relevant file paths>\n\
           Files changed: <list if you modified files>\n\
         </fork_directive>\n\n\
         {prompt}"
    )
}

/// Extract [`AgentOverrides`] from already-parsed agent definition fields.
///
/// Returns `None` when all fields are empty (no overrides needed).
pub fn overrides_from_agent_def(
    system_prompt: &str,
    tone: &Option<String>,
    proactiveness: &Option<String>,
) -> Option<AgentOverrides> {
    let persona = if system_prompt.is_empty() {
        None
    } else {
        Some(system_prompt.to_string())
    };
    let overrides = AgentOverrides {
        persona,
        tone: tone.clone(),
        proactiveness: proactiveness.clone(),
    };
    if overrides.is_empty() {
        None
    } else {
        Some(overrides)
    }
}

#[cfg(test)]
#[path = "fork_test.rs"]
mod tests;
