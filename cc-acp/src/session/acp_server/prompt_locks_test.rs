use super::*;
use crate::acp_server::SessionState;

fn make_session(session_id: &str) -> SessionState {
    SessionState {
        session_id: session_id.to_string(),
        thread_id: session_id.to_string(),
        cwd: String::new(),
        history: Vec::new(),
        cancel_token: None,
        background_registry: Arc::default(),
        steering: None,
        frozen: None,
        recall_items: Vec::new(),
        agent_pool: Arc::default(),
        approval_memory: cc_middlewares::hitl::ApprovalMemory::new(),
        mcp_over_acp_servers: HashMap::new(),
    }
}

#[tokio::test]
async fn test_closed_session_locks_are_released_across_repeated_sessions() {
    let sessions: SharedSessions = Arc::default();
    let locks: SharedPromptLocks = Arc::default();
    for index in 0..100 {
        let session_id = format!("session-{index}");
        sessions
            .lock()
            .await
            .insert(session_id.clone(), make_session(&session_id));
        let lock = Arc::new(tokio::sync::Mutex::new(()));
        let weak_lock = Arc::downgrade(&lock);
        locks.lock().await.insert(session_id.clone(), lock);
        sessions.lock().await.remove(&session_id);
        release_closed_session_lock(&sessions, &locks, &session_id).await;
        assert!(
            locks.lock().await.is_empty(),
            "关闭的空闲会话不能累积锁条目"
        );
        assert!(weak_lock.upgrade().is_none(), "锁本身应随条目释放");
    }
}

#[tokio::test]
async fn test_closed_session_lock_remains_until_running_and_queued_requests_finish() {
    let sessions: SharedSessions = Arc::default();
    let locks: SharedPromptLocks = Arc::default();
    let running_lock = Arc::new(tokio::sync::Mutex::new(()));
    let queued_lock = Arc::clone(&running_lock);
    let weak_lock = Arc::downgrade(&running_lock);
    locks
        .lock()
        .await
        .insert("session".to_string(), Arc::clone(&running_lock));
    let running_guard = running_lock.lock().await;
    release_closed_session_lock(&sessions, &locks, "session").await;
    assert!(
        locks.lock().await.contains_key("session"),
        "运行中的旧锁不能移除"
    );
    assert!(
        queued_lock.try_lock().is_err(),
        "排队请求仍应等待运行中的请求"
    );
    drop(running_guard);
    drop(running_lock);
    release_closed_session_lock(&sessions, &locks, "session").await;
    assert!(
        locks.lock().await.contains_key("session"),
        "排队请求也必须保留旧锁"
    );
    assert!(
        queued_lock.try_lock().is_ok(),
        "前一个请求结束后排队请求可继续"
    );
    drop(queued_lock);
    release_closed_session_lock(&sessions, &locks, "session").await;
    assert!(
        locks.lock().await.is_empty(),
        "最后一个请求结束后应清理锁表"
    );
    assert!(
        weak_lock.upgrade().is_none(),
        "最后一个请求结束后应释放锁对象"
    );
}

#[tokio::test]
async fn test_reloaded_session_reuses_old_lock_while_requests_are_in_flight() {
    let sessions: SharedSessions = Arc::default();
    let locks: SharedPromptLocks = Arc::default();
    let running_lock = Arc::new(tokio::sync::Mutex::new(()));
    locks
        .lock()
        .await
        .insert("session".to_string(), Arc::clone(&running_lock));
    let running_guard = running_lock.lock().await;
    release_closed_session_lock(&sessions, &locks, "session").await;
    sessions
        .lock()
        .await
        .insert("session".to_string(), make_session("session"));
    let reloaded_lock = {
        let mut locks = locks.lock().await;
        Arc::clone(
            locks
                .entry("session".to_string())
                .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(()))),
        )
    };
    assert!(
        Arc::ptr_eq(&running_lock, &reloaded_lock),
        "同 ID 恢复必须复用原串行锁"
    );
    assert!(
        reloaded_lock.try_lock().is_err(),
        "新请求不能绕过尚未完成的旧请求"
    );
    drop(running_guard);
    drop(running_lock);
    drop(reloaded_lock);
    release_closed_session_lock(&sessions, &locks, "session").await;
    assert!(
        locks.lock().await.contains_key("session"),
        "仍存活的会话应保留串行锁"
    );
    sessions.lock().await.remove("session");
    release_closed_session_lock(&sessions, &locks, "session").await;
    assert!(locks.lock().await.is_empty(), "再次关闭后应正常清理");
}
