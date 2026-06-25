# 三档流式渲染策略：Streaming / Block / None

**状态**：Open
**优先级**：中
**类型**：性能
**创建日期**：2026-05-30
**更新日期**：2026-06-03

## 问题描述

TUI 流式渲染时，每收到一个 LLM chunk 都会触发 Markdown 解析 + wrap 计算 + UI 重绘。长会话或高速输出时，累积的 CPU 开销明显。当前没有机制让用户在「流畅感」和「CPU 占用」之间手动取舍。

需要设计三档流式渲染模式，用户通过 slash command 手动切换：

| 档位 | 行为 | CPU 占用 |
|------|------|----------|
| **Streaming** | 逐 token 实时渲染 + 自适应帧率（当前默认行为） | 高 |
| **Block** | 按 Markdown block 粒度整块渲染（段落/代码块/列表等完成后一次性渲染） | 中 |
| **None** | 不渲染流式内容，LLM 完成后一次性显示 | 低 |

## 症状详情

| 场景 | 当前表现 | 期望表现 |
|------|---------|---------|
| 高速流式输出（>50 token/s） | 队列积压，显示落后实际输出 1-2 秒，CPU 高 | Streaming 模式下快速收敛；Block 模式下按段落渲染；None 模式下零流式 CPU |
| 低速流式输出（<10 token/s） | 每 100ms 重绘一次，多数无新内容 | Block 模式下跳过中间态，段落完成后渲染 |
| 长上下文会话 | 持续 CPU 开销 | 用户可切换 Block/None 降低 CPU |
| 低性能环境（SSH/老机器） | 流式渲染导致终端卡顿 | 切换 None 模式完全避免流式渲染开销 |

## 三档方案详情

### Streaming（默认）

当前行为的增强版。引入自适应帧率（`AdaptiveChunkingPolicy`），在 Smooth 模式（逐行提交）和 CatchUp 模式（批量排空）之间动态切换。

- Smooth 模式：最小 16ms 间隔（~60fps）
- CatchUp 触发条件：队列深度 ≥ 8 行 或 最老行年龄 ≥ 120ms
- CatchUp 退出条件：队列 ≤ 2 行 且 年龄 ≤ 40ms

### Block

按 Markdown block 粒度渲染。流式 chunk 持续累积到内存，但**不触发渲染**，直到检测到一个完整的 Markdown block 边界时才一次性渲染该 block。

Markdown block 边界检测规则：
- 双空行 `\n\n` 分隔的段落
- 闭合的代码围栏 ` ``` ` （三个反引号）
- 列表项结束（下一个非列表内容或空行）
- 标题、引用块、表格等 Markdown 块级元素

实现要点：
- `MessagePipeline` 新增 `pending_blocks: Vec<String>` 缓冲区
- 每个 chunk 追加到当前 block buffer，检测边界
- 边界触发时：将完整 block 提交给渲染管线
- 工具调用（ToolStart/ToolEnd）也视为 block 边界

### None

完全不渲染流式内容。LLM 输出期间只显示 spinner/loading 状态，`Done` 或 `Interrupt` 事件时一次性渲染全部内容。

实现要点：
- `MessagePipeline` 累积所有 chunk 到 `current_ai_text`，但不触发任何 `RebuildAll`
- 仅在 `finalize_current_ai()` 时一次性提交渲染
- 流式期间显示「正在生成...」placeholder

## 用户交互

### Slash Command

```
/streaming        # 查看当前模式
/streaming streaming   # 切换到逐 token 渲染
/streaming block       # 切换到按段落渲染
/streaming none        # 切换到完成后渲染
```

- 命令类型：`CommandKind::Immediate`（立即执行，不经 agent）
- 无效参数时显示用法提示和当前模式
- 切换后通过 status bar hint 显示确认信息（如「渲染模式：Block」）

### 状态持久

- 模式切换仅在当前会话生效，不持久化到配置
- 每次新会话默认为 Streaming

## 涉及文件

- `peri-tui/src/app/message_pipeline/mod.rs` —— `check_throttle()` 节流逻辑，需根据模式分支
- `peri-tui/src/app/message_pipeline/transform.rs` —— 消息转换逻辑
- `peri-acp/src/session/command/mod.rs` —— 新增 `/streaming` slash command
- `peri-tui/src/ui/render_thread.rs` —— 渲染线程，Block/None 模式下调整 rebuild 触发
- `peri-tui/src/ui/main_ui/status_bar.rs` —— 状态栏显示当前渲染模式

## 建议实施顺序

| 阶段 | 方案 | 说明 |
|------|------|------|
| P0 | Streaming 增强（AdaptiveChunkingPolicy） | 替换固定 100ms 节流，已有详细计划 `docs/superpowers/plans/2026-05-30-adaptive-streaming.md` |
| P1 | None 模式 | 最简实现：chunk 时不触发 rebuild，done 时一次性渲染 |
| P2 | Block 模式 | Markdown block 边界检测 + 缓冲区管理 |
| P3 | Slash command + status bar 集成 | `/streaming` 命令注册 + 状态栏显示 |

## 原始描述（2026-05-30）

`MessagePipeline::check_throttle()` 使用固定的 100ms 节流窗口控制流式文本的重绘频率。这个策略在低速输出时足够，但在高速输出（如 LLM 快速吐出大量 token）时会导致队列积压，用户感知到明显的延迟。反之，在低速输出时 100ms 间隔又过于频繁。

Codex 项目实现了 `AdaptiveChunkingPolicy`，在 Smooth 模式（逐行提交）和 CatchUp 模式（批量排空）之间动态切换，值得借鉴。

原始代码：

```rust
// message_pipeline/mod.rs:check_throttle()
let should_fire = match self.throttle_last_fire {
    None => true,
    Some(last) => now.duration_since(last) >= Duration::from_millis(100),
};
```

## 关联 Issue

- `spec/issues/2026-05-30-cpu-spike-on-session-restore.md` —— 长上下文会话恢复 CPU 暴涨（渲染优化相关）
- `docs/superpowers/plans/2026-05-30-adaptive-streaming.md` —— AdaptiveChunkingPolicy 详细实施计划
