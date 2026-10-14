//! 记忆系统核心模块，提供路径计算、MEMORY.md 截断与系统提示词生成。

pub mod entrypoint;
pub mod middleware;
pub mod paths;
pub mod prompt;

pub use entrypoint::{
    read_and_truncate_entrypoint, truncate_entrypoint_content, EntrypointTruncation,
    ENTRYPOINT_NAME, MAX_ENTRYPOINT_BYTES, MAX_ENTRYPOINT_LINES,
};
pub use middleware::MemoryMiddleware;
pub use paths::{
    ensure_memory_dir_exists, find_canonical_project_root, get_auto_memory_dir,
    get_auto_memory_entrypoint, is_auto_mem_path, sanitize_project_path,
};
pub use prompt::build_auto_memory_prompt;

#[cfg(test)]
mod tests {
    use super::*;
    include!("memory_test.rs");
}
