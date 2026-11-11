//! 会话工作目录的共享句柄。
//!
//! peri 的 `AgentState.cwd` 是按轮次构造的不可变字符串，工具拿不到会话状态。
//! `EnterWorktree` / `ExitWorktree` 需要在运行中切换工作目录，因此把 cwd 抽成
//! 一个可共享的句柄：
//!
//! - ACP 会话持有句柄，每轮 `execute_prompt` 从这里取当前值；
//! - 工具持有同一个句柄，切换后**下一轮**的 state 与工具集即使用新目录。
//!
//! 已知限制：同一轮内并行执行的其他工具仍使用切换前的目录（工具集在轮次开始时
//! 构建）。这与「工具调用立即 chdir」的实现有差异，但对模型可见的行为是
//! 「切换生效后所有后续操作都在新目录」，由工具返回值明确告知路径。

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

/// 切换工作目录。
pub fn switch(handle: &SessionCwd, cwd: impl Into<String>) {
    *handle.write() = cwd.into();
}
