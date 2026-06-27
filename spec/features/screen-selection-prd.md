# PRD: TUI 全局选中复制（Screen Selection）

## 背景

当前 TUI 的选中复制由两套独立系统组成：

1. **TextSelection**：覆盖消息区域（conversation area），通过 `wrap_map` 做视觉坐标→逻辑坐标的映射，字符级精度，word-wrap 感知。
2. **PanelTextSelection**：覆盖底部面板区域，通过 `panel_plain_lines` 提取文本。

**问题**：

- 7 个面板（memory、hooks、plugin/list 等）缺少选区高亮渲染——文本虽能复制，但用户看不到选中了什么。
- 状态栏、sticky header、bg agent bar 等区域完全不支持选中复制。
- 面板选区和消息选区互斥，无法跨区域选中。
- 用户期望的是**屏幕任意位置拖动鼠标就能选中、松手即复制**。

## 目标

在保留消息区域现有结构化选区的基础上，新增基于渲染 Buffer 的全局选区，覆盖其余所有区域：

- **消息区域**：保留现有 TextSelection（wrap_map 字符级精度，复制干净文本）
- **其他区域**（面板、状态栏、sticky header、bg agent bar、空白区域）：新增 Buffer 选区（所见即所得）
- 两套系统可跨区域衔接——从面板拖到消息区域，或反过来
- 松开鼠标自动复制到剪贴板，蓝色高亮显示

## 选区策略：混合模式

```
┌──────────────────────────────────────────┐
│  Sticky Header                           │ ← Buffer 选区
├──────────────────────────────────────────┤
│                                          │
│  Message Area (conversation)             │ ← TextSelection（保留现有）
│                                          │
├──────────────────────────────────────────┤
│  Panel (cron/mcp/agent/memory/hooks...)  │ ← Buffer 选区
├──────────────────────────────────────────┤
│  [textarea input]                        │ ← textarea 内置选区
├──────────────────────────────────────────┤
│  Status Bar                              │ ← Buffer 选区
├──────────────────────────────────────────┤
│  BG Agent Bar                            │ ← Buffer 选区
└──────────────────────────────────────────┘
```

### 各区域选区行为

| 区域 | 选区方案 | 复制内容 | 高亮方式 |
|------|---------|---------|---------|
| 消息区域 | TextSelection（现有） | 纯消息文本，grapheme 级精度 | 通过 wrap_map 在 Span 上应用 SELECTION_BG |
| 面板区域 | ScreenSelection（新增） | 屏幕可见文本 | 通过 Buffer Cell 修改背景色 |
| 状态栏 | ScreenSelection（新增） | 屏幕可见文本 | 通过 Buffer Cell 修改背景色 |
| Sticky Header | ScreenSelection（新增） | 屏幕可见文本 | 通过 Buffer Cell 修改背景色 |
| BG Agent Bar | ScreenSelection（新增） | 屏幕可见文本 | 通过 Buffer Cell 修改背景色 |
| TextArea | textarea 内置 | textarea 内容 | textarea 自带 |

### 跨区域选区

当用户从一个区域拖拽到另一个区域时，统一由 ScreenSelection 接管。具体规则：

- **MouseDown 在消息区域**：启动 TextSelection（现有行为不变）
- **MouseDown 在其他区域**：启动 ScreenSelection
- **Drag 跨越区域边界**：
  - TextSelection 活跃时拖出消息区域 → 切换为 ScreenSelection，清除 TextSelection
  - ScreenSelection 活跃时拖入消息区域 → 继续 ScreenSelection（覆盖消息区域的 Buffer 内容）
- **MouseDown 在 textarea**：启动 textarea 选区 + 记录 screen pending 点
  - 未拖拽（click）→ 清除 pending，click 正常工作（定位光标）
  - 拖拽 → 切换为 ScreenSelection

## 技术方案

### 核心概念：ScreenSnapshot + ScreenSelection

**ScreenSnapshot**：`terminal.draw()` 完成后克隆 ratatui `Buffer`（二维 Cell 网格），作为非消息区域选区操作的文本源。

```
terminal.draw(render)
    ↓
Buffer clone → ScreenSnapshot { cells, width, height }
    ↓
MouseUp 时从 snapshot 提取选区文本 → 复制到剪贴板
```

**ScreenSelection**：非消息区域的选区状态，使用绝对屏幕坐标 `(row, col)`。

```rust
struct ScreenSelection {
    start: Option<(u16, u16)>,  // (row, col) 绝对屏幕坐标
    end: Option<(u16, u16)>,
    dragging: bool,
}
```

### 数据流

```
┌─ draw_app ──────────────────────────────────────────┐
│  terminal.draw(|f| render(f, app))                   │
│       ↓                                              │
│  // 消息区域高亮在 render 内部完成（现有逻辑不变）      │
│       ↓                                              │
│  if screen_selection.is_active() {                   │
│      写入高亮到 terminal backend buffer               │
│  }                                                   │
│       ↓                                              │
│  app.ui.screen_snapshot = Some(backend buffer clone) │
│  // terminal.flush() 由 Terminal::draw 自动完成       │
└─────────────────────────────────────────────────────┘

┌─ Mouse Events ──────────────────────────────────────┐
│  Down 在消息区域:  text_selection.start_drag()       │
│         （现有逻辑不变）                               │
│                                                      │
│  Down 在其他区域:  screen_selection.start_drag()     │
│         + 清除 text_selection                         │
│                                                      │
│  Down 在 textarea: textarea.start_selection()        │
│         + 记录 pending_screen_start                   │
│                                                      │
│  Drag:                                                │
│    text_selection.dragging → 更新 text_selection     │
│    screen_selection.dragging → 更新 screen_selection │
│    pending_screen_start + 移动 → 激活 screen_selection│
│                                                      │
│  Up:                                                  │
│    text_selection → 现有逻辑提取+复制                 │
│    screen_selection → 从 snapshot 提取+复制           │
│    pending（未拖拽）→ 清除 screen 状态                │
└─────────────────────────────────────────────────────┘
```

### TextArea 兼容策略

TextArea（输入框）有自己的光标和选区系统。兼容方案：

| 场景 | 行为 |
|------|------|
| 点击 textarea 未拖拽 | 清除所有选区状态，click 正常工作（定位光标） |
| 从 textarea 开始拖拽 | ScreenSelection 激活，textarea 选区被清除 |
| 从其他区域拖入 textarea | ScreenSelection 继续，跨越区域边界 |
| 点击 textarea 外部未拖拽 | 清除所有选区（点击即取消选区） |

实现：MouseDown 在 textarea 时记录 `pending_screen_start` 但不激活选区。MouseDrag 时若移动距离 > 0 则激活 ScreenSelection。MouseUp 时若未拖拽则清除 pending 状态。

### 高亮渲染

**消息区域**：现有逻辑不变——在 `render_messages()` 的阶段 3 中，通过 wrap_map 将选区映射到 Line spans 上应用 `SELECTION_BG`。

**其他区域**：在 `draw_app()` 中，`render()` 之后：

```rust
fn apply_screen_selection_highlight(
    backend: &mut CrosstermBackend,
    selection: &ScreenSelection,
    messages_area: Rect,  // 排除消息区域，避免与 TextSelection 冲突
    width: u16,
) {
    let (sr, sc, er, ec) = selection.normalized_range();
    for row in sr..=er {
        // 跳过消息区域的行（由 TextSelection 处理）
        if row >= messages_area.y && row < messages_area.y + messages_area.height {
            // 仅高亮消息区域外的列部分
            // （如果选区跨越消息区域边界，只高亮边界外的部分）
            continue; // 简化：消息区域内的高亮由 TextSelection 处理
        }
        let col_start = if row == sr { sc } else { 0 };
        let col_end = if row == er { ec } else { width - 1 };
        for col in col_start..=col_end {
            let cell = &mut backend.buffer_mut()[(col, row)];
            cell.set_style(Style::default().bg(SELECTION_BG));
        }
    }
}
```

跨区域选区时，消息区域内的部分由 TextSelection 高亮，消息区域外的部分由 Buffer 高亮。两者视觉上无缝衔接（相同的 `SELECTION_BG` 颜色）。

### 文本提取

**消息区域内 TextSelection 活跃时**：现有 `extract_selected_text()` 逻辑不变。

**ScreenSelection 活跃时**：从 ScreenSnapshot 提取选区文本。跨区域时混合提取：

```rust
fn extract_mixed_text(
    snapshot: &ScreenSnapshot,
    screen_sel: &ScreenSelection,
    text_sel: &TextSelection,
    messages_area: Option<Rect>,
    wrap_map: &[WrappedLineInfo],
    usable_width: u16,
) -> Option<String> {
    let (sr, sc, er, ec) = screen_sel.normalized_range();
    let mut lines = Vec::new();
    for row in sr..=er {
        if let Some(area) = messages_area {
            if row >= area.y && row < area.y + area.height {
                // 消息区域行：尝试从 TextSelection 提取（如果该行在 text_sel 范围内）
                // 否则从 snapshot 提取
            }
        }
        // 非消息区域行：从 snapshot 提取
        let col_start = if row == sr { sc as usize } else { 0 };
        let col_end = if row == er { (ec + 1) as usize } else { snapshot.width };
        let line: String = (col_start..col_end)
            .map(|col| snapshot.cell_symbol(row as usize, col))
            .collect();
        lines.push(line.trim_end().to_string());
    }
    Some(lines.join("\n"))
}
```

Cell 符号处理：
- 普通 Cell：`cell.symbol()` 返回字符
- 宽字符占位 Cell（CJK 等）：占位 Cell 的 `symbol()` 为空字符串 `""`，自动拼接为空
- 空白 Cell：`symbol()` 为 `" "`（空格）

### draw_app 改造

```rust
fn draw_app(terminal: &mut Terminal<CrosstermBackend<Stdout>>, app: &mut App) -> Result<()> {
    // render 内部完成消息区域的 TextSelection 高亮
    terminal.draw(|f| ui::main_ui::render(f, app))?;

    // 消息区域外的 ScreenSelection 高亮
    let screen_sel = app.session_mgr.current().ui.screen_selection.clone();
    if screen_sel.is_active() {
        let width = terminal.backend().size()?.width;
        let messages_area = app.session_mgr.current().ui.messages_area;
        apply_screen_selection_highlight(terminal.backend_mut(), &screen_sel, messages_area, width);
    }

    // 快照 Buffer（用于下次文本提取）
    let snapshot = ScreenSnapshot::from_buffer(terminal.backend().buffer());
    app.session_mgr.current_mut().ui.screen_snapshot = Some(snapshot);
    Ok(())
}
```

`terminal.draw()` 结束后 `app` 不再被借用，可以安全修改。

### 旧系统迁移

**Phase 1（本 PRD）**：新增 ScreenSelection + ScreenSnapshot，与旧 TextSelection 并存。消息区域走 TextSelection，其他区域走 ScreenSelection。旧 PanelTextSelection 不再被触发。

**Phase 2（后续）**：确认新系统稳定后，移除旧系统代码：
- `PanelTextSelection` 结构体
- `extract_panel_text()` 函数
- 各面板的 `panel_plain_lines` 存储和 `highlight_line_spans()` 调用
- `UiState` 中的 `panel_selection`、`panel_plain_lines` 字段
- 消息区域的 TextSelection 保留（仍提供最佳的文本复制质量）

## 涉及文件

| 文件 | 改动类型 | 说明 |
|------|---------|------|
| `peri-tui/src/app/text_selection.rs` | 修改 | +`ScreenSelection` +`ScreenSnapshot` +`extract_snapshot_text()` |
| `peri-tui/src/app/ui_state.rs` | 修改 | +`screen_selection` +`screen_snapshot` +`pending_screen_start` |
| `peri-tui/src/main.rs` | 修改 | `draw_app()` 改造：screen highlight + snapshot |
| `peri-tui/src/event/mod.rs` | 修改 | MouseDown/Drag/Up 分发：消息区域→TextSelection，其他→ScreenSelection |
| `peri-tui/src/event/mouse.rs` | 修改 | +`copy_screen_selection_to_clipboard()` |
| `peri-tui/src/ui/main_ui/message_area.rs` | 不变 | 现有 TextSelection 高亮逻辑保留 |
| 各面板渲染文件（7个） | Phase 2 | 移除 panel_selection 高亮代码 |

## 验收标准

1. **消息区域选中**：行为与现在完全一致——字符级精度，复制干净文本，蓝色高亮。
2. **面板区域选中**：所有面板（包括 memory/hooks/plugin 等之前缺少高亮的）均可拖动选中并显示蓝色高亮。
3. **其他区域选中**：状态栏、sticky header、bg agent bar 均可拖动选中并高亮。
4. **自动复制**：松开鼠标后文本自动写入剪贴板，显示 "已复制 N 个字符" toast。
5. **跨区域选中**：选区可跨越消息区域和面板区域的边界，视觉上无缝衔接。
6. **TextArea 不受影响**：点击输入框可正常定位光标和输入文字；从输入框拖拽可选中文字。
7. **CJK 支持**：中日韩宽字符选中和复制正确。
8. **取消选区**：点击任意位置（非拖拽）取消所有选区高亮。
9. **Resize 清除**：终端 resize 时清除所有选区。
10. **构建通过**：`cargo build -p peri-tui` 和 `cargo test -p peri-tui` 通过。
