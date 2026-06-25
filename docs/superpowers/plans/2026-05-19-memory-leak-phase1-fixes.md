# Memory Leak Fix — Phase 1 发现修复计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 修复长时间运行中的两个内存泄露点：后台任务注册表不清理已完成条目、TokenTracker.request_history 无限增长。

**Architecture:** 两个独立修复，互不依赖。(1) `BackgroundTaskRegistry::complete()` 中持锁期间调用 `retain()` 移除所有非 Running 任务；(2) `TokenTracker::accumulate()` 中当 `request_history` 超上限时 drain 旧条目。

**Tech Stack:** Rust, parking_lot::Mutex, tokio

---

### Task 1: BackgroundTaskRegistry — complete() 内清理已完成任务

**Files:**
- Modify: `peri-middlewares/src/subagent/background.rs:62-76`
- Modify: `peri-middlewares/src/subagent/background_test.rs:59-71`

- [ ] **Step 1: 修改 `complete()` 方法，持锁期间调用 `retain()` 清理**

在 `peri-middlewares/src/subagent/background.rs:62-76`，将当前实现：

```rust
    pub fn complete(&self, task_id: &str, result: BackgroundTaskResult) {
        if let Some(task) = self.tasks.lock().get_mut(task_id) {
            task.status = if result.success {
                BackgroundTaskStatus::Completed
            } else {
                BackgroundTaskStatus::Failed
            };
        }
        if self.notification_tx.send(result).is_err() {
            warn!(
                task_id = %task_id,
                "background task complete: failed to send notification (channel closed)"
            );
        }
    }
```

替换为：

```rust
    pub fn complete(&self, task_id: &str, result: BackgroundTaskResult) {
        // 持锁：更新状态 + 清理所有非 Running 任务，防止 JoinHandle 长期驻留内存
        let mut tasks = self.tasks.lock();
        if let Some(task) = tasks.get_mut(task_id) {
            task.status = if result.success {
                BackgroundTaskStatus::Completed
            } else {
                BackgroundTaskStatus::Failed
            };
        }
        tasks.retain(|_, t| matches!(t.status, BackgroundTaskStatus::Running));
        drop(tasks);

        if self.notification_tx.send(result).is_err() {
            warn!(
                task_id = %task_id,
                "background task complete: failed to send notification (channel closed)"
            );
        }
    }
```

- [ ] **Step 2: 更新现有测试 `test_complete_sends_notification`**

在 `peri-middlewares/src/subagent/background_test.rs:43-71`，将：

```rust
        registry.complete("bg-1", result);

        // 任务状态应变为 Completed
        let tasks = registry.list_tasks();
        assert_eq!(tasks.len(), 1);
        assert!(matches!(tasks[0].1, BackgroundTaskStatus::Completed));
        assert_eq!(registry.active_count(), 0);

        // 通知应已发送
        let received = rx.try_recv().unwrap();
        assert_eq!(received.task_id, "bg-1");
        assert!(received.success);
```

替换为：

```rust
        registry.complete("bg-1", result);

        // 已完成任务应被立即清理，list_tasks 不再返回
        let tasks = registry.list_tasks();
        assert_eq!(tasks.len(), 0, "completed tasks should be cleaned up immediately");
        assert_eq!(registry.active_count(), 0);

        // 通知应已发送
        let received = rx.try_recv().unwrap();
        assert_eq!(received.task_id, "bg-1");
        assert!(received.success);
```

- [ ] **Step 3: 运行 background_test 确认通过**

```bash
cargo test -p peri-middlewares --lib background_test -- --nocapture
```
Expected: 所有 test 通过。

- [ ] **Step 4: 运行完整 peri-middlewares 测试**

```bash
cargo test -p peri-middlewares --lib
```
Expected: PASS（所有中间件测试通过）。

- [ ] **Step 5: 提交**

```bash
git add peri-middlewares/src/subagent/background.rs peri-middlewares/src/subagent/background_test.rs
git commit -m "fix(background): cleanup completed tasks in complete() to prevent memory leak

complete() now calls retain() to remove all non-Running entries from the
HashMap, ensuring completed/failed BackgroundTask entries (and their
JoinHandle allocations) don't accumulate across an entire agent session.

Previously cleanup_completed() was defined but never called in production.

Co-Authored-By: deepseek-v4-pro <deepseek-ai@claude-code-best.win>"
```

---

### Task 2: TokenTracker.request_history 上限裁剪

**Files:**
- Modify: `peri-agent/src/agent/token.rs:27`
- Modify: `peri-agent/src/agent/token_test.rs`（新增测试）

- [ ] **Step 1: 在 `accumulate()` 中添加上限裁剪逻辑**

在 `peri-agent/src/agent/token.rs:27`，在 `self.request_history.push(...)` 之后立即添加裁剪：

```rust
    pub fn accumulate(&mut self, usage: &TokenUsage) {
        self.request_history.push(RequestRecord::from_usage(usage));
        // 防止长时间会话中 request_history 无限增长
        if self.request_history.len() > 1000 {
            let excess = self.request_history.len() - 1000;
            self.request_history.drain(0..excess);
        }
        self.total_input_tokens += usage.input_tokens as u64;
        self.total_output_tokens += usage.output_tokens as u64;
        if let Some(v) = usage.cache_creation_input_tokens {
            self.total_cache_creation_tokens += v as u64;
        }
        if let Some(v) = usage.cache_read_input_tokens {
            self.total_cache_read_tokens += v as u64;
        }
        // 只在 input_tokens > 0 时更新 last_usage，
        // 防止异常 API 响应（input_tokens=0）覆盖正常的上下文估算
        if usage.input_tokens > 0 {
            self.last_usage = Some(usage.clone());
        }
        self.llm_call_count += 1;
        self.last_request_id = usage.request_id.clone();
    }
```

- [ ] **Step 2: 新增测试 `test_request_history_capped_at_1000`**

在 `peri-agent/src/agent/token_test.rs:449`（文件末尾，`test_reset_clears_history` 之后）追加：

```rust
#[test]
fn test_request_history_capped_at_1000() {
    let mut tracker = TokenTracker::default();
    // 推入 1500 条记录
    for i in 0..1500u32 {
        tracker.accumulate(&make_usage(i, i / 2, None, None));
    }
    // request_history 不应超过 1000 条
    assert_eq!(tracker.request_history.len(), 1000);
    // 保留的应是最新的 1000 条（idx 500..1499）
    assert_eq!(tracker.request_history[0].input_tokens, 500);
    assert_eq!(tracker.request_history[999].input_tokens, 1499);
    // 累计值不受裁剪影响
    let expected_total_input: u64 = (0..1500u64).sum();
    assert_eq!(tracker.total_input_tokens, expected_total_input,
        "累计值不受 history 裁剪影响");
    assert_eq!(tracker.llm_call_count, 1500);
    // last_usage 应为最后一次调用
    assert_eq!(tracker.estimated_context_tokens(), Some(1499));
}
```

- [ ] **Step 3: 运行 token_test 确认通过**

```bash
cargo test -p peri-agent --lib token_test::test_request_history_capped_at_1000 -- --nocapture
```
Expected: PASS。

- [ ] **Step 4: 运行完整 token 测试套件**

```bash
cargo test -p peri-agent --lib token_test -- --nocapture
```
Expected: 所有已有测试 + 新测试全部通过。

- [ ] **Step 5: 提交**

```bash
git add peri-agent/src/agent/token.rs peri-agent/src/agent/token_test.rs
git commit -m "fix(token): cap TokenTracker.request_history at 1000 entries

Prevents unbounded memory growth of request_history Vec during long
sessions between compacts. Oldest entries are drained when count exceeds
1000. Cumulative counters (total_input_tokens etc.) are unaffected.

Co-Authored-By: deepseek-v4-pro <deepseek-ai@claude-code-best.win>"
```

---

## 自检

**1. Spec 覆盖：** 两个 Task 分别覆盖 Phase 1 的两个最高优先级发现（BackgroundTask 泄露 + TokenTracker 泄露）。

**2. Placeholder 扫描：** 无 TBD/TODO，所有步骤有确切代码和命令。

**3. 类型一致性：** `result.success` 是 `bool`（Copy），在锁内访问无借用冲突。`make_usage()` 辅助函数已存在于测试文件中，签到一致。
