# Plan 2: SubAgent LLM 复用 PR 的 P0 修复

## 背景

暂存区审核发现 3 个 P0 问题（误报 1 个已排除）。

**P0-1（误报）**：bg SubAgent Arc 泄露阻止 pool 恢复。核实后不成立——bg task 只持有 `Arc<dyn BaseModel>`（从缓存 clone），不持有 `Arc<Mutex<AgentPool>>`。`llm_factory` 闭包的 `pool_for_subagent` 在 `executor.rs:554` `drop(agent_output.executor)` 时随 ReActAgent 释放，早于 `mod.rs:171` / `acp_stdio.rs:528` 的 `try_unwrap`。

## 有效 P0

| # | 问题 | 严重度 |
|---|------|--------|
| P0-1 | ~~bg SubAgent Arc 泄露~~ → 误报，不修复 | — |
| P0-2 | `session/update_config`（TUI）缺少 `invalidate()` | 模型/凭证切换后旧缓存残留 |
| P0-3 | Stdio 路径缺少 `session/update_config` handler | IDE client 收到 "Method not found" |
| P0-4 | 缺少 3 个 AgentPool 验收测试 | 缓存核心行为无自动化保障 |

## Step 1: `session/update_config`（TUI）补充 `invalidate()`

**文件**：`peri-tui/src/acp_server/requests.rs:455-459`

```rust
// 当前代码：
*c rate.peri_config.write() = new_cfg.clone();
if let Some(p) = LlmProvider::from_config(&new_cfg) {
    *cfg.provider.write() = p;
}
// ❌ 缺少 pool.invalidate()
```

**修复**：在 provider 替换后添加 invalidate：

```rust
*c rate.peri_config.write() = new_cfg.clone();

if let Some(p) = LlmProvider::from_config(&new_cfg) {
    *cfg.provider.write() = p;
}

// Invalidate cached LLM instances (Main Agent + SubAgent)
if let Some(s) = sessions.get_mut(session_id) {
    s.agent_pool.invalidate();
}
```

**边界**：
- `session_id` 为空字符串时 `sessions.get_mut("")` 找不到 session，不执行 invalidate。与现有 `session/set_model` handler 行为一致。
- 即使 `LlmProvider::from_config` 返回 None（无法解析新 provider），也应 invalidate（config 已变，旧缓存不可靠）。

→ 验证：`cargo build -p peri-tui`

## Step 2: Stdio 路径补充 `session/update_config` handler

**文件**：`peri-tui/src/acp_stdio.rs`

当前 stdio 路径只处理 `initialize`、`session/new`、`session/prompt`、`session/cancel`、`session/set_config_option`，缺失 `session/update_config`。

**实现**：参照 TUI 路径（`requests.rs:436-470`）添加 handler，结构完全一致。

```rust
"session/update_config" => {
    let session_id = extract_session_id(params, "");
    let new_cfg: crate::config::PeriConfig =
        serde_json::from_value(params.get("config").cloned().unwrap_or_default())
            .map_err(|e| AcpError::new(-32602, format!("Invalid config: {e}")))?;

    if new_cfg.config.providers.is_empty() {
        return Err(AcpError::new(-32602, "providers cannot be empty"));
    }
    // ... active_provider_id 校验（同 TUI） ...

    *ctx.peri_config.write() = new_cfg.clone();

    if let Some(p) = LlmProvider::from_config(&new_cfg) {
        *ctx.provider.write() = p;
    }

    // Invalidate cached LLM instances
    {
        let mut sessions = ctx.sessions.write();
        if let Some(s) = sessions.get_mut(&session_id) {
            s.agent_pool.invalidate();
        }
    }

    let config_options = /* build_config_options (同 TUI) */;
    send_config_option_update(...).await;
    serde_json::to_value(SetSessionConfigOptionResponse::new(config_options))
        .map_err(|e| AcpError::new(-32603, format!("Serialize failed: {e}")))
}
```

**注意**：stdio 路径中 `ctx.sessions` 是 `Arc<parking_lot::RwLock<HashMap<...>>>`，持锁方式与 TUI 路径的 `&mut Sessions` 参数不同，需适配。

→ 验证：`cargo build -p peri-tui`

## Step 3: 补充 AgentPool 验收测试

**文件**：`peri-acp/src/session/agent_pool_test.rs`

新增 3 个测试，每个 ≤30 行：

### test_subagent_cache_miss_creates_new

```rust
#[test]
fn test_subagent_cache_miss_creates_new() {
    let pool = AgentPool::new();
    // 首次查询 → cache miss → 应创建新实例
    let model = get_or_create_subagent_llm(
        &Arc::new(parking_lot::Mutex::new(pool)),
        "OpenAI:gpt-4o",
        || Box::new(mock_base_model("gpt-4o")),
    );
    assert_eq!(model.model_id(), "gpt-4o");
    assert_eq!(model.provider_name(), "Mock");
}
```

### test_subagent_cache_hit_returns_same

```rust
#[test]
fn test_subagent_cache_hit_returns_same() {
    let pool = Arc::new(parking_lot::Mutex::new(AgentPool::new()));
    let m1 = get_or_create_subagent_llm(
        &pool, "OpenAI:gpt-4o",
        || Box::new(mock_base_model("gpt-4o")),
    );
    let m2 = get_or_create_subagent_llm(
        &pool, "OpenAI:gpt-4o",
        || Box::new(mock_base_model("gpt-4o")),
    );
    // 相同 fingerprint → 返回同一 Arc（ptr_eq）
    assert!(Arc::ptr_eq(&m1, &m2));
}
```

### test_subagent_cache_different_fingerprint_isolation

```rust
#[test]
fn test_subagent_cache_different_fingerprint_isolation() {
    let pool = Arc::new(parking_lot::Mutex::new(AgentPool::new()));
    let m1 = get_or_create_subagent_llm(
        &pool, "OpenAI:gpt-4o",
        || Box::new(mock_base_model("gpt-4o")),
    );
    let m2 = get_or_create_subagent_llm(
        &pool, "OpenAI:gpt-4o-mini",
        || Box::new(mock_base_model("gpt-4o-mini")),
    );
    assert_ne!(m1.model_id(), m2.model_id());
    assert!(!Arc::ptr_eq(&m1, &m2));
}
```

**前置依赖**：需要把 `builder.rs:291-311` 的双检锁逻辑提取为 `AgentPool::get_or_create_subagent_llm()` 方法（当前 P1-5）。可直接在 agent_pool.rs 中实现：

```rust
// agent_pool.rs 新增方法
impl AgentPool {
    /// 获取或创建 SubAgent LLM 实例（双检锁）。
    pub(crate) fn get_or_create_subagent_llm(
        pool: &Arc<parking_lot::Mutex<AgentPool>>,
        fingerprint: &str,
        create: impl FnOnce() -> Box<dyn BaseModel>,
    ) -> Arc<dyn BaseModel> {
        // 快速路径：持锁查缓存
        {
            let guard = pool.lock();
            if let Some(cached) = guard.subagent_llm_cache.get(fingerprint) {
                return Arc::clone(cached);
            }
        }
        // 慢路径：锁外创建
        let new_model: Arc<dyn BaseModel> = Arc::from(create());
        // 锁内写入
        pool.lock()
            .subagent_llm_cache
            .entry(fingerprint.to_string())
            .or_insert(new_model)
            .clone()
    }
}
```

然后 `builder.rs:291-311` 简化为调用此方法。

→ 验证：`cargo test -p peri-acp --lib -- agent_pool`

## 影响范围

| 变更 | 文件 | 风险 |
|------|------|------|
| update_config + invalidate | `requests.rs` | 低，与 set_model 模式一致 |
| stdio update_config handler | `acp_stdio.rs` | 低，参照 TUI 路径 |
| get_or_create 封装 | `agent_pool.rs`, `builder.rs` | 低，纯重构 |
| 3 个新测试 | `agent_pool_test.rs` | 无 |

## 不修复项

| 问题 | 原因 |
|------|------|
| P0-1 bg Arc 泄露 | 误报，已核实无此问题 |
| P1-6 TOCTOU 浪费 | 设计文档已接受 tradeoff（每个模型别名最多一次） |
| P1-7 thinking_effort invalidation | fingerprint 不含 thinking 配置，需更大改动，后续单开 issue |

## 工作量

2-3 小时（3 个 step，每个 ≤1h）
