# Agent 工具 Ok("Error:") 修复实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将 Agent 工具中所有 `Ok("Error: ...")` 返回改为 `Err(...into())`，使 `is_error` 字段正确标记为 true，同时改善 `subagent_type` 参数描述以降低 3.35% 的错误率。

**Architecture:** 修改 `define.rs` 和 `execute_bg.rs` 中 4 处 `Ok("Error:")` 为 `Err()`，并更新 `subagent_type` 参数描述使其更明确地说明"不提供则自动 fork"的语义。同步更新已有测试的断言方式（从 `result.contains()` 改为 `result.unwrap_err().to_string().contains()`）。

**Tech Stack:** Rust, tokio, async-trait, serde_json

---

## File Structure

| 文件 | 操作 | 职责 |
|------|------|------|
| `peri-middlewares/src/subagent/tool/define.rs` | 修改 | 将 2 处 `Ok("Error:")` 改为 `Err()`；更新 `subagent_type` 参数描述 |
| `peri-middlewares/src/subagent/tool/execute_bg.rs` | 修改 | 将 2 处 `Ok("Error:")` 改为 `Err()` |
| `peri-middlewares/src/subagent/tool/tool_test.rs` | 修改 | 更新 3 个测试的断言方式以匹配 `Err()` 返回 |

---

### Task 1: define.rs — 将 Ok("Error:") 改为 Err() 返回

**Files:**
- Modify: `peri-middlewares/src/subagent/tool/define.rs:332,366-370`

- [ ] **Step 1: 修改 define.rs 第 332 行 — prompt 缺失**

将:
```rust
None => return Ok("Error: missing required parameter prompt".to_string()),
```
改为:
```rust
None => return Err("Error: missing required parameter prompt".into()),
```

- [ ] **Step 2: 修改 define.rs 第 364-371 行 — subagent_type 缺失**

将:
```rust
let agent_id = match &subagent_type {
    Some(id) => id.clone(),
    None => {
        return Ok(
            "Error: please provide subagent_type parameter to specify the agent type, or use fork: true for fork mode"
                .to_string(),
        )
    }
};
```
改为:
```rust
let agent_id = match &subagent_type {
    Some(id) => id.clone(),
    None => {
        return Err(
            "Error: please provide subagent_type parameter to specify the agent type, or use fork: true for fork mode"
                .into(),
        )
    }
};
```

- [ ] **Step 3: 运行 cargo check 验证编译**

Run: `cargo check -p peri-middlewares 2>&1 | grep -E "^error"`
Expected: 无输出（编译通过）

- [ ] **Step 4: Commit**

```bash
git add peri-middlewares/src/subagent/tool/define.rs
git commit -m "fix: Agent tool prompt/subagent_type errors return Err() instead of Ok()

Ok(\"Error: ...\") 导致 is_error=false，监控系统完全遗漏了这 3.35% 的工具调用错误。
改为 Err() 返回使 is_error 正确标记为 true。

Refs: spec/issues/2026-06-05-agent-tool-3-percent-error-rate-subagent-type-missing"
```

---

### Task 2: execute_bg.rs — 将 Ok("Error:") 改为 Err() 返回

**Files:**
- Modify: `peri-middlewares/src/subagent/tool/execute_bg.rs:39,55-58`

- [ ] **Step 1: 修改 execute_bg.rs 第 38-42 行 — 并发上限**

将:
```rust
if registry.active_count() >= 3 {
    return Ok("Error: maximum 3 concurrent background tasks reached. \
         Wait for a running task to complete before starting a new one."
        .to_string());
}
```
改为:
```rust
if registry.active_count() >= 3 {
    return Err("Error: maximum 3 concurrent background tasks reached. \
         Wait for a running task to complete before starting a new one."
        .into());
}
```

- [ ] **Step 2: 修改 execute_bg.rs 第 52-59 行 — 后台模式 subagent_type 缺失**

将:
```rust
let agent_id =
    match &subagent_type {
        Some(id) => id.clone(),
        None => return Ok(
            "Error: background mode requires subagent_type parameter (or use fork: true)"
                .to_string(),
        ),
    };
```
改为:
```rust
let agent_id =
    match &subagent_type {
        Some(id) => id.clone(),
        None => return Err(
            "Error: background mode requires subagent_type parameter (or use fork: true)"
                .into(),
        ),
    };
```

- [ ] **Step 3: 运行 cargo check 验证编译**

Run: `cargo check -p peri-middlewares 2>&1 | grep -E "^error"`
Expected: 无输出（编译通过）

- [ ] **Step 4: Commit**

```bash
git add peri-middlewares/src/subagent/tool/execute_bg.rs
git commit -m "fix: Agent tool background errors return Err() instead of Ok()

并发上限和 subagent_type 缺失错误从 Ok() 改为 Err()，
使 is_error 正确标记，监控和分析器可捕获。

Refs: spec/issues/2026-06-05-agent-tool-3-percent-error-rate-subagent-type-missing"
```

---

### Task 3: define.rs — 改善 subagent_type 参数描述

**Files:**
- Modify: `peri-middlewares/src/subagent/tool/define.rs:298-301`

- [ ] **Step 1: 更新 parameters() 中 subagent_type 的描述**

将:
```rust
"subagent_type": {
    "type": "string",
    "description": "The agent ID from the available agents list (e.g., 'code-reviewer', 'explorer'). Must exactly match an agent definition file at .claude/agents/{subagent_type}.md or .claude/agents/{subagent_type}/agent.md. When empty or not provided, creates a fork of the current agent with all tools"
},
```
改为:
```rust
"subagent_type": {
    "type": "string",
    "description": "The agent ID from the available agents list (e.g., 'code-reviewer', 'explorer'). Must exactly match an agent definition file at .claude/agents/{subagent_type}.md or .claude/agents/{subagent_type}/agent.md. REQUIRED unless fork=true. When not provided and fork is not set, the call will fail with an error"
},
```

同时更新 `AGENT_DESCRIPTION` 常量（第 54 行）中的对应说明。

将:
```rust
- Specify subagent_type matching an existing agent definition file. When not provided, creates a fork of the current agent
```
改为:
```rust
- **subagent_type is REQUIRED** unless fork=true. Specify an agent ID matching an existing agent definition file. Do NOT omit this parameter unless you intend to fork the current agent
```

- [ ] **Step 2: 运行 cargo check 验证编译**

Run: `cargo check -p peri-middlewares 2>&1 | grep -E "^error"`
Expected: 无输出

- [ ] **Step 3: Commit**

```bash
git add peri-middlewares/src/subagent/tool/define.rs
git commit -m "docs: clarify Agent tool subagent_type is REQUIRED unless fork=true

历史数据显示 80% 的 Agent 工具错误（36/45）源于 LLM 遗漏 subagent_type。
更新参数描述和工具说明，强调该参数为必填（除非 fork=true）。

Refs: spec/issues/2026-06-05-agent-tool-3-percent-error-rate-subagent-type-missing"
```

---

### Task 4: 更新测试断言以匹配 Err() 返回

**Files:**
- Modify: `peri-middlewares/src/subagent/tool/tool_test.rs:84-107,111-124,156-171`

- [ ] **Step 1: 更新 test_agent_prompt_missing_returns_error（第 84-107 行）**

将:
```rust
let result = t
    .invoke(serde_json::json!({
        "subagent_type": "test-agent",
        "cwd": dir.path().to_str().unwrap()
    }))
    .await
    .unwrap();
assert!(
    result.contains("prompt"),
    "Should return missing prompt error: {}",
    result
);
```
改为:
```rust
let result = t
    .invoke(serde_json::json!({
        "subagent_type": "test-agent",
        "cwd": dir.path().to_str().unwrap()
    }))
    .await;
let err_msg = result.unwrap_err().to_string();
assert!(
    err_msg.contains("prompt"),
    "Should return missing prompt error: {}",
    err_msg
);
```

- [ ] **Step 2: 更新 test_agent_subagent_type_missing_returns_error（第 111-124 行）**

将:
```rust
let result = t
    .invoke(serde_json::json!({
        "prompt": "do something"
    }))
    .await
    .unwrap();
assert!(
    result.contains("subagent_type") || result.contains("fork"),
    "Should return missing subagent_type error with fork hint: {}",
    result
);
```
改为:
```rust
let result = t
    .invoke(serde_json::json!({
        "prompt": "do something"
    }))
    .await;
let err_msg = result.unwrap_err().to_string();
assert!(
    err_msg.contains("subagent_type") || err_msg.contains("fork"),
    "Should return missing subagent_type error with fork hint: {}",
    err_msg
);
```

- [ ] **Step 3: 更新 test_tool_agent_not_found（第 156-171 行）**

将:
```rust
let result = t
    .invoke(serde_json::json!({
        "subagent_type": "nonexistent-agent",
        "prompt": "do something",
        "cwd": "/tmp"
    }))
    .await
    .unwrap();
assert!(
    result.contains("cannot find"),
    "Should return not found error: {}",
    result
);
```
改为:
```rust
let result = t
    .invoke(serde_json::json!({
        "subagent_type": "nonexistent-agent",
        "prompt": "do something",
        "cwd": "/tmp"
    }))
    .await;
let err_msg = result.unwrap_err().to_string();
assert!(
    err_msg.contains("cannot find"),
    "Should return not found error: {}",
    err_msg
);
```

- [ ] **Step 4: 运行测试验证全部通过**

Run: `cargo test -p peri-middlewares --lib -- subagent::tool 2>&1 | tail -5`
Expected: `test result: ok. X passed; 0 failed`

- [ ] **Step 5: Commit**

```bash
git add peri-middlewares/src/subagent/tool/tool_test.rs
git commit -m "test: update Agent tool error tests to assert Err() return

测试从 .unwrap() + .contains() 改为 .unwrap_err() + .contains()，
验证错误现在通过 Err() 返回而非 Ok(\"Error: ...\")。

Refs: spec/issues/2026-06-05-agent-tool-3-percent-error-rate-subagent-type-missing"
```

---

### Task 5: 全量验证 + 更新 issue

**Files:**
- Modify: `spec/issues/2026-06-05-agent-tool-3-percent-error-rate-subagent-type-missing.md`

- [ ] **Step 1: 运行全量测试和 clippy**

Run: `cargo test -p peri-middlewares 2>&1 | tail -3`
Expected: `test result: ok`

Run: `cargo clippy -p peri-middlewares 2>&1 | grep -E "^error"`
Expected: 无输出

- [ ] **Step 2: 更新 issue 状态**

在 `spec/issues/2026-06-05-agent-tool-3-percent-error-rate-subagent-type-missing.md` 的「状态变更记录」追加:

```markdown
| 2026-06-05 | Open | Fixed | agent | 修复 #1: 4 处 Ok("Error:") → Err()，参数描述强化 |
```

在「修复记录」下追加:

```markdown
### 修复 #1（2026-06-05）

- **操作人**：agent
- **用户原意**：Agent 工具错误应对监控系统可见，且 LLM 应更少遗漏 subagent_type 参数
- **修复内容**：
  1. `define.rs`: prompt 缺失 + subagent_type 缺失，2 处 `Ok("Error:")` → `Err()`
  2. `execute_bg.rs`: 并发上限 + 后台 subagent_type 缺失，2 处 `Ok("Error:")` → `Err()`
  3. `define.rs`: subagent_type 参数描述强调 REQUIRED unless fork=true
  4. `tool_test.rs`: 3 个测试断言从 `.unwrap()` 改为 `.unwrap_err()`
- **验证状态**：待验证
```

- [ ] **Step 3: Commit**

```bash
git add spec/issues/2026-06-05-agent-tool-3-percent-error-rate-subagent-type-missing.md
git commit -m "docs: update Agent tool error issue with fix #1 record

Refs: spec/issues/2026-06-05-agent-tool-3-percent-error-rate-subagent-type-missing"
```
