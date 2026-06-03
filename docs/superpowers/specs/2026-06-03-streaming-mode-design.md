# 三档流式渲染策略设计

**日期**：2026-06-03
**状态**：Draft
**关联 Issue**：`spec/issues/2026-05-30-adaptive-streaming-frame-rate.md`

---

## 背景

TUI 流式渲染时，每收到一个 LLM chunk 都触发 Markdown 解析 + wrap 计算 + UI 重绘。长会话或高速输出时累积 CPU 开销明显。当前没有机制让用户手动在「流畅感」和「CPU 占用」之间取舍。

## 目标

新增三档流式渲染模式，用户通过 `/streaming` 本地命令手动切换：

| 档位 | 行为 | CPU |
|------|------|-----|
| **Streaming**（默认） | 逐 token 实时渲染 + 自适应帧率（现有行为） | 高 |
| **Block** | 按 Markdown block 粒度整块渲染（block 完成前内容完全隐藏） | 中 |
| **None** | 不渲染流式内容，LLM 完成后一次性显示 | 低 |

## 设计决策

### D1：实现层次——Pipeline 层拦截

在 `MessagePipeline` 中新增 `StreamingMode` 字段，`check_throttle()` 和 `handle_event()` 根据模式分支。

**不选方案**：
- 事件过滤层（在 poll_agent 和 render_thread 之间过滤 Rebuild 事件）：Pipeline 内部状态已构建好 ViewModel，过滤事件导致状态不一致
- RenderThread 层控制：RenderThread 是纯渲染层，无 Markdown 语义感知，做不了 Block 边界检测

**理由**：当前 `AdaptiveChunkingPolicy` 已在 Pipeline 层控制节流，三档是其自然扩展。改动集中在 Pipeline，不动 RenderThread。

### D2：命令注册——TUI 本地命令

`/streaming` 不走 ACP executor 的 CommandRegistry，而是 TUI input handler 直接拦截。

**理由**：只影响 TUI 渲染行为，和 agent/executor 无关。直接修改 `pipeline.streaming_mode`，不需要跨进程传播。

### D3：Block 边界检测——简单状态机

用轻量状态机检测双空行（段落边界）和代码围栏，不做完整 Markdown AST 解析。

**理由**：pulldown-cmark 解析性能开销不符合「降低 CPU」的目标。双空行 + 代码围栏覆盖 95% 实际场景。

---

## 核心类型

```rust
/// 流式渲染模式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum StreamingMode {
    /// 逐 token 实时渲染 + 自适应帧率（默认）
    #[default]
    Streaming,
    /// 按 Markdown block 粒度整块渲染
    Block,
    /// 不渲染流式内容，完成后一次性显示
    None,
}
```

## MessagePipeline 新增字段

```rust
pub(crate) struct MessagePipeline {
    // ... 现有字段 ...

    /// 当前流式渲染模式
    streaming_mode: StreamingMode,

    // ── Block 模式缓冲 ──
    /// Block 模式下累积未完成 block 的 chunk
    block_buffer: String,
    /// 是否在代码围栏内部
    inside_code_fence: bool,
}
```

- 非 Block 模式下 `block_buffer` 和 `inside_code_fence` 不使用（零开销）
- `StreamingMode` 暴露为 `pub(crate)` 以便 input handler 和 status bar 读取

## check_throttle() 三模式行为

```
Streaming 模式：
  保持现有 AdaptiveChunkingPolicy 逻辑
  Smooth → 最小 16ms → RebuildAll
  CatchUp → 立即排空 → RebuildAll

Block 模式：
  始终返回 None（不触发 RebuildAll）
  渲染触发点在 handle_event() 的 block 边界检测
  ToolStart / ToolEnd / Done / Interrupt → 强制 flush block_buffer

None 模式：
  始终返回 None
  仅 done() / interrupt() 的 finalize_current_ai() 触发最终渲染
```

## Block 模式边界检测

### 状态机

```
输入：每个 chunk
状态：{ OutsideFence, InsideFence }

OutsideFence:
  检测 "\n\n"（双空行）→ flush block_buffer
  检测行首 "```" → 进入 InsideFence，继续累积

InsideFence:
  继续累积到 block_buffer
  检测行首 "```" → flush block_buffer + 回到 OutsideFence
```

### push_chunk() 行为

```rust
// Block 模式下 push_chunk 的处理逻辑
fn push_chunk_for_block(&mut self, chunk: &str) {
    self.block_buffer.push_str(chunk);

    if self.inside_code_fence {
        // 检测闭合围栏
        if self.detect_closing_fence(&self.block_buffer) {
            self.flush_block();
            self.inside_code_fence = false;
        }
    } else {
        // 检测双空行（段落边界）
        if self.block_buffer.contains("\n\n") {
            self.flush_block();
        }
        // 检测开始围栏
        else if self.detect_opening_fence(&self.block_buffer) {
            self.inside_code_fence = true;
        }
    }
}

fn flush_block(&mut self) {
    // 将 block_buffer 内容 append 到 current_ai_text
    self.current_ai_text.push_str(&self.block_buffer);
    self.block_buffer.clear();
    // 触发 RebuildAll（通过设置 adaptive_policy 标记或直接返回 action）
    self.block_pending_flush = true;
}
```

### 强制 flush 触发点

以下事件发生时，无论 block_buffer 内容是否到达自然边界，都强制 flush：
- `ToolStart`：工具调用前的文本必须显示
- `ToolEnd`：工具结果后的文本必须显示
- `Done` / `Interrupt`：LLM 结束时所有内容必须显示
- `StateSnapshot`：reconcile 前必须 flush

## /streaming 命令

### 实现

TUI input handler 在提交用户输入前拦截 `/streaming` 前缀：

```rust
// input handler 伪代码
fn handle_user_input(&mut self, text: &str) {
    if let Some(args) = text.strip_prefix("/streaming") {
        self.handle_streaming_command(args.trim());
        return;
    }
    // ... 正常 submit_message 路径 ...
}

fn handle_streaming_command(&mut self, args: &str) {
    let mode = match args {
        "" => {
            // 无参数：显示当前模式
            self.show_system_note(&format!("当前渲染模式：{:?}", self.pipeline.streaming_mode()));
            return;
        }
        "streaming" => StreamingMode::Streaming,
        "block" => StreamingMode::Block,
        "none" => StreamingMode::None,
        _ => {
            self.show_system_note("用法：/streaming [streaming|block|none]");
            return;
        }
    };

    self.session_mgr.current_mut().messages.pipeline.set_streaming_mode(mode);
    self.show_system_note(&format!("渲染模式已切换为：{:?}", mode));
}
```

### 不注册为 ACP Command

- 只影响 TUI 渲染，与 agent/executor 无关
- 直接修改 `pipeline.streaming_mode`
- Stdio 模式下不支持此命令（Stdio 无渲染层概念）

## Status Bar 显示

| 模式 | 显示 |
|------|------|
| Streaming | 不显示（默认） |
| Block | `[Block]` |
| None | `[None]` |

位置：status bar hint 区域。

切换时在消息区显示系统通知：`[渲染模式已切换为：Block]`

## 涉及文件

| 文件 | 改动 |
|------|------|
| `peri-tui/src/app/message_pipeline/mod.rs` | 新增 `StreamingMode` 枚举 + `MessagePipeline` 字段 + `check_throttle()` 分支 + `push_chunk()` Block 逻辑 + 边界检测 |
| `peri-tui/src/app/message_pipeline/message_pipeline_test.rs` | 新增三模式单元测试 |
| `peri-tui/src/app/agent_ops/polling.rs` | input handler 拦截 `/streaming` 命令（或移到 input 处理层） |
| `peri-tui/src/ui/main_ui/status_bar.rs` | 显示当前模式标签 |

## 风险

1. **Block 边界误判**：简单状态机可能对特殊 Markdown 结构（如嵌套列表、blockquote 内代码块）判断不准确。但 Block 模式本身是「降低 CPU」的妥协方案，用户接受不完美的流式体验。
2. **模式切换时机**：用户在流式输出过程中切换模式，`block_buffer` 中可能有未 flush 的内容。切换时应强制 flush 当前 buffer。
3. **None 模式下的 spinner**：当前设计中 None 模式无任何 UI 反馈，用户可能以为卡住了。可以后续考虑添加轻量 placeholder。
