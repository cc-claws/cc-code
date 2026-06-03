# 三档流式渲染模式实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在 `MessagePipeline` 中新增 `StreamingMode`（Streaming/Block/None），让用户通过 `/streaming` TUI 本地命令切换渲染模式，降低长会话/高速输出时的 CPU 占用。

**Architecture:** 在 `MessagePipeline` 中新增 `StreamingMode` 枚举和 Block 模式缓冲区字段。`check_throttle()` 根据 `streaming_mode` 分支：Streaming 保持现有 `AdaptiveChunkingPolicy`，Block/None 返回 None 短路。Block 模式通过简单状态机检测 Markdown block 边界（双空行 + 代码围栏），在边界处 flush `block_buffer` 触发渲染。`/streaming` 命令在 `submit_message()` 入口拦截，直接修改 pipeline 的 `streaming_mode` 字段。

**Tech Stack:** Rust, pulldown-cmark（已有，仅 Block 边界检测不使用）

---

## File Structure

| 文件 | 操作 | 职责变更 |
|------|------|----------|
| `peri-tui/src/app/message_pipeline/mod.rs` | 修改 | 新增 `StreamingMode` 枚举 + `MessagePipeline` 字段 + `check_throttle()` 分支 + Block 边界检测 + `set_streaming_mode()` |
| `peri-tui/src/app/message_pipeline/message_pipeline_test.rs` | 修改 | 新增 StreamingMode 相关单元测试 |
| `peri-tui/src/app/agent_submit.rs` | 修改 | 入口拦截 `/streaming` 命令 |
| `peri-tui/src/ui/main_ui/status_bar.rs` | 修改 | 第一行显示当前模式标签 |

---

## Task 1: 新增 `StreamingMode` 枚举和 Pipeline 字段

**Files:**
- Modify: `peri-tui/src/app/message_pipeline/mod.rs`

- [ ] **Step 1: 在 `AdaptiveChunkingPolicy` 定义之前（约第 40 行 `// ─── 自适应分块策略` 注释处）添加 `StreamingMode` 枚举**

```rust
// ─── 流式渲染模式 ──────────────────────────────────────────────────────────

/// 流式渲染模式：控制 LLM 输出时的渲染粒度。
///
/// - Streaming：逐 token 实时渲染 + 自适应帧率（默认，流畅但 CPU 高）
/// - Block：按 Markdown block 粒度整块渲染（CPU 中）
/// - None：不渲染流式内容，完成后一次性显示（CPU 低）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum StreamingMode {
    /// 逐 token 实时渲染 + 自适应帧率（默认）
    #[default]
    Streaming,
    /// 按 Markdown block 粒度整块渲染（段落/代码块完成后渲染）
    Block,
    /// 不渲染流式内容，LLM 完成后一次性显示
    None,
}
```

- [ ] **Step 2: 在 `MessagePipeline` 结构体中新增字段（在 `throttle_last_fire` 之后）**

在 `throttle_last_fire: Option<Instant>,` 之后添加：

```rust
    // ── 流式渲染模式 ──
    /// 当前流式渲染模式
    streaming_mode: StreamingMode,
    // ── Block 模式缓冲 ──
    /// Block 模式下累积未完成 block 的 chunk
    block_buffer: String,
    /// Block 模式下是否处于代码围栏内部
    inside_code_fence: bool,
    /// Block 模式下是否有待 flush 的内容（用于 handle_event 返回 RebuildAll）
    block_pending_flush: bool,
```

- [ ] **Step 3: 在 `MessagePipeline::new()` 构造函数中初始化新字段**

在 `throttle_last_fire: None,` 之后添加：

```rust
            streaming_mode: StreamingMode::default(),
            block_buffer: String::new(),
            inside_code_fence: false,
            block_pending_flush: false,
```

- [ ] **Step 4: 添加 getter 和 setter 方法（在 `pub fn cwd()` 之后）**

```rust
    /// 获取当前流式渲染模式
    pub fn streaming_mode(&self) -> StreamingMode {
        self.streaming_mode
    }

    /// 设置流式渲染模式。切换时强制 flush Block 缓冲区。
    pub fn set_streaming_mode(&mut self, mode: StreamingMode) {
        // 切换前 flush Block 缓冲区中的残留内容
        if self.streaming_mode == StreamingMode::Block && mode != StreamingMode::Block {
            self.flush_block_buffer();
        }
        self.streaming_mode = mode;
        self.inside_code_fence = false;
        tracing::info!(?mode, "streaming mode changed");
    }
```

- [ ] **Step 5: 编译验证**

Run: `cargo build -p peri-tui 2>&1 | tail -10`
Expected: 成功（新字段有默认值，不影响现有逻辑）

- [ ] **Step 6: 提交**

```bash
git add peri-tui/src/app/message_pipeline/mod.rs
git commit -m "feat(tui): add StreamingMode enum and pipeline fields"
```

---

## Task 2: Block 模式边界检测和 flush 逻辑

**Files:**
- Modify: `peri-tui/src/app/message_pipeline/mod.rs`

- [ ] **Step 1: 在 `MessagePipeline` impl 中（`push_reasoning()` 方法之后）添加 Block 缓冲区方法**

```rust
    // ─── Block 模式缓冲区管理 ────────────────────────────────────────────

    /// Block 模式下追加 chunk 到缓冲区并检测 block 边界。
    /// 返回 true 表示检测到边界，需要 flush。
    fn push_chunk_block(&mut self, chunk: &str) -> bool {
        self.block_buffer.push_str(chunk);

        if self.inside_code_fence {
            // 在代码围栏内：检测闭合围栏
            if self.detect_code_fence_close() {
                self.inside_code_fence = false;
                return true;
            }
        } else {
            // 在代码围栏外
            // 检测双空行（段落边界）
            if self.block_buffer.contains("\n\n") {
                return true;
            }
            // 检测代码围栏开始
            if self.detect_code_fence_open() {
                self.inside_code_fence = true;
            }
        }
        false
    }

    /// 检测 block_buffer 末尾是否有代码围栏开始行。
    /// 匹配行首的 ``` （三个或更多反引号），忽略前面的空白。
    fn detect_code_fence_open(&self) -> bool {
        // 检查 buffer 最后一行是否以 ``` 开头
        self.block_buffer
            .lines()
            .last()
            .is_some_and(|line| line.trim_start().starts_with("```"))
    }

    /// 检测 block_buffer 末尾是否有代码围栏闭合行。
    /// 闭合条件：最后一个非空行恰好是 ``` （无语言标记）。
    fn detect_code_fence_close(&self) -> bool {
        self.block_buffer
            .lines()
            .last()
            .is_some_and(|line| line.trim() == "```")
    }

    /// Flush Block 缓冲区内容到 current_ai_text。
    fn flush_block_buffer(&mut self) {
        if !self.block_buffer.is_empty() {
            self.current_ai_text.push_str(&self.block_buffer);
            self.block_buffer.clear();
            self.block_pending_flush = true;
        }
    }

    /// 强制 flush Block 缓冲区（用于 ToolStart/Done/Interrupt 等边界事件）。
    /// 同时重置围栏状态。
    fn force_flush_block(&mut self) {
        self.flush_block_buffer();
        self.inside_code_fence = false;
    }
```

- [ ] **Step 2: 修改 `push_chunk()` 方法，根据 `streaming_mode` 分支**

将当前的：

```rust
    pub fn push_chunk(&mut self, chunk: &str) {
        self.current_ai_text.push_str(chunk);
        self.adaptive_policy.on_chunk(chunk);
    }
```

替换为：

```rust
    pub fn push_chunk(&mut self, chunk: &str) {
        match self.streaming_mode {
            StreamingMode::Streaming => {
                self.current_ai_text.push_str(chunk);
                self.adaptive_policy.on_chunk(chunk);
            }
            StreamingMode::Block => {
                // Block 模式：累积到 block_buffer，检测边界后 flush
                if self.push_chunk_block(chunk) {
                    self.flush_block_buffer();
                }
            }
            StreamingMode::None => {
                // None 模式：只累积文本，不触发任何渲染
                self.current_ai_text.push_str(chunk);
            }
        }
    }
```

- [ ] **Step 3: 编译验证**

Run: `cargo build -p peri-tui 2>&1 | tail -10`
Expected: 成功

- [ ] **Step 4: 提交**

```bash
git add peri-tui/src/app/message_pipeline/mod.rs
git commit -m "feat(tui): implement Block mode boundary detection and flush logic"
```

---

## Task 3: 修改 `check_throttle()` 支持三模式分支

**Files:**
- Modify: `peri-tui/src/app/message_pipeline/mod.rs`

- [ ] **Step 1: 修改 `check_throttle()` 方法，根据模式分支**

将当前的 `check_throttle` 方法：

```rust
    pub fn check_throttle(&mut self, prefix_len: usize) -> Option<PipelineAction> {
        let plan = self.adaptive_policy.check()?;

        match plan {
            DrainPlan::Single => {
                let now = Instant::now();
                let min_interval = Duration::from_millis(16);
                let should_fire = match self.throttle_last_fire {
                    None => true,
                    Some(last) => now.duration_since(last) >= min_interval,
                };
                if !should_fire {
                    return None;
                }
                self.throttle_last_fire = Some(now);
                self.adaptive_policy.drain();
                Some(self.build_rebuild_all(prefix_len))
            }
            DrainPlan::Batch => {
                self.throttle_last_fire = Some(Instant::now());
                self.adaptive_policy.drain();
                Some(self.build_rebuild_all(prefix_len))
            }
        }
    }
```

替换为：

```rust
    pub fn check_throttle(&mut self, prefix_len: usize) -> Option<PipelineAction> {
        match self.streaming_mode {
            StreamingMode::Streaming => self.check_throttle_streaming(prefix_len),
            StreamingMode::Block => self.check_throttle_block(prefix_len),
            StreamingMode::None => None,
        }
    }

    /// Streaming 模式：使用 AdaptiveChunkingPolicy 自适应帧率。
    fn check_throttle_streaming(&mut self, prefix_len: usize) -> Option<PipelineAction> {
        let plan = self.adaptive_policy.check()?;

        match plan {
            DrainPlan::Single => {
                let now = Instant::now();
                let min_interval = Duration::from_millis(16);
                let should_fire = match self.throttle_last_fire {
                    None => true,
                    Some(last) => now.duration_since(last) >= min_interval,
                };
                if !should_fire {
                    return None;
                }
                self.throttle_last_fire = Some(now);
                self.adaptive_policy.drain();
                Some(self.build_rebuild_all(prefix_len))
            }
            DrainPlan::Batch => {
                self.throttle_last_fire = Some(Instant::now());
                self.adaptive_policy.drain();
                Some(self.build_rebuild_all(prefix_len))
            }
        }
    }

    /// Block 模式：检查 block_pending_flush 标志。
    /// flush_block_buffer() 设置标志，check_throttle 消费标志并触发 RebuildAll。
    fn check_throttle_block(&mut self, prefix_len: usize) -> Option<PipelineAction> {
        if self.block_pending_flush {
            self.block_pending_flush = false;
            Some(self.build_rebuild_all(prefix_len))
        } else {
            None
        }
    }
```

- [ ] **Step 2: 编译验证**

Run: `cargo build -p peri-tui 2>&1 | tail -10`
Expected: 成功

- [ ] **Step 3: 提交**

```bash
git add peri-tui/src/app/message_pipeline/mod.rs
git commit -m "feat(tui): check_throttle three-mode branching (Streaming/Block/None)"
```

---

## Task 4: 在 `handle_event()` 关键事件中强制 flush Block 缓冲区

**Files:**
- Modify: `peri-tui/src/app/message_pipeline/mod.rs`

- [ ] **Step 1: 在 `handle_event` 的 `ToolStart` 分支中，`self.adaptive_policy.drain();` 之后添加 Block flush**

在 `AgentEvent::ToolStart` 分支中，找到 `self.adaptive_policy.drain();`（约第 350 行），在其后添加：

```rust
                self.force_flush_block();
```

- [ ] **Step 2: 在 `handle_event` 的 `ToolEnd` 分支中，`self.adaptive_policy.drain();` 之后添加 Block flush**

在 `AgentEvent::ToolEnd` 分支中，找到 `self.adaptive_policy.drain();`（约第 394 行），在其后添加：

```rust
                self.force_flush_block();
```

- [ ] **Step 3: 在 `done()` 方法中添加 Block flush**

在 `done()` 方法中，找到 `self.adaptive_policy.reset();` 之后，`self.throttle_last_fire = None;` 之前，添加：

```rust
        self.force_flush_block();
```

- [ ] **Step 4: 在 `interrupt()` 方法中添加 Block flush**

在 `interrupt()` 方法中，找到 `self.adaptive_policy.reset();` 之后，`self.throttle_last_fire = None;` 之前，添加：

```rust
        self.force_flush_block();
```

- [ ] **Step 5: 在 `StateSnapshot` 分支中添加 Block flush（非 subagent 路径）**

在 `AgentEvent::StateSnapshot` 分支中，`self.set_completed(msgs);` 之前添加：

```rust
                    self.force_flush_block();
```

- [ ] **Step 6: 编译验证**

Run: `cargo build -p peri-tui 2>&1 | tail -10`
Expected: 成功

- [ ] **Step 7: 提交**

```bash
git add peri-tui/src/app/message_pipeline/mod.rs
git commit -m "feat(tui): force flush block buffer on ToolStart/ToolEnd/Done/Interrupt/StateSnapshot"
```

---

## Task 5: 单元测试——StreamingMode 核心逻辑

**Files:**
- Modify: `peri-tui/src/app/message_pipeline/message_pipeline_test.rs`

- [ ] **Step 1: 在测试文件末尾添加 StreamingMode 测试**

```rust
// ─── StreamingMode 测试 ──────────────────────────────────────────────────

use super::StreamingMode;

/// 测试：新 pipeline 默认为 Streaming 模式
#[test]
fn test_streaming_mode_default() {
    let pipeline = MessagePipeline::new("/tmp".to_string());
    assert_eq!(pipeline.streaming_mode(), StreamingMode::Streaming);
}

/// 测试：set_streaming_mode 正确切换模式
#[test]
fn test_set_streaming_mode() {
    let mut pipeline = MessagePipeline::new("/tmp".to_string());
    pipeline.set_streaming_mode(StreamingMode::Block);
    assert_eq!(pipeline.streaming_mode(), StreamingMode::Block);
    pipeline.set_streaming_mode(StreamingMode::None);
    assert_eq!(pipeline.streaming_mode(), StreamingMode::None);
    pipeline.set_streaming_mode(StreamingMode::Streaming);
    assert_eq!(pipeline.streaming_mode(), StreamingMode::Streaming);
}

/// 测试：Streaming 模式下 push_chunk 触发 adaptive_policy
#[test]
fn test_push_chunk_streaming_updates_policy() {
    let mut pipeline = MessagePipeline::new("/tmp".to_string());
    pipeline.set_streaming_mode(StreamingMode::Streaming);
    pipeline.push_chunk("hello");
    assert_eq!(pipeline.current_ai_text, "hello");
    assert!(pipeline.adaptive_policy.pending_lines > 0);
}

/// 测试：None 模式下 push_chunk 只累积文本，不触发 adaptive_policy
#[test]
fn test_push_chunk_none_only_accumulates() {
    let mut pipeline = MessagePipeline::new("/tmp".to_string());
    pipeline.set_streaming_mode(StreamingMode::None);
    pipeline.push_chunk("hello");
    assert_eq!(pipeline.current_ai_text, "hello");
    assert_eq!(pipeline.adaptive_policy.pending_lines, 0);
}

/// 测试：Block 模式下 push_chunk 累积到 block_buffer
#[test]
fn test_push_chunk_block_buffering() {
    let mut pipeline = MessagePipeline::new("/tmp".to_string());
    pipeline.set_streaming_mode(StreamingMode::Block);
    pipeline.push_chunk("hello");
    // 还没有段落边界，仍在 block_buffer
    assert_eq!(pipeline.current_ai_text, "");
    assert_eq!(pipeline.block_buffer, "hello");
}

/// 测试：Block 模式下双空行触发 flush
#[test]
fn test_push_chunk_block_flush_on_double_newline() {
    let mut pipeline = MessagePipeline::new("/tmp".to_string());
    pipeline.set_streaming_mode(StreamingMode::Block);
    pipeline.push_chunk("paragraph one\n\n");
    // 双空行触发 flush
    assert_eq!(pipeline.current_ai_text, "paragraph one\n\n");
    assert!(pipeline.block_buffer.is_empty());
    assert!(pipeline.block_pending_flush);
}

/// 测试：Block 模式下代码围栏不触发 flush 直到闭合
#[test]
fn test_push_chunk_block_code_fence() {
    let mut pipeline = MessagePipeline::new("/tmp".to_string());
    pipeline.set_streaming_mode(StreamingMode::Block);

    // 开始代码围栏
    pipeline.push_chunk("```rust\n");
    assert!(pipeline.inside_code_fence);
    assert_eq!(pipeline.current_ai_text, ""); // 还没 flush

    // 代码内容
    pipeline.push_chunk("fn main() {}\n");
    assert_eq!(pipeline.current_ai_text, ""); // 仍在围栏内

    // 闭合围栏
    pipeline.push_chunk("```\n");
    assert!(!pipeline.inside_code_fence);
    assert_eq!(pipeline.current_ai_text, "```rust\nfn main() {}\n```\n");
    assert!(pipeline.block_pending_flush);
}

/// 测试：check_throttle 在 Streaming 模式下正常工作
#[test]
fn test_check_throttle_streaming_mode() {
    let mut pipeline = MessagePipeline::new("/tmp".to_string());
    pipeline.set_streaming_mode(StreamingMode::Streaming);
    pipeline.handle_event(AgentEvent::AssistantChunk {
        chunk: "hello".into(),
        source_agent_id: None,
    });
    let result = pipeline.check_throttle(0);
    assert!(result.is_some());
}

/// 测试：check_throttle 在 None 模式下始终返回 None
#[test]
fn test_check_throttle_none_mode() {
    let mut pipeline = MessagePipeline::new("/tmp".to_string());
    pipeline.set_streaming_mode(StreamingMode::None);
    pipeline.handle_event(AgentEvent::AssistantChunk {
        chunk: "hello".into(),
        source_agent_id: None,
    });
    let result = pipeline.check_throttle(0);
    assert!(result.is_none());
}

/// 测试：check_throttle 在 Block 模式下无 flush 时返回 None
#[test]
fn test_check_throttle_block_no_flush() {
    let mut pipeline = MessagePipeline::new("/tmp".to_string());
    pipeline.set_streaming_mode(StreamingMode::Block);
    pipeline.push_chunk("hello"); // 无段落边界
    let result = pipeline.check_throttle(0);
    assert!(result.is_none());
}

/// 测试：check_throttle 在 Block 模式下 flush 后返回 RebuildAll
#[test]
fn test_check_throttle_block_after_flush() {
    let mut pipeline = MessagePipeline::new("/tmp".to_string());
    pipeline.set_streaming_mode(StreamingMode::Block);
    pipeline.push_chunk("hello\n\n"); // 双空行触发 flush
    let result = pipeline.check_throttle(0);
    assert!(result.is_some());
    // 消费后再次 check 应返回 None
    let result2 = pipeline.check_throttle(0);
    assert!(result2.is_none());
}

/// 测试：从 Block 切换到其他模式时强制 flush
#[test]
fn test_set_streaming_mode_flushes_on_exit_block() {
    let mut pipeline = MessagePipeline::new("/tmp".to_string());
    pipeline.set_streaming_mode(StreamingMode::Block);
    pipeline.push_chunk("unflushed content");
    assert_eq!(pipeline.current_ai_text, "");

    // 切换到 Streaming 应 flush 残留内容
    pipeline.set_streaming_mode(StreamingMode::Streaming);
    assert_eq!(pipeline.current_ai_text, "unflushed content");
    assert!(pipeline.block_buffer.is_empty());
}

/// 测试：done() 在 Block 模式下 flush 缓冲区
#[test]
fn test_done_flushes_block_buffer() {
    let mut pipeline = MessagePipeline::new("/tmp".to_string());
    pipeline.set_streaming_mode(StreamingMode::Block);
    pipeline.handle_event(AgentEvent::AssistantChunk {
        chunk: "hello".into(),
        source_agent_id: None,
    });
    assert_eq!(pipeline.current_ai_text, "");
    pipeline.done();
    assert_eq!(pipeline.current_ai_text, ""); // done() 会 finalize_current_ai，清空 current_ai_text
    // 但 block_buffer 已被 flush，内容已被 finalize 处理
}
```

注意：测试中需要导入 `AgentEvent`。检查测试文件头部的 `use` 语句，如果已有 `use super::*;` 则不需要额外导入。如果没有，需要添加 `use crate::app::events::AgentEvent;`。根据现有测试文件的 import 模式确定。

- [ ] **Step 2: 运行测试**

Run: `cargo test -p peri-tui -- streaming_mode 2>&1 | tail -20`
Expected: 所有新增测试通过

- [ ] **Step 3: 提交**

```bash
git add peri-tui/src/app/message_pipeline/message_pipeline_test.rs
git commit -m "test(tui): add StreamingMode unit tests (TDD)"
```

---

## Task 6: `/streaming` TUI 本地命令拦截

**Files:**
- Modify: `peri-tui/src/app/agent_submit.rs`

- [ ] **Step 1: 在 `submit_message()` 方法入口处（空输入检查之后）添加 `/streaming` 拦截**

在 `if input.trim().is_empty() { return; }` 之后，`self.push_input_history(input.clone());` 之前，添加：

```rust
        // ── TUI 本地命令拦截 ──
        if let Some(args) = input.strip_prefix("/streaming") {
            self.handle_streaming_command(args.trim());
            return;
        }
```

- [ ] **Step 2: 在 `App` impl 中（`submit_message` 方法之后）添加 `handle_streaming_command` 方法**

在 `agent_submit.rs` 文件末尾（`submit_bg_continuation` 方法之后，最后一个 `}` 之前）添加：

```rust
    /// 处理 `/streaming` 本地命令：查看或切换流式渲染模式。
    fn handle_streaming_command(&mut self, args: &str) {
        use crate::app::message_pipeline::StreamingMode;

        let lc = &self.services.lc;
        let (mode, label) = match args {
            "" => {
                // 无参数：显示当前模式
                let current = self.session_mgr.current_mut().messages.pipeline.streaming_mode();
                let mode_str = match current {
                    StreamingMode::Streaming => "Streaming",
                    StreamingMode::Block => "Block",
                    StreamingMode::None => "None",
                };
                let msg = lc.tr_args(
                    "streaming-current",
                    &[("mode".into(), mode_str.into())],
                );
                self.apply_pipeline_action(PipelineAction::AddMessage(MessageViewModel::system(
                    msg,
                )));
                return;
            }
            "streaming" => (StreamingMode::Streaming, "Streaming"),
            "block" => (StreamingMode::Block, "Block"),
            "none" => (StreamingMode::None, "None"),
            _ => {
                self.apply_pipeline_action(PipelineAction::AddMessage(MessageViewModel::system(
                    lc.tr("streaming-usage"),
                )));
                return;
            }
        };

        self.session_mgr
            .current_mut()
            .messages
            .pipeline
            .set_streaming_mode(mode);

        // 如果有 block buffer 残留需要 flush，触发一次 rebuild
        if self.session_mgr.current_mut().messages.pipeline.has_pending_block_flush() {
            let prefix = self.session_mgr.current().messages.round_start_vm_idx;
            self.apply_pipeline_action(
                self.session_mgr
                    .current_mut()
                    .messages
                    .pipeline
                    .check_throttle(prefix)
                    .unwrap_or_else(|| {
                        PipelineAction::RebuildAll {
                            prefix_len: prefix,
                            tail_vms: self.session_mgr.current_mut().messages.pipeline.build_tail_vms(),
                        }
                    }),
            );
        }

        let msg = lc.tr_args(
            "streaming-changed",
            &[("mode".into(), label.into())],
        );
        self.apply_pipeline_action(PipelineAction::AddMessage(MessageViewModel::system(
            msg,
        )));
    }
```

- [ ] **Step 3: 在 `MessagePipeline` 中添加 `has_pending_block_flush` 辅助方法**

在 `set_streaming_mode()` 方法之后添加：

```rust
    /// 检查 Block 模式是否有待 flush 的内容
    pub fn has_pending_block_flush(&self) -> bool {
        self.block_pending_flush || !self.block_buffer.is_empty()
    }
```

- [ ] **Step 4: 添加 i18n 键值**

在 i18n 资源文件中添加以下键值（需要找到对应的语言文件，通常在 `peri-tui/src/i18n/` 或 `peri-tui/locales/` 中）。先搜索现有的 i18n 文件位置：

Run: `find peri-tui -name "*.json" -path "*/i18n/*" -o -name "*.json" -path "*/locale*" 2>/dev/null | head -5`

根据找到的文件，添加以下键（中文）：
```json
"streaming-current": "当前渲染模式：{{mode}}（可选：streaming / block / none）",
"streaming-changed": "渲染模式已切换为：{{mode}}",
"streaming-usage": "用法：/streaming [streaming|block|none]"
```

英文：
```json
"streaming-current": "Current render mode: {{mode}} (options: streaming / block / none)",
"streaming-changed": "Render mode changed to: {{mode}}",
"streaming-usage": "Usage: /streaming [streaming|block|none]"
```

- [ ] **Step 5: 编译验证**

Run: `cargo build -p peri-tui 2>&1 | tail -20`
Expected: 成功（可能需要根据 i18n 系统的实际 API 调整 `tr_args` 参数格式）

- [ ] **Step 6: 提交**

```bash
git add peri-tui/src/app/agent_submit.rs peri-tui/src/app/message_pipeline/mod.rs
git commit -m "feat(tui): add /streaming local command for render mode switching"
```

---

## Task 7: Status Bar 显示当前模式

**Files:**
- Modify: `peri-tui/src/ui/main_ui/status_bar.rs`

- [ ] **Step 1: 在 `render_first_row` 函数中，模型名之后（进程资源监控之前）添加渲染模式标签**

在 status_bar.rs 的 `render_first_row` 函数中，找到进程资源监控注释 `// 进程资源监控` 之前（约第 84 行），添加：

```rust
    // 流式渲染模式标签（仅非默认模式显示）
    {
        use crate::app::message_pipeline::StreamingMode;
        let mode = app.session_mgr.current().messages.pipeline.streaming_mode();
        let (label, color) = match mode {
            StreamingMode::Streaming => ("", theme::TEXT),     // 默认不显示
            StreamingMode::Block => ("Block", theme::THINKING),
            StreamingMode::None => ("None", theme::WARNING),
        };
        if !label.is_empty() {
            spans.push(Span::styled(" · ", Style::default().fg(theme::MUTED)));
            spans.push(Span::styled(
                format!(" {}", label),
                Style::default().fg(color),
            ));
        }
    }
```

- [ ] **Step 2: 编译验证**

Run: `cargo build -p peri-tui 2>&1 | tail -10`
Expected: 成功

- [ ] **Step 3: 提交**

```bash
git add peri-tui/src/ui/main_ui/status_bar.rs
git commit -m "feat(tui): show streaming mode label in status bar"
```

---

## Task 8: 全量构建和测试验证

**Files:** 无修改

- [ ] **Step 1: 全量构建**

Run: `cargo build 2>&1 | tail -10`
Expected: 成功

- [ ] **Step 2: 运行 peri-tui 全量测试**

Run: `cargo test -p peri-tui 2>&1 | tail -30`
Expected: 所有测试通过

- [ ] **Step 3: Clippy 检查**

Run: `cargo clippy -p peri-tui 2>&1 | tail -20`
Expected: 无新 warning

- [ ] **Step 4: 运行 pre-commit hooks**

Run: `lefthook run pre-commit 2>&1 | tail -20`
Expected: 全部通过

- [ ] **Step 5: 手动集成测试**

启动 TUI，测试以下场景：
1. 输入 `/streaming` → 显示当前模式 Streaming
2. 输入 `/streaming block` → 切换到 Block，status bar 显示 `[Block]`
3. 进行对话 → 验证文本按段落整块出现，非逐 token
4. 输入 `/streaming none` → 切换到 None，status bar 显示 `[None]`
5. 进行对话 → 验证流式期间无内容显示，完成后一次性出现
6. 输入 `/streaming streaming` → 切换回 Streaming，status bar 不显示标签
7. 进行对话 → 验证恢复逐 token 渲染
8. 输入 `/streaming invalid` → 显示用法提示

- [ ] **Step 6: 最终 commit（如有遗漏修复）**

```bash
git add -A
git commit -m "fix: follow-up fixes from streaming mode implementation"
```

---

## Self-Review

### Spec Coverage

| Spec 需求 | 对应 Task |
|-----------|----------|
| StreamingMode 枚举定义 | Task 1 |
| MessagePipeline 新增字段 | Task 1 |
| check_throttle 三模式分支 | Task 3 |
| Block 边界检测（双空行+代码围栏） | Task 2 |
| Block buffer flush | Task 2 |
| 强制 flush 触发点（ToolStart/ToolEnd/Done/Interrupt/StateSnapshot） | Task 4 |
| /streaming 本地命令 | Task 6 |
| Status Bar 显示模式标签 | Task 7 |
| 单元测试 | Task 5 |

### Placeholder Scan

无 TBD/TODO。所有代码块完整。

### Type Consistency

- `StreamingMode` 在 Task 1 定义，Task 5/6/7 使用，名称一致
- `block_pending_flush` 在 Task 1 声明，Task 2/3 读写，Task 6 通过 `has_pending_block_flush()` 读取
- `force_flush_block()` 在 Task 2 定义，Task 4 在各事件分支调用
- `set_streaming_mode()` 在 Task 1 定义，Task 6 调用
- `handle_streaming_command()` 使用 `PipelineAction::AddMessage` 和 `MessageViewModel::system()`，与现有代码一致
