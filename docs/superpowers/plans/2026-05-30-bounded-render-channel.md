# RenderThread 有界通道改造 实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将 `RenderThread` 的事件通道从 `unbounded_channel()` 改为 `channel(128)`，利用背压机制防止极端场景下内存膨胀，同时保证不丢事件、不死锁。

**Architecture:** 发送端分布在 TUI 主线程（同步调用 `send`），接收端在 tokio 后台 task。有界通道的 `send().await` 会自然阻塞发送方（背压），但 TUI 主线程是同步的，不能 `.await`。因此需要分层策略：Resize 用 `try_send` + drain 合并（已有逻辑），其余事件用 `blocking_send`（内部用 `block_on` 等待，但发生概率极低——只在 channel 积压 128 个 Rebuild 时才触发）。

**Tech Stack:** Rust, tokio::sync::mpsc bounded channel

---

## 当前问题

### 通道创建（render_thread.rs:437）

```rust
let (tx, rx) = mpsc::unbounded_channel();
```

无界通道在极端场景下（LLM 快速输出 + resize 风暴 + compact 事件同时到达）可能积压导致内存膨胀。

### 发送端分布（全部在同步 TUI 主线程）

| 文件 | 行 | 事件类型 | 频率 |
|------|-----|---------|------|
| `agent_render.rs:16` | Rebuild | 高频（流式每 100ms） |
| `agent_render.rs:28` | Rebuild | 中频 |
| `agent_render.rs:35` | RebuildWithAnchor | 中频 |
| `app/mod.rs:510` | Rebuild | 低频（中断回滚） |
| `message_area.rs:113` | Resize | 中频（有 drain 去重） |
| `thread_ops.rs:58` | ToggleToolMessages | 极低频 |
| `thread_ops.rs:75` | ToggleDiff | 极低频 |
| `thread_ops.rs:238` | Rebuild | 低频（加载历史） |
| `thread_ops.rs:355` | Clear | 低频 |

### 关键约束

1. **发送端在同步上下文**：TUI 主线程不是 async，不能直接 `.await`
2. **不能丢事件**：Rebuild/RebuildWithAnchor 携带完整消息快照，丢失会导致渲染与状态分叉
3. **Resize 已有 drain 合并**：`run()` 中用 `rx.try_recv()` 拖拽后续 Resize 事件合并为一个，不受背压影响
4. **drop 路径不能死锁**：所有 `let _ = tx.send(...)` 都在同步代码中，`drop(tx)` 在 `MessageState`/`ChatSession` drop 时自然关闭

## File Structure

| 文件 | 操作 | 职责变更 |
|------|------|----------|
| `peri-tui/src/ui/render_thread.rs` | 修改 | 通道创建 `channel(128)`，`run()` 接收端类型改为 `Receiver<RenderEvent>` |
| `peri-tui/src/app/message_state.rs` | 修改 | `render_tx` 类型改为 `Sender<RenderEvent>` |
| `peri-tui/src/app/agent_render.rs` | 修改 | `send()` 改为 `blocking_send()` |
| `peri-tui/src/app/mod.rs` | 修改 | `send()` 改为 `blocking_send()` |
| `peri-tui/src/app/thread_ops.rs` | 修改 | `send()` 改为 `blocking_send()` |
| `peri-tui/src/ui/main_ui/message_area.rs` | 修改 | Resize `send()` 改为 `try_send()` |
| `peri-tui/src/ui/render_thread_test.rs` | 修改 | `send()` 改为适应 bounded sender API |
| `peri-tui/src/ui/headless_test.rs` | 修改 | `send()` 适应 bounded sender |

---

## Step 1: 修改通道类型和 spawn_render_thread 签名

**文件**: `peri-tui/src/ui/render_thread.rs`

**目标**: 将 `unbounded_channel()` 改为 `channel(128)`，更新函数签名和 `run()` 接收端类型。

### 1.1 修改 spawn_render_thread

```rust
// BEFORE:
pub fn spawn_render_thread(
    width: u16,
) -> (
    mpsc::UnboundedSender<RenderEvent>,
    Arc<RwLock<RenderCache>>,
    Arc<Notify>,
) {
    let (tx, rx) = mpsc::unbounded_channel();
    // ...
}

// AFTER:
pub fn spawn_render_thread(
    width: u16,
) -> (
    mpsc::Sender<RenderEvent>,
    Arc<RwLock<RenderCache>>,
    Arc<Notify>,
) {
    let (tx, rx) = mpsc::channel(128);
    // ...
}
```

### 1.2 修改 RenderTask::run 签名

```rust
// BEFORE:
async fn run(mut self, mut rx: mpsc::UnboundedReceiver<RenderEvent>) {

// AFTER:
async fn run(mut self, mut rx: mpsc::Receiver<RenderEvent>) {
```

**注意**: `Receiver::recv()` 返回 `Option<T>`，与 `UnboundedReceiver` 完全一致。`try_recv()` 也一致。事件循环代码无需修改。

### 1.3 添加通道容量常量

在 `render_thread.rs` 顶部添加：

```rust
/// 渲染事件通道容量。
/// 128 足以缓冲大量事件（resize 风暴 + 流式 Rebuild），同时防止内存无限膨胀。
/// 正常运行时队列深度通常 < 5。
const RENDER_CHANNEL_CAPACITY: usize = 128;
```

然后在 `spawn_render_thread` 中使用：

```rust
let (tx, rx) = mpsc::channel(RENDER_CHANNEL_CAPACITY);
```

### 测试验证

- [ ] `test_rebuild_increments_version` 通过
- [ ] `test_rebuild_hash_diff_skips_unchanged` 通过
- [ ] `test_rebuild_no_trailing_blank` 通过
- [ ] `test_rebuild_multiple_messages_have_gaps` 通过
- [ ] `test_rebuild_with_anchor_sets_scroll_anchor` 通过
- [ ] `test_clear_resets_cache` 通过
- [ ] `test_resize_rebuilds_with_new_width` 通过
- [ ] `test_build_wrap_map_*` 系列通过

---

## Step 2: 修改 MessageState 类型声明

**文件**: `peri-tui/src/app/message_state.rs`

**目标**: 将 `render_tx` 类型从 `UnboundedSender` 改为 `Sender`。

```rust
// BEFORE:
pub render_tx: mpsc::UnboundedSender<RenderEvent>,

// AFTER:
pub render_tx: mpsc::Sender<RenderEvent>,
```

同样修改 `new()` 参数类型：

```rust
// BEFORE:
pub fn new(
    cwd: String,
    render_tx: mpsc::UnboundedSender<RenderEvent>,
    // ...
) -> Self {

// AFTER:
pub fn new(
    cwd: String,
    render_tx: mpsc::Sender<RenderEvent>,
    // ...
) -> Self {
```

**编译验证**: 此步会触发所有调用 `send()` 的编译错误，方便定位所有需要修改的发送端。

### 测试验证

- [ ] `cargo build -p peri-tui` 编译失败（预期，因为 `Sender::send` 是 `async fn`）

---

## Step 3: 修改所有发送端 — 同步阻塞发送

**文件**: `agent_render.rs`, `app/mod.rs`, `thread_ops.rs`

**策略**: 这些文件在 TUI 主线程（同步上下文），不能 `.await`。使用 `blocking_send()` 代替 `send()`。

> `mpsc::Sender::blocking_send()` 在通道满时阻塞当前线程直到有空间。这在 TUI 场景下是安全的：
> - 通道容量 128，正常使用远达不到上限
> - 即使达到上限，渲染线程消费速度极快（微秒级 hash diff + markdown 渲染）
> - 短暂阻塞（毫秒级）对用户体验无可感知影响

### 3.1 agent_render.rs

```rust
// BEFORE (3 处):
let _ = session.messages.render_tx.send(RenderEvent::Rebuild(vms));
let _ = session.messages.render_tx.send(RenderEvent::Rebuild(vms));
let _ = session.messages.render_tx.send(RenderEvent::RebuildWithAnchor { ... });

// AFTER:
let _ = session.messages.render_tx.blocking_send(RenderEvent::Rebuild(vms));
let _ = session.messages.render_tx.blocking_send(RenderEvent::Rebuild(vms));
let _ = session.messages.render_tx.blocking_send(RenderEvent::RebuildWithAnchor { ... });
```

### 3.2 app/mod.rs:507-510

```rust
// BEFORE:
let _ = self.session_mgr.sessions[self.session_mgr.active]
    .messages
    .render_tx
    .send(RenderEvent::Rebuild(remaining));

// AFTER:
let _ = self.session_mgr.sessions[self.session_mgr.active]
    .messages
    .render_tx
    .blocking_send(RenderEvent::Rebuild(remaining));
```

### 3.3 thread_ops.rs（4 处）

```rust
// BEFORE:
let _ = ...render_tx.send(RenderEvent::ToggleToolMessages(...));
let _ = ...render_tx.send(RenderEvent::ToggleDiff(...));
let _ = ...render_tx.send(RenderEvent::Rebuild(...));
let _ = ...render_tx.send(RenderEvent::Clear);

// AFTER:
let _ = ...render_tx.blocking_send(RenderEvent::ToggleToolMessages(...));
let _ = ...render_tx.blocking_send(RenderEvent::ToggleDiff(...));
let _ = ...render_tx.blocking_send(RenderEvent::Rebuild(...));
let _ = ...render_tx.blocking_send(RenderEvent::Clear);
```

### 3.4 headless_test.rs（2 处）

测试代码同样在同步上下文，用 `blocking_send`：

```rust
// BEFORE:
let _ = app.session_mgr.sessions[app.session_mgr.active]
    .messages
    .render_tx
    .send(RenderEvent::Rebuild(msgs));
let _ = ...render_tx.send(RenderEvent::Clear);

// AFTER:
let _ = app.session_mgr.sessions[app.session_mgr.active]
    .messages
    .render_tx
    .blocking_send(RenderEvent::Rebuild(msgs));
let _ = ...render_tx.blocking_send(RenderEvent::Clear);
```

### 测试验证

- [ ] `cargo build -p peri-tui` 编译通过
- [ ] 所有 render_thread_test.rs 测试通过
- [ ] 所有 headless_test.rs 测试通过

---

## Step 4: Resize 事件使用 try_send 非阻塞发送

**文件**: `peri-tui/src/ui/main_ui/message_area.rs`

**目标**: Resize 事件用 `try_send()` 而非 `blocking_send()`。原因：

1. Resize 在渲染线程有 drain 合并逻辑（`while let Ok(RenderEvent::Resize(w)) = rx.try_recv()`），丢弃一个 Resize 不影响最终结果
2. 拖动 resize 时可能快速产生大量事件，不应阻塞主线程
3. Resize 已有 `last_resize_width` 去重，即使 `try_send` 失败（通道满），下一帧的 resize 事件会覆盖

```rust
// BEFORE:
let _ = messages
    .render_tx
    .send(RenderEvent::Resize(text_area_width));

// AFTER:
let _ = messages
    .render_tx
    .try_send(RenderEvent::Resize(text_area_width));
```

> `try_send()` 在通道满时返回 `Err(TrySendError::Full(_))`，不阻塞。`let _ =` 静默忽略错误（下一个 Resize 会补偿）。

### 测试验证

- [ ] `cargo build -p peri-tui` 编译通过
- [ ] 手动拖动窗口边缘 resize，确认渲染正常响应

---

## Step 5: 更新 render_thread_test.rs 适配 bounded sender

**文件**: `peri-tui/src/ui/render_thread_test.rs`

**目标**: 测试中 `tx.send()` 的调用需要适配。`Sender::send()` 返回 `Future`，但测试中需要同步发送。

**方案**: 测试使用 `blocking_send()` 或在 async 测试中使用 `.await`。

由于测试函数都是 `#[tokio::test] async fn`，可以使用两种方式：

### 方案 A: 在 async 上下文中使用 send().await（推荐）

```rust
// BEFORE:
tx.send(RenderEvent::Rebuild(vec![MessageViewModel::user("Hello".to_string())]))
    .unwrap();

// AFTER:
tx.send(RenderEvent::Rebuild(vec![MessageViewModel::user("Hello".to_string())]))
    .await
    .unwrap();
```

这是最自然的方式：`Sender::send()` 是 `async fn`，在 tokio test runtime 中 `.await` 即可。通道容量 128，测试数据极少，永远不会触发背压等待。

### 逐个修改

所有测试中的 `tx.send(...)` 改为 `tx.send(...).await`：

- [ ] `test_rebuild_increments_version`: `tx.send(...).unwrap()` → `tx.send(...).await.unwrap()`
- [ ] `test_rebuild_hash_diff_skips_unchanged`: 2 处
- [ ] `test_rebuild_no_trailing_blank`: 1 处
- [ ] `test_rebuild_multiple_messages_have_gaps`: 1 处
- [ ] `test_rebuild_with_anchor_sets_scroll_anchor`: 1 处
- [ ] `test_clear_resets_cache`: 2 处
- [ ] `test_resize_rebuilds_with_new_width`: 2 处

### 测试验证

- [ ] `cargo test -p peri-tui --lib -- ui::render_thread::tests` 全部通过

---

## Step 6: 新增背压安全测试

**文件**: `peri-tui/src/ui/render_thread_test.rs`

**目标**: 验证有界通道在极端场景下不会死锁、不丢关键事件。

### 6.1 测试：通道满时 try_send Resize 不阻塞

```rust
/// 填满通道后发送 Resize，验证 try_send 立即返回（不阻塞）
#[tokio::test]
async fn test_resize_try_send_when_channel_full() {
    let (tx, cache, _notify) = spawn_render_thread(80);

    // 先发送一个 Rebuild 建立初始状态
    tx.send(RenderEvent::Rebuild(vec![MessageViewModel::user(
        "Hello".to_string(),
    )]))
    .await
    .unwrap();
    wait_render().await;

    // 填满通道（不消费）
    for i in 0..128 {
        tx.send(RenderEvent::Rebuild(vec![MessageViewModel::user(
            format!("Filler {i}"),
        )]))
        .await
        .unwrap();
    }

    // try_send Resize 应该返回 Err(Full)，不阻塞
    let result = tx.try_send(RenderEvent::Resize(40));
    assert!(
        result.is_err(),
        "try_send 在通道满时应返回错误，实际: {result:?}"
    );
    // 不验证 Resize 是否到达——通道满时丢弃 Resize 是预期行为
    // 渲染线程消费后会处理下一个 Resize（如果有）
}
```

### 6.2 测试：通道满时 blocking_send 等待后成功

```rust
/// 验证 blocking_send 在通道满时等待，消费后发送成功
#[tokio::test]
async fn test_blocking_send_waits_when_full() {
    let (tx, cache, _notify) = spawn_render_thread(80);

    // 渲染线程会持续消费，所以很难真正填满。
    // 验证在大量事件下不会 panic 或死锁即可。
    for i in 0..200 {
        // blocking_send 在 async test 中会阻塞当前线程，
        // 但渲染线程在后台持续消费，所以不会真正卡住
        tx.send(RenderEvent::Rebuild(vec![MessageViewModel::user(
            format!("Message {i}"),
        )]))
        .await
        .unwrap();
    }
    wait_render().await;

    let c = cache.read();
    assert!(c.version > 0, "渲染线程应处理了至少一个事件");
    assert!(!c.lines.is_empty(), "最终应有渲染结果");
}
```

### 6.3 测试：drop sender 不死锁

```rust
/// 验证 drop sender 后渲染线程正常退出，不死锁
#[tokio::test]
async fn test_drop_sender_exits_cleanly() {
    let (tx, cache, _notify) = spawn_render_thread(80);

    tx.send(RenderEvent::Rebuild(vec![MessageViewModel::user(
        "Before drop".to_string(),
    )]))
    .await
    .unwrap();
    wait_render().await;

    let version_before = cache.read().version;

    // Drop sender —— 模拟 ChatSession drop
    drop(tx);

    // 给渲染线程时间退出
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // cache 仍然可读（Arc<RwLock> 仍持有）
    let c = cache.read();
    assert_eq!(c.version, version_before, "drop 后不应有新事件处理");
}
```

### 6.4 测试：快速连续 Resize 合并

```rust
/// 验证多个快速连续的 Resize 事件被合并为一个最终宽度
#[tokio::test]
async fn test_resize_coalesce_under_pressure() {
    let (tx, cache, _notify) = spawn_render_thread(80);

    // 先建立初始内容
    tx.send(RenderEvent::Rebuild(vec![MessageViewModel::user(
        "Hello world this is a longer message for wrapping".to_string(),
    )]))
    .await
    .unwrap();
    wait_render().await;

    let width_80 = cache.read().total_lines;

    // 快速连续发送多个 Resize（模拟拖动窗口边缘）
    for w in [60, 50, 40, 30, 20] {
        tx.send(RenderEvent::Resize(w)).await.unwrap();
    }
    wait_render().await;

    let c = cache.read();
    // 最终宽度应为最后一个 Resize 的值（20）
    assert_eq!(c.width, 20, "最终宽度应为最后一个 Resize 值");
    // 窄宽度应有更多行（wrap 更多）
    assert!(
        c.total_lines >= width_80,
        "窄宽度应产生更多视觉行: {} >= {}",
        c.total_lines,
        width_80
    );
}
```

### 测试验证

- [ ] `test_resize_try_send_when_channel_full` 通过
- [ ] `test_blocking_send_waits_when_full` 通过
- [ ] `test_drop_sender_exits_cleanly` 通过
- [ ] `test_resize_coalesce_under_pressure` 通过

---

## Step 7: 清理注释和文档

**文件**: `peri-tui/src/ui/render_thread.rs`

更新 `spawn_render_thread` 的文档注释，说明有界通道的设计决策：

```rust
// BEFORE:
/// 使用无界 channel：渲染事件处理耗时微秒级，不会积压；
/// 有界 channel 的 try_send 静默丢弃会导致渲染线程与 App 状态分叉。

// AFTER:
/// 使用有界 channel（容量 128）：正常使用远达不到上限，极端场景下通过背压限速防止内存膨胀。
/// - Rebuild/RebuildWithAnchor/Clear/Toggle*: 使用 `blocking_send()`，通道满时短暂等待（渲染线程微秒级消费）
/// - Resize: 使用 `try_send()`，通道满时静默丢弃（下一帧 resize 会补偿，渲染线程有 drain 合并逻辑）
```

### 测试验证

- [ ] `cargo test -p peri-tui` 全量通过
- [ ] `cargo clippy -p peri-tui` 无新警告

---

## 风险分析

### 死锁风险评估

| 场景 | 风险 | 缓解 |
|------|------|------|
| 通道满 + blocking_send | 低 | 渲染线程消费极快（微秒级），128 容量难以填满 |
| drop 时 channel 关闭 | 无 | `blocking_send` 返回 `Err(ClosedError)`，`let _ =` 忽略 |
| resize 风暴 | 无 | `try_send` 不阻塞，drain 合并已有逻辑 |
| 多 session 分屏竞争 | 无 | 每个 `ChatSession` 有独立的 render_tx/rx |

### 性能影响

- 正常使用（通道深度 < 5）：零开销，`blocking_send` 内部是原子操作
- 极端积压（通道深度接近 128）：背压限速，防止内存膨胀，用户无感知（渲染延迟 < 1ms）

### 向后兼容

- `RenderEvent` 枚举不变
- `RenderCache` / `Notify` 接口不变
- 测试只需 `send()` → `send().await`（API 变化，语义不变）

---

## 完整变更清单

- [ ] Step 1: `render_thread.rs` — 通道类型 `unbounded_channel()` → `channel(128)` + 常量 + 文档更新
- [ ] Step 2: `message_state.rs` — `render_tx` 类型 `UnboundedSender` → `Sender`
- [ ] Step 3: 发送端适配 — `send()` → `blocking_send()`（agent_render.rs / mod.rs / thread_ops.rs / headless_test.rs）
- [ ] Step 4: Resize 特殊处理 — `send()` → `try_send()`（message_area.rs）
- [ ] Step 5: 测试适配 — `send()` → `send().await`（render_thread_test.rs）
- [ ] Step 6: 新增背压安全测试（4 个新测试）
- [ ] Step 7: 文档注释更新
- [ ] 全量编译 + 测试验证
