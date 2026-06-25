# ACP-TUI 耦合解耦 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 消除 ACP→TUI 同进程序列化往返 + 移除已死代码 `InteractionRequest`，降低 2 层间的耦合度。

**Architecture:** 
- Task 1-6 (Candidate A)：新增 `DirectEventSink` → 让 `ExecutorEvent` 不经 JSON 序列化直达 TUI pump，消除 `peri/agent_event` 的 serde 往返。同时保留 `TransportEventSink` 的 `session/update` 和 `peri/*` 通知路径（ACP 合规所需）。
- Task 7 (Candidate C1)：移除 TUI `AgentEvent::InteractionRequest` 变体及全部处理代码——该变体无任何生产代码发送，全部 HITL/AskUser 已走 ACP RPC 路径。

**Tech Stack:** Rust, tokio mpsc channels, peri-acp EventSink trait

---

## 前置验证

- [ ] **Step 0: 确认 InteractionRequest 无发送者**

```bash
# 搜索所有 .rs 文件中创建 InteractionRequest 的代码
cd /Users/konghayao/code/ai/perihelion
rg "InteractionRequest\s*\{" --glob '*.rs' -l | grep -v spec/ | grep -v _test
```

Expected: 仅 `peri-tui/src/app/events.rs`（定义）和 `peri-tui/src/app/agent_ops.rs`（匹配）。无生产代码构造该值并发送。

---

### Task 1: 新增 DirectEventSink

**Files:**
- Create: `peri-acp/src/session/event_sink_direct.rs`
- Modify: `peri-acp/src/session/mod.rs`

直接实现 `EventSink` trait：将 `ExecutorEvent` 直接发送到 `mpsc::UnboundedSender<ExecutorEvent>`，不经过 JSON。

- [ ] **Step 1: 创建 `event_sink_direct.rs`**

```rust
// peri-acp/src/session/event_sink_direct.rs

use async_trait::async_trait;
use peri_agent::agent::events::AgentEvent as ExecutorEvent;
use tokio::sync::mpsc;

use super::event_sink::EventSink;

/// [`EventSink`] that sends [`ExecutorEvent`]s directly through an mpsc channel,
/// bypassing JSON serialization. Used by the TUI in-process path alongside
/// [`TransportEventSink`] (which handles `session/update` and `peri/*` notifications).
pub struct DirectEventSink {
    tx: mpsc::UnboundedSender<ExecutorEvent>,
}

impl DirectEventSink {
    pub fn new(tx: mpsc::UnboundedSender<ExecutorEvent>) -> Self {
        Self { tx }
    }
}

#[async_trait]
impl EventSink for DirectEventSink {
    async fn push_event(&self, _session_id: &str, event: &ExecutorEvent, _context_window: u32) {
        let _ = self.tx.send(event.clone());
    }

    async fn push_done(&self, _session_id: &str) {
        // Channel close signals done to the consumer.
    }
}
```

- [ ] **Step 2: 注册模块**

在 `peri-acp/src/session/mod.rs` 添加：

```rust
pub mod event_sink_direct;
```

- [ ] **Step 3: 构建验证**

```bash
cargo build -p peri-acp 2>&1
```

Expected: 编译通过。

- [ ] **Step 4: Commit**

```bash
git add peri-acp/src/session/event_sink_direct.rs peri-acp/src/session/mod.rs
git commit -m "feat(acp): add DirectEventSink for same-process ExecutorEvent delivery

Eliminates JSON serialization round-trip when TUI and ACP server
run in the same process. Complement to TransportEventSink which
continues handling session/update and peri/* notifications.

Co-Authored-By: deepseek-v4-pro <deepseek-ai@claude-code-best.win>"
```

---

### Task 2: TransportEventSink 支持可选 DirectTx

**Files:**
- Modify: `peri-acp/src/session/event_sink.rs`

让 `TransportEventSink` 保留 `session/update` 和 `peri/*` 通知，但当 `direct_tx` 为 `Some` 时，`peri/agent_event` 改为走直连通道。

- [ ] **Step 1: 添加 `direct_tx` 字段和构造函数**

修改 `TransportEventSink` 定义（约 line 36-44）：

```rust
pub struct TransportEventSink {
    transport: std::sync::Arc<dyn AcpTransport>,
    /// Optional direct channel for same-process event delivery.
    /// When set, `peri/agent_event` notifications bypass JSON serialization.
    direct_tx: Option<mpsc::UnboundedSender<ExecutorEvent>>,
}

impl TransportEventSink {
    pub fn new(transport: std::sync::Arc<dyn AcpTransport>) -> Self {
        Self {
            transport,
            direct_tx: None,
        }
    }

    /// Create with a direct event channel for same-process delivery.
    pub fn with_direct(
        transport: std::sync::Arc<dyn AcpTransport>,
        direct_tx: mpsc::UnboundedSender<ExecutorEvent>,
    ) -> Self {
        Self {
            transport,
            direct_tx: Some(direct_tx),
        }
    }
}
```

需要在上方添加 `use tokio::sync::mpsc;` 和 `use peri_agent::agent::events::AgentEvent as ExecutorEvent;`（后者已在第 8 行）。

- [ ] **Step 2: 修改 `push_event` 方法的 `peri/agent_event` 部分**

将 method body（约 line 48-97）中 `peri/agent_event` 的序列化+发送逻辑改为条件分支：

```rust
impl EventSink for TransportEventSink {
    async fn push_event(&self, session_id: &str, event: &ExecutorEvent, context_window: u32) {
        // 1. peri/agent_event — use direct channel if available, else JSON
        if let Some(ref tx) = self.direct_tx {
            let _ = tx.send(event.clone());
        } else {
            let event_value = match serde_json::to_value(event) {
                Ok(v) => v,
                Err(e) => {
                    error!(error = %e, "EventSink: serialize ExecutorEvent failed");
                    return;
                }
            };
            let agent_event_params = json!({
                "sessionId": session_id,
                "event": event_value,
            });
            if let Err(e) = self
                .transport
                .send_notification("peri/agent_event", agent_event_params)
                .await
            {
                error!(error = %e, "EventSink: send peri/agent_event failed");
                return;
            }
        }

        // 2. peri/* custom notifications (compact, session lifecycle)
        let peri_notifs = map_executor_to_peri_notifications(event);
        for (method, mut payload) in peri_notifs {
            if let serde_json::Value::Object(ref mut map) = payload {
                map.insert("sessionId".to_string(), json!(session_id));
            }
            let _ = self.transport.send_notification(method, payload).await;
        }

        // 3. session/update — standard ACP SessionUpdate
        let updates = map_executor_to_updates(event, context_window);
        for update in updates {
            // ... (保持不变)
        }
    }

    // push_done 保持不变
}
```

- [ ] **Step 3: 构建验证**

```bash
cargo build -p peri-acp 2>&1
```

Expected: 编译通过。

- [ ] **Step 4: Commit**

```bash
git add peri-acp/src/session/event_sink.rs
git commit -m "feat(acp): TransportEventSink supports optional DirectEventSink bypass

When direct_tx is Some, peri/agent_event bypasses JSON serialization.
session/update and peri/* notifications still go through transport for
ACP protocol compliance.

Co-Authored-By: deepseek-v4-pro <deepseek-ai@claude-code-best.win>"
```

---

### Task 3: AcpTuiClient 新增直接事件接收通道

**Files:**
- Modify: `peri-tui/src/acp_client/client.rs`

让 `AcpTuiClient` 持有 `direct_event_rx: Option<mpsc::UnboundedReceiver<ExecutorEvent>>`，在 pump 中与 transport 并行轮询。

- [ ] **Step 1: 添加字段和构造函数修改**

在 `AcpTuiClient` struct（约 line 46-50）添加字段：

```rust
pub struct AcpTuiClient {
    transport: Arc<MpscClientTransport>,
    notification_tx: mpsc::UnboundedSender<AcpNotification>,
    current_session_id: Arc<Mutex<Option<String>>>,
    /// Receives ExecutorEvent directly (bypasses JSON serialization).
    /// Set via set_direct_event_rx() after construction.
    direct_event_rx: Arc<Mutex<Option<mpsc::UnboundedReceiver<ExecutorEvent>>>>,
}
```

通过 `use peri_agent::agent::events::AgentEvent as ExecutorEvent;` 引入。

修改 `new()` 函数：

```rust
pub fn new(transport: MpscClientTransport) -> (Self, mpsc::UnboundedReceiver<AcpNotification>) {
    let (notification_tx, notification_rx) = mpsc::unbounded_channel();
    let client = Self {
        transport: Arc::new(transport),
        notification_tx,
        current_session_id: Arc::new(Mutex::new(None)),
        direct_event_rx: Arc::new(Mutex::new(None)),
    };
    (client, notification_rx)
}
```

- [ ] **Step 2: 添加 `set_direct_event_rx` 方法**

```rust
impl AcpTuiClient {
    /// Wire a direct ExecutorEvent receiver channel.
    /// When set, the pump reads from this channel in parallel with the transport.
    pub fn set_direct_event_rx(&self, rx: mpsc::UnboundedReceiver<ExecutorEvent>) {
        *self.direct_event_rx.lock().unwrap() = Some(rx);
    }
```

- [ ] **Step 3: 修改 `spawn_pump` 传参**

```rust
pub fn spawn_pump(&self) {
    let transport = self.transport.clone();
    let notification_tx = self.notification_tx.clone();
    let direct_event_rx = self.direct_event_rx.clone();
    tokio::spawn(async move {
        Self::run_pump(transport, notification_tx, direct_event_rx).await;
    });
}
```

- [ ] **Step 4: 修改 `run_pump` 使用 `tokio::select!`**

将 `run_pump` 签名为：

```rust
async fn run_pump(
    transport: Arc<MpscClientTransport>,
    notification_tx: mpsc::UnboundedSender<AcpNotification>,
    direct_event_rx: Arc<Mutex<Option<mpsc::UnboundedReceiver<ExecutorEvent>>>>,
) {
    let mut event_count: u64 = 0;
    // Take the receiver out once
    let mut direct_rx = direct_event_rx.lock().unwrap().take();
    loop {
        let msg = {
            if let Some(ref mut rx) = direct_rx {
                tokio::select! {
                    // Direct events get priority (non-blocking try_recv in select)
                    Some(event) = rx.recv() => {
                        event_count += 1;
                        Some(AcpNotification::AgentEvent {
                            session_id: String::new(), // session_id not needed for direct path
                            event,
                        })
                    }
                    msg = transport.recv() => {
                        msg.map(|m| Self::dispatch_transport_msg(m, &mut event_count))
                            .flatten()
                    }
                }
            } else {
                // No direct channel — transport only
                let msg = transport.recv().await;
                msg.map(|m| Self::dispatch_transport_msg(m, &mut event_count))
                    .flatten()
            }
        };
        match msg {
            Some(notif) => {
                let _ = notification_tx.send(notif);
            }
            None => {
                debug!("ACP client pump: exiting");
                break;
            }
        }
    }
}
```

需要将现有的 `run_pump` body 中 `transport.recv()` → dispatch 的逻辑提取为辅助函数 `dispatch_transport_msg`：

```rust
fn dispatch_transport_msg(msg: IncomingMessage, event_count: &mut u64) -> Option<AcpNotification> {
    match msg {
        IncomingMessage::Notification { method, params } => {
            if method == "peri/agent_event" {
                *event_count += 1;
                let session_id = params
                    .get("sessionId")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                if let Some(event_value) = params.get("event") {
                    match serde_json::from_value::<ExecutorEvent>(event_value.clone()) {
                        Ok(event) => {
                            debug!(event_count = event_count, session_id = %session_id, "agent_event via transport");
                            Some(AcpNotification::AgentEvent { session_id, event })
                        }
                        Err(e) => {
                            error!(event_count = event_count, error = %e, "failed to parse AgentEvent");
                            Some(AcpNotification::Other {
                                msg: format!("failed to parse AgentEvent: {e}"),
                            })
                        }
                    }
                } else {
                    None
                }
            } else if method == "session/update" {
                // ... (existing code, extract to match arm)
            } else if method == "peri/agent_event_done" {
                // ... (existing code)
            } else if method.starts_with("notifications/peri/") {
                // ... (existing code)
            } else {
                // ... (existing code)
            }
        }
        IncomingMessage::Request { id, method, params } => {
            // ... (existing code)
        }
        IncomingMessage::Response { .. } => None,
    }
}
```

- [ ] **Step 5: 构建验证**

```bash
cargo build -p peri-tui 2>&1
```

需确保 `ExecutorEvent` 在 `client.rs` 的作用域内可用（添加 `use`）。

- [ ] **Step 6: Commit**

```bash
git add peri-tui/src/acp_client/client.rs
git commit -m "feat(tui): AcpTuiClient supports direct ExecutorEvent channel

Added direct_event_rx to bypass JSON deserialization when DirectEventSink
is wired. The pump uses tokio::select! to poll both the direct channel
and transport simultaneously.

Co-Authored-By: deepseek-v4-pro <deepseek-ai@claude-code-best.win>"
```

---

### Task 4: acp_server 和 main.rs 布线 DirectEventSink

**Files:**
- Modify: `peri-tui/src/main.rs`
- Modify: `peri-tui/src/acp_server.rs` (调整注释，无需代码变更——布线在 main.rs)

在 `main.rs` 中创建 `DirectEventSink` 的 channel pair，将 rx 端传给 `AcpTuiClient`，tx 端传给 `TransportEventSink::with_direct()`。

- [ ] **Step 1: 创建 direct 通道并布线**

在 `main.rs` 的 ACP 初始化部分（约 line 829-841），当前代码：

```rust
let (client_transport, server_transport) = mpsc_transport_pair();
tokio::spawn(async move {
    run_acp_server(Arc::new(server_transport), server_config).await;
});

let (acp_client, notification_rx) = AcpTuiClient::new(client_transport);
acp_client.spawn_pump();
app.session_mgr.sessions[app.session_mgr.active]
    .agent
    .acp_notification_rx = Some(notification_rx);
app.acp_client = Some(acp_client);
```

修改为：

```rust
use peri_acp::session::event_sink_direct::DirectEventSink;

let (client_transport, server_transport) = mpsc_transport_pair();

// 创建 DirectEventSink 通道：ExecutorEvent 不经 JSON 直达 TUI pump
let (direct_tx, direct_rx) = tokio::sync::mpsc::unbounded_channel::<ExecutorEvent>();

tokio::spawn(async move {
    // TransportEventSink 负责 session/update 和 peri/* 通知
    // DirectEventSink 负责 peri/agent_event 直连
    run_acp_server(
        Arc::new(server_transport),
        server_config,
        Some(direct_tx),  // new parameter
    ).await;
});

let (acp_client, notification_rx) = AcpTuiClient::new(client_transport);
// 将 direct_rx 注入 pump，使其与 transport 并行轮询
acp_client.set_direct_event_rx(direct_rx);
acp_client.spawn_pump();
app.session_mgr.sessions[app.session_mgr.active]
    .agent
    .acp_notification_rx = Some(notification_rx);
app.acp_client = Some(acp_client);
```

需要导入 `use peri_agent::agent::events::AgentEvent as ExecutorEvent;`。

- [ ] **Step 2: 修改 `run_acp_server` 签名**

在 `peri-tui/src/acp_server.rs` 中，修改 `run_acp_server` 接受 `direct_tx`：

```rust
pub async fn run_acp_server(
    transport: Arc<dyn peri_acp::transport::AcpTransport>,
    cfg: AcpServerConfig,
    direct_tx: Option<tokio::sync::mpsc::UnboundedSender<
        peri_agent::agent::events::AgentEvent,
    >>,
) {
```

在 `execute_prompt` 内部函数中，修改 `event_sink` 的创建（约 line 389）：

```rust
let event_sink: Arc<dyn peri_acp::session::event_sink::EventSink> = 
    if let Some(dt) = direct_tx {
        Arc::new(TransportEventSink::with_direct(Arc::clone(transport), dt))
    } else {
        Arc::new(TransportEventSink::new(Arc::clone(transport)))
    };
```

- [ ] **Step 3: 构建验证**

```bash
cargo build -p peri-tui 2>&1
```

检查是否有未使用的 import 或编译错误。

- [ ] **Step 4: Commit**

```bash
git add peri-tui/src/main.rs peri-tui/src/acp_server.rs
git commit -m "feat(tui): wire DirectEventSink into ACP server→TUI pump pipeline

ExecutorEvent now bypasses JSON serialization for same-process TUI path.
DirectEventSink channel is created alongside MpscTransport and connected
to AcpTuiClient's direct_event_rx.

Co-Authored-By: deepseek-v4-pro <deepseek-ai@claude-code-best.win>"
```

---

### Task 5: 清理 client.rs 中已无用的 peri/agent_event JSON 反序列化

**Files:**
- Modify: `peri-tui/src/acp_client/client.rs`

由于 `TransportEventSink.with_direct()` 不再发送 `peri/agent_event` 通知，`dispatch_transport_msg` 中的 `peri/agent_event` 反序列化分支成为死代码。保留以防回退，但在 direct 模式下不应触发。

- [ ] **Step 1: 添加守卫日志**

在 `dispatch_transport_msg` 的 `peri/agent_event` 匹配分支开头添加告警日志：

```rust
if method == "peri/agent_event" {
    warn!("peri/agent_event received via transport (expected via direct channel) — deserializing anyway");
    // ... (existing code)
}
```

这不会改变行为，但在控制流意外到达此处时给出可见信号。

- [ ] **Step 2: 构建验证**

```bash
cargo build -p peri-tui 2>&1
```

- [ ] **Step 3: Commit**

```bash
git add peri-tui/src/acp_client/client.rs
git commit -m "chore(tui): add warn log for legacy peri/agent_event transport path

Co-Authored-By: deepseek-v4-pro <deepseek-ai@claude-code-best.win>"
```

---

### Task 6: 端到端功能测试

**Files:**
- 无新文件（测试在现有 headless 框架下运行）

- [ ] **Step 1: 全量测试**

```bash
cargo test -p peri-tui --lib 2>&1
```

Expected: 所有现有测试通过。

- [ ] **Step 2: 编译全 workspace**

```bash
cargo build 2>&1
```

Expected: 全部 crate 编译通过。

- [ ] **Step 3: 运行 clippy**

```bash
cargo clippy --all-targets 2>&1 | head -50
```

Expected: 无新增 warning。

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "test: verify all tests pass after DirectEventSink integration

Co-Authored-By: deepseek-v4-pro <deepseek-ai@claude-code-best.win>"
```

---

### Task 7: 移除死代码 InteractionRequest

**Files:**
- Modify: `peri-tui/src/app/events.rs`
- Modify: `peri-tui/src/app/agent_ops.rs`
- Modify: `peri-tui/src/app/message_pipeline/mod.rs`

`AgentEvent::InteractionRequest` 无任何生产代码发送——所有 HITL/AskUser 已通过 ACP RPC 路径（`AcpTransportBroker` → `session/request_permission` / `elicitation/create` → `AcpNotification::RequestPermission` / `Elicitation` → `handle_acp_request_permission` / `handle_acp_elicitation`）。

- [ ] **Step 1: 从 AgentEvent 枚举删除 InteractionRequest 变体**

修改 `peri-tui/src/app/events.rs`（约 line 37-40）：

删除：
```rust
    /// 统一人机交互请求（HITL 审批 / AskUser 问答）
    InteractionRequest {
        ctx: InteractionContext,
        response_tx: oneshot::Sender<InteractionResponse>,
    },
```

同时删除文件顶部的两个 `use`：
```rust
use peri_agent::interaction::{InteractionContext, InteractionResponse};
use tokio::sync::oneshot;
```

检查：`oneshot` 是否被其他变体使用（`OAuthAuthorizationNeeded` 使用了 `callback_tx: oneshot::Sender<OAuthCallbackResult>`，但 `oneshot` 已在 scope 中通过其他 use 引入）。删除 `InteractionContext` 和 `InteractionResponse` 两个 use 不影响其他代码——这些类型是 `InteractionRequest` 专用的。

- [ ] **Step 2: 从 handle_agent_event 删除匹配分支**

修改 `peri-tui/src/app/agent_ops.rs`，删除约 line 978-1076 的整个 `AgentEvent::InteractionRequest { ctx, response_tx } => { ... }` 匹配分支。

确认：该分支内部的 `use` 语句 (`use peri_agent::interaction::{...}`) 在删除后可能影响外侧作用域——检查文件顶部是否已有相同 import。当前 `agent_ops.rs:1-5` 已有 `use peri_middlewares::hitl::BatchItem;`，`use tokio::sync::oneshot;` 出现在 `handle_acp_request_permission`（line 62）中，不受影响。

删除后确保 `match event { ... }` 块完整闭合。

- [ ] **Step 3: 从 message_pipeline 删除匹配**

修改 `peri-tui/src/app/message_pipeline/mod.rs`（约 line 360）：

将：
```rust
            AgentEvent::Error(_)
            | AgentEvent::InteractionRequest { .. }
            | AgentEvent::TodoUpdate(_)
```

改为：
```rust
            AgentEvent::Error(_)
            | AgentEvent::TodoUpdate(_)
```

- [ ] **Step 4: 构建验证**

```bash
cargo build -p peri-tui 2>&1
```

Expected: 编译通过，无 unused import 警告。

- [ ] **Step 5: 全量测试**

```bash
cargo test -p peri-tui --lib 2>&1
```

- [ ] **Step 6: Commit**

```bash
git add peri-tui/src/app/events.rs peri-tui/src/app/agent_ops.rs peri-tui/src/app/message_pipeline/mod.rs
git commit -m "refactor(tui): remove dead InteractionRequest event variant

All HITL/AskUser interactions now go through ACP RPC path
(AcpTransportBroker → RequestPermission/Elicitation). The legacy
InteractionRequest event was never sent by any production code.

Co-Authored-By: deepseek-v4-pro <deepseek-ai@claude-code-best.win>"
```

---

### Task 8: 最终验证

- [ ] **Step 1: 全 workspace 编译+测试**

```bash
cargo build --workspace 2>&1
cargo test --workspace --lib 2>&1
```

- [ ] **Step 2: Clippy 无新增 warning**

```bash
cargo clippy --workspace --all-targets 2>&1 | grep -c "warning"
```

- [ ] **Step 3: Pre-commit hooks**

```bash
lefthook run pre-commit 2>&1
```

- [ ] **Step 4: Commit (如有需要)**

```bash
git add -A
git commit -m "chore: final verification after ACP-TUI decoupling

Co-Authored-By: deepseek-v4-pro <deepseek-ai@claude-code-best.win>"
```
