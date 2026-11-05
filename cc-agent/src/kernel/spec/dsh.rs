// <!-- upstream: deepseek-harness (dsh) @ main, checked 2026-06-01 -->

use super::tool;
use crate::kernel::{KernelSpec, ParamSpec, PromptSource, ToolSpec};

const RENAME_NONE: ParamSpec = ParamSpec::Rename(&[]);
/// dsh 的 grep 用 `include` 表达 peri 的 `glob` 过滤参数。
const RENAME_GREP: ParamSpec = ParamSpec::Rename(&[("include", "glob")]);
/// dsh 的 schedule_create 用 `rrule`（string）表达 peri 的 `expression`。
const RENAME_CRON: ParamSpec = ParamSpec::Rename(&[("rrule", "expression")]);

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
    tool(
        "CronRegister",
        "schedule_create",
        "Create a schedule using an rrule expression.",
        "schedule_create: create a schedule",
        RENAME_CRON,
        &[],
        None,
    ),
    tool(
        "CronList",
        "schedule_list",
        "List all schedules.",
        "schedule_list: list schedules",
        RENAME_NONE,
        &[],
        None,
    ),
];

/// 管辖边界 = 公共 14 项 + 本内核显式映射的两个定时工具。
/// `CronRemove` 不在此列：dsh 通过 `schedule_edit` 的 `action: delete` 实现，
/// 语义非同构，故不映射、也不裁剪（非管辖工具一律放行）。
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
    "CronRegister",
    "CronList",
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
