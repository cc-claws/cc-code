### Task 2: 新增 dispatch/session_load.rs — load_session_messages

**背景：** TUI 路径的 `requests.rs:233-293` 中 `session/load` handler 的核心业务逻辑是调用 `thread_store.load_messages()`。stdio 路径需要相同的逻辑。提取到 dispatch 层作为纯数据函数。

**注意：** `thread_store.load_messages()` 接受 `&ThreadId`，而 TUI 和 stdio 的 session_id/thread_id 都是 `String`。函数签名使用 `&str` 参数，内部构造 `ThreadId::from(id)`。

#### 执行步骤

- [ ] **Step 2.1**: 创建 `peri-acp/src/dispatch/session_load.rs`

```rust
//! Load session messages from ThreadStore.

use peri_agent::messages::BaseMessage;
use peri_agent::thread::{ThreadId, ThreadStore};

/// Load message history for a session thread.
///
/// Returns an empty `Vec` if the thread does not exist (with a warning log).
pub async fn load_session_messages(
    thread_store: &dyn ThreadStore,
    thread_id: &str,
) -> Vec<BaseMessage> {
    match thread_store.load_messages(&ThreadId::from(thread_id.to_string())).await {
        Ok(msgs) => msgs,
        Err(e) => {
            tracing::warn!(thread_id = %thread_id, error = %e, "session/load: thread not found, returning empty history");
            Vec::new()
        }
    }
}
```

**参考：** 逻辑取自 `requests.rs:239-251` 的 match 分支，提取为纯函数。

- [ ] **Step 2.2**: 添加单元测试文件 `peri-acp/src/dispatch/session_load_test.rs`

```rust
use super::session_load::load_session_messages;
use peri_agent::messages::BaseMessage;
use peri_agent::thread::{ThreadId, ThreadMeta, ThreadStore};

struct MockThreadStore {
    messages: Vec<BaseMessage>,
    should_fail: bool,
}

#[async_trait::async_trait]
impl ThreadStore for MockThreadStore {
    async fn create_thread(&self, _meta: ThreadMeta) -> Result<String, String> {
        Ok("test-id".into())
    }
    async fn load_messages(&self, _id: &ThreadId) -> Result<Vec<BaseMessage>, String> {
        if self.should_fail {
            Err("not found".into())
        } else {
            Ok(self.messages.clone())
        }
    }
    async fn append_messages(&self, _id: &ThreadId, _msgs: &[BaseMessage]) -> Result<(), String> {
        Ok(())
    }
    async fn list_threads(&self) -> Result<Vec<peri_agent::thread::ThreadInfo>, String> {
        Ok(vec![])
    }
}

#[tokio::test]
async fn test_load_session_messages_success() {
    let msg = BaseMessage::human("hello");
    let store = MockThreadStore {
        messages: vec![msg.clone()],
        should_fail: false,
    };
    let result = load_session_messages(&store, "test-id").await;
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].content(), "hello");
}

#[tokio::test]
async fn test_load_session_messages_not_found_returns_empty() {
    let store = MockThreadStore {
        messages: vec![],
        should_fail: true,
    };
    let result = load_session_messages(&store, "missing-id").await;
    assert!(result.is_empty(), "不存在的线程应返回空列表");
}
```

#### 检查步骤

- [ ] `cargo build -p peri-acp` 编译通过
- [ ] `cargo test -p peri-acp --lib session_load_test` 测试通过
- [ ] `cargo clippy -p peri-acp` 通过

---
