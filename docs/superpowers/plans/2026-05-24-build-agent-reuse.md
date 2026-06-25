# build_agent 跨 Prompt 复用优化 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 减少 `build_agent()` 每轮 ~68 万次瞬态 malloc/free，通过 session 级缓存复用 LLM 实例、工具集和无状态 Middleware，降低 arena 碎片化。

**Architecture:** 在 `peri-acp/src/session/` 新增 `AgentPool` 结构体，持有 session 级可复用对象（LLM 实例、工具集、Middleware 链）。`execute_prompt()` 首次调用时填充池，后续调用从池中获取并更新 per-turn 字段（event_handler、cancel、compact_event_tx）。`ReActAgent::execute(&self, ...)` 已设计为 `&self`，无需重建 agent 即可跨 turn 复用。

**Tech Stack:** Rust, Arc 共享引用, parking_lot::RwLock, tokio mpsc channel

---

## File Structure

| 文件 | 职责 | 变更类型 |
|------|------|---------|
| `peri-acp/src/session/agent_pool.rs` | **新建**：session 级可复用对象池 | Create |
| `peri-acp/src/session/mod.rs` | 注册 agent_pool 模块 | Modify |
| `peri-acp/src/session/executor.rs` | 从 pool 获取/更新 agent 替代每轮 build_agent | Modify |
| `peri-acp/src/agent/builder.rs` | 拆分为 pool 初始化 + per-turn 更新两个阶段 | Modify |
| `peri-agent/src/agent/executor/mod.rs` | 添加 `update_per_turn` 方法 | Modify |
| `peri-middlewares/src/hitl/mod.rs` | `LlmAutoClassifier` 提取为独立可复用单元 | Modify |
| `peri-middlewares/src/git_attribution.rs` | 添加 `reset()` 清理 `pending_old_content` | Modify |
| `peri-middlewares/src/compact_middleware.rs` | 添加 `reset()` 清理 `micro_compact_done` 标志 | Modify |
| `peri-tui/src/acp_server/prompt.rs` | 传递 `AgentPool` 到 executor | Modify |
| `peri-tui/src/acp_server/mod.rs` | `SessionState` 持有 `AgentPool` | Modify |

---

## Task 1: LLM 实例跨 Prompt 复用（最大收益）

**Files:**
- Create: `peri-acp/src/session/agent_pool.rs`
- Modify: `peri-acp/src/session/mod.rs`
- Modify: `peri-acp/src/session/executor.rs`
- Test: `peri-acp/src/session/agent_pool_test.rs`

### 背景分析

当前每轮创建 3 个 LLM 实例，各含独立 `reqwest::Client`（~1-2 MB + TLS session + connection pool）：
- 主 LLM：`BaseModelReactLLM::new(provider.into_model())` → `RetryableLLM`
- auto_classifier：`LlmAutoClassifier::new(Arc::new(Mutex::new(provider.into_model())))`
- compact_model：`provider.clone().into_model().into()`

`reqwest::Client` 内部持有 `hyper` 连接池和 TLS session cache，天然可跨 turn 复用。`RetryableLLM` 和 `BaseModelReactLLM` 均无 per-turn 累积状态（重试计数器在栈上）。

- [ ] **Step 1: 创建 agent_pool.rs 结构体**

```rust
// peri-acp/src/session/agent_pool.rs
//! Session-scoped agent pool for reusing heavy objects across prompts.
//!
//! Reduces transient allocations from ~680k mallocs/turn to near-zero
//! for the LLM and middleware layers by caching session-stable objects.

use std::sync::Arc;

use peri_agent::agent::events::AgentEventHandler;
use peri_agent::agent::token::ContextBudget;
use peri_agent::agent::{AgentCancellationToken, ReActAgent, State};
use peri_agent::agent::compact::CompactConfig;
use peri_agent::llm::BaseModel;
use peri_agent::messages::BaseMessage;
use peri_middlewares::prelude::*;

/// Session-scoped reusable agent components.
///
/// Populated on first prompt, reused on subsequent prompts.
/// Per-turn fields (event_handler, cancel token, compact_event_tx)
/// are updated via `prepare_for_turn()`.
pub struct AgentPool {
    /// Whether the pool has been initialized (false = first turn, needs full build).
    initialized: bool,

    /// Cached main LLM instance (RetryableLLM<BaseModelReactLLM>).
    /// Reused when provider config unchanged.
    // NOTE: ReActAgent owns the LLM, so we rebuild the agent but reuse
    // the LLM by detaching it from the previous agent.
    // This is tracked via model_fingerprint to detect config changes.
    model_fingerprint: String,
}

impl AgentPool {
    pub fn new() -> Self {
        Self {
            initialized: false,
            model_fingerprint: String::new(),
        }
    }

    /// Check if pool needs full rebuild (first turn or provider changed).
    pub fn needs_rebuild(&self, provider: &crate::provider::LlmProvider) -> bool {
        let fp = Self::fingerprint(provider);
        !self.initialized || self.model_fingerprint != fp
    }

    /// Mark pool as initialized with the given provider.
    pub fn mark_initialized(&mut self, provider: &crate::provider::LlmProvider) {
        self.model_fingerprint = Self::fingerprint(provider);
        self.initialized = true;
    }

    /// Reset pool state (e.g., after model change or session clear).
    pub fn reset(&mut self) {
        self.initialized = false;
        self.model_fingerprint.clear();
    }

    fn fingerprint(provider: &crate::provider::LlmProvider) -> String {
        format!("{}:{}", provider.display_name(), provider.model_name())
    }
}
```

- [ ] **Step 2: 在 session/mod.rs 中注册模块**

在 `peri-acp/src/session/mod.rs` 的模块声明中添加：

```rust
pub mod agent_pool;
```

- [ ] **Step 3: 将 AgentPool 注入 SessionState**

修改 `peri-tui/src/acp_server/mod.rs` 中 `SessionState` 添加：

```rust
pub struct SessionState {
    // ... existing fields ...
    /// Session-scoped agent pool for reusing heavy objects.
    pub agent_pool: peri_acp::session::agent_pool::AgentPool,
}
```

在 `SessionState::new()` 中初始化：

```rust
agent_pool: peri_acp::session::agent_pool::AgentPool::new(),
```

- [ ] **Step 4: 修改 executor.rs 签名，接受 pool 参数**

在 `execute_prompt()` 参数列表中添加：

```rust
pub async fn execute_prompt(
    // ... existing params ...
    pool: Arc<parking_lot::Mutex<AgentPool>>,  // 新增
) -> PromptResult {
```

- [ ] **Step 5: 在 executor.rs 中实现 pool 逻辑**

核心思路：首次调用 `build_agent()` 全量构建，后续调用复用。

由于 `ReActAgent` 使用 builder 模式且 `execute(&self, ...)` 设计为可复用，
最简方案是：首次 build 后把 `ReActAgent` 存入 pool，后续 turn 只更新 per-turn 字段。

但 `ReActAgent` 泛型参数 `L = RetryableLLM<BaseModelReactLLM>` 使存储复杂。
更务实的方案是：**缓存最重的对象（LLM 实例），仍然每轮 build ReActAgent 但复用 LLM 内部的 reqwest Client**。

修改 `builder.rs`，将 LLM 构建拆分：

```rust
// builder.rs — 新增辅助结构
pub struct CachedLlmInstances {
    /// 复用的 compact_model（含 reqwest Client 连接池）
    pub compact_model: Arc<dyn BaseModel>,
    /// 复用的 auto_classifier LLM（含 reqwest Client 连接池）
    pub auto_classifier_model: Arc<tokio::sync::Mutex<Box<dyn BaseModel>>>,
    /// 构建时的 provider fingerprint
    pub fingerprint: String,
}
```

修改 `AgentPool`：

```rust
pub struct AgentPool {
    initialized: bool,
    model_fingerprint: String,
    /// 缓存的 LLM 实例
    pub cached_llm: Option<CachedLlmInstances>,
}
```

修改 `build_agent()` 接受可选的 cached_llm：

```rust
pub fn build_agent(cfg: AcpAgentConfig, cached_llm: Option<&CachedLlmInstances>) -> (AcpAgentOutput, Option<CachedLlmInstances>) {
    // 复用 compact_model
    let compact_model = cfg.compact_model.or_else(|| {
        cached_llm.and_then(|c| Some(c.compact_model.clone()))
    });

    // 复用 auto_classifier model
    let auto_classifier_model = cached_llm
        .and_then(|c| Some(c.auto_classifier_model.clone()))
        .unwrap_or_else(|| {
            Arc::new(tokio::sync::Mutex::new(provider_for_factory.clone().into_model()))
        });

    // ... build rest as before ...

    // 返回 cached 实例供下次复用
    let new_cache = CachedLlmInstances {
        compact_model: compact_model.unwrap(), // 存在 compact_model 时
        auto_classifier_model,
        fingerprint: Self::fingerprint(&cfg.provider),
    };
    (output, Some(new_cache))
}
```

- [ ] **Step 6: 编写测试**

```rust
// peri-acp/src/session/agent_pool_test.rs
use super::agent_pool::AgentPool;

#[test]
fn test_pool_needs_rebuild_initially() {
    let pool = AgentPool::new();
    assert!(pool.needs_rebuild(&mock_provider("openai", "gpt-4")));
}

#[test]
fn test_pool_no_rebuild_after_init() {
    let mut pool = AgentPool::new();
    let provider = mock_provider("openai", "gpt-4");
    pool.mark_initialized(&provider);
    assert!(!pool.needs_rebuild(&provider));
}

#[test]
fn test_pool_needs_rebuild_after_model_change() {
    let mut pool = AgentPool::new();
    pool.mark_initialized(&mock_provider("openai", "gpt-4"));
    assert!(pool.needs_rebuild(&mock_provider("anthropic", "claude-4")));
}

#[test]
fn test_pool_reset() {
    let mut pool = AgentPool::new();
    pool.mark_initialized(&mock_provider("openai", "gpt-4"));
    pool.reset();
    assert!(pool.needs_rebuild(&mock_provider("openai", "gpt-4")));
}
```

- [ ] **Step 7: 运行测试确认通过**

Run: `cargo test -p peri-acp --lib -- agent_pool`
Expected: 4 tests PASS

- [ ] **Step 8: Commit**

```bash
git add peri-acp/src/session/agent_pool.rs peri-acp/src/session/agent_pool_test.rs peri-acp/src/session/mod.rs
git commit -m "feat(peri-acp): add AgentPool for session-scoped LLM instance reuse"
```

---

## Task 2: ReActAgent per-turn 更新方法

**Files:**
- Modify: `peri-agent/src/agent/executor/mod.rs`
- Test: `peri-agent/src/agent/executor/mod_test.rs`

### 背景分析

`ReActAgent::execute(&self, input, &mut state, cancel)` 使用 `&self`，但以下字段需要 per-turn 更新：
- `event_handler` — 每轮不同（包装不同的 event channel）
- `system_prompt` — 可能因模型切换变化（Git Attribution 含 model_name）
- `notification_rx` — 来自 bg_notification channel

当前 builder 模式返回 `Self`（消费 self），无法更新已有实例。需要添加 `&mut self` 更新方法。

- [ ] **Step 1: 添加 update 方法到 ReActAgent**

在 `peri-agent/src/agent/executor/mod.rs` 的 `impl<L: ReactLLM, S: State> ReActAgent<L, S>` 中添加：

```rust
/// Update event handler for a new turn.
/// Called after retrieving agent from pool.
pub fn set_event_handler(&mut self, handler: Arc<dyn AgentEventHandler>) {
    self.event_handler = Some(handler);
}

/// Update system prompt for a new turn.
/// Called when model changes (Git Attribution text changes).
pub fn set_system_prompt(&mut self, prompt: impl Into<String>) {
    self.system_prompt = Some(prompt.into());
}

/// Update notification receiver for a new turn.
pub fn set_notification_rx(
    &mut self,
    rx: tokio::sync::mpsc::UnboundedReceiver<BackgroundTaskResult>,
) {
    self.notification_rx = Some(tokio::sync::Mutex::new(rx));
}
```

- [ ] **Step 2: 编写测试**

```rust
// 在 mod_test.rs 中添加
#[tokio::test]
async fn test_react_agent_update_event_handler() {
    use crate::agent::react::MockReactLLM;
    use crate::agent::state::AgentState;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    let llm = MockReactLLM::new();
    let mut agent = ReActAgent::new(llm).max_iterations(1);

    let counter = Arc::new(AtomicUsize::new(0));
    let handler = Arc::new(crate::agent::events::FnEventHandler({
        let c = counter.clone();
        move |_| { c.fetch_add(1, Ordering::SeqCst); }
    }));

    agent.set_event_handler(handler.clone());
    // agent.event_handler 应为 Some
    assert!(agent.event_handler.is_some());
}

#[tokio::test]
async fn test_react_agent_update_system_prompt() {
    let llm = MockReactLLM::new();
    let mut agent = ReActAgent::new(llm);
    assert!(agent.system_prompt.is_none());

    agent.set_system_prompt("test prompt");
    assert_eq!(agent.system_prompt.as_deref(), Some("test prompt"));
}
```

- [ ] **Step 3: 运行测试**

Run: `cargo test -p peri-agent --lib -- agent::executor::tests::test_react_agent_update`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add peri-agent/src/agent/executor/mod.rs peri-agent/src/agent/executor/mod_test.rs
git commit -m "feat(peri-agent): add per-turn update methods to ReActAgent"
```

---

## Task 3: Middleware reset 方法（有状态 Middleware）

**Files:**
- Modify: `peri-middlewares/src/git_attribution.rs`
- Modify: `peri-middlewares/src/compact_middleware.rs`
- Test: 对应 test 文件

### 背景分析

以下 Middleware 有 per-turn 累积状态，需要 reset 才能跨 turn 复用：
1. **GitAttributionMiddleware** — `pending_old_content: HashMap<String, String>` 累积每轮文件修改追踪
2. **CompactMiddleware** — `micro_compact_done: AtomicBool` per-prompt 标志
3. **HookMiddleware** — `once_fired: HashSet<String>` session 级去重（不需要 reset，设计为跨 turn 累积）

- [ ] **Step 1: 为 GitAttributionMiddleware 添加 reset()**

```rust
// peri-middlewares/src/git_attribution.rs
impl GitAttributionMiddleware {
    /// Clear per-turn state for reuse across prompts.
    pub fn reset(&mut self) {
        self.pending_old_content.clear();
    }
}
```

编写测试：

```rust
#[test]
fn test_git_attribution_reset_clears_pending() {
    let mut mw = GitAttributionMiddleware::new("test-model");
    mw.pending_old_content.insert("file.rs".to_string(), "old content".to_string());
    assert_eq!(mw.pending_old_content.len(), 1);

    mw.reset();
    assert!(mw.pending_old_content.is_empty());
}
```

- [ ] **Step 2: 为 CompactMiddleware 添加 reset()**

```rust
// peri-middlewares/src/compact_middleware.rs
impl CompactMiddleware {
    /// Reset per-turn state for reuse across prompts.
    pub fn reset(&mut self) {
        self.micro_compact_done.store(false, std::sync::atomic::Ordering::SeqCst);
    }
}
```

编写测试：

```rust
#[test]
fn test_compact_middleware_resets_micro_compact_flag() {
    // 构造测试用 CompactMiddleware
    let mw = CompactMiddleware::new(/* ... */);
    mw.micro_compact_done.store(true, std::sync::atomic::Ordering::SeqCst);
    assert!(mw.micro_compact_done.load(std::sync::atomic::Ordering::SeqCst));

    mw.reset();
    assert!(!mw.micro_compact_done.load(std::sync::atomic::Ordering::SeqCst));
}
```

- [ ] **Step 3: 运行测试**

Run: `cargo test -p peri-middlewares --lib -- git_attribution compact_middleware`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add peri-middlewares/src/git_attribution.rs peri-middlewares/src/compact_middleware.rs
git commit -m "feat(peri-middlewares): add reset() to stateful middleware for cross-turn reuse"
```

---

## Task 4: AgentPool 完整实现 — 缓存 ReActAgent

**Files:**
- Modify: `peri-acp/src/session/agent_pool.rs`
- Modify: `peri-acp/src/session/executor.rs`
- Modify: `peri-acp/src/agent/builder.rs`
- Modify: `peri-tui/src/acp_server/prompt.rs`
- Test: `peri-acp/src/session/agent_pool_test.rs`

### 背景分析

Task 1-3 准备好了基础设施。现在把 AgentPool 打通为完整的 session 级缓存。

AgentPool 将持有：
- `ReActAgent` 实例（通过 `Box<dyn Any>` 存储以绕过泛型）
- 或者更务实地：缓存 builder 的中间产物（LLM 实例 + tool set + middleware chain）

**选择务实方案**：缓存 `AcpAgentOutput` 的核心组件，每轮重建 `ReActAgent` 但复用 LLM 和 tools。
原因：`ReActAgent` 的泛型参数 `L` 使 `dyn` 存储困难；builder 消费 self 模式与复用矛盾；
而 LLM 实例（含 reqwest Client）是最大的分配来源。

- [ ] **Step 1: 完善 AgentPool 结构**

```rust
// peri-acp/src/session/agent_pool.rs
use std::sync::Arc;
use peri_agent::llm::BaseModel;

/// Session-scoped cached LLM instances.
pub struct CachedLlmInstances {
    /// compact_model（含 reqwest Client 连接池）— 复用自上一轮
    pub compact_model: Arc<dyn BaseModel>,
    /// auto_classifier 的 LLM（含 reqwest Client 连接池）— 复用自上一轮
    pub auto_classifier_model: Arc<tokio::sync::Mutex<Box<dyn BaseModel>>>,
    /// 构建时的 provider fingerprint（provider_name:model_name）
    pub fingerprint: String,
}

/// Session-scoped agent component pool.
pub struct AgentPool {
    /// Cached LLM instances (biggest allocation win).
    cached_llm: Option<CachedLlmInstances>,
    /// Provider fingerprint for invalidation.
    fingerprint: String,
}

impl AgentPool {
    pub fn new() -> Self {
        Self {
            cached_llm: None,
            fingerprint: String::new(),
        }
    }

    /// Whether the cached LLM instances are valid for this provider.
    pub fn has_valid_cache(&self, provider: &crate::provider::LlmProvider) -> bool {
        let fp = fingerprint(provider);
        self.cached_llm.is_some() && self.fingerprint == fp
    }

    /// Store LLM instances after building.
    pub fn store_llm(&mut self, instances: CachedLlmInstances) {
        self.fingerprint = instances.fingerprint.clone();
        self.cached_llm = Some(instances);
    }

    /// Get cached LLM instances (returns None if invalid).
    pub fn get_cached_llm(&self) -> Option<&CachedLlmInstances> {
        self.cached_llm.as_ref()
    }

    /// Invalidate cache (on model change, session clear, etc.).
    pub fn invalidate(&mut self) {
        self.cached_llm = None;
        self.fingerprint.clear();
    }
}

fn fingerprint(provider: &crate::provider::LlmProvider) -> String {
    format!("{}:{}", provider.display_name(), provider.model_name())
}
```

- [ ] **Step 2: 修改 builder.rs 接受 cached_llm**

修改 `build_agent()` 签名：

```rust
pub fn build_agent(
    cfg: AcpAgentConfig,
    cached_llm: Option<&CachedLlmInstances>,
) -> (AcpAgentOutput, Option<CachedLlmInstances>) {
```

在函数体中：

```rust
    // 复用 compact_model（避免每轮创建 reqwest Client）
    let compact_model = cfg.compact_model.or_else(|| {
        cached_llm.and_then(|c| Some(c.compact_model.clone()))
    });

    // 复用 auto_classifier model（避免每轮创建第二个 reqwest Client）
    let auto_classifier_model = cached_llm
        .and_then(|c| Some(c.auto_classifier_model.clone()))
        .unwrap_or_else(|| {
            Arc::new(tokio::sync::Mutex::new(provider_for_factory.clone().into_model()))
        });

    // ... 后续构建逻辑不变 ...

    // HITL middleware — 使用复用的 auto_classifier_model
    let auto_classifier: Option<Arc<dyn AutoClassifier>> =
        Some(Arc::new(LlmAutoClassifier::new(auto_classifier_model.clone())));
```

在函数末尾返回缓存：

```rust
    let new_cache = CachedLlmInstances {
        compact_model: compact_model.unwrap(), // compact 启用时存在
        auto_classifier_model,
        fingerprint: fingerprint(&cfg.provider),
    };

    (AcpAgentOutput {
        executor,
        todo_rx,
        context_window,
        bg_event_rx,
    }, Some(new_cache))
}
```

- [ ] **Step 3: 修改 executor.rs 使用 pool**

```rust
pub async fn execute_prompt(
    // ... existing params ...
    pool: Arc<parking_lot::Mutex<AgentPool>>,  // 新增
) -> PromptResult {
    // ... existing code up to build_agent call ...

    let (agent_output, new_cache) = {
        let pool_guard = pool.lock();
        let cached = if pool_guard.has_valid_cache(provider) {
            pool_guard.get_cached_llm()
        } else {
            None
        };
        // 释放锁后再 build（避免持锁跨 await）
        drop(pool_guard);

        let result = builder::build_agent(AcpAgentConfig { ... }, cached);

        // 存入 pool
        if let Some(cache) = result.1 {
            pool.lock().store_llm(cache);
        }
        result
    };

    // ... rest of execute_prompt unchanged ...
```

- [ ] **Step 4: 修改 prompt.rs 传递 pool**

在 `peri-tui/src/acp_server/prompt.rs` 的 `execute_prompt()` 中：

```rust
    // 从 SessionState 获取 AgentPool
    let pool = {
        let mut sessions = sessions.lock().await;
        let state = sessions
            .get_mut(&session_id)
            .ok_or_else(|| AcpError::new(-32602, "session not found"))?;
        std::mem::take(&mut state.agent_pool)  // take ownership
    };

    let result = executor::execute_prompt(
        // ... existing args ...
        Arc::new(parking_lot::Mutex::new(pool)),  // wrap in Arc<Mutex>
    ).await;

    // Store pool back
    {
        let mut sessions = sessions.lock().await;
        if let Some(state) = sessions.get_mut(&session_id) {
            state.agent_pool = Arc::try_unwrap(pool)
                .unwrap_or_else(|arc| arc.into_inner())
                .into_inner();
        }
    }
```

- [ ] **Step 5: 处理 model change 的 cache invalidation**

在 `session/set_model` handler 中（`peri-tui/src/acp_server/requests.rs`）：

```rust
// 在处理 session/set_model 时
if let Some(state) = sessions.get_mut(&session_id) {
    state.agent_pool.invalidate();  // model 变化时清除缓存
}
```

- [ ] **Step 6: 编写集成测试**

```rust
#[tokio::test]
async fn test_agent_pool_reuses_llm_across_turns() {
    let pool = Arc::new(parking_lot::Mutex::new(AgentPool::new()));
    let provider = mock_provider();

    // First turn — full build, cache populated
    let result1 = executor::execute_prompt(
        &provider, /* ... */ pool.clone(),
    ).await;

    // Second turn — should reuse cached LLM
    let pool_guard = pool.lock();
    assert!(pool_guard.has_valid_cache(&provider));
    assert!(pool_guard.get_cached_llm().is_some());
    drop(pool_guard);

    let result2 = executor::execute_prompt(
        &provider, /* ... */ pool.clone(),
    ).await;

    // Verify both turns complete successfully
    assert!(result1.ok);
    assert!(result2.ok);
}

#[tokio::test]
async fn test_agent_pool_invalidates_on_model_change() {
    let pool = Arc::new(parking_lot::Mutex::new(AgentPool::new()));
    let provider1 = mock_provider_with("openai", "gpt-4");
    let provider2 = mock_provider_with("anthropic", "claude-4");

    // First turn with provider1
    let _ = executor::execute_prompt(&provider1, /* ... */ pool.clone()).await;

    // Model change — invalidate
    pool.lock().invalidate();

    // Second turn with provider2 — should rebuild
    assert!(!pool.lock().has_valid_cache(&provider2));
    let _ = executor::execute_prompt(&provider2, /* ... */ pool.clone()).await;

    // Cache now valid for provider2
    assert!(pool.lock().has_valid_cache(&provider2));
}
```

- [ ] **Step 7: 运行全量测试**

Run: `cargo test -p peri-acp --lib`
Expected: All tests PASS

- [ ] **Step 8: Commit**

```bash
git add peri-acp/src/session/agent_pool.rs peri-acp/src/session/executor.rs peri-acp/src/agent/builder.rs peri-tui/src/acp_server/prompt.rs peri-tui/src/acp_server/mod.rs
git commit -m "feat(peri-acp): integrate AgentPool for cross-prompt LLM instance reuse

Reduces ~680k transient mallocs/turn by caching LLM instances
(reqwest Client + TLS session) across prompts within a session.
Invalidated on model change via session/set_model."
```

---

## Task 5: 工具集跨 Prompt 缓存

**Files:**
- Modify: `peri-acp/src/session/agent_pool.rs`
- Modify: `peri-acp/src/agent/builder.rs`
- Test: `peri-acp/src/session/agent_pool_test.rs`

### 背景分析

`parent_tools: Vec<Box<dyn BaseTool>>` 包含 FilesystemMiddleware（6 个工具）+ TerminalMiddleware（1 个）+ MCP tool bridges。
这些工具在 cwd 不变时完全一致。MCP pool 的 `build_tool_bridges()` 每轮 clone Arc 但不重建连接。

`FilesystemMiddleware::build_tools(&cwd)` 创建 6 个无状态工具，完全可缓存。

- [ ] **Step 1: 在 AgentPool 中添加 tool cache**

```rust
pub struct AgentPool {
    // ... existing fields ...

    /// Cached parent tools (valid when cwd unchanged).
    cached_tools: Option<CachedTools>,
    /// Cwd fingerprint for tool invalidation.
    cwd_fingerprint: String,
}

pub struct CachedTools {
    pub parent_tools: Vec<Box<dyn peri_agent::tools::BaseTool>>,
    pub mcp_pool_fingerprint: Option<String>,  // None = no MCP
}

impl AgentPool {
    pub fn has_valid_tools(&self, cwd: &str, mcp_pool: Option<&Arc<peri_middlewares::mcp::McpClientPool>>) -> bool {
        let cwd_fp = cwd.to_string();
        self.cached_tools.is_some() && self.cwd_fingerprint == cwd_fp
    }

    pub fn store_tools(&mut self, cwd: &str, tools: Vec<Box<dyn peri_agent::tools::BaseTool>>) {
        self.cwd_fingerprint = cwd.to_string();
        self.cached_tools = Some(CachedTools {
            parent_tools: tools,
            mcp_pool_fingerprint: None,
        });
    }

    pub fn get_cached_tools(&self) -> Option<&CachedTools> {
        self.cached_tools.as_ref()
    }
}
```

- [ ] **Step 2: 修改 builder.rs 复用 tools**

```rust
// 在 build_agent() 中
let parent_tools = if let Some(cached) = cached_tools {
    // 复用缓存的工具集（cwd 不变时）
    cached.parent_tools.iter().map(|t| t.box_clone()).collect()
} else {
    let mut tools: Vec<Box<dyn peri_agent::tools::BaseTool>> =
        FilesystemMiddleware::build_tools(&cwd);
    tools.extend(TerminalMiddleware::build_tools(&cwd));
    if let Some(ref pool) = mcp_pool {
        let mcp_tools = peri_middlewares::mcp::build_tool_bridges(pool);
        for tool in mcp_tools {
            tools.push(tool);
        }
        if pool.has_resources() {
            tools.push(Box::new(peri_middlewares::mcp::McpResourceTool::new(
                Arc::clone(pool),
            )));
        }
    }
    tools
};
```

**注意**：`BaseTool` 需要 `box_clone()` 方法（或使用 `tool_setup::box_to_arc` 转为 Arc 后共享）。检查 `BaseTool` trait 是否有 clone 支持。

如果 `BaseTool` 不支持 clone，替代方案是**将 tools 存为 `Vec<Arc<dyn BaseTool>>`**：
- 首次构建时 `box_to_arc` 转为 Arc
- 缓存 Arc 集合
- 后续 turn 直接 Arc::clone

- [ ] **Step 3: 运行测试**

Run: `cargo test -p peri-acp --lib -- agent_pool`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add peri-acp/src/session/agent_pool.rs peri-acp/src/agent/builder.rs
git commit -m "feat(peri-acp): cache tool set in AgentPool for cwd-stable sessions"
```

---

## Task 6: 模型切换时 pool invalidation 联动

**Files:**
- Modify: `peri-tui/src/acp_server/requests.rs`

### 背景分析

`session/set_model` 和 `session/set_config_option(model)` 需要触发 pool invalidation。
当前模型切换发生在 `requests.rs` 的 handler 中。

- [ ] **Step 1: 在 set_model handler 中添加 invalidation**

在 `peri-tui/src/acp_server/requests.rs` 中处理 `session/set_model` 和 `session/set_config_option` 的分支中：

```rust
// 在更新 provider 后
{
    let mut sessions = sessions.lock().await;
    if let Some(state) = sessions.get_mut(&session_id) {
        state.agent_pool.invalidate();
    }
}
```

- [ ] **Step 2: 运行全量测试**

Run: `cargo test -p peri-tui --lib`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add peri-tui/src/acp_server/requests.rs
git commit -m "feat(peri-tui): invalidate AgentPool on model change"
```

---

## Task 7: 验证和性能测量

**Files:**
- No code changes, verification only

### 验证步骤

- [ ] **Step 1: 构建全量**

Run: `cargo build`
Expected: 编译通过，无 warning

- [ ] **Step 2: 运行全量测试**

Run: `cargo test`
Expected: 所有测试通过

- [ ] **Step 3: 运行 clippy**

Run: `cargo clippy -- -D warnings`
Expected: 无新 warning

- [ ] **Step 4: 手动验证 — 启动 TUI 执行多轮对话**

Run: `cargo run -p peri-tui`
验证：
1. 第一轮正常执行
2. 第二轮执行（应复用 cached LLM）
3. `Alt+M` 切换模型后再执行（应 invalidate 并重建）
4. 检查日志中是否有 "AgentPool cache hit" 类似信息

- [ ] **Step 5: 最终 Commit（如有 lint 修复）**

```bash
git add -A
git commit -m "chore: lint fixes for AgentPool integration"
```

---

## 风险和缓解

| 风险 | 缓解措施 |
|------|---------|
| LLM 实例跨 turn 持有过期状态 | `RetryableLLM` 和 `BaseModelReactLLM` 已验证无 per-turn 状态 |
| 模型切换后使用了旧 LLM | `fingerprint` 检测 + `invalidate()` 联动 |
| cancel token 跨 turn 泄漏 | cancel token 是 per-turn 传入的 `AgentCancellationToken`，不缓存 |
| MCP pool 连接断开后 tools 失效 | 检测 cwd 变化时 invalidation；MCP reconnection 由 pool 自身处理 |
| compact 后消息结构变化 | compact 由 `CompactMiddleware` 处理，与 pool 无关 |
| 系统提示词稳定性原则 | pool 不缓存 system prompt 内容，per-turn 仍使用 `FrozenSessionData` |

## 预期收益

| 对象 | 当前 | 优化后 | 节省 |
|------|------|--------|------|
| reqwest Client（compact_model） | 每轮新建 ~1-2 MB | session 级复用 | ~1-2 MB/turn |
| reqwest Client（auto_classifier） | 每轮新建 ~1-2 MB | session 级复用 | ~1-2 MB/turn |
| 工具集（cwd 不变时） | 每轮 ~0.5-1 MB | session 级复用 | ~0.5-1 MB/turn |
| 总瞬态 malloc | ~680k/turn | 预计降至 ~100-200k/turn | ~70-85% |
