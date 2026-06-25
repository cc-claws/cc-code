# Scroll CPU 优化计划

> **For agentic workers:** Use superpowers:subagent-driven-development or superpowers:executing-plans to implement. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 降低 TUI 滚动时的 CPU 开销，当前存在三个可优化点：coalesce 未累积 delta、每次滚动 clone 全量 lines、无帧率控制。

**Architecture:** 两个改动：(1) `coalesce_mouse_events` 按原始设计累积 delta 并合成事件；(2) `render_messages` 中只 clone 可视范围内的 lines，移除 `Paragraph::scroll()` 改用直接切片渲染。

**Tech Stack:** Rust, crossterm event polling, ratatui

**Files:**
- `peri-tui/src/event/mod.rs` — coalesce_mouse_events 重写
- `peri-tui/src/ui/main_ui/message_area.rs` — 可见区域切片渲染

---

### Task 1: 重写 `coalesce_mouse_events`，累积 scroll delta

**当前问题** (`event/mod.rs:95-143`)：
```rust
// 只保留最后一个事件，不累积 delta
MouseEventKind::ScrollDown => {
    last_ev = next;  // ← 覆盖而非累积
}
```
结果：10 次滚轮事件 → 1 次 redraw 但只滚 3 行，用户需多次滚轮才能到达目标 → 更多 CPU。

**原始设计**（`docs/superpowers/plans/2026-05-23-scroll-event-coalescing.md`）要求累积 delta 并合成最终事件：
```rust
let mut scroll_delta: i32 = 0;
MouseEventKind::ScrollDown => { scroll_delta += 3; last_ev = next; }
MouseEventKind::ScrollUp => { scroll_delta -= 3; last_ev = next; }
// Drain 结束后
if scroll_delta != 0 {
    last_ev = Event::Mouse(合成事件，方向由 delta 符号决定);
}
```

**注意**：drag 事件仍然只保留最后位置（不累积），非 scroll/drag 事件终止 drain。

- [ ] **Step 1: 重写 `coalesce_mouse_events` 函数**
  - 文件: `peri-tui/src/event/mod.rs:95-143`
  - 添加 `scroll_delta: i32` 累积变量
  - ScrollUp → `scroll_delta -= 3`，ScrollDown → `scroll_delta += 3`
  - Drag → `scroll_delta = 0`，保留最新位置
  - Drain 结束后：若 `scroll_delta != 0`，从 `last_ev` 重建 `MouseEvent`，方向由 delta 符号决定
  - 保持非 scroll/drag 事件终止 drain 的逻辑不变

- [ ] **Step 2: 构建验证**
  - Run: `cargo build -p peri-tui`
  - Expected: Clean build

- [ ] **Step 3: 手动验证**
  - Run: `cargo run -p peri-tui`
  - 打开长对话，快速滚轮：滚轮多圈应有更大的滚动距离（而非每圈只 3 行）
  - 拖拽滚动条功能正常
  - 滚动方向正确

- [ ] **Step 4: 提交**
  ```bash
  git add peri-tui/src/event/mod.rs
  git commit -m "perf(tui): accumulate scroll deltas in coalesce_mouse_events"
  ```

---

### Task 2: 滚动时只 clone 可见区域 lines

**当前问题** (`message_area.rs:118-165`)：
```rust
// 每次 redraw 都 clone 全量 cache.lines（可能是几千行 Line<'static>）
(cache.lines.clone(), ...)
// ...
Paragraph::new(Text::from(all_lines)).scroll((offset, 0))
```

每条 `Line<'static>` 包含 `Vec<Span<'static>>`，Span 含 `Cow<'static, str>`。数千行的 clone 在每次滚动 redraw 时产生不必要的内存分配。

**优化方案**：只 clone 可视范围内的 lines，移除 `Paragraph::scroll()`：

```rust
// 取缓存统计信息（不 clone lines）
let (total_lines, max_scroll, offset, ...) = { cache.read() 中计算... };

// 只 clone 可视区域
let visible_start = offset as usize;
let visible_end = (visible_start + visible_height as usize).min(cache.lines.len());
let visible_lines = cache.lines[visible_start..visible_end].to_vec();

// 渲染时不需 scroll()
Paragraph::new(Text::from(visible_lines)).wrap(Wrap { trim: false })
```

scrollbar 仍使用 `total_lines` 计算位置不受影响。

**scroll_follow 模式**：offset = max_scroll 时，`visible_start = max_scroll`，`visible_end = cache.lines.len()`。屏幕显示最后 `visible_height` 行，与原来行为一致。

**spinner/loading 行**：spinner_line、tip、todo 等动态行在 clone 的切片后追加，不受影响。

- [ ] **Step 1: 修改 `render_messages` 可见区域逻辑**
  - 文件: `peri-tui/src/ui/main_ui/message_area.rs:117-180`
  - 将 `let (mut all_lines, ...)` 变为分开获取 stats 和 lines
  - Stats（total_lines, max_scroll, offset 等）仍在 RwLock read guard 内计算
  - `all_lines` 只在可视范围 `cache.lines[visible_start..visible_end]` 上进行 `.to_vec()`
  - 移除 `Paragraph::scroll((offset, 0))` → 直接用 `Paragraph::new(Text::from(visible_lines))`
  - 验证 spinner 和 todo 行在渲染逻辑末尾追加时位置正确

- [ ] **Step 2: 构建验证**
  - Run: `cargo build -p peri-tui`
  - Expected: Clean build

- [ ] **Step 3: 运行测试**
  - Run: `cargo test -p peri-tui`
  - Expected: 全部通过

- [ ] **Step 4: 手动验证**
  - 长对话中滚动：内容渲染正确，无缺失/偏移
  - scroll_follow 模式：自动滚动到底部正常
  - 滚动条位置/大小与之前一致
  - spinner/loading 行正确显示

- [ ] **Step 5: 提交**
  ```bash
  git add peri-tui/src/ui/main_ui/message_area.rs
  git commit -m "perf(tui): clone only visible lines during scroll, remove Paragraph::scroll()"
  ```

---

## 自检

**覆盖范围：**
- coalesce 累积 delta ✅
- 可视区域切片 clone ✅
- 不需独立帧率限制（50ms poll + coalesce 已充分）✅

**占位符扫描：** 无 TBD、无 TODO ✅

**边界情况：**
- `max_scroll == 0`（无滚动）：visible_start=0, visible_end=min(height, cache.lines.len()) ✅
- `scroll_follow` 模式：visible 从末尾取 ✅
- `lines` 为空：visible_lines 为空，Paragraph 渲染空内容 ✅
- 渲染宽度变化本就会触发 RenderEvent::Resize 进入完全重建路径，不影响切片逻辑 ✅
