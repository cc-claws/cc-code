// <!-- upstream: cc-code (peri 内置呈现) @ main, checked 2026-06-01 -->

use super::{tool, GOVERNED_CORE};
use crate::kernel::{KernelSpec, ParamSpec, PromptSource, ToolSpec};

const RENAME_NONE: ParamSpec = ParamSpec::Rename(&[]);

static CC_TOOLS: &[ToolSpec] = &[
    tool("Read", "Read", "", "Read a file", RENAME_NONE, &[], None),
    tool("Write", "Write", "", "Write a file", RENAME_NONE, &[], None),
    tool("Edit", "Edit", "", "Edit a file", RENAME_NONE, &[], None),
    tool(
        "Glob",
        "Glob",
        "",
        "Find files by name",
        RENAME_NONE,
        &[],
        None,
    ),
    tool(
        "Grep",
        "Grep",
        "",
        "Search file contents",
        RENAME_NONE,
        &[],
        None,
    ),
    tool(
        "Bash",
        "Bash",
        "",
        "Run a shell command",
        RENAME_NONE,
        &[],
        None,
    ),
    tool(
        "WebFetch",
        "WebFetch",
        "",
        "Fetch a URL",
        RENAME_NONE,
        &[],
        None,
    ),
    tool(
        "WebSearch",
        "WebSearch",
        "",
        "Search the web",
        RENAME_NONE,
        &[],
        None,
    ),
    tool(
        "Agent",
        "Agent",
        "",
        "Launch a subagent",
        RENAME_NONE,
        &[],
        None,
    ),
    tool(
        "AskUserQuestion",
        "AskUserQuestion",
        "",
        "Ask the user",
        RENAME_NONE,
        &[],
        None,
    ),
    tool(
        "TodoWrite",
        "TodoWrite",
        "",
        "Manage the todo list",
        RENAME_NONE,
        &[],
        None,
    ),
    tool(
        "Skill",
        "Skill",
        "",
        "Invoke a skill",
        RENAME_NONE,
        &[],
        None,
    ),
    tool(
        "SearchExtraTools",
        "SearchExtraTools",
        "",
        "Discover deferred tools",
        RENAME_NONE,
        &[],
        None,
    ),
    tool(
        "ExecuteExtraTool",
        "ExecuteExtraTool",
        "",
        "Execute a deferred tool",
        RENAME_NONE,
        &[],
        None,
    ),
];

/// cc 内核：工具定义恒等变换，提示词为空段落（逐字节等价回归基线）。
pub static CC: KernelSpec = KernelSpec {
    id: "cc",
    // cc 内核的提示词是空段落：出站/提示词组装对 cc 保持逐字节等价，
    // 现有系统提示词（sections/*.md）原样生效，不追加任何内核内容。
    prompt: PromptSource::Single(""),
    tools: CC_TOOLS,
    governed_tools: GOVERNED_CORE,
    meta_tools: &["SearchExtraTools", "ExecuteExtraTool"],
    exposes_tool_search: true,
};
