> 归档于 2026-05-31，原路径 spec/issues/2026-05-30-render-event-unbounded-channel.md

# RenderThread 事件通道使用 UnboundedChannel，极端情况下可能内存膨胀

**状态**：Fixed
**优先级**：低
**创建日期**：2026-05-30

## 问题描述

`RenderThread` 通过 `tokio::sync::mpsc::unbounded_channel()` 接收 `RenderEvent`。无界通道在正常使用中表现良好（不丢事件），但在极端场景（如 LLM 连续快速输出 + resize 风暴 + 大量 compact 事件同时到达）下，事件可能积压导致内存膨胀。

Codex 和 Zellij 项目使用有界通道（容量 50-128）并利用背压机制自然限速。

## 症状详情

| 场景 | 风险 |
|------|------|
| LLM 快速输出 + resize 风暴 | 事件积压，内存持续增长 |
| 多个 RebuildAll 同时排队 | 旧 RebuildAll 的消息数据未被及时消费 |
| 长时间运行的会话 | 极端情况下可能 OOM |

当前代码：

```rust
// render_thread.rs:spawn_render_thread()
let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
```

## 涉及文件

- `peri-tui/src/ui/render_thread.rs` —— `spawn_render_thread()` 创建通道
- `peri-tui/src/app/message_pipeline/mod.rs` —— 发送 `RenderEvent` 的调用方

## 建议方向

将 `unbounded_channel()` 改为 `channel(128)`（有界通道），并实现背压策略：

1. **正常路径**：`tx.send().await` 在通道满时自然等待（背压）
2. **Resize 去重**：`RenderEvent::Resize` 已有 drain 合并逻辑，不受背压影响
3. **紧急路径**：高优先级事件（如 Interrupt）可使用 `try_send` + 覆盖策略

注意：需确保发送方不会因背压死锁，特别是 `drop` 路径中的 cleanup 发送。
