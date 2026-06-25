# Fix `i == last_idx` Cache Isolation Break Fix Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix Anthropic `i == last_idx` fallback that incorrectly marks the dynamic system block as cached, breaking the design intent of `__SYSTEM_PROMPT_DYNAMIC_BOUNDARY__`.

**Architecture:** Replace the unconditional `i == last_idx` fallback with a conditional check — only mark the last block as cache_control when no preceding block already has it. This preserves the fallback for the 1-block (no-boundary) edge case while restoring the intended cache isolation for the 2-block split case.

**Tech Stack:** Rust, serde_json, Anthropic Prompt Caching

---

### Task 1: Write tests for cache_control assignment in system blocks

**Files:**
- Create: `peri-agent/src/llm/anthropic_test.rs` (append tests)

- [ ] **Step 1: Add test — 2-block split (boundary present) only marks static block**

```rust
/// 2-block split: static block already has cache_control → last block should NOT get it.
#[test]
fn test_system_blocks_cache_control_two_blocks() {
    // Simulate split_system_blocks output: [static(cached), dynamic(not cached)]
    let blocks = vec![
        cache::SystemPromptBlock { text: "static".into(), cache_control: true },
        cache::SystemPromptBlock { text: "dynamic with date/cwd/language".into(), cache_control: false },
    ];
    let json_blocks = build_system_blocks_json(&blocks);
    assert_eq!(json_blocks.len(), 2);
    // Block 0: already cache_control=true from split_system_blocks
    assert_eq!(json_blocks[0]["cache_control"]["type"], "ephemeral");
    // Block 1: should NOT get cache_control (dynamic, no fallback needed since Block 0 covers it)
    assert!(!json_blocks[1].as_object().unwrap().contains_key("cache_control"));
}
```

- [ ] **Step 2: Run test to verify it FAILS**

Run: `cargo test -p peri-agent --lib test_system_blocks_cache_control_two_blocks`
Expected: FAIL — Block 1 still has `cache_control` due to `i == last_idx`

- [ ] **Step 3: Add test — 1-block (no boundary) still marks single block**

```rust
/// 1-block (no boundary): no preceding cache_control → last block gets fallback.
#[test]
fn test_system_blocks_cache_control_single_block() {
    let blocks = vec![
        cache::SystemPromptBlock { text: "plain prompt without boundary".into(), cache_control: false },
    ];
    let json_blocks = build_system_blocks_json(&blocks);
    assert_eq!(json_blocks.len(), 1);
    // Single block: no preceding cache_control, fallback adds it
    assert_eq!(json_blocks[0]["cache_control"]["type"], "ephemeral");
}
```

- [ ] **Step 4: Run test to verify it PASSES** (current behavior already passes for 1-block)

Run: `cargo test -p peri-agent --lib test_system_blocks_cache_control_single_block`
Expected: PASS

- [ ] **Step 5: Add test — 3+ blocks (middleware + split) only marks static**

```rust
/// 3 blocks (middleware Chunks + split output): static block in middle has cache_control.
#[test]
fn test_system_blocks_cache_control_middleware_blocks() {
    let blocks = vec![
        cache::SystemPromptBlock { text: "middleware chunk 1".into(), cache_control: false },
        cache::SystemPromptBlock { text: "middleware chunk 2".into(), cache_control: false },
        cache::SystemPromptBlock { text: "static".into(), cache_control: true },
        cache::SystemPromptBlock { text: "dynamic".into(), cache_control: false },
    ];
    let json_blocks = build_system_blocks_json(&blocks);
    assert_eq!(json_blocks.len(), 4);
    // Blocks 0-1: middleware, no cache_control
    assert!(!json_blocks[0].as_object().unwrap().contains_key("cache_control"));
    assert!(!json_blocks[1].as_object().unwrap().contains_key("cache_control"));
    // Block 2: static, has cache_control
    assert_eq!(json_blocks[2]["cache_control"]["type"], "ephemeral");
    // Block 3: dynamic, should NOT get fallback (Block 2 already has it)
    assert!(!json_blocks[3].as_object().unwrap().contains_key("cache_control"));
}
```

- [ ] **Step 6: Run test to verify it FAILS**

Run: `cargo test -p peri-agent --lib test_system_blocks_cache_control_middleware_blocks`
Expected: FAIL — Block 3 still has cache_control

- [ ] **Step 7: Add test — all blocks false, fallback applies**

```rust
/// All blocks cache_control=false: fallback applies to last block.
#[test]
fn test_system_blocks_cache_control_all_false_fallback() {
    let blocks = vec![
        cache::SystemPromptBlock { text: "chunk 1".into(), cache_control: false },
        cache::SystemPromptBlock { text: "chunk 2".into(), cache_control: false },
    ];
    let json_blocks = build_system_blocks_json(&blocks);
    assert_eq!(json_blocks.len(), 2);
    // Block 0: no cache_control
    assert!(!json_blocks[0].as_object().unwrap().contains_key("cache_control"));
    // Block 1: fallback (no preceding cache_control)
    assert_eq!(json_blocks[1]["cache_control"]["type"], "ephemeral");
}
```

- [ ] **Step 8: Run test to verify it PASSES**

Run: `cargo test -p peri-agent --lib test_system_blocks_cache_control_all_false_fallback`
Expected: PASS

---

### Task 2: Implement the fix in invoke.rs

**Files:**
- Modify: `peri-agent/src/llm/anthropic/invoke.rs:324-340`

- [ ] **Step 1: Extract system blocks JSON construction to a testable function**

Add a `pub(super)` function so tests can call it without constructing a full `LlmRequest`:

```rust
/// 将 SystemPromptBlock 列表转换为 Anthropic system blocks JSON。
/// cache_control 规则：
/// - 已有 cache_control=true 的 block 保留标记
/// - 仅当所有前置 block 都没有 cache_control 时，最后一个 block 获得 fallback cache_control
pub(super) fn build_system_blocks_json(blocks: &[SystemPromptBlock]) -> Vec<Value> {
    let has_cached = blocks.iter().any(|b| b.cache_control);
    let last_idx = blocks.len().saturating_sub(1);
    blocks
        .iter()
        .enumerate()
        .map(|(i, b)| {
            let mut block = json!({"type": "text", "text": &b.text});
            if b.cache_control || (i == last_idx && !has_cached) {
                block["cache_control"] = json!({"type": "ephemeral"});
            }
            block
        })
        .collect()
}
```

Place this function in `invoke.rs` immediately before `build_request_body`.

- [ ] **Step 2: Update build_request_body to call the new function**

Replace lines 325-339 in `invoke.rs`:

```rust
// Before (lines 325-339):
if adapter.enable_cache {
    // system 多块格式：静态块 + 最后一块标记 cache_control
    if !system_blocks.is_empty() {
        let last_idx = system_blocks.len() - 1;
        let blocks_json: Vec<Value> = system_blocks
            .iter()
            .enumerate()
            .map(|(i, b)| {
                let mut block = json!({"type": "text", "text": &b.text});
                if b.cache_control || i == last_idx {
                    block["cache_control"] = json!({"type": "ephemeral"});
                }
                block
            })
            .collect();
        body["system"] = Value::Array(blocks_json);
    }
}

// After:
if adapter.enable_cache {
    if !system_blocks.is_empty() {
        body["system"] = Value::Array(build_system_blocks_json(&system_blocks));
    }
}
```

- [ ] **Step 3: Update the comment above (line 325)**

Replace `// system 多块格式：静态块 + 最后一块标记 cache_control` with:

```rust
// system 多块格式：静态块已有 cache_control → 动态块不重复标记
```

---

### Task 3: Verify all tests pass

**Files:**
- Test: `peri-agent/src/llm/anthropic_test.rs`

- [ ] **Step 1: Run the new test suite**

```bash
cargo test -p peri-agent --lib test_system_blocks_cache_control
```

Expected: 4 tests PASS

- [ ] **Step 2: Run the full anthropic test suite**

```bash
cargo test -p peri-agent --lib anthropic_test
```

Expected: All tests PASS (existing split_system_blocks + messages_to_anthropic + cache_control tests must not regress)

- [ ] **Step 3: Run clippy + fmt**

```bash
cargo fmt -p peri-agent
cargo clippy -p peri-agent --lib
```

Expected: No warnings or errors

- [ ] **Step 4: Commit**

```bash
git add peri-agent/src/llm/anthropic/invoke.rs peri-agent/src/llm/anthropic_test.rs
git commit -m "fix(anthropic): remove i==last_idx fallback when static block already cached

The i==last_idx fallback in build_request_body unconditionally marked
the last (dynamic) system block as cached, breaking the design intent
of __SYSTEM_PROMPT_DYNAMIC_BOUNDARY__. Now the fallback only applies
when no preceding block already has cache_control=true.

This means:
- 2-block split (normal): only static Block 0 cached ✓
- 1-block (no boundary): single block cached (necessary fallback) ✓
- 3+ blocks (middleware): only static block cached ✓

Co-Authored-By: deepseek-v4-pro <deepseek-ai@claude-code-best.win>"
```

---

## Self-Review

### 1. Spec coverage
- [x] Fix `i == last_idx` in `build_request_body` → Task 2
- [x] Preserve 1-block fallback → Task 1 tests cover this, Task 2 implementation handles it
- [x] Extract to testable function → Task 2 Step 1
- [x] Tests for all block count scenarios → Task 1 (2-block, 1-block, 3+ blocks, all-false)
- [x] No regression on existing tests → Task 3 Step 2

### 2. Placeholder scan
No TBD, TODO, or placeholders — all steps have exact code.

### 3. Type consistency
- `build_system_blocks_json` takes `&[SystemPromptBlock]` returns `Vec<Value>` — consistent with existing types in `invoke.rs`
- Import `SystemPromptBlock` from `cache` module (already used in invoke.rs)
- Test code uses `cache::SystemPromptBlock` (same as existing tests)

---

### Task 4 (optional): Verify cache behavior in integration

- [ ] **Step 1: Confirm with Anthropic docs** — the Anthropic API allows 1-4 cache_control breakpoints total (system + messages). Removing the redundant system breakpoint frees capacity for user-message breakpoints.
- [ ] **Step 2: Consider monitoring** — after deploy, check Langfuse for `cache_read_input_tokens` vs `cache_creation_input_tokens` to verify the dynamic block is no longer being re-cached on every turn.
