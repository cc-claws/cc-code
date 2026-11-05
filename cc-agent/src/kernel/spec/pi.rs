// <!-- upstream: earendil-works/pi @ main, checked 2026-06-01 -->

use super::{tool, GOVERNED_CORE};
use crate::kernel::{KernelSpec, MutationForm, ParamSpec, PromptSource, ToolSpec};

const RENAME_NONE: ParamSpec = ParamSpec::Rename(&[]);
const RENAME_PATH: ParamSpec = ParamSpec::Rename(&[("path", "file_path")]);
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
    tool("Glob", "find", "Find files matching a glob pattern.", "find: find files by name", RENAME_NONE, &[], None),
    tool("Grep", "grep", "Search file contents with a pattern.", "grep: search contents", RENAME_NONE, &[HINT_GREP], None),
    tool("Bash", "bash", "Execute a shell command.", "bash: run a command", RENAME_NONE, &[HINT_BASH], None),
];

/// pi 内核：只映射 6 个工具，无延迟加载元工具，全量放行非管辖工具。
pub static PI: KernelSpec = KernelSpec {
    id: "pi",
    prompt: PromptSource::Single(include_str!("../prompts/pi.md")),
    tools: PI_TOOLS,
    governed_tools: GOVERNED_CORE,
    meta_tools: &[],
    exposes_tool_search: false,
};
