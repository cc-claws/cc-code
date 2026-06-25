# 实施计划: F013 - SelectableList 鼠标交互扩展 + 面板列表统一

## 依赖图

```
Phase 1: Widget 层基础（Step 1-6）
=================================
Step 1 (ListState 鼠标字段 + update_mouse/hovered/set_cursor_by_mouse/on_select)
  |
  +---> Step 2 (SelectableList hover_style + render_item 三态签名变更)
  |       |
  |       +---> Step 3 (list.rs 内联测试——鼠标悬停/点击/滚动)
  |
  +---> Step 4 (ListOverlay 浮动容器——新文件)
          |
          +---> Step 5 (list_overlay.rs 内联测试)
          |
          +---> Step 6 (lib.rs 导出)

Phase 2: TUI 基础设施（Step 7-10）
=================================
Step 7 (PanelList<T> 统一状态管理器——新文件)
  |
  +---> Step 8 (PanelComponent trait 增加 handle_mouse)
  |       |
  |       +---> Step 9 (PanelManager dispatch_mouse + dispatch_scroll 统一)
  |               |
  |               +---> Step 10 (event.rs 鼠标事件统一路由——消除 ad-hoc 处理)

Phase 3: 面板一次性迁移到 PanelList<T>（Step 11-19）
===================================================
Step 10 完成后，以下 Step 分两批执行：

第一批（标准列表——互不依赖，可并行）：
  Step 11  AgentPanel      PanelList<AgentItem> 替代 cursor + scroll_offset
  Step 12  ModelPanel      PanelList 替代 cursor（固定 4 项，无滚动）
  Step 13  MemoryPanel     PanelList<MemoryEntry> 替代 cursor + scroll_offset
  Step 14  HooksPanel      PanelList<HookEventEntry> 替代 cursor + scroll_offset
  Step 15  CronPanel       PanelList<CronTask> 替代 cursor + scroll_offset

第二批（多视图/混合面板——稍复杂）：
  Step 16  McpPanel        server_list: PanelList<ServerInfo>, detail: 独立 cursor
  Step 17  PluginPanel     installed/discover/marketplace 各持 PanelList
  Step 18  ConfigPanel     Browse 模式: PanelList 替代 cursor + scroll_offset
  Step 19  LoginPanel      Browse 模式: PanelList 替代 cursor + scroll_offset

Phase 4: 清理（Step 20-21）
===========================
Step 20  移除 panel_ops.rs 中的 ensure_cursor_visible 及辅助函数
Step 21  移除 App 上的 mcp_panel_scroll_up/down、agent_panel_move_up/down、hooks_panel_move_up/down
```

**Phase 1** 是基础，Step 2 和 Step 4 可并行。**Phase 2** 建立统一的 `PanelList<T>` 和鼠标分发机制。**Phase 3** 的核心变更是所有面板从分散的 `cursor: usize + scroll_offset: u16` 迁移到统一的 `PanelList<T>`，鼠标和滚动能力随之自动获得。**Phase 4** 清理遗留代码。

---

## Step 1: ListState 增加鼠标字段和方法

**文件**: `peri-widgets/src/list.rs`

**改动:**

1. `ListState<T>` 新增两个字段：

```rust
pub struct ListState<T> {
    items: Vec<T>,
    cursor: usize,
    pub scroll: ScrollState,
    // 新增
    mouse_pos: Option<(u16, u16)>,  // 鼠标在列表可见区域内的相对坐标 (row, col)
    on_select: Option<Box<dyn Fn(usize)>>,  // 选中回调
}
```

1. `new()` 初始化新字段：`mouse_pos: None, on_select: None`

2. 新增方法：

```rust
/// 更新鼠标位置（渲染前由 TUI 层调用，传入相对坐标）
pub fn update_mouse(&mut self, pos: Option<(u16, u16)>) {
    self.mouse_pos = pos;
}

/// 根据鼠标位置计算悬停的 item 索引（考虑 scroll offset）
pub fn hovered(&self) -> Option<usize> {
    let (row, _) = self.mouse_pos?;
    let idx = row as usize + self.scroll.offset() as usize;
    if idx < self.items.len() { Some(idx) } else { None }
}

/// 设置鼠标位置对应的 item 为 cursor（点击选择）
pub fn set_cursor_by_mouse(&mut self, row: u16) {
    let idx = row as usize + self.scroll.offset() as usize;
    if idx < self.items.len() {
        self.cursor = idx;
    }
}

/// 触发选中回调
pub fn select(&self) {
    if let Some(ref cb) = self.on_select {
        cb(self.cursor);
    }
}

/// 设置选中回调
pub fn on_select(mut self, f: impl Fn(usize) + 'static) -> Self {
    self.on_select = Some(Box::new(f));
    self
}
```

**验证**: `cargo build -p peri-widgets` 编译通过

---

## Step 2: SelectableList 增加 hover_style 和三态渲染

**文件**: `peri-widgets/src/list.rs`

**改动:**

1. `SelectableList` 新增 `hover_style` 字段：

```rust
pub struct SelectableList<'a, T> {
    #[allow(clippy::type_complexity)]
    render_item: Box<dyn Fn(&T, bool, bool) -> Line<'a>>,  // (item, is_cursor, is_hovered)
    cursor_marker: &'a str,
    cursor_style: Style,
    hover_style: Style,        // 新增
    normal_style: Style,
}
```

1. `new()` 签名变更：

```rust
pub fn new(render_item: impl Fn(&T, bool, bool) -> Line<'a> + 'static) -> Self {
    Self {
        render_item: Box::new(render_item),
        cursor_marker: "▶ ",
        cursor_style: Style::default(),
        hover_style: Style::default(),
        normal_style: Style::default(),
    }
}
```

1. 新增 builder 方法：

```rust
pub fn hover_style(mut self, style: Style) -> Self {
    self.hover_style = style;
    self
}
```

1. `StatefulWidget::render` 修改——三态样式选择：

```rust
fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
    let cursor = state.cursor;
    let hovered_idx = state.hovered();

    let mut lines: Vec<Line<'_>> = Vec::with_capacity(state.items.len());
    for (i, item) in state.items.iter().enumerate() {
        let is_cursor = i == cursor;
        let is_hovered = hovered_idx == Some(i);

        let line = (self.render_item)(item, is_cursor, is_hovered);

        // 三态：cursor 优先 > hover > normal
        let style = if is_cursor {
            self.cursor_style
        } else if is_hovered {
            self.hover_style
        } else {
            self.normal_style
        };
        let marker = Span::styled(
            " ".repeat(self.cursor_marker.chars().count()),
            style,
        );
        let mut spans = vec![marker];
        spans.extend(line.spans.iter().cloned());
        let styled_line = Line::from(
            spans.into_iter().map(|s| s.patch_style(style)).collect::<Vec<_>>()
        );
        lines.push(styled_line);
    }

    let text = Text::from(lines);
    let visible = area.height;
    state.scroll.ensure_visible(cursor as u16, visible);

    let paragraph = Paragraph::new(text).scroll((state.scroll.offset(), 0));
    Widget::render(paragraph, area, buf);
}
```

**向后兼容注意**：

- `render_item` 闭包从 `Fn(&T, bool)` 变为 `Fn(&T, bool, bool)`——这是 **破坏性变更**
- 当前 TUI 层无 `SelectableList` 调用方（grep 确认），仅在测试中使用
- Step 3 中同步更新所有测试闭包签名

**验证**: `cargo build -p peri-widgets` 编译通过

---

## Step 3: list.rs 鼠标测试用例

**文件**: `peri-widgets/src/list.rs`（内联 `#[cfg(test)] mod tests`）

**改动**: 在现有 `mod tests` 块中追加以下测试：

| 测试名 | 场景 |
|--------|------|
| `test_list_state_hovered_within_viewport` | `update_mouse(Some((1, 0)))` + scroll offset=0 → `hovered()` 返回 Some(1) |
| `test_list_state_hovered_with_scroll_offset` | scroll offset=5 + `update_mouse(Some((0, 0)))` → `hovered()` 返回 Some(5) |
| `test_list_state_hovered_out_of_bounds` | `update_mouse(Some((100, 0)))` → `hovered()` 返回 None |
| `test_list_state_hovered_none_when_no_mouse` | `update_mouse(None)` → `hovered()` 返回 None |
| `test_list_state_set_cursor_by_mouse` | `update_mouse(Some((2, 0)))` + `set_cursor_by_mouse(2)` → cursor 变为 2+scroll.offset |
| `test_selectable_list_hover_style_applied` | 渲染列表，设置 `hover_style(bg=Color::Blue)`，检查 hovered 行 cell 的 bg |
| `test_selectable_list_cursor_overrides_hover` | cursor 和 hover 指向同一行，验证使用 cursor_style |
| `test_selectable_list_no_mouse_unchanged` | 不调用 `update_mouse`，渲染结果与原有一致（回归测试） |

**注意**: 更新现有测试 `selectable_list_renders_cursor_marker` 和 `selectable_list_custom_render_item` 中的闭包签名，从 `|item: &&str, _is_cursor: bool|` 改为 `|item: &&str, _is_cursor: bool, _is_hovered: bool|`。

**验证**: `cargo test -p peri-widgets --lib -- list`

---

## Step 4: ListOverlay 浮动容器

**文件**: `peri-widgets/src/list_overlay.rs`（**新增**）

**改动**: 按照设计文档实现 `ListOverlay` 组件：

- `Anchor` 枚举：`Below { x, y }` / `Above { x, y }` / `Centered`
- `OverlayPosition` 枚举：`Auto` / `Below` / `Above`
- `ListOverlayState`：追踪 `last_area: Option<Rect>`
- `ListOverlay<'a, T>`：组合 `SelectableList` + `BorderedPanel` + `Clear`
  - `new(list)` / `title()` / `border_style()` / `position()` / `max_height()` / `anchor()` / `width()` builder
  - `render(f, viewport, list_state, overlay_state)` 方法
  - `calculate_area()` 私有方法处理锚点定位和边界钳位

**关键实现要点**:

- 复用 `BorderedPanel::render(f, area)` 获取 inner Rect
- `Clear` widget 清除浮动区域背景
- 面板高度 = `content_height + 2`（上下边框各 1 行）
- x 坐标钳位到 `viewport.width.saturating_sub(self.width)`

**验证**: `cargo build -p peri-widgets` 编译通过

---

## Step 5: list_overlay.rs 测试

**文件**: `peri-widgets/src/list_overlay.rs`（内联 `#[cfg(test)] mod tests`）

| 测试名 | 场景 |
|--------|------|
| `test_overlay_state_initial_none` | `ListOverlayState::new()` 的 `area()` 返回 None |
| `test_overlay_state_tracks_area` | 渲染后 `area()` 返回非零 Rect |
| `test_overlay_renders_items` | 渲染 3 项列表，检查 buffer 中包含预期内容 |
| `test_overlay_below_anchor` | Anchor::Below，验证 y >= anchor.y |
| `test_overlay_above_anchor_fallback` | viewport 上方空间不足时回退到 Below |
| `test_overlay_max_height_clamped` | items > max_height 时面板高度 = max_height + 2 |
| `test_overlay_clears_background` | 渲染前在 area 写入内容，渲染后确认被 Clear 覆盖 |

**验证**: `cargo test -p peri-widgets --lib -- list_overlay`

---

## Step 6: lib.rs 导出

**文件**: `peri-widgets/src/lib.rs`

**改动:**

1. 新增模块声明：`pub mod list_overlay;`
2. 新增重导出：

```rust
pub use list_overlay::{Anchor, ListOverlay, ListOverlayState, OverlayPosition};
```

**验证**: `cargo build -p peri-widgets` 编译通过；`cargo test -p peri-widgets` 全量通过

---

## Step 7: PanelList<T> 统一状态管理器

**文件**: `peri-tui/src/app/panel_list.rs`（**新增**）

**改动**: 按照设计文档实现 `PanelList<T>`：

```rust
/// 统一面板列表状态管理器
///
/// 封装 cursor / scroll_offset / items，提供统一的键盘导航、
/// 鼠标点击和滚轮滚动处理。面板不再直接管理这些字段。
pub struct PanelList<T> {
    items: Vec<T>,
    cursor: usize,
    scroll_offset: u16,
}
```

实现以下方法：

| 方法 | 签名 | 说明 |
|------|------|------|
| `new` | `pub fn new() -> Self` | 空列表，cursor=0, scroll_offset=0 |
| `set_items` | `pub fn set_items(&mut self, items: Vec<T>)` | 替换列表，clamp cursor |
| `items` | `pub fn items(&self) -> &[T]` | 不可变引用 |
| `cursor` | `pub fn cursor(&self) -> usize` | 当前光标 |
| `len` | `pub fn len(&self) -> usize` | 列表长度 |
| `is_empty` | `pub fn is_empty(&self) -> bool` | 是否为空 |
| `move_cursor` | `pub fn move_cursor(&mut self, delta: isize)` | clamp 模式，不循环 |
| `handle_scroll` | `pub fn handle_scroll(&mut self, lines: i16, visible_height: u16)` | clamp 到合法范围 |
| `handle_mouse_click` | `pub fn handle_mouse_click(&mut self, mouse_row: u16, mouse_col: u16, area: Rect, border_top: u16) -> bool` | 计算 item 索引 |
| `ensure_visible` | `pub fn ensure_visible(&mut self, visible_height: u16)` | cursor 跟随滚动 |
| `visible_range` | `pub fn visible_range(&self, visible_height: u16) -> Range<usize>` | 可视范围 |
| `scroll_offset` | `pub fn scroll_offset(&self) -> u16` | 当前滚动偏移 |
| `selected` | `pub fn selected(&self) -> Option<&T>` | 当前选中项 |
| `selected_mut` | `pub fn selected_mut(&mut self) -> Option<&mut T>` | 可变引用 |
| `clamp_cursor` | `pub fn clamp_cursor(&mut self)` | 确保 cursor 合法 |

**关键设计决策**:

- `move_cursor` 使用 `clamp` 而非 `rem_euclid`——与当前 AgentPanel/HooksPanel 循环行为不同，统一为 clamp
- `handle_scroll` 限制 `scroll_offset` 不超过 `items.len() - visible_height`——修复 McpPanel/PluginPanel 无限滚动
- `handle_mouse_click` 返回 `bool`——由面板决定点击后行为
- 不含 `on_select` 回调——操作由面板自行决定

**内联测试**:

| 测试名 | 场景 |
|--------|------|
| `test_panel_list_new_empty` | 初始状态为空 |
| `test_panel_list_move_cursor_clamp` | 移动不超过边界 |
| `test_panel_list_move_cursor_no_wrap` | 不循环（与 rem_euclid 不同） |
| `test_panel_list_set_items_clamp_cursor` | 缩短列表时 cursor 被 clamp |
| `test_panel_list_handle_scroll_clamp` | 滚动不超过上界 |
| `test_panel_list_handle_scroll_down_clamp` | 向下滚动不超过 items.len - visible_height |
| `test_panel_list_ensure_visible_up` | cursor 在视口上方时 scroll_offset 跟随 |
| `test_panel_list_ensure_visible_down` | cursor 在视口下方时 scroll_offset 跟随 |
| `test_panel_list_handle_mouse_click_valid` | 点击有效区域返回 true 并更新 cursor |
| `test_panel_list_handle_mouse_click_outside` | 点击面板外返回 false |
| `test_panel_list_handle_mouse_click_below_items` | 点击超出列表项范围返回 false |
| `test_panel_list_visible_range` | 验证可视范围计算正确 |

**验证**: `cargo build -p peri-tui` 编译通过；`cargo test -p peri-tui --lib -- panel_list`

---

## Step 8: PanelComponent trait 增加 handle_mouse

**文件**: `peri-tui/src/app/panel_component.rs`

**改动:**

1. 新增 import：

```rust
use ratatui::crossterm::event::MouseEvent;
```

1. 在 `handle_scroll` 方法之后新增 `handle_mouse` 默认实现：

```rust
/// 处理鼠标事件（点击、悬停移动等）
///
/// 默认不消费。面板按需覆写以支持鼠标点击选择等交互。
/// 鼠标滚轮事件通过 `handle_scroll` 分发，不经过此方法。
fn handle_mouse(
    &mut self,
    _mouse: MouseEvent,
    _area: Rect,
    _ctx: &mut PanelContext<'_>,
) -> EventResult {
    EventResult::NotConsumed
}
```

**验证**: `cargo build -p peri-tui` 编译通过（所有面板通过默认实现兼容）

---

## Step 9: PanelManager 增加 dispatch_mouse + dispatch_scroll 统一

**文件**: `peri-tui/src/app/panel_manager.rs`

**改动:**

在 `dispatch_scroll` 方法之后新增 `dispatch_mouse`，模式与 `dispatch_scroll` 一致：

```rust
/// 分发鼠标事件到当前激活面板
pub fn dispatch_mouse(
    &mut self,
    mouse: MouseEvent,
    area: Rect,
    ctx: &mut PanelContext<'_>,
) -> EventResult {
    use super::panel_component::PanelComponent;
    let Some(state) = self.active.as_mut() else {
        return EventResult::NotConsumed;
    };
    match state {
        PanelState::Model(p) => p.handle_mouse(mouse, area, ctx),
        PanelState::Agent(p) => p.handle_mouse(mouse, area, ctx),
        PanelState::Hooks(p) => p.handle_mouse(mouse, area, ctx),
        PanelState::Status(p) => p.handle_mouse(mouse, area, ctx),
        PanelState::Memory(p) => p.handle_mouse(mouse, area, ctx),
        PanelState::Login(p) => p.handle_mouse(mouse, area, ctx),
        PanelState::Config(p) => p.handle_mouse(mouse, area, ctx),
        PanelState::ThreadBrowser(p) => p.handle_mouse(mouse, area, ctx),
        PanelState::Mcp(p) => p.handle_mouse(mouse, area, ctx),
        PanelState::Cron(p) => p.handle_mouse(mouse, area, ctx),
        PanelState::Plugin(p) => p.handle_mouse(mouse, area, ctx),
    }
}
```

**注意**：需新增 `MouseEvent` import。

**验证**: `cargo build -p peri-tui` 编译通过

---

## Step 10: event.rs 鼠标事件统一路由

**文件**: `peri-tui/src/event.rs`

**现状问题**：

当前鼠标滚轮处理（约 line 1008-1066）存在大量 ad-hoc 代码：

- McpPanel 滚轮：直接调用 `app.mcp_panel_scroll_up/down(3)`，不经过 `dispatch_scroll`
- PluginPanel 滚轮：直接操作 `panel.scroll_offset` 字段，不经过 `dispatch_scroll`
- 面板区域 hit test 逻辑重复 4 次（ScrollUp × 2 + ScrollDown × 2）
- 鼠标点击事件（Down(MouseButton::Left)）中面板无法拦截点击

**改动**：

1. **提取 `mouse_in_rect` 辅助函数**（消除重复 hit test）：

```rust
fn mouse_in_rect(mouse: &MouseEvent, area: Rect) -> bool {
    mouse.row >= area.y
        && mouse.row < area.y + area.height
        && mouse.column >= area.x
        && mouse.column < area.x + area.width
}
```

1. **统一滚轮分发**：将 McpPanel、PluginPanel 的滚轮处理从 ad-hoc 改为走 `dispatch_scroll`：

```rust
MouseEventKind::ScrollUp => {
    if let Some(area) = panel_area {
        if mouse_in_rect(&mouse, area) && panel_active {
            if app.global_panels.dispatch_scroll(-3, ctx) == EventResult::Consumed {
                return Ok(Some(Action::Redraw));
            }
        }
    }
    app.scroll_up();
}
MouseEventKind::ScrollDown => {
    if let Some(area) = panel_area {
        if mouse_in_rect(&mouse, area) && panel_active {
            if app.global_panels.dispatch_scroll(3, ctx) == EventResult::Consumed {
                return Ok(Some(Action::Redraw));
            }
        }
    }
    app.scroll_down();
}
```

1. **新增鼠标点击分发**：在 `Down(MouseButton::Left)` 分支中，面板区域优先拦截：

```rust
MouseEventKind::Down(MouseButton::Left) => {
    if let Some(area) = panel_area {
        if mouse_in_rect(&mouse, area) && panel_active {
            if app.global_panels.dispatch_mouse(mouse, area, ctx) == EventResult::Consumed {
                return Ok(Some(Action::Redraw));
            }
        }
    }
    // 现有的 session 切换、textarea 定位等逻辑保持不变...
}
```

1. **删除 ad-hoc 代码**：
   - 移除 `app.mcp_panel_scroll_up/down(3)` 调用
   - 移除 PluginPanel `panel.scroll_offset` 直接操作
   - 移除 `current_list_len()` 调用

**注意**：dispatch_mouse 和 dispatch_scroll 需要使用 `std::mem::take` 模式处理借用冲突（与现有的 dispatch_key 模式一致）。

**验证**: `cargo build -p peri-tui`；手动测试 McpPanel/PluginPanel 滚轮行为不变

---

## Step 11: AgentPanel 迁移到 PanelList\<AgentItem\>

**文件**: `peri-tui/src/app/agent_panel.rs`

**当前状态**:

- `cursor: usize` + `scroll_offset: u16`（分散字段）
- `move_cursor()` 使用 `rem_euclid`（循环导航）
- `handle_key` 中手动调用 `ensure_cursor_visible()`
- 无 `handle_scroll`、无 `handle_mouse`

**改动**:

1. 结构体变更：

```rust
pub struct AgentPanel {
    pub agents: Vec<AgentItem>,
    pub selected_id: Option<String>,
    list: PanelList<AgentItem>,  // 替代 cursor + scroll_offset
}
```

1. 移除 `cursor`、`scroll_offset` 字段和 `move_cursor()` 方法。新增委托方法：

```rust
pub fn cursor(&self) -> usize { self.list.cursor() }
pub fn scroll_offset(&self) -> u16 { self.list.scroll_offset() }
pub fn total(&self) -> usize { self.list.len() }
pub fn current_agent(&self) -> Option<&AgentItem> { self.list.selected() }
```

1. `handle_key` 变更：

```rust
Input { key: Key::Up, .. } => {
    self.list.move_cursor(-1);  // clamp，不再循环
    self.list.ensure_visible(10);
    EventResult::Consumed
}
Input { key: Key::Down, .. } => {
    self.list.move_cursor(1);
    self.list.ensure_visible(10);
    EventResult::Consumed
}
```

**行为变更**: 导航从循环（`rem_euclid`）变为 clamp（到顶/底停住）。

1. 新增 `handle_scroll`：

```rust
fn handle_scroll(&mut self, lines: i16, _ctx: &mut PanelContext<'_>) -> EventResult {
    self.list.handle_scroll(lines, 10);
    EventResult::Consumed
}
```

1. 新增 `handle_mouse`：

```rust
fn handle_mouse(&mut self, mouse: MouseEvent, area: Rect, ctx: &mut PanelContext<'_>) -> EventResult {
    if mouse.kind == MouseEventKind::Down(MouseButton::Left) {
        if self.list.handle_mouse_click(mouse.row, mouse.column, area, 1) {
            return self.handle_key(
                Input::from(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
                ctx,
            );
        }
    }
    EventResult::Consumed
}
```

**渲染影响**: `render_agent_panel` 中 `panel.cursor` 和 `panel.scroll_offset` 改为 `panel.cursor()` 和 `panel.scroll_offset()`。

**验证**: 手动测试——键盘导航（不再循环）、鼠标点击选择 agent、滚轮滚动

---

## Step 12: ModelPanel 迁移到 PanelList

**文件**: `peri-tui/src/app/model_panel.rs`

**当前状态**: 固定 4 项列表（Opus/Sonnet/Haiku/Effort），`cursor: usize`，循环导航。

**改动**:

1. 结构体变更：

```rust
pub struct ModelPanel {
    pub provider_name: String,
    pub active_tab: AliasTab,
    pub buf_thinking_effort: String,
    list: PanelList<AliasTab>,  // items = [Opus, Sonnet, Haiku, Effort]
}
```

1. `new()` 中 `list.set_items(vec![AliasTab::Opus, AliasTab::Sonnet, AliasTab::Haiku, AliasTab::Effort])`。

2. `move_cursor` 委托到 `self.list.move_cursor(delta)`——**行为变更**: 从循环变为 clamp。4 项固定列表，效果几乎等价。

3. `handle_key` 中 `self.cursor` 改为 `self.list.cursor()`。

4. 新增 `handle_mouse`：

```rust
fn handle_mouse(&mut self, mouse: MouseEvent, area: Rect, ctx: &mut PanelContext<'_>) -> EventResult {
    if mouse.kind == MouseEventKind::Down(MouseButton::Left) {
        if self.list.handle_mouse_click(mouse.row, mouse.column, area, 1) {
            return self.handle_key(
                Input::from(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
                ctx,
            );
        }
    }
    EventResult::Consumed
}
```

**渲染影响**: `render_model_panel` 中 `panel.cursor` 改为 `panel.list.cursor()`。

**验证**: 手动测试——点击模型项切换模型

---

## Step 13: MemoryPanel 迁移到 PanelList\<MemoryEntry\>

**文件**: `peri-tui/src/app/memory_panel.rs`

**当前状态**: 2 个固定条目，`cursor: usize`，手动 `move_cursor_up/down`。

**改动**:

1. 结构体变更：

```rust
pub struct MemoryPanel {
    pub entries: Vec<MemoryEntry>,
    list: PanelList<MemoryEntry>,  // 替代 cursor
}
```

1. 移除 `cursor`、`scroll_offset` 字段和 `move_cursor_up/down` 方法。

2. `handle_key` 变更：

```rust
Input { key: Key::Up, .. } => { self.list.move_cursor(-1); EventResult::Consumed }
Input { key: Key::Down, .. } => { self.list.move_cursor(1); EventResult::Consumed }
```

1. 新增 `handle_mouse`：

```rust
fn handle_mouse(&mut self, mouse: MouseEvent, area: Rect, ctx: &mut PanelContext<'_>) -> EventResult {
    if mouse.kind == MouseEventKind::Down(MouseButton::Left) {
        if self.list.handle_mouse_click(mouse.row, mouse.column, area, 1) {
            return self.handle_key(
                Input::from(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
                ctx,
            );
        }
    }
    EventResult::Consumed
}
```

**渲染影响**: `render_memory_panel` 中 `panel.cursor` 改为 `panel.list.cursor()`。`panel_ops.rs` 中 `memory_panel_open_editor` 引用 `p.cursor` 改为 `p.list.cursor()`。

**验证**: 手动测试——点击条目打开编辑器

---

## Step 14: HooksPanel 迁移到 PanelList\<HookEventEntry\>

**文件**: `peri-tui/src/app/hooks_panel.rs`

**当前状态**: `entries: Vec<HookEventEntry>`，`cursor: usize`，`scroll_offset: u16`，`move_cursor()` 使用 `rem_euclid`（循环）。

**改动**:

1. 结构体变更：

```rust
pub struct HooksPanel {
    pub entries: Vec<HookEventEntry>,
    list: PanelList<HookEventEntry>,  // 替代 cursor + scroll_offset
}
```

1. 移除 `cursor`、`scroll_offset` 字段和 `move_cursor()` 方法。

2. 委托方法：
   - `total()` → `self.list.len()`
   - `current_entry()` → `self.list.selected()`
   - `cursor_line()` 保持不变（仍基于 `self.list.cursor()` 计算 header_lines + cursor）

3. `handle_key` 变更：

```rust
Input { key: Key::Up, .. } => {
    self.list.move_cursor(-1);
    self.list.ensure_visible(10);
    EventResult::Consumed
}
```

**行为变更**: 导航从循环变为 clamp。

1. 新增 `handle_scroll`：

```rust
fn handle_scroll(&mut self, lines: i16, _ctx: &mut PanelContext<'_>) -> EventResult {
    self.list.handle_scroll(lines, 10);
    EventResult::Consumed
}
```

1. 新增 `handle_mouse`（信息型面板，点击仅移动 cursor）：

```rust
fn handle_mouse(&mut self, mouse: MouseEvent, area: Rect, _ctx: &mut PanelContext<'_>) -> EventResult {
    if mouse.kind == MouseEventKind::Down(MouseButton::Left) {
        self.list.handle_mouse_click(mouse.row, mouse.column, area, 1);
    }
    EventResult::Consumed
}
```

**注意**: HooksPanel 的展开详情使得 entries 索引和渲染行号不一致。`PanelList::ensure_visible` 按 entries 索引工作，对 HooksPanel 仅管理 cursor。scroll 的 clamp 逻辑可以在面板层覆写 `handle_scroll`。

**渲染影响**: `render_hooks_panel` 中 `panel.cursor` 改为 `panel.list.cursor()`，`panel.scroll_offset` 改为 `panel.list.scroll_offset()`。

**验证**: 手动测试——点击事件条目查看详情、滚轮滚动

---

## Step 15: CronPanel 迁移到 PanelList\<CronTask\>

**文件**: `peri-tui/src/app/cron_state.rs`

**当前状态**: `tasks: Vec<CronTask>`，`cursor: usize`，`scroll_offset: u16`，`move_cursor()` 使用 clamp。

**改动**:

1. 结构体变更：

```rust
pub struct CronPanel {
    list: PanelList<CronTask>,  // 替代 tasks + cursor + scroll_offset
    pub confirm_delete: bool,
}
```

1. 移除 `tasks`、`cursor`、`scroll_offset` 字段和 `move_cursor()` 方法。

2. 委托方法：
   - `tasks` → `self.list.items()`（`refresh` 中通过 `self.list.set_items(new_tasks)` 替换）

3. `handle_key` 变更：

```rust
Input { key: Key::Up, .. } => { self.list.move_cursor(-1); EventResult::Consumed }
Input { key: Key::Down, .. } => { self.list.move_cursor(1); EventResult::Consumed }
```

1. 新增 `handle_scroll`：

```rust
fn handle_scroll(&mut self, lines: i16, _ctx: &mut PanelContext<'_>) -> EventResult {
    self.list.handle_scroll(lines, 10);
    EventResult::Consumed
}
```

1. 新增 `handle_mouse`（信息型，点击仅移动 cursor）：

```rust
fn handle_mouse(&mut self, mouse: MouseEvent, area: Rect, _ctx: &mut PanelContext<'_>) -> EventResult {
    if mouse.kind == MouseEventKind::Down(MouseButton::Left) {
        self.list.handle_mouse_click(mouse.row, mouse.column, area, 2);
    }
    EventResult::Consumed
}
```

**注意**: `border_top = 2`（CronPanel 有边框 + 标题行）。

**渲染影响**: `render_cron_panel` 中 `panel.tasks` 改为 `panel.list.items()`，`panel.cursor` 改为 `panel.list.cursor()`，`panel.scroll_offset` 改为 `panel.list.scroll_offset()`。

**验证**: 手动测试——点击任务移动光标、滚轮滚动

---

## Step 16: McpPanel 迁移到 PanelList\<ServerInfo\>

**文件**: `peri-tui/src/app/mcp_panel.rs`

**当前状态**: `servers: Vec<ServerInfo>`，`cursor: usize`，`scroll_offset: u16`，双视图（ServerList / ServerDetail），已有 `handle_scroll`。

**改动**:

1. 结构体变更：

```rust
pub struct McpPanel {
    pub servers: Vec<ServerInfo>,  // 保留：内部方法直接访问
    server_list: PanelList<ServerInfo>,  // ServerList 视图的光标/滚动管理
    pub view: McpPanelView,
    pub confirm_delete: Option<String>,
    detail_scroll_offset: u16,  // ServerDetail 视图的滚动（保留独立字段）
}
```

**设计决策**: `servers` 保留为公开字段，因为 `do_enter`、`toggle_disabled` 等方法需要直接访问和替换 servers 列表。`server_list` 仅管理 ServerList 视图的 cursor 和 scroll_offset。

1. `new()` 中初始化 `server_list: PanelList::new()`，然后 `server_list.set_items(servers.clone())`。

2. 移除 `cursor` 和 `scroll_offset` 字段。新增委托：

```rust
pub fn cursor(&self) -> usize { self.server_list.cursor() }
pub fn scroll_offset(&self) -> u16 { self.server_list.scroll_offset() }
```

1. `do_move_up/do_move_down` 变更——按视图分派到 `server_list.move_cursor()` 或 detail cursor。

2. `handle_scroll` 变更——按视图分派到 `server_list.handle_scroll()` 或 `detail_scroll_offset`。

3. 新增 `handle_mouse`：

```rust
fn handle_mouse(&mut self, mouse: MouseEvent, area: Rect, ctx: &mut PanelContext<'_>) -> EventResult {
    if mouse.kind == MouseEventKind::Down(MouseButton::Left) {
        match &self.view {
            McpPanelView::ServerList => {
                if self.server_list.handle_mouse_click(mouse.row, mouse.column, area, 2) {
                    return self.handle_key(
                        Input::from(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
                        ctx,
                    );
                }
            }
            McpPanelView::ServerDetail { actions, .. } => {
                // detail 视图点击操作菜单项
                let inner_y = area.y + 4;
                if mouse.row >= inner_y {
                    let clicked = (mouse.row - inner_y) as usize;
                    if clicked < actions.len() { /* 执行操作 */ }
                }
            }
        }
    }
    EventResult::Consumed
}
```

1. servers 列表变更后同步更新 `server_list`：`self.server_list.set_items(self.servers.clone())`。

**渲染影响**: `render_mcp_panel` 中 `panel.cursor` 改为 `panel.cursor()`，`panel.scroll_offset` 改为 `panel.scroll_offset()`。

**验证**: 手动测试——点击服务器进入详情、滚轮滚动、详情页操作菜单

---

## Step 17: PluginPanel 迁移到 PanelList

**文件**: `peri-tui/src/app/plugin_panel.rs`

**当前状态**: 4 个视图（Installed/Discover/Marketplaces/Errors），每个视图有独立的 cursor 和 scroll_offset。已有 `handle_scroll`。

**改动**:

1. 结构体变更——移除分散的 cursor/scroll_offset 字段，替换为各视图的 `PanelList`：

```rust
pub struct PluginPanel {
    // --- 列表状态管理 ---
    installed_list: PanelList<PluginEntry>,     // Installed 视图
    discover_list: PanelList<DiscoverPlugin>,   // Discover 视图
    marketplace_list: PanelList<MarketplaceViewEntry>,  // Marketplaces 视图

    // 移除: cursor, scroll_offset, discover_cursor, discover_scroll,
    //       marketplace_cursor, marketplace_scroll

    // ... 其余字段保持不变
}
```

1. `new()` 中初始化各 `PanelList`。

2. 委托方法——按视图返回对应的 `list.cursor()` 和 `list.scroll_offset()`。

3. `handle_scroll` 变更——按视图分派到对应的 `PanelList::handle_scroll`。

4. 新增 `handle_mouse`——按视图分派到对应的 `PanelList::handle_mouse_click`。

5. 视图切换时同步 items：`self.installed_list.set_items(self.entries.clone())` 等。

**渲染影响**: 渲染代码中引用 `panel.cursor`、`panel.scroll_offset`、`panel.discover_cursor`、`panel.discover_scroll`、`panel.marketplace_cursor`、`panel.marketplace_scroll` 的地方需要改为对应的 `PanelList` 方法调用。

**验证**: 手动测试——4 个视图的点击、滚轮、导航

---

## Step 18: ConfigPanel 迁移到 PanelList

**文件**: `peri-tui/src/app/config_panel.rs`

**当前状态**: 6 个配置字段，Browse/Edit 双模式，`cursor: usize`，`scroll_offset: u16`。Browse 模式循环导航。

**改动**:

1. 结构体变更：

```rust
pub struct ConfigPanel {
    pub mode: ConfigPanelMode,
    browse_list: PanelList<ConfigEditField>,  // Browse 模式光标管理
    pub edit_field: ConfigEditField,
    // ... 编辑缓冲区字段保持不变
}
```

1. `from_config()` 中 `browse_list.set_items(vec![Autocompact, CompactThreshold, Language, Persona, Tone, Proactiveness])`。

2. `handle_key` Browse 模式变更：

```rust
ConfigPanelMode::Browse => match input {
    Input { key: Key::Up, .. } => {
        self.browse_list.move_cursor(-1);  // clamp，不再循环
        EventResult::Consumed
    }
    Input { key: Key::Down, .. } => {
        self.browse_list.move_cursor(1);
        EventResult::Consumed
    }
    // ...
}
```

**行为变更**: Browse 导航从循环变为 clamp。6 项固定列表，影响极小。

1. 新增 `handle_scroll`（Browse 模式）和 `handle_mouse`（Browse 模式点击进入编辑，Edit 模式点击 toggle 切换）。

**渲染影响**: `render_config_panel` 中 `panel.cursor` 改为 `panel.browse_list.cursor()`。

**验证**: 手动测试——Browse 模式点击字段进入编辑、滚轮滚动

---

## Step 19: LoginPanel 迁移到 PanelList

**文件**: `peri-tui/src/app/login_panel.rs`

**当前状态**: Browse/Edit/New 三模式，`cursor: usize`，`scroll_offset: u16`，Browse 模式循环导航。

**改动**:

1. 结构体变更：

```rust
pub struct LoginPanel {
    pub providers: Vec<ProviderConfig>,
    pub mode: LoginPanelMode,
    browse_list: PanelList<ProviderConfig>,  // Browse 模式光标管理
    pub edit_field: LoginEditField,
    // ... 编辑缓冲区字段保持不变
}
```

1. `from_config()` 中 `browse_list.set_items(providers.clone())`。

2. `move_cursor` 委托到 `browse_list.move_cursor(delta)`。

**行为变更**: Browse 导航从循环变为 clamp。

1. 新增 `handle_scroll`（Browse 模式）和 `handle_mouse`（Browse 模式点击进入编辑）。

**渲染影响**: `render_login_panel` 中 `panel.cursor` 改为 `panel.browse_list.cursor()`。

**验证**: 手动测试——Browse 模式点击 provider 进入编辑

---

## Step 20: 移除 panel_ops.rs 中的遗留辅助函数

**文件**: `peri-tui/src/app/panel_ops.rs`

**改动**:

1. 移除 `agent_panel_move_up()` 方法——已由 AgentPanel 的 `handle_key(Up)` 通过 `PanelList::move_cursor` 覆盖
2. 移除 `agent_panel_move_down()` 方法——同上
3. 移除 `hooks_panel_move_up()` 方法——已由 HooksPanel 的 `handle_key(Up)` 覆盖
4. 移除 `hooks_panel_move_down()` 方法——同上

**文件**: `peri-tui/src/app/mod.rs`

1. 移除 `ensure_cursor_visible()` 函数——已由 `PanelList::ensure_visible` 覆盖

**验证**: `cargo build -p peri-tui` 编译通过；确认无其他调用方

---

## Step 21: 移除 App 上的 ad-hoc 面板滚动方法

**文件**: `peri-tui/src/app/mod.rs`（或对应 impl App 块）

**改动**:

1. 移除 `mcp_panel_scroll_up()` 方法——Step 10 中已不再被 `event.rs` 调用
2. 移除 `mcp_panel_scroll_down()` 方法——同上

**验证**: `cargo build -p peri-tui` 编译通过；`cargo test -p peri-tui` 全量通过

---

## 风险与缓解

| 风险 | 缓解 |
|------|------|
| `render_item` 签名变更破坏外部调用方 | grep 确认 TUI 层无调用方；仅 widgets 内测试需更新 |
| `PanelList::move_cursor` clamp 替代循环导致行为回归 | 设计意图——与鼠标操作一致；4 项以下列表用户几乎无感知 |
| HooksPanel scroll 与 entries 索引不一致 | HooksPanel 对 scroll 的管理覆写 `handle_scroll`，不直接用 `PanelList::handle_scroll` |
| McpPanel/PluginPanel 多视图 PanelList 同步 | 视图切换时 `set_items` + cursor 恢复；测试覆盖视图切换后状态一致性 |
| `event.rs` 中 `dispatch_mouse` 的 `std::mem::take` 借用冲突 | 复用现有 `dispatch_key` 的 `take/put` 模式，已验证可行 |
| 渲染代码大量 `panel.cursor` 引用需要改为 `panel.list.cursor()` | 全局搜索替换，编译器捕获遗漏 |
| `on_select` 用 `Box<dyn Fn>` 存储在 `ListState` 中 | `ListState` 已非 `Copy`，不影响；`on_select` 为 `Option` 默认 None |

## 无新 crate 依赖

所有改动使用现有代码结构。`PanelList<T>` 是纯 Rust 泛型结构体，不引入新依赖。

---

### 关键实施文件

- `peri-widgets/src/list.rs` — ListState 鼠标字段/方法、SelectableList 三态渲染、hover_style
- `peri-widgets/src/list_overlay.rs` — **新增**: ListOverlay/ListOverlayState/Anchor/OverlayPosition
- `peri-widgets/src/lib.rs` — 模块导出
- `peri-tui/src/app/panel_list.rs` — **新增**: PanelList<T> 统一状态管理器
- `peri-tui/src/app/panel_component.rs` — trait 新增 handle_mouse 默认方法
- `peri-tui/src/app/panel_manager.rs` — 新增 dispatch_mouse
- `peri-tui/src/event.rs` — 鼠标事件统一路由、提取 mouse_in_rect、消除 ad-hoc 滚轮代码
- `peri-tui/src/app/agent_panel.rs` — 标准面板迁移的参考实现
- `peri-tui/src/app/panel_ops.rs` — 移除遗留辅助函数
- `peri-tui/src/app/mod.rs` — 移除 ensure_cursor_visible
