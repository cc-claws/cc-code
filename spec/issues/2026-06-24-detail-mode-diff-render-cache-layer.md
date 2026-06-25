# detail_mode diff 渲染缓存层级不清，热路径依赖 widget 层 LRU 兜底

**状态**：Closed
**优先级**：中
**模块**：tui/widgets
**创建日期**：2026-06-24
**发现方式**：代码审计（PR #27 跟进）

## 问题描述

Write/Edit 工具的内联 diff 视图（Ctrl+O 切换的 detail_mode）每次 redraw 都会从 `MessageViewModel::ToolBlock.diff_input` 调用 `peri_widgets::diff::render_diff(input, width, theme)` 重新渲染。当前没有 VM 渲染层的显式缓存机制，靠 `peri-widgets/src/diff/renderer.rs` 内部的全局 `RENDER_CACHE`（LRU 64 容量）兜底命中。

这个设计在 PR #27（diff 视图宽度跟随终端宽度）之后暴露：原本 VM 持 `diff_lines: Option<Vec<Line>>` 是预渲染缓存（虽然宽度硬编码 80 是错的），改成持 `diff_input: DiffInput` 后，渲染时机移到 `message_render.rs`，但**没在 VM 渲染层补回显式缓存**，而是隐式依赖 widget 层 LRU。

## 现状

### 调用路径

```
RenderThread::rebuild_safe()
  → render_one(vm, index, width, ...)
    → render_view_model(vm, _, width, ...)
      → MessageViewModel::ToolBlock { diff_input, .. } 分支
        → if detail_mode {
              let diff_width = width.saturating_sub(4);
              let rendered = peri_widgets::diff::render_diff(diff_input, diff_width, &DarkTheme);  // ← 每次都调
              ...
          }
```

### 兜底机制（widget 层）

`peri-widgets/src/diff/renderer.rs:18-25`：
```rust
const RENDER_CACHE_CAPACITY: usize = 64;
static RENDER_CACHE: Lazy<Mutex<LruCache<RenderCacheKey, Vec<Line<'static>>>>> = ...;
```

Key 是 `(old_hash, new_hash, flags, width)`，命中后返回 `Vec<Line>::clone()`。

### 期望改进方向

把"按 (diff_input, width) 缓存渲染结果"的职责从 widget 层提到 VM 渲染层，让 widget 层只做纯函数渲染。具体目标：

1. widget 层 `render_diff` 退化为纯函数（无全局状态），便于测试和复用
2. VM 渲染层（`message_render.rs` 或更上层的 `RenderTask`/`RenderCache`）维护 `(diff_input_hash, width) → Vec<Line>` 显式缓存
3. width 变化时通过既有 `RenderEvent::Resize` 路径失效相关缓存项
4. detail_mode 切换、new message、resize 三种 redraw 路径都走显式缓存而非隐式 LRU

## 涉及文件

- `peri-tui/src/ui/message_render.rs` — detail_mode 分支（L758-778）调用 `render_diff`，是优化的主要受益方
- `peri-widgets/src/diff/renderer.rs` — `RENDER_CACHE` 全局 LRU 所在，优化后可能整体移除
- `peri-widgets/src/diff/mod.rs` — `render_diff` 公开入口
- `peri-tui/src/render_thread.rs`（如存在显式 RenderCache 容器）或 `peri-tui/src/app/message_pipeline.rs` — VM 渲染层缓存的归属候选

## Phase 0 基线测量（2026-06-24）

读 `render_thread.rs` + `message_render.rs` 后实际收益评估：

### VM 层已有 hash-based 增量机制

`render_thread.rs:119` 持 `message_hashes: Vec<u64>`，每个 VM 的 `content_hash` 含 `diff_input`。机制：

- **常规路径**：`diff_input` 不变 → `content_hash` 不变 → VM 在 `rebuild()` 中被跳过 `render_one` → 完全不调 `render_diff`
- **Resize 路径**：`RenderEvent::Resize` 触发 `message_hashes.clear()` + 全量 `rebuild_safe()`，每个 ToolBlock VM 都会调 `render_diff`（widget LRU 兜底）
- **新 ToolEnd 路径**：单个 VM 的 `diff_input` 变化 → `content_hash` 变化 → 只重渲染该 VM，仍走 widget LRU

### widget LRU 实际命中率推断

LRU key 是 `(old_hash, new_hash, flags, width)`，命中条件严苛：
- 同一 diff_input + 同一 width 反复渲染
- 在 Resize 场景下，每次 width 变化都是新 key，首次必 miss
- 用户实际不会反复 Resize 到同一宽度

**结论**：widget LRU 在常规场景几乎不起作用，仅在 Resize 边角场景有少量命中。把它提到 VM 渲染层**不会带来明显性能收益**。

### 仍值得做的理由（代码质量而非性能）

1. **正确性**：`RENDER_CACHE` 是全局 mutable static，跨 session 不隔离。虽然 key 含 hash 几乎不可能碰撞，但全局可变状态本身是异味
2. **可测试性**：纯函数 `render_diff` 便于单元测试，避免测试间状态污染
3. **概念清晰**：缓存职责归 VM 渲染层（已有 `message_hashes` 基础设施），widget 层只做纯渲染，职责单一

### 修订后的优化方向

原 issue 描述的「热路径每次都调」夸大了性能影响。实际优化重点是**架构清晰度**而非性能：

- 选项 A（推荐）：删除 widget 层 `RENDER_CACHE`，让 `render_diff` 退化为纯函数。VM 层 `message_hashes` 已经做了正确增量，不需要再加一层缓存。改动小、收益清晰。
- 选项 B：在 VM 层加显式 `(diff_input_hash, width) → Vec<Line>` 缓存。复杂度高，但收益不明显（hash 增量已经覆盖）。
- 选项 C：保持现状，仅补 widget 层 LRU 的命中率埋点，确认实际命中率后决定。

## 关联 Issue

- `spec/issues/2026-06-24-diff-render-width-hardcoded-80.md` — 引出本次架构演进的前序修复（diff_lines → diff_input）
- `spec/issues/2026-06-06-edit-diff-view-rendering-not-elegant.md` — diff 视觉样式相关

## 验证标准

- widget 层 `render_diff` 无全局 mutable state（删除 `RENDER_CACHE` 或仅保留为兼容层）
- VM 渲染层有显式缓存容器，key 含 width 字段
- 同一 (diff_input, width) 二次渲染的 wall time 显著低于首次（benchmark 或单元测试断言）
- detail_mode 切换、resize、new message 三种路径在 100 个 ToolBlock 场景下的 redraw 总耗时下降

## 关闭原因

Phase 0 基线测量发现：VM 层 `render_thread.rs:119` 已有 `message_hashes: Vec<u64>` hash-based 增量机制，`content_hash` 含 `diff_input`，diff_input 不变时 VM 被跳过 `render_one` 完全不调 `render_diff`。widget 层 LRU 实际只在 Resize 全量重渲染场景下起作用，且只对"同一 width 反复触发"命中（罕见）。

性能影响被原审计描述夸大，实际无优化必要。用户决策（2026-06-24）：既然没影响就不改。

## 状态变更记录

| 日期 | 从 | 到 | 操作人 | 说明 |
|------|-----|-----|--------|------|
| 2026-06-24 | — | Open | agent | 创建（PR #27 审计跟进） |
| 2026-06-24 | Open | Open | agent | Phase 0 完成：实际收益不及预期，性能影响被夸大，重新定性为架构清晰度优化 |
| 2026-06-24 | Open | Closed | agent | 用户决策：无性能影响则不修，关闭 |

## 修复记录

（由 fix-issue 或 issue-verify skill 追加，创建时留空）
