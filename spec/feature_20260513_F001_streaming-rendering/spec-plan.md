# 实施计划: 20260513_F001 - streaming-rendering

## 依赖图

```
Step 1 (rendered_prefix_len 字段)
  |
  +---> Step 2 (find_last_block_boundary)
  |       |
  |       +---> Step 3 (ensure_rendered_incremental)
  |               |
  |               +---> Step 4 (render_one 使用增量解析)
  |
Step 5 (prefix_stable_len 优化) -- 独立于 Steps 2-4
  |
  +---> Step 6 (is_cosmetic_change)
  |       |
  |       +---> Step 7 (集成 cosmetic 检测到 rebuild)
  |
Step 8 (测试) -- 依赖上述所有步骤
```

Steps 1-4（增量 markdown）和 Steps 5-7（前缀/cosmetic 优化）可并行开发。Step 8 覆盖两条线。

---

## Step 1: ContentBlockView::Text 新增 rendered_prefix_len 字段

**文件:** `peri-tui/src/ui/message_view.rs`

**改动:**

1. `ContentBlockView::Text` 新增两个字段：
   - `rendered_prefix_len: usize` — 已渲染到 `raw` 的字节偏移
   - `rendered_prefix_lines: usize` — `rendered` 中对应前缀的行数（避免重解析计数）

2. 更新所有 `ContentBlockView::Text` 构造点，添加 `rendered_prefix_len: 0, rendered_prefix_lines: 0`：
   - `from_base_message_with_cwd()` 中的 5 处 Text 构造
   - `append_chunk()` 创建新 Text block
   - `build_streaming_bubble()` in `message_pipeline.rs`

3. `PartialEq` impl：忽略 `rendered_prefix_len` 和 `rendered_prefix_lines`（缓存字段，非语义）
4. `Hash` impl：忽略这两个字段（同上）

---

## Step 2: 实现 find_last_block_boundary()

**文件:** `peri-tui/src/ui/markdown/mod.rs`

**新增函数:** `pub fn find_last_block_boundary(text: &str, prefix_len: usize) -> usize`

**算法:**
1. 从 `text[..prefix_len]` 的末尾向前扫描
2. 维护 `in_code_fence: bool` 状态（遇 ````` `` 翻转）
3. 遇到 `\n\n` 且不在代码围栏内 → 返回该字节位置
4. 扫到 0 仍未找到 → 返回 0（全量重解析）

**边界情况:**
- `prefix_len == 0` → 返回 0
- 文本中间有未闭合的代码围栏 → 回退到围栏起始前
- 无双换行 → 返回 0

---

## Step 3: 实现 ensure_rendered_incremental()

**文件:** `peri-tui/src/ui/markdown/mod.rs`

**新增函数:** `pub fn ensure_rendered_incremental(block: &mut ContentBlockView, max_width: usize)`

**逻辑:**
1. `!dirty` 或 `raw.len() == rendered_prefix_len` → 提前返回
2. 调用 `find_last_block_boundary(raw, rendered_prefix_len)` 得 `last_stable_boundary`
3. 三条路径：
   - `last_stable_boundary == rendered_prefix_len`（前文稳定）：只解析 `raw[rendered_prefix_len..]`，追加到 `rendered.lines` 末尾
   - `0 < last_stable_boundary < rendered_prefix_len`（有不稳定块）：保留 `rendered.lines[..rendered_prefix_lines]`，重解析 `raw[last_stable_boundary..]`，拼接
   - `last_stable_boundary == 0`：全量 `parse_markdown(raw, width)` 兜底
4. 更新 `rendered_prefix_len = raw.len()`、`rendered_prefix_lines = rendered.lines.len()`、`dirty = false`

**行数映射:** `rendered_prefix_lines` 记录前缀对应的渲染行数，无需重解析计数。

---

## Step 4: render_one 使用增量解析

**文件:** `peri-tui/src/ui/render_thread.rs`

**改动（`render_one` 方法，约 line 162）:**

- 将 `ensure_rendered(block, width)` 替换为 `ensure_rendered_incremental(block, width)`
- UserBubble 路径不变（创建后内容不可变，只渲染一次）

---

## Step 5: rebuild() 实现 prefix_stable_len 优化

**文件:** `peri-tui/src/ui/render_thread.rs`

**改动（`rebuild` 方法）:**

1. 保存旧消息引用：`let old_last_messages = std::mem::replace(&mut self.last_messages, messages.clone());`
2. 计算 `new_hashes`
3. 扫描前缀：从 index 0 开始，找到第一个 `new_hashes[i] != old_hashes[i]` 的位置，之前即为 `prefix_stable_len`
4. 只从 `prefix_stable_len` 开始遍历渲染
5. 拼接：前缀的 `message_lines` 直接复用，只拼接尾部新渲染结果

**截断安全:** `new_len < old_len` 时 `prefix_stable_len` 截断到 `min(old, new)`，`message_lines.resize(new_len)` 自动移除多余条目。

---

## Step 6: 实现 is_cosmetic_change()

**文件:** `peri-tui/src/ui/render_thread.rs`

**新增函数:** `fn is_cosmetic_change(old: &MessageViewModel, new: &MessageViewModel) -> bool`

**判定规则:**

| 场景 | 结果 | 理由 |
|------|------|------|
| AssistantBubble blocks 相同，仅 is_streaming 变化 | cosmetic | 渲染输出一致 |
| SubAgentGroup recent_messages + final_result 相同，仅 is_running 变化 | cosmetic | 渲染输出一致 |
| ToolBlock collapsed 变化 | 非 cosmetic | 影响行数 |
| blocks/recent_messages 内容变化 | 非 cosmetic | 文本内容改变 |

---

## Step 7: 集成 cosmetic 检测到 rebuild()

**文件:** `peri-tui/src/ui/render_thread.rs`

**改动（rebuild() 渲染循环内）:**

在 `prefix_stable_len` 之后的渲染循环中，对 hash 不同的消息先检查 `is_cosmetic_change()`：
- 如果是 cosmetic → 复用 `old_message_lines[i]`，跳过 `render_one()`
- 否则 → 正常调用 `render_one()`

注意：需要保存 `old_message_lines`（在 resize/`message_lines` 覆盖前），可使用 `std::mem::take` + put-back 模式或在循环前 clone。

---

## Step 8: 单元测试

### A. markdown/mod.rs 内联测试

| 测试名 | 场景 |
|--------|------|
| `test_find_last_block_boundary_basic` | 双换行边界检测 |
| `test_find_last_block_boundary_code_fence` | 代码围栏内跳过空行 |
| `test_find_last_block_boundary_unclosed_fence` | 未闭合围栏回退 |
| `test_find_last_block_boundary_empty` | 空文本返回 0 |
| `test_ensure_rendered_incremental_basic` | 增量解析只追加新行 |
| `test_ensure_rendered_incremental_full_fallback` | 无边界时全量重解析 |
| `test_ensure_rendered_incremental_code_block_recovery` | 代码块闭合后的正确处理 |

### B. render_thread.rs 内联测试

| 测试名 | 场景 |
|--------|------|
| `test_rebuild_prefix_stable_len` | 尾部变化时前缀不重渲染 |
| `test_is_cosmetic_change_streaming_flag` | is_streaming 变化检测为 cosmetic |
| `test_is_cosmetic_change_subagent_running` | is_running 变化检测为 cosmetic |
| `test_is_cosmetic_change_blocks_differ` | 实际内容变化非 cosmetic |
| `test_rebuild_cosmetic_change_reuses_cache` | 端到端：streaming→reconcile 复用缓存 |

---

## 风险与缓解

| 风险 | 缓解 |
|------|------|
| `rendered_prefix_lines` 跟踪不准确 | 增量解析兜底路径校验行数一致性，debug 断言 |
| `find_last_block_boundary` 代码围栏边界情况 | 初版仅支持 ````` ``，后续按需扩展 ``~~~`` |
| `aggregate_tool_groups` 导致前缀失效 | hash 对比自然检测到合并变化，前缀从变化点断开 |
| Resize 使所有缓存失效 | 已有 `message_hashes.clear()` 强制全量重渲染，增量解析在 width 变化时自动全量回退 |

## 无新 crate 依赖

所有改动使用现有代码结构，增量解析使用已有的 `pulldown-cmark`。
