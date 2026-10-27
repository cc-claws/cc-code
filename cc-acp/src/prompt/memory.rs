//! 自动记忆的单次读取快照；规则渲染与会话兼容字段共用这一份内容。

use std::path::PathBuf;

use cc_middlewares::memory::{
    ensure_memory_dir_exists, get_auto_memory_dir, get_auto_memory_display_dir,
    read_and_truncate_entrypoint, ENTRYPOINT_NAME,
};

pub(crate) struct AutoMemorySnapshot {
    pub directory: PathBuf,
    pub display_directory: String,
    /// 空串表示已捕获的空索引，不能回退为下一轮重读磁盘。
    pub content: String,
}

impl AutoMemorySnapshot {
    pub fn capture(cwd: &str) -> Self {
        Self::load(get_auto_memory_dir(cwd), get_auto_memory_display_dir(cwd))
    }

    pub(super) fn load(directory: PathBuf, display_directory: String) -> Self {
        let _ = ensure_memory_dir_exists(&directory);
        let content = read_and_truncate_entrypoint(&directory.join(ENTRYPOINT_NAME))
            .map(|entrypoint| entrypoint.content)
            .unwrap_or_default();
        Self {
            directory,
            display_directory,
            content,
        }
    }

    /// 保留公开 FrozenSessionData 的旧格式，宿主不再将它作为另一条 System 注入。
    pub fn rendered_entrypoint(&self) -> String {
        if self.content.is_empty() {
            return String::new();
        }
        let display_directory = self.display_directory.trim_end_matches(['/', '\\']);
        format!(
            "Contents of {display_directory}/{ENTRYPOINT_NAME} (auto-memory):\n\n{}",
            self.content
        )
    }
}
