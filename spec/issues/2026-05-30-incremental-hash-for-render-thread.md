# TUI 渲染线程每帧全量计算消息 Hash

**状态**：Open
**优先级**：中
**创建日期**：2026-05-30

## 问题描述

`RenderThread::rebuild()` 每次被调用时，对所有消息重新计算 Hash（`messages.iter().map(Self::compute_hash).collect()`），用于 Hash Diff 增量渲染的前缀稳定区检测。随着会话消息增长（>100 条），全量 Hash 计算成为可观的 CPU 开销。

## 症状详情

当前代码路径：

```rust
// render_thread.rs:rebuild()
let new_hashes: Vec<u64> = messages.iter().map(Self::compute_hash).collect();
```

- 每条消息的 Hash 遍历其 `MessageViewModel` 的所有语义字段
- 即使前缀未变，仍需对前缀消息计算 Hash 用于比较
- 会话越长，每次 RebuildAll 的 Hash 计算量越大

**实测场景**：200+ 条消息的会话中，resize 或 RebuildAll 时可感知到延迟。

## 涉及文件

- `peri-tui/src/ui/render_thread.rs` —— `rebuild()` 中的 `compute_hash()` 调用
- `peri-tui/src/ui/message_view/mod.rs` —— `MessageViewModel` 的 `Hash` 实现

## 建议方向

将 Hash 值存入 `MessageViewModel` 自身，创建/修改时增量计算：

```rust
pub struct MessageViewModel {
    content_hash: u64,  // 创建时计算，不变时复用
}
```

在 `MessageViewModel` 构造或内容变更时调用 `update_hash()`，`rebuild()` 直接读取 `vm.content_hash()` 而非重新计算。

预期收益：60-80% Hash 计算时间减少（前缀区完全跳过计算）。
