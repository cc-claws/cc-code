# Windows 长对话 streaming 渲染残留 — flush_scrollback_history 在 loading 期间跳过导致旧内容卡死

**状态**：Open
**优先级**：高
**创建日期**：2026-06-15
**GitHub Issue**：#172 (https://github.com/cc-claws/cc-code/issues/172)
**平台**：Windows（conhost / Windows Terminal）

## 问题描述

在 Windows 上进行长对话时，agent streaming 期间消息区内容逐渐累积，但旧渲染内容无法被新内容"顶掉"。表现：

1. 底部 spinner/loading 动画正常更新（"局部在动"）
2. 消息区中间和上部的历史消息渲染卡住不动
3. 新回复内容无法覆盖旧内容，viewport 内出现新旧内容混杂
4. streaming 结束后状态部分恢复，但残留碎片无法清除

## 根因分析

### 渲染架构

```
draw_app() {
    flush_scrollback_history(terminal, app)?;  // ① insert_before 推旧消息入 scrollback
    terminal.draw(|f| render(f, app))?;         // ② ratatui buffer diff 渲染 viewport
}
```

TUI 使用 `Viewport::Inline(terminal_rows - 1)` 固定高度内联视口。消息超出视口高度时，旧消息通过 `terminal.insert_before()` 推入终端原生 scrollback，viewport 整体下移。

### 缺陷点：`flush_scrollback_history` 在 loading 期间完全跳过

`peri-tui/src/main.rs:918-920`：

```rust
fn flush_scrollback_history(terminal, app) -> Result<()> {
    let session = app.session_mgr.current();
    if session.ui.loading {   // ← loading=true 时直接返回，不做任何 flush
        return Ok(());
    }
    // ... insert_before 逻辑
}
```

**整个 agent streaming 期间（loading=true），没有任何内容被推入 scrollback。**

### 连锁效应

| 阶段 | `scrollback_committed_lines` | viewport 行为 |
|------|------------------------------|---------------|
| streaming 开始 | 0 | 正常，viewport 显示尾部 |
| streaming 中期（消息累积） | 0（未更新） | viewport 内内容不断增长，靠 `Paragraph::scroll(offset)` 跳转尾部 |
| streaming 后期（大量消息） | 0（仍未更新） | `cache.lines` 膨胀至数千行，viewport 位置从未下移 |
| streaming 结束 | 首次 flush 执行 | `insert_before` 一次性推送大量行，viewport 大幅下移 |

### Windows 终端差异导致 diff 失同步

ratatui 的 `terminal.draw()` **不会先清除 viewport 区域**，依赖 buffer diff 机制：

```
clear()    → 内部 buffer 重置为空格（不清除终端屏幕）
render()   → widget 填充 buffer
flush()    → prev_buffer vs curr_buffer 逐 cell diff，只输出变化 cells
```

在 `Viewport::Inline` 模式下，当 `flush_scrollback_history` 不被调用、viewport 位置长期不变时：

1. **帧 N**：viewport 显示 cache.lines[9000..9030]，prev_buffer 记录这些 cells
2. **帧 N+1**：新消息到达，viewport 应显示 [9010..9040]，curr_buffer 有新内容
3. ratatui diff 逐 cell 比较 → 输出变化 cells

**但在 Windows 终端上，高频 `draw()` + 大 buffer diff 存在问题**：

- crossterm 在 Windows 上使用 `WriteConsoleOutputCharacter` API（非 ANSI 序列）
- 光标跳转 + 字符写入在高频率下可能被终端缓冲区合并或丢失
- 当大部分 cells 未变（只有尾部几行在更新），diff 输出量小但光标跳转频繁
- Windows Terminal / conhost 的光标定位在 `Viewport::Inline` 模式下可能存在竞态

**结果**：diff 输出的部分 cells 丢失，旧内容残留在终端屏幕上。

### streaming 结束后的不可恢复状态

streaming 结束后 `flush_scrollback_history` 首次执行：

1. `insert_before(H)` 一次性推送 H 行到 scrollback
2. ratatui 标记全量重绘（viewport 位置变化）
3. `terminal.draw()` 执行全量 diff → 覆盖 viewport 区域

**但 viewport 之上（scrollback 区域）的残留碎片永远无法被清除**——这些是 streaming 期间 diff 丢失导致的终端屏幕脏数据。

## 复现条件

- **复现频率**：Windows 上高概率（消息越多越容易触发）
- **触发步骤**：
  1. 在 Windows Terminal 或 conhost 中启动 peri TUI
  2. 发送一个会生成大量长输出的请求（如"分析整个项目架构"）
  3. 等待 agent streaming 期间观察消息区
  4. 消息累积到超过 viewport 高度后，历史消息开始"卡住"
- **环境**：Windows（macOS/Linux 终端的 ANSI 序列处理更可靠，较少触发）

## 涉及文件

| 文件 | 行号 | 问题 |
|------|------|------|
| `peri-tui/src/main.rs` | 918-920 | `flush_scrollback_history` 在 `loading=true` 时完全跳过 |
| `peri-tui/src/main.rs` | 907-911 | `draw_app` 每帧调用但 scrollback flush 依赖 loading 状态 |
| `peri-tui/src/ui/main_ui/message_area.rs` | 208-383 | `viewport_clip` 逻辑正确，但依赖底层渲染无损 |
| `peri-tui/src/ui/render_thread.rs` | 360-447 | `rebuild` 正确更新 cache，但 cache 更新不等于屏幕更新 |

## 修复方向

### 方案 A（推荐）：streaming 期间也执行 scrollback flush

移除 `loading` 守卫，或改为按帧间隔 flush：

```rust
fn flush_scrollback_history(terminal, app) -> Result<()> {
    // 移除: if session.ui.loading { return Ok(()); }
    // 改为: 每 N 帧或每 M 条新行 flush 一次，避免 insert_before 过于频繁
    ...
}
```

优点：从根本上解决 viewport 内容无限累积问题。
风险：`insert_before` 在 streaming 期间频繁调用可能有性能开销，需要节流。

### 方案 B：streaming 结束后强制全量清除

在 `loading` 从 true 变为 false 的边界，调用 `terminal.clear()` 强制清除整个 viewport，然后重新渲染。

优点：改动最小。
风险：可能产生闪烁；且 streaming 期间的渲染残留仍然存在。

### 方案 C：Windows 平台 streaming 期间定期全量重绘

在 Windows 上，streaming 期间每隔 N 帧强制触发一次全量 buffer diff（清空 prev_buffer），确保丢失的 cells 被补回来。

优点：兼容现有架构，不需要改 flush 逻辑。
风险：增加 Windows 平台的 CPU 开销。

## [TRAP] 经验沉淀

**`Viewport::Inline` 模式下，`flush_scrollback_history` 不能在长时间内跳过。**

**Why:** ratatui 的 `draw()` 依赖 buffer diff 机制，不会主动清除 viewport 区域。当 viewport 位置长期不变、内容持续变化时，diff 机制在部分终端（尤其是 Windows）上可能出现丢失。`insert_before` 不仅是"推旧内容入 scrollback"，它同时也是"移动 viewport 位置 + 标记全量重绘"的关键操作——跳过它等于跳过了终端状态同步的机会。

**How to apply:**
- `flush_scrollback_history` 应在所有状态下执行（包括 loading/streaming）
- 可以通过节流（每 N 帧一次）来控制性能开销，但不能完全跳过
- Windows 平台的 crossterm 渲染路径与 Unix 不同（Console API vs ANSI），对 buffer diff 的可靠性假设需要降低

## 状态变更记录

| 日期 | 从 | 到 | 操作人 | 说明 |
|------|-----|-----|--------|------|
| 2026-06-15 | — | Open | agent | 根因分析完成，待确认修复方向 |
