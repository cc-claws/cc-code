# 流式文本节流使用固定 100ms 间隔，无法根据队列压力自适应调整

**状态**：Open
**优先级**：中
**创建日期**：2026-05-30

## 问题描述

`MessagePipeline::check_throttle()` 使用固定的 100ms 节流窗口控制流式文本的重绘频率。这个策略在低速输出时足够，但在高速输出（如 LLM 快速吐出大量 token）时会导致队列积压，用户感知到明显的延迟。反之，在低速输出时 100ms 间隔又过于频繁。

Codex 项目实现了 `AdaptiveChunkingPolicy`，在 Smooth 模式（逐行提交）和 CatchUp 模式（批量排空）之间动态切换，值得借鉴。

## 症状详情

| 场景 | 当前表现 | 期望表现 |
|------|---------|---------|
| 高速流式输出（>50 token/s） | 队列积压，显示落后实际输出 1-2 秒 | 快速收敛，CatchUp 模式批量排空 |
| 低速流式输出（<10 token/s） | 每 100ms 重绘一次，多数无新内容 | 仅在有完整行时重绘 |
| 队列深度突增 | 仍以 100ms 间隔逐个消费 | 检测到积压后切换批量模式 |

当前代码：

```rust
// message_pipeline/mod.rs:check_throttle()
let should_fire = match self.throttle_last_fire {
    None => true,
    Some(last) => now.duration_since(last) >= Duration::from_millis(100),
};
```

## 涉及文件

- `peri-tui/src/app/message_pipeline/mod.rs` —— `check_throttle()` 节流逻辑

## 建议方向

实现自适应分块策略，借鉴 Codex 的 `AdaptiveChunkingPolicy`：

```rust
enum ChunkingMode {
    Smooth { drain_per_tick: usize },  // 正常模式，逐行提交
    CatchUp,                           // 积压模式，批量排空
}
```

关键参数（可调）：
- 队列深度阈值：8 行（进入 CatchUp）
- 最老行年龄阈值：120ms（进入 CatchUp）
- 退出阈值：队列 ≤ 2 行且年龄 ≤ 40ms

预期收益：流式输出在"流畅感"和"CPU 占用"之间更好平衡，高速输出时延迟降低。
