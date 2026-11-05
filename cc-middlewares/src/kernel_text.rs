//! 内核文案方言化辅助：面向**代码生成的提示语**，绝不用于工具输出数据。
//!
//! `cc-middlewares` 依赖 `cc-agent`，方向正确；内核数据只读。

use cc_agent::kernel::{text, Kernel};

/// 把一段提示语里的规范工具名替换为当前内核方言名。
pub fn dialectize_hint(kernel: &Kernel, hint: &str) -> String {
    text::dialectize(kernel, hint)
}

/// 对已知提示语片段做定点替换（供工具结果里的提示语使用）。
pub fn dialectize_known_hints(kernel: &Kernel, body: &str) -> String {
    text::dialectize_known_hints(kernel, body)
}

#[cfg(test)]
#[path = "kernel_text_test.rs"]
mod tests;
