# TUI 渲染缺少显式帧率限制，loading 动画期间持续满帧重绘

**状态**：Fixed
**优先级**：低
**创建日期**：2026-05-30

## 问题描述

Perihelion 的主事件循环在 `loading` 状态为 true 时会无条件触发 `terminal.draw()`，没有显式的帧率限制。ratatui 的 `terminal.draw()` 内部会执行完整的 Buffer diff + Backend flush，在 60Hz+ 终端刷新率下可能导致不必要的 CPU 开销。

Codex 项目使用 `TARGET_FRAME_INTERVAL = 33ms`（约 30 FPS）限制渲染帧率。

## 症状详情

| 场景 | 当前表现 | 期望表现 |
|------|---------|---------|
| Agent 执行中（loading = true） | 每次事件循环都重绘，可能 >30 FPS | 限制在 30 FPS，CPU 占用下降 |
| 流式文本 100ms 节流 | 自然限频，无问题 | 无变化 |
| 空闲等待 | version 对比跳过重绘 | 无变化 |

当前代码：

```rust
// main.rs
if cache_updated || agent_updated || bg_updated || loading {
    terminal.draw(|f| ui::main_ui::render(f, &mut app))?;
}
```

`loading` 为 true 时，每次事件循环迭代都触发 draw，无时间间隔检查。

## 涉及文件

- `peri-tui/src/main.rs` —— 主事件循环中的重绘逻辑

## 建议方向

添加帧率限制，在 `loading` 路径中检查距上次渲染的时间间隔：

```rust
const TARGET_FRAME_INTERVAL: Duration = Duration::from_millis(33); // ~30 FPS

let should_render = cache_updated || agent_updated || bg_updated || loading;
if should_render {
    let now = Instant::now();
    let elapsed = now.duration_since(last_render);
    if elapsed >= TARGET_FRAME_INTERVAL || !loading {
        terminal.draw(|f| ui::main_ui::render(f, &mut app))?;
        last_render = now;
    }
}
```

预期收益：loading 期间 CPU 占用下降（从持续 draw → 30 FPS 节流）。
