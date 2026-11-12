//! 会话工作目录的共享句柄。
//!
//! `AgentState.cwd` 是普通字符串快照，工具不直接持有会话状态。
//! `EnterWorktree` / `ExitWorktree` 需要在运行中切换工作目录，因此把 cwd 抽成
//! 一个可共享的句柄：
//!
//! - ACP 会话持有句柄，每轮 `execute_prompt` 从这里取当前值；
//! - worktree、Bash 与文件工具持有同一个句柄，每次调用读取当前目录。
//! - WorktreeMiddleware 在下一模型 step 前同步状态，供审批等中间件使用。
//!
//! 已经启动的命令不会被切换目录；仍持有 cwd 字符串快照的其他工具，要到下一轮
//! 重建工具集后才会使用新目录。

use std::sync::Arc;

use parking_lot::RwLock;

/// 会话工作目录句柄（可在会话、工具、中间件之间共享）。
pub type SessionCwd = Arc<RwLock<String>>;

/// 用初始目录创建句柄。
pub fn new_session_cwd(cwd: impl Into<String>) -> SessionCwd {
    Arc::new(RwLock::new(cwd.into()))
}

/// 读取当前工作目录。
pub fn current(handle: &SessionCwd) -> String {
    handle.read().clone()
}

/// 有会话句柄时取实时目录，否则保留工具原有的固定目录。
pub fn current_or(handle: Option<&SessionCwd>, fallback: &str) -> String {
    handle.map_or_else(|| fallback.to_owned(), current)
}

/// 切换工作目录。
pub fn switch(handle: &SessionCwd, cwd: impl Into<String>) {
    *handle.write() = cwd.into();
}
