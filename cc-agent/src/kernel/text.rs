//! 面向运行时拼接文案的内核辅助函数。

use super::Kernel;

/// 取某规范名在当前内核下的展示名（找不到则原样返回）。
///
/// 只用于**由代码生成的提示语**（如中间件注入文案），禁止用于工具输出数据，
/// 否则会污染文件内容与命令输出。
pub fn name_of<'a>(kernel: &Kernel, canonical: &'a str) -> &'a str {
    kernel.echo_name(canonical)
}

/// 把一段提示语里的规范名替换为当前内核方言名。
pub fn dialectize(kernel: &Kernel, hint: &str) -> String {
    kernel.dialectize_hint(hint).into_owned()
}

/// 已知的、由代码生成的提示语片段（**不含工具输出数据**）。
///
/// 这些是 §6.3 列出的 7 处运行时拼接文案的核心子串。只对精确片段做替换，
/// 不对工具结果做全表扫描，避免污染文件内容与命令输出。
pub const TOOL_HINT_FRAGMENTS: &[&str] = &[
    "call the Read tool with file_path=",
    "Grep the full MEMORY.md",
    "Read their topic files",
    "Use Glob to locate relevant files and Read to inspect",
    "write to it directly with the Write tool",
    "invoke Skill with its name",
    "NEVER read SKILL.md directly with Read",
    "use Read on the skill path",
];

/// 只对固定提示语片段做方言替换，绝不扫描整段工具输出数据。
pub fn dialectize_known_hints(kernel: &Kernel, text: &str) -> String {
    if kernel.is_cc() {
        return text.to_string();
    }
    let mut out = text.to_string();
    for fragment in TOOL_HINT_FRAGMENTS {
        if out.contains(fragment) {
            let replaced = kernel.dialectize_hint(fragment).into_owned();
            out = out.replace(fragment, &replaced);
        }
    }
    out
}
