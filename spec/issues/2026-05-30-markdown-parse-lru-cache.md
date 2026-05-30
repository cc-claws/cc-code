# TUI Markdown 解析缺少 LRU 缓存，每次渲染完整重解析

**状态**：Fixed
**优先级**：中
**创建日期**：2026-05-30

## 问题描述

`peri-widgets` 中的 `parse_markdown()` 函数每次调用都完整解析 Markdown 文本（pulldown-cmark），包括语法高亮、表格处理等。在流式输出、窗口 resize、RebuildAll 等场景下，同一内容的 Markdown 被反复完整解析，造成不必要的 CPU 开销。

## 症状详情

| 场景 | 表现 |
|------|------|
| 流式输出中宽度变化 | 已完成的 Markdown 块被完整重解析 |
| RebuildAll 触发 | 所有消息的 Markdown 重新解析（即使内容未变） |
| 大段代码块（500+ 行） | 解析耗时明显，可感知卡顿 |

当前调用路径：

```
render_thread.rs:render_one()
  → MessageViewModel 渲染
    → peri-widgets markdown::parse_markdown()   ← 每次完整解析
```

## 性能数据

- 大段 Markdown（500+ 行代码块）单次解析耗时在 Release 构建中可感知
- 同一内容在 resize 事件中可能被解析 N 次（N = resize 次数）
- Hash Diff 跳过了渲染，但变化的消息仍需完整解析

## 出现场景

- 会话中有长 Markdown 回复时拖动终端边框 resize
- 流式输出大量代码时
- 上下文压缩后 RebuildAll

## 涉及文件

- `peri-widgets/src/markdown/mod.rs` —— `parse_markdown()` 入口，当前无缓存
- `peri-tui/src/ui/render_thread.rs` —— `render_one()` 调用 Markdown 渲染
- `peri-tui/src/ui/markdown/mod.rs` —— 增量解析逻辑（已有 `ensure_rendered_incremental`，但不跨消息缓存）

## 建议方向

引入 LRU 缓存，key = `(content_hash, max_width)`，value = `Text<'static>`。在 `parse_markdown` 入口处检查缓存命中：

```rust
struct MarkdownCache {
    cache: Mutex<LruCache<(u64, u16), Text<'static>>>,
}
```

预期收益：50-70% Markdown 解析时间减少。
