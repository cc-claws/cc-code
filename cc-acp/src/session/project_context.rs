//! 会话目录派生的资源；不修改进程 cwd，也不复用其他项目的 MCP 连接。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock, Weak};

use cc_middlewares::hooks::RegisteredHook;
use cc_middlewares::mcp::{McpClientPool, McpInitStatus, OAuthFlowEvent};
use tokio::sync::watch;

type ProjectMcp = (Weak<McpClientPool>, watch::Receiver<McpInitStatus>);
type OAuthCallback = Box<dyn Fn(OAuthFlowEvent) + Send + Sync>;

/// TUI 面板与 ACP 执行器共用同项目的池；最后一个使用者离开后连接可释放。
pub fn project_mcp_pool(
    cwd: &str,
    oauth_callback: Option<OAuthCallback>,
) -> (Arc<McpClientPool>, watch::Receiver<McpInitStatus>) {
    static POOLS: OnceLock<parking_lot::Mutex<HashMap<PathBuf, ProjectMcp>>> = OnceLock::new();
    let key = std::fs::canonicalize(cwd).unwrap_or_else(|_| PathBuf::from(cwd));
    let mut pools = POOLS.get_or_init(Default::default).lock();
    pools.retain(|_, (pool, _)| pool.strong_count() > 0);
    if let Some((pool, rx)) = pools.get(&key) {
        if let Some(pool) = pool.upgrade() {
            return (pool, rx.clone());
        }
    }
    let pool = Arc::new(McpClientPool::new_pending());
    let (tx, rx) = watch::channel(McpInitStatus::Pending);
    pools.insert(key, (Arc::downgrade(&pool), rx.clone()));
    let init_pool = Arc::clone(&pool);
    let cwd = cwd.to_string();
    let claude_home = dirs_next::home_dir().unwrap_or_default().join(".claude");
    tokio::spawn(async move {
        McpClientPool::run_initialize(
            init_pool,
            Path::new(&cwd),
            &claude_home,
            tx,
            oauth_callback,
            None,
        )
        .await;
    });
    (pool, rx)
}

/// plugin/global hook 仍沿用传入快照，项目 hook 则只从当前 session cwd 读取。
pub fn project_hook_groups(
    groups: Vec<Vec<RegisteredHook>>,
    cwd: &str,
) -> Vec<Vec<RegisteredHook>> {
    let mut groups: Vec<Vec<_>> = groups
        .into_iter()
        .map(|group| {
            group
                .into_iter()
                .filter(|hook| hook.plugin_id != "settings.local")
                .collect()
        })
        .filter(|group: &Vec<_>| !group.is_empty())
        .collect();
    let local = cc_middlewares::hooks::loader::load_settings_local_hooks(cwd);
    if !local.is_empty() {
        groups.push(local);
    }
    groups
}

#[cfg(test)]
#[path = "project_context_test.rs"]
mod tests;
