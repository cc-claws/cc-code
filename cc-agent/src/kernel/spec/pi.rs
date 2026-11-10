// <!-- upstream: earendil-works/pi @ main, checked 2026-06-01 -->

use super::{tool, GOVERNED_CORE};
use crate::kernel::{KernelSpec, MutationForm, ParamSpec, PromptSource, ToolSpec};

const RENAME_NONE: ParamSpec = ParamSpec::Rename(&[]);
const RENAME_PATH: ParamSpec = ParamSpec::Rename(&[("path", "file_path")]);
const RENAME_GREP: ParamSpec = ParamSpec::Rename(&[
    ("ignoreCase", "-i"),
    ("literal", "fixed_strings"),
    ("limit", "head_limit"),
]);
/// pi 的 Edit 是结构性差异：顶层 `path` + `edits[]{oldText,newText}`。
const RENAME_EDIT: ParamSpec = ParamSpec::RenameWithArrayFields {
    top: &[("path", "file_path")],
    array_key: "edits",
    fields: &[("oldText", "old_string"), ("newText", "new_string")],
};

const HINT_READ: (&[&str], &str) = (&["path"], "read");
const HINT_EDIT: (&[&str], &str) = (&["edits"], "edit");
const HINT_GREP: (&[&str], &str) = (&["pattern"], "grep");
const HINT_BASH: (&[&str], &str) = (&["command"], "bash");

static PI_TOOLS: &[ToolSpec] = &[
    // 注意：offset 沿用 peri 语义（从 0 开始跳过的行数），与上游 pi 的 1-indexed 行号不同；
    // 已在 description 中显式说明，避免模型按上游文档产生 off-by-one。
    tool("Read", "read", "Read a file. Use path (not file_path). offset is 0-based: number of lines to skip.", "read: read a file at path", RENAME_PATH, &[HINT_READ], None),
    tool("Write", "write", "Write content to a file at path.", "write: write a file at path", RENAME_PATH, &[], None),
    tool("Edit", "edit", "Apply multiple non-overlapping edits to one file: pass path and an edits array of {oldText,newText}.", "edit: batch edits with oldText/newText", RENAME_EDIT, &[HINT_EDIT], Some(MutationForm::BatchReplace)),
    tool("Glob", "find", "Find files matching a glob pattern, with optional path and limit (default 1000). The underlying search returns at most 1000 files even if a larger limit is requested.", "find: find files by name", RENAME_NONE, &[], None),
    tool("Grep", "grep", "Search file contents with pattern, optional ignoreCase, literal and limit (default 250; 0 is unlimited). Uses peri search semantics: output_mode defaults to files_with_matches; use content for matching lines. Matching is smart-case unless ignoreCase is true.", "grep: search contents", RENAME_GREP, &[HINT_GREP], None),
    // 与 read.offset 一样保留 peri 执行语义，在模型可见描述中明确单位差异。
    tool("Bash", "bash", "Execute a shell command. timeout is in milliseconds, NOT upstream pi seconds (use 30000 for 30 seconds). It limits foreground waiting; with background support the command may continue. execution_timeout is the hard runtime limit, also in milliseconds.", "bash: run a command", RENAME_NONE, &[HINT_BASH], None),
    // Skills are user-configured shared capabilities, independent of the kernel.
    tool("Skill", "skill", "", "skill: invoke a configured skill", RENAME_NONE, &[], None),
];

/// pi 内核：6 个基础工具和共享 Skill，无延迟加载元工具。
pub static PI: KernelSpec = KernelSpec {
    id: "pi",
    prompt: PromptSource::Single(include_str!("../prompts/pi.md")),
    tools: PI_TOOLS,
    governed_tools: GOVERNED_CORE,
    meta_tools: &[],
    exposes_tool_search: false,
};
