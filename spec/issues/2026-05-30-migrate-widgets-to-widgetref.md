# peri-widgets 组件未使用 WidgetRef，渲染路径存在不必要克隆

**状态**：Fixed
**优先级**：低
**创建日期**：2026-05-30

## 问题描述

`peri-widgets` 中的 Widget 组件（Markdown 块、代码块、工具卡片等）使用标准 `Widget` trait 渲染，该 trait 消费所有权。在多帧渲染场景中（如流式输出时每 100ms 重绘），这意味着频繁的 Widget 重建和潜在的数据克隆。

ratatui 0.30 提供了 `WidgetRef` trait（需启用 `unstable-widget-ref` feature），允许通过引用渲染，避免所有权转移和克隆。

## 症状详情

| 场景 | 影响 |
|------|------|
| 流式 Markdown 输出 | 同一 Widget 每 100ms 重建一次 |
| 多消息同时渲染 | 每条消息的 Widget 独立构建 |
| 大代码块 | `Text<'static>` 在 Widget 重建时可能被克隆 |

Codex 项目已使用 `WidgetRef` 模式（通过 `Renderable` trait 封装），Perihelion 未跟进。

## 涉及文件

- `peri-widgets/src/markdown/mod.rs` —— Markdown Widget 实现
- `peri-widgets/src/` 其他组件（代码块、工具卡片等）
- `peri-tui/Cargo.toml` —— 需启用 `unstable-widget-ref` feature

## 建议方向

1. 在 `peri-tui/Cargo.toml` 中启用 ratatui 的 `unstable-widget-ref` feature
2. 为高频渲染的 Widget 实现 `WidgetRef` trait
3. 在渲染调用处使用 `FrameExt::render_widget_ref()` 替代 `render_widget()`

```rust
// 迁移示例
impl WidgetRef for MarkdownWidget {
    fn render_ref(&self, area: Rect, buf: &mut Buffer) {
        // 通过引用渲染，避免 clone
    }
}
```

预期收益：减少渲染路径 clone 开销，特别是大 Markdown 块和代码块。
