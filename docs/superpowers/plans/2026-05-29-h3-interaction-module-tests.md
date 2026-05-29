# H3: Interaction 模块单元测试 实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 为 `peri-agent/src/interaction/` 模块添加完整的单元测试覆盖，包括 ChannelBroker、MultiplexBroker 和 ChannelState。

**Architecture:** 新建 `interaction/channel_broker_test.rs` 和 `interaction/multiplex_test.rs` 测试文件。使用 MockBroker 和 MockNotificationSender 验证权限审批流程、超时、竞速等行为。

**Tech Stack:** Rust, tokio async test, async-trait, parking_lot

---

## 文件结构

| 操作 | 文件路径 | 职责 |
|------|----------|------|
| 创建 | `peri-agent/src/interaction/channel_broker_test.rs` | ChannelBroker + ChannelState 测试 |
| 创建 | `peri-agent/src/interaction/multiplex_test.rs` | MultiplexBroker 测试 |
| 参考 | `peri-agent/src/interaction/mod.rs` | trait 定义、InteractionContext |
| 参考 | `peri-agent/src/interaction/channel_broker.rs` | ChannelBroker 实现 |
| 参考 | `peri-agent/src/interaction/multiplex.rs` | MultiplexBroker 实现 |
| 参考 | `peri-agent/src/interaction/channel_state.rs` | ChannelState 实现 |

---

### Task 1: 添加 ChannelState 单元测试

**Files:**
- Create: `peri-agent/src/interaction/channel_broker_test.rs`

- [ ] **Step 1: 读取 channel_state.rs 确认所有公开方法签名**

Read `peri-agent/src/interaction/channel_state.rs` 确认 `authorize`、`revoke`、`close_all`、`register_session`、`unregister_session` 的精确签名。

- [ ] **Step 2: 创建 channel_broker_test.rs，包含 ChannelState 测试**

```rust
use super::*;
use super::channel_state::ChannelState;
use tokio::sync::mpsc;

// === ChannelState 测试 ===

#[test]
fn test_channel_state_authorize_添加授权() {
    // 验证 authorize 添加 server 到授权列表
    let state = ChannelState::new();
    state.authorize("server-1", "plugin-a".to_string());
    let authorized = state.authorized.read();
    assert!(authorized.contains_key("server-1"));
    assert_eq!(authorized.get("server-1").unwrap(), "plugin-a");
}

#[test]
fn test_channel_state_revoke_移除授权() {
    // 验证 revoke 移除 server
    let state = ChannelState::new();
    state.authorize("server-1", "source".to_string());
    state.revoke("server-1");
    let authorized = state.authorized.read();
    assert!(!authorized.contains_key("server-1"));
}

#[test]
fn test_channel_state_revoke_不存在的server无异常() {
    // 验证 revoke 不存在的 server 不 panic
    let state = ChannelState::new();
    state.revoke("non-existent");
}

#[test]
fn test_channel_state_close_all_清空所有授权() {
    // 验证 close_all 清空所有授权
    let state = ChannelState::new();
    state.authorize("server-1", "a".to_string());
    state.authorize("server-2", "b".to_string());
    state.close_all();
    let authorized = state.authorized.read();
    assert!(authorized.is_empty());
}

#[test]
fn test_channel_state_register_session() {
    // 验证 session 注册和注销
    let state = ChannelState::new();
    let (tx, _rx) = mpsc::unbounded_channel();
    state.register_session("sess-1".to_string(), tx);
    let txs = state.channel_msg_txs.read();
    assert!(txs.contains_key("sess-1"));
}

#[test]
fn test_channel_state_unregister_session() {
    // 验证注销后 session 不在列表中
    let state = ChannelState::new();
    let (tx, _rx) = mpsc::unbounded_channel();
    state.register_session("sess-1".to_string(), tx);
    state.unregister_session("sess-1");
    let txs = state.channel_msg_txs.read();
    assert!(!txs.contains_key("sess-1"));
}

#[test]
fn test_channel_state_unregister_不存在的session无异常() {
    // 验证注销不存在的 session 不 panic
    let state = ChannelState::new();
    state.unregister_session("non-existent");
}
```

- [ ] **Step 3: 确保 mod.rs 中声明了测试模块**

检查 `peri-agent/src/interaction/mod.rs` 中是否已有 `#[cfg(test)] mod channel_broker_test;`。如果没有，需要添加。

- [ ] **Step 4: 运行测试验证通过**

Run: `cargo test -p peri-agent --lib -- interaction::channel_broker_test`
Expected: ALL PASS

- [ ] **Step 5: 提交**

```bash
git add peri-agent/src/interaction/channel_broker_test.rs peri-agent/src/interaction/mod.rs
git commit -m "test: add ChannelState unit tests (authorize/revoke/close_all/session)"
```

---

### Task 2: 添加 ChannelBroker 单元测试

**Files:**
- Modify: `peri-agent/src/interaction/channel_broker_test.rs`

- [ ] **Step 1: 读取 channel_broker.rs 确认 request() 行为**

Read `peri-agent/src/interaction/channel_broker.rs` 确认：
- 无授权 server 时的返回值
- 超时逻辑
- 通知发送流程

- [ ] **Step 2: 创建 MockNotificationSender 和 ChannelBroker 测试**

在 `channel_broker_test.rs` 末尾追加：

```rust
use super::channel_broker::ChannelBroker;
use async_trait::async_trait;
use std::sync::{Arc, Mutex};

// MockNotificationSender
#[derive(Default)]
struct MockNotificationSender {
    sent: Arc<Mutex<Vec<(String, String, serde_json::Value)>>>,
}

#[async_trait]
impl ChannelNotificationSender for MockNotificationSender {
    async fn send_notification(
        &self,
        server_name: &str,
        method: &str,
        params: serde_json::Value,
    ) -> Result<(), String> {
        self.sent.lock().unwrap().push((
            server_name.to_string(),
            method.to_string(),
            params,
        ));
        Ok(())
    }
}

impl MockNotificationSender {
    fn sent_count(&self) -> usize {
        self.sent.lock().unwrap().len()
    }

    fn sent_to(&self) -> Vec<String> {
        self.sent.lock().unwrap().iter().map(|(s, _, _)| s.clone()).collect()
    }
}

fn make_approval_items() -> Vec<ApprovalItem> {
    vec![ApprovalItem {
        tool_name: "Bash".to_string(),
        tool_input: serde_json::json!({"command": "ls"}),
        request_id: "req-1".to_string(),
    }]
}

// === ChannelBroker 测试 ===

#[tokio::test]
async fn test_channel_broker_无授权server全部reject() {
    // 无授权 server → 所有审批项返回 Reject
    let state = ChannelState::new();
    let sender = Arc::new(MockNotificationSender::default());
    let broker = ChannelBroker::new(state, sender);
    let ctx = InteractionContext::Approval {
        items: make_approval_items(),
    };
    let response = broker.request(ctx).await;
    match response {
        InteractionResponse::Decisions(decisions) => {
            assert_eq!(decisions.len(), 1);
            assert!(matches!(decisions[0], ApprovalDecision::Reject { .. }));
        }
        _ => panic!("期望 Decisions 响应"),
    }
}

#[tokio::test]
async fn test_channel_broker_有授权server发送通知() {
    // 有授权 server → 发送通知给该 server
    let state = ChannelState::new();
    state.authorize("server-1", "source".to_string());
    let sender = Arc::new(MockNotificationSender::default());
    let broker = ChannelBroker::new(state, sender.clone());
    let ctx = InteractionContext::Approval {
        items: make_approval_items(),
    };
    // 注意：此测试会在超时后返回 Reject（无响应者）
    let response = broker.request(ctx).await;
    match response {
        InteractionResponse::Decisions(decisions) => {
            assert_eq!(decisions.len(), 1);
            assert!(matches!(decisions[0], ApprovalDecision::Reject { .. }));
        }
        _ => panic!("期望 Decisions 响应"),
    }
    // 验证通知已发送
    assert!(sender.sent_count() > 0);
    assert!(sender.sent_to().contains(&"server-1".to_string()));
}
```

**注意**：测试中涉及 `ApprovalItem` 和 `ApprovalDecision` 的具体字段名，需根据 `mod.rs` 中的实际定义调整。如果 ChannelBroker 的 `request()` 使用了超时（5 分钟），需要将超时测试单独处理或使用较短的超时 mock。

- [ ] **Step 3: 运行测试验证通过**

Run: `cargo test -p peri-agent --lib -- interaction::channel_broker_test::test_channel_broker`
Expected: ALL PASS（超时测试可能需要调整）

- [ ] **Step 4: 提交**

```bash
git add peri-agent/src/interaction/channel_broker_test.rs
git commit -m "test: add ChannelBroker unit tests (no auth server, notification sending)"
```

---

### Task 3: 添加 MultiplexBroker 单元测试

**Files:**
- Create: `peri-agent/src/interaction/multiplex_test.rs`

- [ ] **Step 1: 读取 multiplex.rs 确认竞速逻辑**

Read `peri-agent/src/interaction/multiplex.rs` 确认：
- 空 broker 列表的返回值
- 单 broker 的快速路径
- 多 broker 竞速的首响应胜出逻辑

- [ ] **Step 2: 创建 multiplex_test.rs**

```rust
use super::*;
use super::multiplex::MultiplexBroker;
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::Mutex;

// MockBroker 用于测试 MultiplexBroker 竞速逻辑
struct MockBroker {
    response: Arc<Mutex<Option<InteractionResponse>>>,
    delay_ms: u64,
}

impl MockBroker {
    fn immediate(response: InteractionResponse) -> Self {
        Self {
            response: Arc::new(Mutex::new(Some(response))),
            delay_ms: 0,
        }
    }

    fn with_delay(response: InteractionResponse, delay_ms: u64) -> Self {
        Self {
            response: Arc::new(Mutex::new(Some(response))),
            delay_ms,
        }
    }

    fn never_responds() -> Self {
        Self {
            response: Arc::new(Mutex::new(None)),
            delay_ms: 0,
        }
    }
}

#[async_trait]
impl UserInteractionBroker for MockBroker {
    async fn request(&self, _ctx: InteractionContext) -> InteractionResponse {
        if self.delay_ms > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(self.delay_ms)).await;
        }
        loop {
            let guard = self.response.lock().await;
            if let Some(resp) = guard.as_ref() {
                // 这里需要 clone，但 InteractionResponse 可能不支持 Clone
                // 使用简单的测试用例避免 clone 问题
                return InteractionResponse::Decisions(vec![]);
            }
            drop(guard);
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    }
}

// === MultiplexBroker 测试 ===

#[tokio::test]
async fn test_multiplex_空broker列表返回空decisions() {
    // 空 broker → 空 Decisions
    let broker = MultiplexBroker::new(vec![]);
    let ctx = InteractionContext::Approval { items: vec![] };
    let response = broker.request(ctx).await;
    match response {
        InteractionResponse::Decisions(decisions) => {
            assert!(decisions.is_empty());
        }
        _ => panic!("期望 Decisions 响应"),
    }
}
```

**注意**：`MultiplexBroker` 的多 broker 竞速测试需要 `InteractionResponse` 支持 clone 或使用其他方式创建多个不同响应。具体实现需根据 `mod.rs` 中的 `InteractionResponse` 定义调整。

- [ ] **Step 3: 确保 mod.rs 中声明了 multiplex_test 模块**

检查 `peri-agent/src/interaction/mod.rs` 中是否已有 `#[cfg(test)] mod multiplex_test;`。如果没有，需要添加。

- [ ] **Step 4: 运行测试验证通过**

Run: `cargo test -p peri-agent --lib -- interaction::multiplex_test`
Expected: ALL PASS

- [ ] **Step 5: 提交**

```bash
git add peri-agent/src/interaction/multiplex_test.rs peri-agent/src/interaction/mod.rs
git commit -m "test: add MultiplexBroker unit tests (empty brokers)"
```

---

### Task 4: 运行全量测试确认无回归

- [ ] **Step 1: 运行 peri-agent 全量测试**

Run: `cargo test -p peri-agent --lib`
Expected: ALL PASS

- [ ] **Step 2: 运行 peri-acp 全量测试**

Run: `cargo test -p peri-acp --lib`
Expected: ALL PASS
