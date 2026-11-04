//! cc-tools — 无状态核心工具与跨平台进程包装（叶子 crate）。
//!
//! 模块名与 `cc-middlewares` 保持一致，使搬迁后 `crate::tools::*` /
//! `crate::process::*` 路径无需改动（见拆分计划 §9.5）。

pub mod bash;
pub mod process;

pub mod tools {
    pub mod filesystem;
    pub mod output_filter;
    pub mod output_persist;
    pub mod todo;
}
