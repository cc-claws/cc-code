# Edit 工具错误可见性与诊断性升级

**日期**：2026-06-03
**状态**：Approved
**关联 Issue**：`spec/issues/2026-06-03-edit-tool-errors-invisible-and-retry-inefficient.md`
**关联 Issue**：`spec/issues/2026-06-03-edit-tool-tab-indent-mismatch.md`

---

## 问题

Edit 工具（`peri-middlewares/src/tools/filesystem/edit.rs`）存在 3 个系统性缺陷：

1. **错误不可见**：所有错误以 `Ok("Error: ...")` 返回，`is_error` 恒为 false。284 次失败对监控系统和 TUI 错误样式完全不可见。
2. **错误信息缺乏可操作性**：`old_string not found` 只说"找不到"，不告诉 Agent 文件哪里变了。62% 的失败因文件已被之前的 Edit 修改（内容过期）。
3. **not_unique 缺少定位**：70 次 `not_unique` 错误只说"有 N 处匹配"，不列出具体行号，Agent 无法决定扩大哪部分上下文。

数据来源：`agent-defect-analyzer --focus edit`，6,233 次调用中 284 次失败（4.6%）。

## 设计

### 改动 1：错误返回方式改为 `Err()`

**文件**：`edit.rs`

5 处 `Ok(format!("Error: ..."))` 改为 `Err(format!("Error: ...").into())`：

| 行号 | 场景 |
|------|------|
| `:85` | `old_string` 为空 |
| `:93` | 文件不存在 |
| `:124` | `old_string not found`（replace_all 模式） |
| `:151` | `old_string not found`（单次替换模式） |
| `:157` | `old_string not unique` |

**效果**：
- `is_error` 变为 true → `tool_errors` 分析器可见
- TUI 显示红色错误样式（而非普通文本）
- HookMiddleware `PostToolUseFailure` 事件触发
- `tool_dispatch.rs` 的 `run_on_error` 被调用

**不需改动 `tool_dispatch.rs`**：框架层已正确处理 `Err()` → `ToolResult::error()` → `is_error: true`。

### 改动 2：模糊匹配提示（`old_string not found` 场景）

**文件**：`edit.rs`，新增 `build_not_found_hint(content: &str, old_string: &str) -> String`

逻辑流程：

```
old_string.len() > 5000?
  ├─ YES → 返回 "建议先 Read 此文件获取最新内容再重试。"
  └─ NO → 取 old_string 前 5 行（prefix）
           content.contains(prefix)?
             ├─ YES → 计算前缀在文件中的行号范围
             │        返回 "old_string 前 5 行匹配到第 X-Y 行，但整体不匹配。
             │               文件可能已被修改。建议先 Read 此文件获取最新内容再重试。"
             └─ NO → 回退：行数近似匹配
                      在文件中找与 old_string 行数相同的滑动窗口
                      统计共同行数最多的位置
                      报告差异行号
                      返回 "最接近的匹配在文件第 X-Y 行，但存在差异。
                             建议先 Read 此文件获取最新内容再重试。"
```

**前缀匹配实现**：

```rust
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
                prefix_lines.len(), line_start, line_end
            );
        }
    }

    // 策略 2：行数近似匹配（回退）
    // 找与 old_string 行数相同的滑动窗口，统计共同行数
    let old_lines: Vec<&str> = old_string.lines().collect();
    let file_lines: Vec<&str> = content.lines().collect();
    let window_len = old_lines.len();

    if window_len > 0 && window_len <= file_lines.len() {
        let mut best_pos = 0;
        let mut best_common = 0;

        for start in 0..=file_lines.len().saturating_sub(window_len) {
            let window = &file_lines[start..start + window_len];
            let common = window.iter()
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

**性能保护**：
- `old_string.len() > 5000` 时跳过
- 前缀匹配用 `content.find()`，O(N) 线性搜索
- 行数近似匹配用滑动窗口，O(N×M) 但 N/M 受 5000 字符限制约束
- 超长文件本身不受影响（限制的是 old_string 长度）

### 改动 3：not_unique 行号提示

**文件**：`edit.rs`，修改 `:157` 的错误信息

现有代码只统计匹配次数。改为找到每个匹配的起始位置并转换为行号：

```rust
// 现有
if occurrences > 1 {
    return Ok(format!(
        "Error: old_string is not unique in {} (found {} occurrences). \
         Please provide more context or set replace_all to true.",
        resolved.display(), occurrences
    ));
}

// 改为
if occurrences > 1 {
    let locations: Vec<String> = content
        .match_indices(old_string)
        .map(|(offset, _)| {
            let line = content[..offset].lines().count() + 1;
            let end_line = line + old_string.lines().count() - 1;
            format!("第 {}-{} 行", line, end_line)
        })
        .collect();
    return Err(format!(
        "Error: old_string is not unique in {} (found {} occurrences).\n\
         匹配位置：{}。\n\
         请提供更多上下文使其唯一，或设置 replace_all=true。",
        resolved.display(),
        occurrences,
        locations.join("、")
    ).into());
}
```

**限制**：当 `occurrences > 10` 时只报告前 10 个位置 + 总数，避免错误信息过长。

### 改动 4：测试更新

**文件**：`edit_test.rs`

- 所有 Edit 错误场景的断言从 `assert!(result.is_ok())` 改为 `assert!(result.is_err())`
- 验证错误文本包含关键词（如 "not found"、"建议先 Read"、"匹配位置"）
- 新增测试：
  - `test_edit_not_found_with_fuzzy_prefix_match` —— 前缀匹配提示
  - `test_edit_not_found_with_line_diff_hint` —— 回退行数匹配
  - `test_edit_not_found_long_old_string_skip_fuzzy` —— 超长 old_string 跳过
  - `test_edit_not_unique_shows_line_ranges` —— not_unique 行号提示
  - `test_edit_not_unique_many_occurrences_truncated` —— 超多次匹配截断

## 范围外

- ❌ 不改 `tool_dispatch.rs`（框架层已正确处理 Err）
- ❌ 不加 Edit 失败自动重试（Agent 侧策略）
- ❌ 不改 inline diff 设计（`is_error=true → 丢弃缓存` 行为不变）
- ❌ 不改 Read/Write 工具的 is_error 问题
- ❌ 不改 Bash 等其他工具的 is_error 问题

## 风险

| 风险 | 缓解 |
|------|------|
| `Err()` 路径触发 `run_on_error` 影响中间件链 | 这是期望行为；现有中间件的 `on_error` 只做日志/telemetry，不改变状态 |
| 模糊匹配在极端大文件上性能问题 | `old_string.len() > 5000` 跳过；行数近似匹配的滑动窗口受 old_string 行数约束 |
| 错误信息变长导致 context 消耗增加 | 提示文本控制在 200 字符以内；not_unique 超过 10 个匹配时截断 |
