### Task 3: 新增 dispatch/session_fork.rs — fork_session

**背景：** TUI 路径的 `requests.rs:389-441` 中 `session/fork` handler 的核心业务逻辑是：创建新 ThreadStore entry + 复制源 session 的消息到新 entry。stdio 路径需要相同逻辑。

**注意：** `requests.rs:408` 中 `create_thread(ThreadMeta::new(cwd))` 的 `ThreadMeta` 位于 `peri_agent::thread::ThreadMeta`。

#### 执行步骤

- [ ] **Step 3.1**: 创建 `peri-acp/src/dispatch/session_fork.rs`

```rust
//! Fork a session: create a new thread and copy messages from source.

use peri_agent::messages::BaseMessage;
use peri_agent::thread::{ThreadMeta, ThreadStore};

/// Fork a session by creating a new thread and copying source messages.
///
/// Returns `Ok((new_thread_id, copied_messages))` on success.
/// The caller is responsible for inserting the new session into its session map.
pub async fn fork_session(
    thread_store: &dyn ThreadStore,
    source_thread_id: &str,
    source_messages: &[BaseMessage],
    cwd: &str,
) -> Result<(String, Vec<BaseMessage>), String> {
    let meta = ThreadMeta::new(cwd);
    let new_thread_id = thread_store
        .create_thread(meta)
        .await
        .map_err(|e| format!("Thread creation failed: {e}"))?;

    if !source_messages.is_empty() {
        if let Err(e) = thread_store
            .append_messages(
                &peri_agent::thread::ThreadId::from(new_thread_id.clone()),
                source_messages,
            )
            .await
        {
            tracing::warn!(error = %e, "session/fork: failed to copy messages to new thread");
        }
    }

    tracing::info!(
        source = %source_thread_id,
        new = %new_thread_id,
        msg_count = source_messages.len(),
        "Session forked"
    );

    Ok((new_thread_id, source_messages.to_vec()))
}
```

**参考：** 逻辑取自 `requests.rs:389-441`，将 TUI 路径中 `thread_store.create_thread(meta)` + `append_messages()` + sessions map 插入分离为：dispatch 处理前两个，transport 层处理 sessions map 插入。

- [ ] **Step 3.2**: 添加单元测试文件 `peri-acp/src/dispatch/session_fork_test.rs`

```rust
use super::session_fork::fork_session;
use peri_agent::messages::BaseMessage;
use peri_agent::thread::{ThreadId, ThreadMeta, ThreadStore};
use std::sync::Mutex;

struct CountingThreadStore {
    counter: Mutex<usize>,
    messages: Mutex<Vec<(String, Vec<BaseMessage>)>>,
}

#[async_trait::async_trait]
impl ThreadStore for CountingThreadStore {
    async fn create_thread(&self, _meta: ThreadMeta) -> Result<String, String> {
        let mut c = self.counter.lock().unwrap();
        *c += 1;
        Ok(format!("forked-{}", *c))
    }
    async fn load_messages(&self, _id: &ThreadId) -> Result<Vec<BaseMessage>, String> {
        Ok(vec![])
    }
    async fn append_messages(&self, id: &ThreadId, msgs: &[BaseMessage]) -> Result<(), String> {
        self.messages.lock().unwrap().push((id.0.clone(), msgs.to_vec()));
        Ok(())
    }
    async fn list_threads(&self) -> Result<Vec<peri_agent::thread::ThreadInfo>, String> {
        Ok(vec![])
    }
}

#[tokio::test]
async fn test_fork_session_creates_new_thread_and_copies_messages() {
    let store = CountingThreadStore {
        counter: Mutex::new(0),
        messages: Mutex::new(vec![]),
    };
    let source_msgs = vec![
        BaseMessage::human("hello"),
        BaseMessage::ai("world"),
    ];
    let (new_id, copied) = fork_session(&store, "source-1", &source_msgs, "/test").await.unwrap();
    assert_eq!(new_id, "forked-1");
    assert_eq!(copied.len(), 2);
    // 验证消息被复制到新线程
    let stored = store.messages.lock().unwrap();
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].0, "forked-1");
    assert_eq!(stored[0].1.len(), 2);
}

#[tokio::test]
async fn test_fork_session_empty_source() {
    let store = CountingThreadStore {
        counter: Mutex::new(0),
        messages: Mutex::new(vec![]),
    };
    let (new_id, copied) = fork_session(&store, "source-2", &[], "/test").await.unwrap();
    assert_eq!(new_id, "forked-1");
    assert!(copied.is_empty());
    // 空消息列表不应调用 append_messages
    let stored = store.messages.lock().unwrap();
    assert!(stored.is_empty());
}
```

#### 检查步骤

- [ ] `cargo build -p peri-acp` 编译通过
- [ ] `cargo test -p peri-acp --lib session_fork_test` 测试通过
- [ ] `cargo clippy -p peri-acp` 通过

---
