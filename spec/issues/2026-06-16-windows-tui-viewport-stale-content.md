# TUI 渲染残留 — insert_before 后 Paragraph 未填满 viewport 导致旧内容残留

**状态**：Open
**优先级**：高
**创建日期**：2026-06-16
**GitHub Issue**：#173 (https://github.com/cc-claws/cc-code/issues/173)

## 问题描述

当对话消息增多后，TUI 渲染出现残留：新内容只能局部更新，历史消息"卡"在屏幕上不被清除，导致用户看到的内容与实际状态不一致。Windows 上更明显。

## 症状详情

1. 消息量超过视口高度后，旧消息残留在屏幕上
2. 只有屏幕局部区域在更新（"只有局部在动"）
3. 历史渲染内容无法被新内容"顶掉"
4. 用户视线内的内容与实际消息状态不一致

## 根因分析（已确认）

### 渲染流程

```
draw_app (main.rs:907):
  1. flush_scrollback_history → terminal.insert_before() 将旧消息推入终端 scrollback
  2. terminal.draw → viewport_clip → Paragraph::scroll + wrap 渲染当前视口
```

### 核心 bug：Paragraph 不填充未覆盖区域 + insert_before 只重置 back buffer

**证据链：**

1. **`Paragraph::render` 不填充剩余行**（ratatui-widgets paragraph.rs:449）：
   ```rust
   fn render_lines<'a, C: LineComposer<'a>>(mut composer: C, area: Rect, buf: &mut Buffer) {
       let mut y = 0;
       while let Some(ref wrapped) = composer.next_line() {
           render_line(wrapped, area, buf, y);
           y += 1;
           if y >= area.height { break; }
       }
       // 循环结束后不填充剩余行！
   }
   ```
   当 Paragraph 内容只有 20 行但 area 高度为 30 行时，buffer 底部 10 行**不被修改**，保留旧值。

2. **`insert_before` 的 `clear()` 只重置 back buffer**（ratatui-core terminal.rs:557）：
   ```rust
   pub fn clear(&mut self) -> Result<(), B::Error> {
       // ... 清除终端 viewport 区域 ...
       self.buffers[1 - self.current].reset(); // 只重置 back buffer！
       Ok(())
   }
   ```
   `current buffer` **未被重置**，保留了上一帧的渲染结果。

3. **`flush()` diff 机制**（ratatui-core terminal.rs:268）：
   ```rust
   pub fn flush(&mut self) -> Result<(), B::Error> {
       let previous_buffer = &self.buffers[1 - self.current]; // back buffer（被 reset，空）
       let current_buffer = &self.buffers[self.current];       // current buffer（部分新内容 + 底部旧内容）
       let updates = previous_buffer.diff(current_buffer);
       self.backend.draw(updates.into_iter())
   }
   ```
   diff 认为 current buffer 中所有非空 cell 都是"新的"，包括底部的旧内容。

### 完整 bug 流程

```
1. flush_scrollback_history:
   - insert_before(height, draw_fn)
   - → clear() 重置 back buffer + 清除终端 viewport 区域
   - → current buffer 未重置，保留旧内容

2. terminal.draw:
   - get_frame() 返回 current buffer（未重置）
   - Paragraph::render 只写入有内容的行
   - buffer 底部保留旧值

3. flush():
   - diff: back buffer（空）vs current buffer（新内容 + 底部旧内容）
   - 输出所有非空 cell → 旧内容被写回终端
   - 结果："历史消息卡在那里"
```

### 为什么 Windows 上更明显

- `Viewport::Inline` + `insert_before` 在 Windows ConPTY 下的 ANSI 序列支持可能不完整
- `loading` 期间跳过 `flush_scrollback_history`，loading 结束后一次性 insert_before 大批量行，更容易触发问题
- Windows 终端渲染差异导致残留更明显

## 复现条件

- **复现频率**：消息量超过视口高度后必现
- **触发步骤**：
  1. 启动 peri TUI
  2. 进行多轮对话，使消息量超过视口高度
  3. `insert_before` 被触发后，观察 viewport 底部是否有旧内容残留
- **环境**：所有平台（Windows 上更明显）

## 涉及文件

- `peri-tui/src/main.rs` —— `draw_app`、`flush_scrollback_history`
- `peri-tui/src/ui/main_ui/message_area.rs` —— `render_messages`、`viewport_clip`
- `peri-tui/src/ui/render_thread.rs` —— `RenderCache`
- ratatui-widgets `paragraph.rs:449` —— `render_lines` 不填充剩余行
- ratatui-core `terminal.rs:557` —— `clear()` 只重置 back buffer

## 修复方案

### 推荐方案：render_messages 中前置清屏

在 `render_messages` 中，Paragraph 渲染前先清除整个 messages_area：

```rust
// message_area.rs render_messages 函数中，在 Paragraph::render 之前
f.render_widget(ratatui::widgets::Clear, inner);
```

ratatui 提供了 `Clear` widget，会将指定区域的所有 cell 重置为空格+默认样式。这确保 current buffer 中的旧内容被清除，Paragraph 只渲染有内容的部分，剩余部分为空格（正确行为）。

**优点**：
- 改动最小（1 行代码）
- 不改变架构
- 确保 viewport 区域每帧都被清空
- ratatui diff 机制会最小化实际输出（空格 vs 空格无变化）

**缺点**：
- 每帧多一次 Clear 操作（但 ratatui diff 会优化掉无变化的 cell）

### 备选方案：在 insert_before 后重置 current buffer

```rust
// main.rs flush_scrollback_history 中
terminal.insert_before(height, draw_fn)?;
terminal.current_buffer_mut().reset(); // 同时重置 current buffer
```

**优点**：从源头解决
**缺点**：侵入 ratatui 内部状态管理，可能有副作用

## 经验沉淀

**ratatui Paragraph 不保证填充整个 area**。`render_lines` 只写入有内容的行，未覆盖的 cell 保留 buffer 中的旧值。在以下场景必须手动清屏：
1. 使用 `insert_before` 后（back buffer 被 reset，current buffer 未 reset）
2. Paragraph 内容动态变化，可能不足以填满 area
3. 使用 `Viewport::Inline` 模式

**ratatui `clear()` 只重置 back buffer**，不重置 current buffer。这是 double-buffering 机制的设计——`clear()` 的目的是强制下一次 diff 输出所有内容，而不是清空 current buffer。

## 状态变更记录

| 日期 | 从 | 到 | 操作人 | 说明 |
|------|-----|-----|--------|------|
| 2026-06-16 | — | Open | agent | 确认根因：Paragraph 不填充 + insert_before 只重置 back buffer |
