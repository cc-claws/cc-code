# 工具参数名别名（path → file_path）实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在 tool_dispatch 执行层添加参数名别名表，将 LLM 误用的 `path` 参数自动归一化为 `file_path`

**Architecture:** 在 `tool_dispatch.rs:274` 的 `input.clone()` 之后、`t.invoke(input)` 之前插入归一化逻辑。使用常量表 `PARAM_ALIASES` 定义别名映射，遍历 JSON object 的键做替换。与已有的 `TOOL_ALIASES`（工具名别名）模式对仗。

**Tech Stack:** Rust 2021, serde_json::Value, tracing

**影响范围:** `peri-agent/src/agent/executor/tool_dispatch.rs`（单文件修改，~30 行新增）

---

### Task 1: 添加 PARAM_ALIASES 常量表和归一化函数

**Files:**
- Modify: `peri-agent/src/agent/executor/tool_dispatch.rs:19` 附近

- [ ] **Step 1: 在 TOOL_ALIASES 下方添加 PARAM_ALIASES 常量和 normalize_params 函数**

```rust
/// 工具参数名别名表：LLM 输出的参数名 → 实际参数名。
/// 主要解决 Read/Write/Edit（file_path）与 Glob/Grep（path）之间的 LLM 参数名混淆。
const PARAM_ALIASES: &[(&str, &str)] = &[("path", "file_path")];

/// 将 LLM 有时会误用的参数名归一化为标准名。
/// 仅在有别名键且无目标键时才替换（不覆盖已有正确值）。
fn normalize_params(input: serde_json::Value) -> serde_json::Value {
    let mut obj = match input {
        serde_json::Value::Object(map) => map,
        _ => return input,
    };

    for (alias, real) in PARAM_ALIASES {
        if obj.contains_key(*alias) && !obj.contains_key(*real) {
            let value = obj.remove(*alias).unwrap();
            obj.insert(real.to_string(), value);
            tracing::warn!(
                alias = %alias,
                resolved = %real,
                "参数名别名归一化：LLM 使用了非标准参数名"
            );
        }
    }

    serde_json::Value::Object(obj)
}
```

- [ ] **Step 2: 在 tool_dispatch.rs:274 之后插入 normalize_params 调用**

在 `let input = call.input.clone();` 之后，修改为：

```rust
let input = call.input.clone();
let input = normalize_params(input); // 新增：参数名归一化
```

当前代码位置：

```rust
// 第 274 行
let input = call.input.clone();
let tool = resolve_tool(&call.name, all_tools);
```

改为：

```rust
// 第 274 行
let input = call.input.clone();
let input = normalize_params(input);
let tool = resolve_tool(&call.name, all_tools);
```

- [ ] **Step 3: 编译验证**

```bash
cargo build -p peri-agent 2>&1
```
Expected: 编译通过，无 warning（`tracing::warn!` 无未使用变量问题）

- [ ] **Step 4: 运行现有测试**

```bash
cargo test -p peri-agent --lib -- tool_dispatch 2>&1
```
Expected: 全部 PASS

- [ ] **Step 5: Commit**

```bash
git add peri-agent/src/agent/executor/tool_dispatch.rs
git commit -m "feat(tool_dispatch): add param alias normalization (path -> file_path)

LLM sometimes confuses Glob/Grep's 'path' with Read/Write/Edit's 'file_path'
due to Claude Code's split naming convention. Add PARAM_ALIASES table
to normalize 'path' -> 'file_path' at the execution layer without changing
the tool schema, mirroring the existing TOOL_ALIASES pattern.
"
```

---

### Task 2: 添加单元测试

**Files:**
- Read and modify: `peri-agent/src/agent/executor/tool_dispatch_test.rs`

- [ ] **Step 1: 先读现有测试文件了解风格**

```bash
grep -n "fn test_\|#\[test\]" peri-agent/src/agent/executor/tool_dispatch_test.rs | head -10
```

- [ ] **Step 2: 在测试文件末尾添加 normalize_params 测试**

```rust
#[test]
fn test_normalize_params_path_to_file_path() {
    // Arrange: input has "path" but no "file_path"
    let input = serde_json::json!({"path": "/tmp/test.rs", "offset": 10});

    // Act
    let normalized = super::normalize_params(input);

    // Assert: "path" → "file_path", "offset" unchanged
    assert_eq!(normalized["file_path"], "/tmp/test.rs");
    assert!(normalized.get("path").is_none());
    assert_eq!(normalized["offset"], 10);
}

#[test]
fn test_normalize_params_file_path_already_exists() {
    // Arrange: input has both "path" and "file_path" (LLM wrote both)
    let input = serde_json::json!({"path": "/tmp/wrong.rs", "file_path": "/tmp/right.rs"});

    // Act
    let normalized = super::normalize_params(input);

    // Assert: "file_path" unchanged, "path" 不清除（保守策略，不丢数据）
    assert_eq!(normalized["file_path"], "/tmp/right.rs");
    // "path" 保留原样（因为已经有了 file_path，不覆盖）
    assert_eq!(normalized["path"], "/tmp/wrong.rs");
}

#[test]
fn test_normalize_params_no_alias_present() {
    // Arrange: normal Read call with correct parameter names
    let input = serde_json::json!({"file_path": "/tmp/test.rs"});

    // Act
    let normalized = super::normalize_params(input);

    // Assert: no change
    assert_eq!(normalized["file_path"], "/tmp/test.rs");
}

#[test]
fn test_normalize_params_non_object_input() {
    // Arrange: input is a string (edge: unlikely but safe)
    let input = serde_json::Value::String("hello".to_string());

    // Act
    let normalized = super::normalize_params(input);

    // Assert: returned as-is
    assert_eq!(normalized, serde_json::Value::String("hello".to_string()));
}
```

- [ ] **Step 3: 运行新测试验证通过**

```bash
cargo test -p peri-agent --lib -- test_normalize_params 2>&1
```
Expected: 4 tests PASS

- [ ] **Step 4: Commit**

```bash
git add peri-agent/src/agent/executor/tool_dispatch_test.rs
git commit -m "test(tool_dispatch): add normalize_params unit tests

Covers: path->file_path, file_path already exists, no alias, non-object input."
```

---

### Task 3: 确认函数可见性 + 自审

- [ ] **Step 1: 检查 normalize_params 和 PARAM_ALIASES 的可见性**

`normalize_params` 只需在 `tool_dispatch_test.rs` 中可见（同模块 `#[path = "..."]` 引用），用 `fn`（无 pub）即可。`PARAM_ALIASES` 同理。`tracing::warn!` 宏需要 `use tracing;` 已存在于文件顶部。

- [ ] **Step 2: 运行 clippy**

```bash
cargo clippy -p peri-agent -- -D warnings 2>&1
```
Expected: 无新 warning

- [ ] **Step 3: 全量测试**

```bash
cargo test -p peri-agent 2>&1
```
Expected: 全部 PASS

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "chore: final verification pass for param alias normalization"
```
