//! 三个内核的编译期快照常量。
//!
//! 提示词与工具描述随 peri 发版；更新方式是编辑 `prompts/*.md` 后重新编译。
//! 增加新内核只需新建一个文件、把常量追加进 `ALL_SPECS`。

use crate::kernel::{MutationForm, ParamSpec, ToolSpec};
use crate::tools::ToolParametersKind;

/// `ToolSpec` 的 const 构造器，供各内核文件声明静态数组。
pub(crate) const fn tool(
    canonical: &'static str,
    exposed: &'static str,
    description: &'static str,
    prompt_snippet: &'static str,
    params: ParamSpec,
    signature_hints: &'static [(&'static [&'static str], &'static str)],
    mutation_form: Option<MutationForm>,
) -> ToolSpec {
    ToolSpec {
        canonical,
        exposed,
        description,
        prompt_snippet,
        prompt_guidelines: &[],
        params,
        signature_hints,
        mutation_form,
        parameters_kind: ToolParametersKind::JsonSchema,
    }
}

/// 12 个 CORE_TOOLS + 2 个 META_TOOLS 的规范名（内核管辖边界的公共部分）。
pub(crate) const GOVERNED_CORE: &[&str] = &[
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

mod cc;
mod dsh;
mod pi;

pub use cc::CC;
pub use dsh::DSH;
pub use pi::PI;
