use std::{collections::HashMap, sync::Arc};

use super::SharedSessions;

pub(super) type SharedPromptLocks =
    Arc<tokio::sync::Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>>;

/// 只回收已关闭且无运行中/排队请求引用的锁，避免同一会话产生两把串行锁。
pub(super) async fn release_closed_session_lock(
    sessions: &SharedSessions,
    prompt_locks: &SharedPromptLocks,
    session_id: &str,
) {
    // 先锁 sessions，再锁锁表；调用方必须先释放具体会话的 prompt guard 和 Arc。
    let sessions = sessions.lock().await;
    if sessions.contains_key(session_id) {
        return;
    }
    let mut locks = prompt_locks.lock().await;
    if locks
        .get(session_id)
        .is_some_and(|lock| Arc::strong_count(lock) == 1)
    {
        locks.remove(session_id);
    }
}

#[cfg(test)]
#[path = "prompt_locks_test.rs"]
mod tests;
