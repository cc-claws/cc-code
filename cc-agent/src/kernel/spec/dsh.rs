// <!-- upstream: deepseek-ai/deepseek-harness @ dsh-v0.2.2-rc.1 (66491b05), branch master, checked 2026-11-10 -->

use super::tool;
use crate::kernel::{KernelSpec, ParamSpec, PromptSource, ToolSpec};

const RENAME_NONE: ParamSpec = ParamSpec::Rename(&[]);
/// dsh 的 grep 用 `include` 表达 peri 的 `glob` 过滤参数。
const RENAME_GREP: ParamSpec = ParamSpec::Rename(&[("include", "glob")]);
const RENAME_BASH: ParamSpec = ParamSpec::Rename(&[("timeoutMs", "timeout")]);
const RENAME_SKILL: ParamSpec = ParamSpec::Rename(&[("name", "skill")]);
const RENAME_QUESTIONS: ParamSpec = ParamSpec::RenameWithArrayFields {
    top: &[],
    array_key: "questions",
    fields: &[("multi_select", "multiSelect")],
};
static DSH_TOOLS: &[ToolSpec] = &[
    tool(
        "Read",
        "read",
        "Read a file from the local filesystem. Use read before edit. offset is 0-based: number of lines to skip (0 reads the first line), not an upstream 1-based line number.",
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
        "Execute a shell command. timeoutMs is in milliseconds. It limits foreground waiting; with background support the command may continue. execution_timeout is the hard runtime limit, also in milliseconds. Uses the session working directory; workdir is not supported. To change directory, include an explicit cd in command.",
        "bash: run a command",
        RENAME_BASH,
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
        "Ask the user a batch of questions. Each questions[] item requires question, header and options (2-4 labeled choices), with optional multi_select. Per-question id is not supported.",
        "ask_user_question: ask the user",
        RENAME_QUESTIONS,
        &[],
        None,
    ),
    tool(
        "Skill",
        "skill",
        "Invoke a named skill. Pass the required name and optional args.",
        "skill: invoke a skill",
        RENAME_SKILL,
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
