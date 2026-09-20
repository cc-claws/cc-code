# Inline Viewport 历史消息残留 — insert_before 后 buffer diff 不同步

**状态**：Open
**优先级**：高
**创建日期**：2026-06-15
**GitHub Issue**：#171 (https://github.com/cc-claws/cc-code/issues/171)

## 问题描述

Windows（及所有平台）下，使用 `Viewport::Inline` 模式时，消息积累到一定量后，TUI 渲染出现"历史消息卡住"现象：只有局部内容（最新流式输出）在更新，其余区域保留旧渲染内容不动。

## 根因分析

### 渲染流程

```
draw_app()
  ├── flush_scrollback_history()    // ① terminal.insert_before(height, ...)
  └── terminal.draw(|| render)      // ② ratatui buffer diff → 只输出变化 cell
```

### 状态不同步机制

`insert_before(height, ...)` 执行时：
1. 滚动终端 viewport 上方的区域，将历史消息插入 scrollback
2. viewport 在终端中下移 `height` 行
3. ratatui 内部更新 `viewport_area.y += height`

随后 `terminal.draw()` 执行时：
1. `autoresize()` 检测 viewport area 变化，创建新 buffer（全空白）
2. widgets 渲染新内容到 buffer
3. **diff 机制**：`previous_buffer.diff(current_buffer)` → 只输出有变化的 cell

**问题**：`previous_buffer` 仍保留 `insert_before` 之前的旧帧内容。viewport 下移后，旧 buffer 中某些 cell 恰好与新 buffer 相同（同一条消息、同一段文本、空白区域），diff 跳过这些 cell。终端上这些 cell 保留的是 `insert_before` 滚动后的旧内容，未被覆盖。

### 表现映射

| 现象 | 原因 |
|------|------|
| "只有局部在动" | 最新流式输出区域的 cell 与旧 buffer 不同 → diff 输出更新 |
| "历史消息卡住" | 中上部 cell 与旧 buffer 相同 → diff 跳过 → 终端保留旧渲染 |
| 消息越多越严重 | `insert_before` 滚动越多 → 旧 buffer 与新 buffer 重叠越多 → 更多 cell 被跳过 |

## 关键代码

`peri-tui/src/main.rs:907-911`：

```rust
fn draw_app(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, app: &mut App) -> Result<()> {
    flush_scrollback_history(terminal, app)?;
    // ↑ insert_before 修改终端状态，但 ratatui previous_buffer 未同步
    terminal.draw(|f| ui::main_ui::render(f, app))?;
    // ↑ diff(previous_buffer, new_buffer) 跳过"相同" cell → 旧内容残留
    Ok(())
}
```

`flush_scrollback_history` (`main.rs:913-975`) 调用链：
- `scrollback_commit_end()` 计算需要提交的行范围
- `terminal.insert_before(height, ...)` 插入历史行到 viewport 上方
- 更新 `scrollback_committed_lines`

## 修复方案

ratatui 提供 `Terminal::clear()` 方法（ratatui-core `terminal.rs:540`）：

```rust
pub fn clear(&mut self) -> Result<(), B::Error> {
    // 清除终端 viewport 区域（不影响 viewport 上方的 scrollback 内容）
    match self.viewport {
        Viewport::Inline(_) => {
            self.backend.set_cursor_position(self.viewport_area.as_position())?;
            self.backend.clear_region(ClearType::AfterCursor)?;
        }
        // ...
    }
    // 重置 previous_buffer → 下次 draw 输出所有 cell（全量重绘）
    self.buffers[1 - self.current].reset();
    Ok(())
}
```

**核心**：`clear()` 重置 `previous_buffer`，使下次 `draw()` 的 diff 将所有 cell 视为"变化"，输出完整帧。

修改 `draw_app`，在 `insert_before` 发生后调用 `clear()` 强制全量重绘：

```rust
fn draw_app(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, app: &mut App) -> Result<()> {
    let did_insert = flush_scrollback_history(terminal, app)?;
    if did_insert {
        terminal.clear()?;
    }
    terminal.draw(|f| ui::main_ui::render(f, app))?;
    Ok(())
}
```

`flush_scrollback_history` 返回 `bool` 表示是否实际调用了 `insert_before`，避免无 insert 时的不必要全量重绘。

## 涉及文件

| 文件 | 改动 |
|------|------|
| `peri-tui/src/main.rs` | `draw_app` 增加 `clear()` 调用；`flush_scrollback_history` 返回 `bool` |

## 复现条件

- **平台**：所有（Windows Terminal 最明显）
- **Viewport 模式**：`Viewport::Inline`（非 Fullscreen）
- **触发步骤**：
  1. 启动 TUI，进行多轮对话积累消息
  2. 等待 scrollback 行被 `insert_before` 提交
  3. 继续发送消息，观察 viewport 中上部是否出现"冻结"的旧渲染内容
- **消息量越大越明显**：因为 `insert_before` 滚动量越大，旧 buffer 与新 buffer 重叠越多

## [TRAP] 经验沉淀

**`insert_before` 后必须 `clear()` 重置 ratatui buffer diff 状态。**

**Why:** ratatui 的 diff 机制基于两帧 buffer 的 cell 级比较。`insert_before` 直接操作终端（滚动、插入），但不更新 ratatui 的 `previous_buffer`。后续 `draw()` 的 diff 认为某些 cell "没变"而跳过输出，但终端上这些 cell 的实际内容已因滚动而改变。这是 ratatui `Viewport::Inline` + `insert_before` 的固有设计缺陷——两个独立的终端操作（insert + draw）之间的 buffer 状态不同步。

**How to apply:**
- 在 `Viewport::Inline` 模式下，任何在 `draw()` 之前调用 `insert_before()` 的场景，都需要在两者之间插入 `terminal.clear()`
- 仅在 `insert_before` 实际执行时才调用 `clear()`，避免无变化时的不必要全量重绘
- `clear()` 在 Inline 模式下只清除 viewport 区域，不影响 viewport 上方已提交的 scrollback 内容
- Fullscreen 模式不使用 `insert_before`，不受此问题影响
