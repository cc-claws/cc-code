//! Fork semantics: tool filtering, fork directive construction, agent override extraction.
//!
//! Pure computation functions for sub-agent inheritance from parent agent.
//! No async, no external state mutation — safe for unit testing without mocks.

use std::sync::Arc;

use cc_agent::tools::BaseTool;

use crate::tool_search::core_tools::TOOL_AGENT;
use crate::{agent_define::AgentOverrides, claude_agent_parser::ToolsValue, tools::ArcToolWrapper};

/// Filter tools from parent set based on agent definition's tools/disallowedTools fields.
///
/// Rules:
/// - `tools` is `Empty` -> inherit all parent tools (but always exclude `Agent` itself to prevent recursion)
/// - `tools` has value -> only keep tools in the list (also exclude `Agent`)
/// - then remove tools listed in `disallowed_tools` from the result
///
/// Matching is case-insensitive (users often write PascalCase in agent.md).
pub fn filter_tools(
    parent_tools: &[Arc<dyn BaseTool>],
    allowed: &ToolsValue,
    disallowed: &ToolsValue,
) -> Vec<Box<dyn BaseTool>> {
    let allowed_list = allowed.to_vec();
    let disallowed_list = disallowed.to_vec();
    let is_wildcard = allowed_list.len() == 1 && allowed_list[0] == "*";

    parent_tools
        .iter()
        .filter(|tool| {
            let name = tool.name();
            let name_lower = name.to_lowercase();
            if name == TOOL_AGENT {
                return false;
            }
            if !is_wildcard
                && !allowed_list.is_empty()
                && !allowed_list.iter().any(|n| n.to_lowercase() == name_lower)
            {
                return false;
            }
            if disallowed_list
                .iter()
                .any(|n| n.to_lowercase() == name_lower)
            {
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
