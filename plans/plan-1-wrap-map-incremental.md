# Plan 1: wrap_map 增量化（v2，审核修正版）

## 目标

消除 `build_wrap_map` 的 O(total_lines) 全量计算。流式更新时只重算变化部分的 wrap_map，前缀稳定区直接复用。

## 问题分析

### 当前数据流

```
Rebuild(vms)
  → rebuild()
    → hash diff → prefix_stable_len (跳过未变化的 VM 的 render_one)
    → message_lines[i] → all_lines（全量拼接）→ deduped（全局连续空行过滤）
    → build_wrap_map(&deduped, width)  ← 🔴 对所有行全量计算
    → cache.message_offsets = offsets（基于 all_lines，非 deduped） ← 🟡 现有 bug
    → cache.wrap_map = wrap_map（基于 deduped）
```

### 瓶颈

1. `build_wrap_map` 对全部 `deduped` 行（可能 3000+）调用 `Paragraph::line_count`
2. 即使 hash diff 跳过了 90% 的 VM 渲染，wrap 计算仍是全量的
3. 流式时 100ms 节流 → 每秒 10 次全量 wrap 计算
4. `build_wrap_map` 占渲染线程总时间的 80-95%（~2-6ms / 总 ~2.5-7ms）

### 关键观察

- `message_offsets` 已记录每条 VM 的起始逻辑行索引
- `prefix_stable_len` 已标识哪些 VM 未变化
- 未变化 VM 的渲染行内容不变 → 其 wrap 结果也不变（宽度不变时）
- 只需对 `prefix_stable_len` 之后的 VM 重新计算 wrap

### 审核发现的现有 bug

**`message_offsets` 与 `wrap_map` 索引空间不一致**：

```rust
// render_thread.rs:331-363
for lines in &self.message_lines {
    offsets.push(all_lines.len());      // ← all_lines 索引空间（dedup 前）
    all_lines.extend(lines.iter().cloned());
}
let deduped = /* 过滤 all_lines 的连续空行 */;
let (total_lines, wrap_map) = build_wrap_map(&deduped, width);  // ← deduped 索引空间
cache.message_offsets = offsets;  // ❌ all_lines 空间
cache.wrap_map = wrap_map;       // deduped 空间
```

当 dedup 过滤了消息间的连续空行时，`message_offsets[i]` 指向 `all_lines` 中的位置，但 `wrap_map` 的索引对应 `deduped`——两者不对应。当前 `RebuildWithAnchor` 中用 `message_offsets` 查 `wrap_map` 存在偏移错误风险（实际触发概率低，因为 dedup 通常只折叠消息末尾的空行，anchor 附近较少触发）。

**本 plan 必须先修复此 bug，否则增量 wrap_map 的索引计算全部错误。**

## 实现步骤

### Step 0: 修复 message_offsets 索引空间 [前置条件]

**文件**: `render_thread.rs`

将 `message_offsets` 改为基于 `deduped` 索引空间构建，而非 `all_lines`。

核心思路：先对每条消息的行做 per-message dedup（去除自身末尾空行），再拼接时记录 deduped 空间的 offsets，最后在拼接的 deduped 上做一次全局 dedup 消除消息间的连续空行。

```rust
fn rebuild(&mut self, messages: Vec<MessageViewModel>) {
    // ... 现有 hash diff + render_one 逻辑（不变）...

    self.message_hashes = new_hashes;

    // ── 新的拼接 + dedup 流程 ──────────────────────────────────
    // 阶段 1：per-message dedup（去除单条消息内部的连续空行和尾部空行）
    let mut per_msg_deduped: Vec<Vec<Line<'static>>> = Vec::with_capacity(new_len);
    for lines in &self.message_lines {
        per_msg_deduped.push(Self::dedup_lines(lines));
    }

    // 阶段 2：拼接时构建 deduped 空间的 offsets
    let mut deduped: Vec<Line<'static>> = Vec::new();
    let mut offsets: Vec<usize> = Vec::new();
    for msg_lines in &per_msg_deduped {
        offsets.push(deduped.len());
        deduped.extend(msg_lines.iter().cloned());
    }

    // 阶段 3：全局 dedup（消除消息间的连续空行）
    // 因为 per_msg_deduped 每条消息末尾可能仍有空行，
    // 两条消息拼接后在边界处可能产生连续空行
    let deduped = Self::dedup_consecutive_empty(deduped);
    // 注意：全局 dedup 不改变 offsets 的正确性——
    // 它只移除空行，而 offsets[i] 之后的非空内容位置不变。
    // 等等，这不对。全局 dedup 移除的是空行，会改变后续元素的索引。
    // ...
}
```

**问题**：全局 dedup 会改变索引，导致 offsets 失效。

**解决方案**：把全局 dedup 的逻辑改为——在拼接阶段就处理消息间的空行：

```rust
fn rebuild(&mut self, messages: Vec<MessageViewModel>) {
    // ... 现有 hash diff + render_one 逻辑（不变）...

    // ── 拼接 + 统一 dedup ──────────────────────────────────
    // 先做 per-message dedup（去除尾部空行 + 内部连续空行）
    let per_msg: Vec<Vec<Line<'static>>> =
        self.message_lines.iter().map(|l| Self::dedup_lines(l)).collect();

    // 拼接时做全局 dedup，同时构建 deduped 空间的 offsets
    let mut deduped: Vec<Line<'static>> = Vec::new();
    let mut offsets: Vec<usize> = Vec::new();
    let mut prev_empty = false;
    for msg_lines in &per_msg {
        offsets.push(deduped.len());
        for line in msg_lines {
            let is_empty = line.spans.is_empty()
                || (line.spans.len() == 1 && line.spans[0].content.is_empty());
            if is_empty && prev_empty {
                continue; // 跳过连续空行（消息边界处）
            }
            prev_empty = is_empty;
            deduped.push(line.clone());
        }
    }
    // 移除末尾空行
    while deduped.last().is_some_and(|l| {
        l.spans.is_empty() || (l.spans.len() == 1 && l.spans[0].content.is_empty())
    }) {
        deduped.pop();
    }

    // 现在 offsets 和 deduped 在同一索引空间 ✅
    let (total_lines, wrap_map) = Self::build_wrap_map(&deduped, self.width);
    // ... 写入 cache ...
}
```

**关键洞察**：由于每条消息末尾最多一个空行（`render_one` 只追加一个 `Line::from("")`），per-message dedup 不会移除它。全局 dedup 只在消息边界处（前一条消息的尾部空行 + 当前消息的头部空行）折叠。通过逐条消息拼接时追踪 `prev_empty` 状态，offsets 记录的就是 deduped 后的准确位置。

但还有一个问题：offsets.push(deduped.len()) 是在当前消息的行被加入 deduped **之前**。如果前一条消息的尾部空行和当前消息的第一行（可能是空行）形成连续空行对，当前消息的 offset 仍然指向正确的位置——因为 offset 记录的是当前消息的第一行在 deduped 中的位置，而第一行可能被 dedup 跳过。

需要更精确的处理：**offset 应指向当前消息中第一个实际出现在 deduped 中的行**。

```rust
    for msg_lines in &per_msg {
        let start_len = deduped.len(); // 记录开始前的 deduped 长度
        for line in msg_lines {
            let is_empty = /* ... */;
            if is_empty && prev_empty { continue; }
            prev_empty = is_empty;
            deduped.push(line.clone());
        }
        offsets.push(start_len); // 始终 push，即使当前消息被完全 dedup（极罕见）
    }
```

**验证**: 
- 新增单元测试 `test_message_offsets_match_deduped_space`
- 验证 `RebuildWithAnchor` 的 anchor_visual_row 计算正确

### Step 1: 增量 wrap_map

**文件**: `render_thread.rs`

在 `rebuild()` 的 deduped + offsets 构建完成后，用增量方式计算 wrap_map。

**核心逻辑**：

```rust
    // ... 上面的拼接 + dedup 流程 ...
    // deduped, offsets 已在统一索引空间

    // 🆕 增量 wrap_map
    let (total_lines, wrap_map) = self.build_wrap_map_incremental(
        &deduped, &offsets, prefix_stable_len,
    );
```

```rust
impl RenderTask {
    /// 增量构建 wrap_map：复用旧前缀，只重算变化部分。
    fn build_wrap_map_incremental(
        &self,
        deduped: &[Line<'static>],
        offsets: &[usize],
        prefix_stable_len: usize,
    ) -> (usize, Vec<WrappedLineInfo>) {
        // 快速路径：无稳定前缀
        if prefix_stable_len == 0 || offsets.is_empty() {
            return Self::build_wrap_map(deduped, self.width);
        }

        // 稳定区在 deduped 中的行范围
        let stable_line_end = if prefix_stable_len < offsets.len() {
            offsets[prefix_stable_len]
        } else {
            deduped.len() // 所有 VM 都稳定
        };

        // 完全稳定：直接复用整个 wrap_map
        if stable_line_end == deduped.len() {
            let old_cache = self.cache.read();
            if old_cache.width == self.width
                && old_cache.wrap_map.len() == deduped.len()
            {
                // 完全一致，clone 并返回
                return (old_cache.total_lines, old_cache.wrap_map.clone());
            }
            drop(old_cache);
            // fallback
            return Self::build_wrap_map(deduped, self.width);
        }

        // 部分稳定：复用前缀 wrap_map
        let old_cache = self.cache.read();
        let can_reuse = old_cache.width == self.width
            && !old_cache.wrap_map.is_empty()
            && old_cache.wrap_map.len() >= stable_line_end;

        if !can_reuse {
            drop(old_cache);
            return Self::build_wrap_map(deduped, self.width);
        }

        // 复用前缀（不 clone 整个 WrappedLineInfo，只 clone 必要的视觉行偏移信息）
        let base_visual = if stable_line_end > 0 {
            old_cache.wrap_map[stable_line_end - 1].visual_row_end
        } else {
            0
        };

        // 只对变化部分计算 wrap
        let (delta_total, mut delta_wrap) =
            Self::build_wrap_map(&deduped[stable_line_end..], self.width);

        // 修正 delta_wrap 的 visual_row 偏移
        for info in &mut delta_wrap {
            info.visual_row_start += base_visual;
            info.visual_row_end += base_visual;
            info.line_idx += stable_line_end;
        }
        drop(old_cache);

        // 拼接：clone 前缀 + extend delta
        // 注意：clone 前缀的 WrappedLineInfo 会 clone 其中的 plain_text: String
        // 和 char_widths: Vec<u8>。开销分析：
        //   stable_line_end ≈ 3000 时，每个 WrappedLineInfo ≈ 80-160 bytes
        //   总 clone ≈ 240-480KB，耗时约 200-500μs
        //   相比 build_wrap_map 全量计算 3000 行的 ~2-6ms，仍节省 60-80%
        //
        // 后续优化（如果需要）：可将 wrap_map 改为 Arc<[WrappedLineInfo]>
        // 或将 plain_text/char_widths 提取为独立索引表避免 clone。
        let old_cache = self.cache.read();
        let mut wrap_map: Vec<WrappedLineInfo> =
            old_cache.wrap_map[..stable_line_end].to_vec();
        drop(old_cache);

        wrap_map.append(&mut delta_wrap);
        let total_lines = base_visual as usize + delta_total;
        (total_lines, wrap_map)
    }
}
```

### Step 2: dedup 级联效应处理

**问题**：全局 dedup 中，前缀最后一条消息的尾部空行 + 变化区域第一条消息的头部空行可能形成新的连续空行对。即使前缀消息本身不变，dedup 后的前缀长度可能因后续消息变化而不同。

**分析**：

每条消息的 per-message dedup 后，末尾最多保留一个空行（`render_one` 追加的 `Line::from("")`）。消息间连续空行只出现在：前一条消息末尾空行 + 下一条消息第一行也是空行的情况。

在 `message_lines` 中，每条消息的第一行通常是内容行（用户消息的 `❯ ...`、AI 消息的文本、工具调用的 `● tool_name`），很少是空行。所以 **实际中级联效应极少触发**。

但为了正确性，增量 wrap_map 使用的是 **deduped 后的索引空间**（Step 0 已修复），`offsets[prefix_stable_len]` 准确定位了前缀在 deduped 中的边界。即使 dedup 级联导致前几行被移除，`offsets` 已经反映了这一点。

**结论**：Step 0 的修复已经解决了此问题。无需额外处理。

### Step 3: all_lines 拼接优化（可选）

**问题**：当前 `all_lines` 拼接仍然 O(n) clone 所有消息行（3000 行 ~0.1-0.3ms）。虽然相对 build_wrap_map 开销小，但可以进一步优化。

**优化方案**：对前缀部分，不 clone 行内容，直接复用 `cache.lines`（前缀部分的内容与上次相同）。

```rust
    // 前缀部分直接复用 cache.lines（不 clone）
    let old_cache = self.cache.read();
    let stable_line_end = offsets[prefix_stable_len];
    let mut deduped: Vec<Line<'static>> = if stable_line_end > 0
        && old_cache.width == self.width
        && old_cache.lines.len() >= stable_line_end
    {
        old_cache.lines[..stable_line_end].to_vec()
    } else {
        Vec::with_capacity(/* ... */)
    };
    drop(old_cache);

    // 只拼接变化部分
    for msg_lines in per_msg.iter().skip(prefix_stable_len) {
        // ... dedup + extend ...
    }
```

**优先级**：低。3000 行 clone ~0.1-0.3ms，不是瓶颈。可以在 Step 1 验证收益后再决定。

### Step 4: Resize 时强制全量重建

**文件**: `render_thread.rs`

Resize 处理中宽度变化时不能复用旧 wrap_map。当前代码 Resize 调用 `self.rebuild(self.last_messages.clone())`，此时：
- `prefix_stable_len` 会等于 `min(old_len, new_len)`（所有 hash 相同）
- 但 `old_cache.width != self.width` → 增量路径自动 fallback 到全量

**无需额外改动**。

**验证**: 现有测试 `test_resize_rebuilds_with_new_width` 仍通过。

### Step 5: 边界条件处理

- `prefix_stable_len == 0`：全量计算（不变）
- `prefix_stable_len == vms.len()`：完全复用 wrap_map（`stable_line_end == deduped.len()`）
- 新增 VM（vms.len() > old_vms.len()）：`prefix_stable_len` 覆盖旧部分，新 VM 走增量
- VM 内容完全不变（hash diff 全跳过）：`deduped` 内容与上次相同，wrap_map 完全复用
- 宽度变化：自动 fallback 到全量

## 测试计划

| 测试 | 验证点 |
|------|--------|
| `test_message_offsets_match_deduped_space` | **Step 0 验证**：offsets[i] 在 deduped 中正确定位第 i 条消息 |
| `test_message_offsets_anchor_correctness` | **Step 0 验证**：RebuildWithAnchor 的 anchor_visual_row 计算正确 |
| `test_dedup_preserves_offsets_after_dedup` | 全局 dedup 后 offsets 仍指向正确行 |
| `test_incremental_wrap_map_matches_full` | **核心**：增量计算结果与全量计算完全一致（随机数据 fuzz） |
| `test_incremental_wrap_map_all_stable` | 所有 VM 不变时，wrap_map 完全复用 |
| `test_incremental_wrap_map_resize_fallback` | 宽度变化时 fallback 到全量 |
| `test_incremental_wrap_map_prefix_stable_len_zero` | 无稳定前缀时走全量路径 |
| `test_incremental_wrap_map_add_new_vm` | 新增 VM 时只重算尾部 |
| 现有测试全部通过 | 不破坏已有行为 |

**核心测试策略**：`test_incremental_wrap_map_matches_full` 对同一输入分别走增量和全量路径，断言 wrap_map 的 `visual_row_start/end`、`total_lines` 完全一致。用 fuzz 测试覆盖各种 prefix_stable_len 值。

## 性能预期

| 场景 | 优化前 | 优化后 |
|------|--------|--------|
| 3000 行历史 + 1 行流式 chunk | O(3000) wrap ~2-6ms | O(1) wrap + clone prefix ~0.3-0.6ms |
| 100 行历史 + 50 行流式 | O(100) wrap ~0.2-0.4ms | O(50) wrap + clone ~0.1-0.2ms |
| Resize | O(3000) ~2-6ms | O(3000)（不变，fallback 全量） |

**修正后收益**：渲染线程流式场景减少 **60-82%**（3-6ms → 0.5-1.1ms）。

**用户可感知度**：低-中。渲染线程是后台异步线程，100ms 节流下不会直接阻塞 UI。但减少 CPU 占用有助于降低 resize 风暴等极端场景下的队列积压。

## 风险

| 风险 | 等级 | 缓解措施 |
|------|------|---------|
| wrap_map visual_row 偏移拼接错误 | 低 | 对比测试（增量 vs 全量）覆盖 |
| message_offsets 索引空间修复引入新 bug | 中 | fuzz 测试 + anchor 正确性测试 |
| to_vec() 克隆前缀 wrap_map 开销（~200-500μs） | 低 | 仍节省 60-80%；后续可用 Arc 优化 |
| dedup 级联效应 | 低 | Step 0 已修复；offsets 在 deduped 空间 |
| 无 API 变更 | - | 完全内部优化，不影响 TUI 或 ACP 层 |

## 工作量估算

1-2 天（含 Step 0 修复 + Step 1 增量 + 测试）

## 实施顺序

```
Step 0 (修复 offsets 索引空间) → 验证测试 → Step 1 (增量 wrap_map) → 验证测试
```

Step 2-3 可选，不影响核心收益。
