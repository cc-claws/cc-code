# Edit 错误可见性与诊断性升级 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 修复 Edit 工具 3 个缺陷——错误不可见（is_error）、错误信息缺乏可操作性、not_unique 缺少定位。

**Architecture:** 改动集中在 `edit.rs`（错误返回 + 模糊匹配 + not_unique 定位）和 `edit_test.rs`（断言更新 + 新测试）。不涉及框架层（`tool_dispatch.rs`）。

**Tech Stack:** Rust, `serde_json`, `tempfile`（测试）, `async-trait`

**Spec:** `docs/superpowers/specs/2026-06-03-edit-error-visibility-and-diagnostics-design.md`

---

## File Structure

| 文件 | 职责 |
|------|------|
| `peri-middlewares/src/tools/filesystem/edit.rs` | Edit 工具实现——5 处 Ok→Err + `build_not_found_hint` + not_unique 行号 |
| `peri-middlewares/src/tools/filesystem/edit_test.rs` | 测试——现有断言更新 + 5 个新测试 |

---

### Task 1: 错误返回改为 Err()（is_error 标记修复）

**Files:**
- Modify: `peri-middlewares/src/tools/filesystem/edit.rs:84-86,92-94,123-128,150-155,156-163`
- Test: `peri-middlewares/src/tools/filesystem/edit_test.rs`

- [ ] **Step 1: 写失败测试——验证错误场景返回 Err**

修改 `edit_test.rs` 中 4 个错误场景的断言，从 `.unwrap()` + 文本检查改为 `is_err()` + 文本检查：

```rust
// test_edit_file_old_string_not_found（:22-34）
let result = tool
    .invoke(serde_json::json!({"file_path": "f.txt", "old_string": "missing", "new_string": "x"}))
    .await;
let err = result.unwrap_err();
assert!(
    err.to_string().contains("not found"),
    "should report not found: {err}"
);

// test_edit_file_ambiguous（:54-68）
let result = tool
    .invoke(
        serde_json::json!({"file_path": "f.txt", "old_string": "foo", "new_string": "bar"}),
    )
    .await;
let err = result.unwrap_err();
assert!(
    err.to_string().contains("not unique"),
    "should report ambiguity: {err}"
);

// test_edit_file_not_found（:70-84）
let result = tool
    .invoke(
        serde_json::json!({"file_path": "ghost.txt", "old_string": "x", "new_string": "y"}),
    )
    .await;
let err = result.unwrap_err();
assert!(
    err.to_string().contains("File not found"),
    "should report file not found: {err}"
);

// test_edit_file_empty_old_string_rejected（:87-102）
let result = tool
    .invoke(serde_json::json!({"file_path": "f.txt", "old_string": "", "new_string": "x", "replace_all": true}))
    .await;
let err = result.unwrap_err();
assert!(
    err.to_string().contains("cannot be empty"),
    "empty old_string should be rejected: {err}"
);
let content = std::fs::read_to_string(dir.path().join("f.txt")).unwrap();
assert_eq!(content, "hello world", "file should not be modified");
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test -p peri-middlewares --lib -- tools::filesystem::edit::tests`
Expected: 4 个测试 FAIL（`Result::unwrap()` on an `Err` value 改为 `unwrap_err()` on an `Ok` value——因为实现还没改）

注意：Step 1 先改测试为 `unwrap_err()`，但实现还没改，所以这 4 个测试此时**通过**（实现仍返回 Ok）。需要先确认：改为 `unwrap_err()` 后，因为实现仍返回 Ok，测试会 FAIL（`unwrap_err()` on Ok）。这是正确的 TDD 流程。

- [ ] **Step 3: 改实现——5 处 Ok→Err**

在 `edit.rs` 中，将 5 处 `Ok(format!("Error: ..."))` 改为 `Err(format!("Error: ...").into())`：

```rust
// :84-86 old_string 为空
if old_string.is_empty() {
    return Err("Error: old_string cannot be empty".into());
}

// :92-94 文件不存在
Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
    return Err(format!("Error: File not found at {file_path}").into());
}

// :123-128 replace_all 模式 not found
if !content.contains(old_string) {
    return Err(format!(
        "Error: old_string not found in {}",
        resolved.display()
    ).into());
}

// :150-155 单次替换 not found
if occurrences == 0 {
    return Err(format!(
        "Error: old_string not found in {}",
        resolved.display()
    ).into());
}

// :156-163 not unique
if occurrences > 1 {
    return Err(format!(
        "Error: old_string is not unique in {} (found {} occurrences). \
         Please provide more context or set replace_all to true.",
        resolved.display(),
        occurrences
    ).into());
}
```

- [ ] **Step 4: 运行测试确认通过**

Run: `cargo test -p peri-middlewares --lib -- tools::filesystem::edit::tests`
Expected: 所有 8 个测试 PASS

- [ ] **Step 5: Commit**

```bash
git add peri-middlewares/src/tools/filesystem/edit.rs peri-middlewares/src/tools/filesystem/edit_test.rs
git commit -m "fix: Edit 工具错误改为 Err() 返回，is_error 正确标记为 true

5 处 Ok(\"Error: ...\") 改为 Err(...into())，使 tool_dispatch 正确设置
is_error=true。现有 4 个错误场景测试更新为 unwrap_err()。

Co-Authored-By: glm-5.1 <zai-org@claude-code-best.win>"
```

---

### Task 2: 模糊匹配提示函数 build_not_found_hint

**Files:**
- Modify: `peri-middlewares/src/tools/filesystem/edit.rs`
- Test: `peri-middlewares/src/tools/filesystem/edit_test.rs`

- [ ] **Step 1: 写失败测试——前缀匹配提示**

在 `edit_test.rs` 末尾新增：

```rust
#[tokio::test]
async fn test_edit_not_found_with_fuzzy_prefix_match() {
    let dir = tempfile::tempdir().unwrap();
    // 文件有 3 行，old_string 前 2 行匹配但第 3 行不同
    std::fs::write(dir.path().join("f.txt"), "line1\nline2\nchanged\n").unwrap();
    let tool = EditFileTool::new(dir.path().to_str().unwrap());
    let err = tool
        .invoke(serde_json::json!({
            "file_path": "f.txt",
            "old_string": "line1\nline2\noriginal\n",
            "new_string": "x"
        }))
        .await
        .unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("前 3 行匹配到文件第 1-3 行"), "应报告前缀匹配位置: {msg}");
    assert!(msg.contains("建议先 Read"), "应建议重新 Read: {msg}");
}
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test -p peri-middlewares --lib -- test_edit_not_found_with_fuzzy_prefix_match`
Expected: FAIL（错误信息尚未包含"前 N 行匹配到"）

- [ ] **Step 3: 写失败测试——行数近似匹配回退**

```rust
#[tokio::test]
async fn test_edit_not_found_with_line_diff_hint() {
    let dir = tempfile::tempdir().unwrap();
    // 前 5 行完全不匹配，但中间有近似区域
    std::fs::write(
        dir.path().join("f.txt"),
        "aaa\nbbb\nccc\nddd\neee\nline1\nline2_CHANGED\nline3\nfff\nggg\n",
    )
    .unwrap();
    let tool = EditFileTool::new(dir.path().to_str().unwrap());
    let err = tool
        .invoke(serde_json::json!({
            "file_path": "f.txt",
            "old_string": "line1\nline2\nline3\n",
            "new_string": "x"
        }))
        .await
        .unwrap_err();
    let msg = err.to_string();
    // 前缀 "line1" 应该能匹配到第 6 行——这个场景前缀匹配就够
    assert!(
        msg.contains("建议先 Read") || msg.contains("最接近的匹配"),
        "应提供匹配提示: {msg}"
    );
}
```

- [ ] **Step 4: 写失败测试——超长 old_string 跳过模糊匹配**

```rust
#[tokio::test]
async fn test_edit_not_found_long_old_string_skip_fuzzy() {
    let dir = tempfile::tempdir().unwrap();
    let long_line = "x".repeat(1000);
    let content = format!("{long_line}\n");
    std::fs::write(dir.path().join("f.txt"), &content).unwrap();
    // old_string > 5000 字符
    let giant_old = "y".repeat(6000);
    let tool = EditFileTool::new(dir.path().to_str().unwrap());
    let err = tool
        .invoke(serde_json::json!({
            "file_path": "f.txt",
            "old_string": giant_old,
            "new_string": "x"
        }))
        .await
        .unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("建议先 Read"), "超长 old_string 应只给建议: {msg}");
    assert!(!msg.contains("匹配到文件"), "超长 old_string 不应做模糊匹配: {msg}");
}
```

- [ ] **Step 5: 实现 build_not_found_hint 函数**

在 `edit.rs` 中（`invoke` 方法之前）新增：

```rust
/// 为 old_string not found 错误构建模糊匹配提示。
///
/// 策略 1：取 old_string 前 5 行做前缀匹配，报告匹配到的行号范围。
/// 策略 2：前缀匹配失败时，用滑动窗口找最接近的区域，报告差异行数。
/// old_string > 5000 字符时跳过，仅返回建议 Read 提示。
fn build_not_found_hint(content: &str, old_string: &str) -> String {
    const MAX_FUZZY_LEN: usize = 5000;
    if old_string.len() > MAX_FUZZY_LEN {
        return "建议先 Read 此文件获取最新内容再重试。".to_string();
    }

    // 策略 1：前缀匹配
    let prefix_lines: Vec<&str> = old_string.lines().take(5).collect();
    let prefix: String = prefix_lines.join("\n");
    if !prefix.is_empty() {
        if let Some(byte_offset) = content.find(&prefix) {
            let line_start = content[..byte_offset].lines().count() + 1;
            let line_end = line_start + prefix_lines.len() - 1;
            return format!(
                "old_string 前 {} 行匹配到文件第 {}-{} 行，但整体不匹配。\
                 文件可能已被修改。建议先 Read 此文件获取最新内容再重试。",
                prefix_lines.len(),
                line_start,
                line_end
            );
        }
    }

    // 策略 2：行数近似匹配（回退）
    let old_lines: Vec<&str> = old_string.lines().collect();
    let file_lines: Vec<&str> = content.lines().collect();
    let window_len = old_lines.len();

    if window_len > 0 && window_len <= file_lines.len() {
        let mut best_pos = 0;
        let mut best_common = 0;

        for start in 0..=file_lines.len().saturating_sub(window_len) {
            let window = &file_lines[start..start + window_len];
            let common = window
                .iter()
                .zip(old_lines.iter())
                .filter(|(a, b)| a.trim() == b.trim())
                .count();
            if common > best_common {
                best_common = common;
                best_pos = start;
            }
        }

        if best_common > 0 {
            let line_start = best_pos + 1;
            let line_end = best_pos + window_len;
            let diff_count = window_len - best_common;
            return format!(
                "最接近的匹配在文件第 {}-{} 行（{} 行中有 {} 行不同）。\
                 建议先 Read 此文件获取最新内容再重试。",
                line_start, line_end, window_len, diff_count
            );
        }
    }

    "建议先 Read 此文件获取最新内容再重试。".to_string()
}
```

- [ ] **Step 6: 在 not found 错误路径中调用 build_not_found_hint**

修改 `edit.rs` 中两处 `old_string not found`（replace_all 模式 `:123-128` 和单次模式 `:150-155`）：

```rust
// replace_all 模式
if !content.contains(old_string) {
    let hint = build_not_found_hint(&content, old_string);
    return Err(format!(
        "Error: old_string not found in {}\n{hint}",
        resolved.display()
    ).into());
}

// 单次替换模式
if occurrences == 0 {
    let hint = build_not_found_hint(&content, old_string);
    return Err(format!(
        "Error: old_string not found in {}\n{hint}",
        resolved.display()
    ).into());
}
```

- [ ] **Step 7: 运行全部 Edit 测试确认通过**

Run: `cargo test -p peri-middlewares --lib -- tools::filesystem::edit::tests`
Expected: 所有 11 个测试 PASS（8 旧 + 3 新）

- [ ] **Step 8: Commit**

```bash
git add peri-middlewares/src/tools/filesystem/edit.rs peri-middlewares/src/tools/filesystem/edit_test.rs
git commit -m "feat: Edit not found 错误增加模糊匹配提示

新增 build_not_found_hint：前缀匹配→行数近似匹配回退。
old_string > 5000 字符时跳过。
3 个新测试覆盖前缀匹配、回退匹配、超长跳过。

Co-Authored-By: glm-5.1 <zai-org@claude-code-best.win>"
```

---

### Task 3: not_unique 行号提示

**Files:**
- Modify: `peri-middlewares/src/tools/filesystem/edit.rs`
- Test: `peri-middlewares/src/tools/filesystem/edit_test.rs`

- [ ] **Step 1: 写失败测试——not_unique 显示行号范围**

```rust
#[tokio::test]
async fn test_edit_not_unique_shows_line_ranges() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("f.txt"), "aaa\nfoo\nbbb\nfoo\nccc\n").unwrap();
    let tool = EditFileTool::new(dir.path().to_str().unwrap());
    let err = tool
        .invoke(serde_json::json!({
            "file_path": "f.txt",
            "old_string": "foo",
            "new_string": "bar"
        }))
        .await
        .unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("第 2-2 行"), "应报告第一个匹配行号: {msg}");
    assert!(msg.contains("第 4-4 行"), "应报告第二个匹配行号: {msg}");
    assert!(msg.contains("匹配位置"), "应包含匹配位置标签: {msg}");
}
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test -p peri-middlewares --lib -- test_edit_not_unique_shows_line_ranges`
Expected: FAIL（当前错误信息不含行号范围）

- [ ] **Step 3: 写失败测试——超多次匹配截断**

```rust
#[tokio::test]
async fn test_edit_not_unique_many_occurrences_truncated() {
    let dir = tempfile::tempdir().unwrap();
    // 15 次 "x\n"
    let content = "x\n".repeat(15);
    std::fs::write(dir.path().join("f.txt"), &content).unwrap();
    let tool = EditFileTool::new(dir.path().to_str().unwrap());
    let err = tool
        .invoke(serde_json::json!({
            "file_path": "f.txt",
            "old_string": "x",
            "new_string": "y"
        }))
        .await
        .unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("15 occurrences"), "应报告总匹配数: {msg}");
    // 最多报告 10 个位置
    let location_count = msg.matches("第").count();
    assert!(
        location_count <= 10,
        "超过 10 个匹配时应截断位置列表，实际 {location_count} 个: {msg}"
    );
}
```

- [ ] **Step 4: 实现 not_unique 行号提示**

修改 `edit.rs` 中 not_unique 错误（当前在 Task 1 已改为 `Err`）：

```rust
if occurrences > 1 {
    let locations: Vec<String> = content
        .match_indices(old_string)
        .take(10)
        .map(|(offset, _)| {
            let line = content[..offset].lines().count() + 1;
            let end_line = line + old_string.lines().count().saturating_sub(1);
            if end_line > line {
                format!("第 {}-{} 行", line, end_line)
            } else {
                format!("第 {} 行", line)
            }
        })
        .collect();
    let location_text = if occurrences > 10 {
        format!("{}（共 {} 处，仅显示前 10 处）", locations.join("、"), occurrences)
    } else {
        locations.join("、")
    };
    return Err(format!(
        "Error: old_string is not unique in {} (found {} occurrences).\n\
         匹配位置：{location_text}。\n\
         请提供更多上下文使其唯一，或设置 replace_all=true。",
        resolved.display(),
        occurrences
    ).into());
}
```

- [ ] **Step 5: 运行全部 Edit 测试确认通过**

Run: `cargo test -p peri-middlewares --lib -- tools::filesystem::edit::tests`
Expected: 所有 13 个测试 PASS

- [ ] **Step 6: Commit**

```bash
git add peri-middlewares/src/tools/filesystem/edit.rs peri-middlewares/src/tools/filesystem/edit_test.rs
git commit -m "feat: Edit not_unique 错误增加匹配行号定位

列出每个匹配位置的行号范围，超过 10 处时截断显示。
2 个新测试覆盖行号提示和截断。

Co-Authored-By: glm-5.1 <zai-org@claude-code-best.win>"
```

---

### Task 4: 最终验证

- [ ] **Step 1: 运行 peri-middlewares 全量测试**

Run: `cargo test -p peri-middlewares --lib`
Expected: 全部 PASS

- [ ] **Step 2: 运行 cargo build 确认编译通过**

Run: `cargo build -p peri-middlewares`
Expected: 编译成功，无 warning

- [ ] **Step 3: 运行 clippy**

Run: `cargo clippy -p peri-middlewares -- -D warnings`
Expected: 无 warning

- [ ] **Step 4: 更新 issue 状态**

在 `spec/issues/2026-06-03-edit-tool-errors-invisible-and-retry-inefficient.md` 的修复记录中追加：

```markdown
### 修复 #1（2026-06-03）

- **操作人**：agent
- **用户原意**：修复 Edit 工具错误不可见、错误信息缺乏可操作性、not_unique 缺少定位
- **修复内容**：
  1. 5 处 Ok("Error: ...") → Err()，is_error 正确标记为 true
  2. 新增 build_not_found_hint 模糊匹配提示（前缀匹配 + 行数近似回退）
  3. not_unique 错误列出匹配行号范围（超 10 处截断）
- **涉及 commit**：[待填]
- **验证状态**：待验证
```
