// <!-- upstream: deepseek-harness (dsh) @ main, checked 2026-06-01 -->

use super::tool;
use crate::kernel::{KernelSpec, ParamSpec, PromptSource, ToolSpec};

const RENAME_NONE: ParamSpec = ParamSpec::Rename(&[]);
/// dsh 的 grep 用 `include` 表达 peri 的 `glob` 过滤参数。
const RENAME_GREP: ParamSpec = ParamSpec::Rename(&[("include", "glob")]);
static DSH_TOOLS: &[ToolSpec] = &[
    tool(
        "Read",
        "read",
        "Read a file from the local filesystem. Use read before edit.",
        "read: read a file",
        RENAME_NONE,
        &[],
        None,
    ),
    tool(
        "Write",
        "write",
        "Write content to a file, creating it if needed.",
        "write: write a file",
        RENAME_NONE,
        &[],
        None,
    ),
    tool(
        "Edit",
        "edit",
        "Perform exact string replacements in a file.",
        "edit: replace text",
        RENAME_NONE,
        &[],
        None,
    ),
    tool(
        "Glob",
        "glob",
        "Find files matching a glob pattern.",
        "glob: find files",
        RENAME_NONE,
        &[],
        None,
    ),
    tool(
        "Grep",
        "grep",
        "Search file contents with a pattern; filter with include.",
        "grep: search contents",
        RENAME_GREP,
        &[],
        None,
    ),
    tool(
        "Bash",
        "bash",
        "Execute a shell command with an optional timeout.",
        "bash: run a command",
        RENAME_NONE,
        &[],
        None,
    ),
    tool(
        "TodoWrite",
        "todo_write",
        "Manage the task todo list.",
        "todo_write: manage todos",
        RENAME_NONE,
        &[],
        None,
    ),
    tool(
        "WebFetch",
        "web_fetch",
        "Fetch the content of a URL.",
        "web_fetch: fetch a URL",
        RENAME_NONE,
        &[],
        None,
    ),
    tool(
        "Agent",
        "subagent",
        "Launch a subagent for a delegated task.",
        "subagent: delegate work",
        RENAME_NONE,
        &[],
        None,
    ),
    tool(
        "AskUserQuestion",
        "ask_user_question",
        "Ask the user a batch of questions with options.",
        "ask_user_question: ask the user",
        RENAME_NONE,
        &[],
        None,
    ),
    tool(
        "Skill",
        "skill",
        "Invoke a named skill.",
        "skill: invoke a skill",
        RENAME_NONE,
        &[],
        None,
    ),
    // 注意：dsh 的 schedule_create 使用 RFC 5545 RRULE（含时区语义），
    // peri 的 CronScheduler 只接受五段 cron 表达式，二者不同构。
    // 按「能力缺口不映射」处理（同 apply_patch），Cron 工具保持规范名直接可见。
];

/// 管辖边界 = 公共 14 项。Cron 系列不在管辖内：dsh 的 RRULE 语义与 peri 的
/// 五段 cron 不同构，保持规范名直接可见（非管辖工具一律放行）。
static DSH_GOVERNED: &[&str] = &[
    "Read",
    "Write",
    "Edit",
    "Glob",
    "Grep",
    "Bash",
    "WebFetch",
    "WebSearch",
    "Agent",
    "AskUserQuestion",
    "TodoWrite",
    "Skill",
    "SearchExtraTools",
    "ExecuteExtraTool",
];

/// dsh 内核：snake_case 小写工具名，参数与 cc 几乎同构，无延迟加载元工具。
pub static DSH: KernelSpec = KernelSpec {
    id: "dsh",
    prompt: PromptSource::Single(include_str!("../prompts/dsh.md")),
    tools: DSH_TOOLS,
    governed_tools: DSH_GOVERNED,
    meta_tools: &[],
    exposes_tool_search: false,
};
