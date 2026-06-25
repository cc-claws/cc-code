# Feature: 20260513_F013 - SelectableList 鼠标交互扩展 + 面板列表统一

## 需求背景

当前 TUI 中 10 个面板各自维护 `cursor: usize` + `scroll_offset: u16`，各自写键盘导航和滚动逻辑，导航行为不一致（有的循环、有的 clamp），鼠标支持仅 McpPanel/PluginPanel 有滚轮（且是 ad-hoc 实现，不经过 `dispatch_scroll`）。

核心问题：**面板列表状态管理分散**，导致：

1. 每次新增面板都要重复实现 cursor/scroll/navigation 逻辑
2. 鼠标支持无法统一添加——需要逐个面板补代码
3. `event.rs` 中鼠标滚轮处理存在大量重复 hit test 代码（约 100 行 ad-hoc）

## 目标

- **统一面板列表状态管理**：所有面板通过 `PanelList<T>` 管理列表状态，消除 cursor/scroll_offset 分散
- **鼠标交互一次到位**：`PanelList<T>` 内置鼠标点击选择 + 滚轮滚动，面板迁移即自动获得鼠标支持
- **统一导航行为**：所有面板统一边界 clamp（不循环），统一 `ensure_cursor_visible` 滚动跟随
- **Widget 层增强**：`ListState` 增加鼠标字段，为未来 widget 层直接使用提供能力

## 方案设计

### 架构概览

两层统一，职责分离：

```
peri-widgets（Widget 层）
├── ListState<T>          ← 增强：mouse_pos, on_select
├── SelectableList        ← 增强：hover_style, 三态渲染
└── ListOverlay           ← 新增：浮动容器

peri-tui（TUI 层）
├── PanelList<T>          ← 新增：统一列表状态管理器
│   ├── cursor / scroll_offset / items
│   ├── move_cursor() / handle_scroll() / handle_mouse_click()
│   └── ensure_visible() / visible_range()
├── PanelComponent trait  ← 增强：默认 handle_scroll/handle_mouse
└── event.rs              ← 重构：统一鼠标路由
```

### Widget 层：ListState 增强

`peri-widgets/src/list.rs` 中 `ListState<T>` 新增字段和方法：

```rust
pub struct ListState<T> {
    items: Vec<T>,
    cursor: usize,
    pub scroll: ScrollState,
    // 新增
    mouse_pos: Option<(u16, u16)>,           // 鼠标相对坐标 (row, col)
    on_select: Option<Box<dyn Fn(usize)>>,   // 选中回调
}
```

新增方法：

- `update_mouse(pos: Option<(u16, u16)>)` — 更新鼠标位置
- `hovered() -> Option<usize>` — 计算悬停索引（考虑 scroll offset）
- `set_cursor_by_mouse(row: u16)` — 鼠标点击设置 cursor
- `select()` — 触发选中回调
- `on_select(f: impl Fn(usize) + 'static) -> Self` — 设置回调

`SelectableList` 新增 `hover_style` 字段，`render_item` 签名从 `Fn(&T, bool)` 变为 `Fn(&T, bool, bool)`（is_cursor, is_hovered），实现三态渲染（cursor > hover > normal）。

**向后兼容**：TUI 层当前无 `SelectableList` 调用方（仅 widgets 内测试），签名变更影响范围可控。

### TUI 层：PanelList<T> 统一状态管理器

新增 `peri-tui/src/app/panel_list.rs`：

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

impl<T> PanelList<T> {
    pub fn new() -> Self;
    pub fn set_items(&mut self, items: Vec<T>);
    pub fn items(&self) -> &[T];
    pub fn cursor(&self) -> usize;
    pub fn len(&self) -> usize;
    pub fn is_empty(&self) -> bool;

    /// 键盘导航：边界 clamp，到顶/底停住
    pub fn move_cursor(&mut self, delta: isize) {
        let new = self.cursor as isize + delta;
        let max = self.items.len().saturating_sub(1) as isize;
        self.cursor = new.clamp(0, max.max(0)) as usize;
    }

    /// 滚轮滚动：更新 scroll_offset，clamp 到合法范围
    pub fn handle_scroll(&mut self, lines: i16, visible_height: u16) {
        let max_offset = self.items.len().saturating_sub(visible_height as usize) as u16;
        if lines > 0 {
            self.scroll_offset = (self.scroll_offset as i16 + lines).min(max_offset as i16) as u16;
        } else {
            self.scroll_offset = self.scroll_offset.saturating_sub((-lines) as u16);
        }
    }

    /// 鼠标点击：根据绝对坐标和面板区域计算点击的 item 索引
    /// border_top: 面板边框/标题占用的行数（通常 1-3）
    /// 返回点击是否在列表范围内
    pub fn handle_mouse_click(&mut self, mouse_row: u16, mouse_col: u16, area: Rect, border_top: u16) -> bool {
        let list_y = area.y + border_top;
        if mouse_row < list_y || mouse_col < area.x || mouse_col >= area.x + area.width {
            return false;
        }
        let clicked = (mouse_row - list_y + self.scroll_offset) as usize;
        if clicked < self.items.len() {
            self.cursor = clicked;
            true
        } else {
            false
        }
    }

    /// 确保 cursor 在可视区域内，更新 scroll_offset
    pub fn ensure_visible(&mut self, visible_height: u16) {
        if visible_height == 0 { return; }
        let cursor_row = self.cursor as u16;
        if cursor_row < self.scroll_offset {
            self.scroll_offset = cursor_row;
        } else if cursor_row >= self.scroll_offset + visible_height {
            self.scroll_offset = cursor_row.saturating_sub(visible_height - 1);
        }
    }

    /// 返回当前可视范围内的 item 索引范围
    pub fn visible_range(&self, visible_height: u16) -> Range<usize> {
        let start = self.scroll_offset as usize;
        let end = (start + visible_height as usize).min(self.items.len());
        start..end
    }

    pub fn scroll_offset(&self) -> u16;
}
```

**关键设计决策**：

- **边界 clamp 统一**：`move_cursor` 不循环，到顶/底停住。与鼠标操作直觉一致
- **滚动 clamp 有上界**：`handle_scroll` 限制不超过 `items.len() - visible_height`，修复 McpPanel/PluginPanel 当前的无限滚动问题
- **不含渲染逻辑**：`PanelList` 是纯状态管理，渲染仍由各面板的 render 函数负责
- **不含 on_select 回调**：点击后的操作（Enter 对应的行为）由面板自行决定，不在 `PanelList` 中耦合

### PanelComponent trait 增强

`peri-tui/src/app/panel_component.rs`：

```rust
/// 处理鼠标事件（点击、悬停移动等）
fn handle_mouse(
    &mut self,
    _mouse: MouseEvent,
    _area: Rect,
    _ctx: &mut PanelContext<'_>,
) -> EventResult {
    EventResult::NotConsumed
}
```

`PanelManager` 新增 `dispatch_mouse` 方法，模式与 `dispatch_scroll` 一致。

### event.rs 鼠标事件统一路由

重构 `event.rs` 中鼠标事件处理：

1. **提取 `mouse_in_rect` 辅助函数**——消除 4 处重复 hit test
2. **统一滚轮分发**：McpPanel/PluginPanel 的 ad-hoc 滚轮代码改为走 `dispatch_scroll`
3. **新增点击分发**：`Down(MouseButton::Left)` 先尝试 `dispatch_mouse`，面板消费则不再穿透

```rust
// 重构后的滚轮处理伪代码
MouseEventKind::ScrollUp => {
    if let Some(area) = panel_area {
        if mouse_in_rect(&mouse, area) && panel_active {
            if app.global_panels.dispatch_scroll(-3, ctx) == EventResult::Consumed {
                return Ok(Some(Action::Redraw));
            }
        }
    }
    app.scroll_up();  // fallback: 消息区滚动
}
```

### 面板迁移

#### 分类处理

| 类别 | 面板 | 迁移方式 |
|------|------|----------|
| 标准列表 | AgentPanel, HooksPanel, CronPanel | `PanelList<T>` 完全替代 cursor + scroll_offset |
| 固定列表 | MemoryPanel (2项) | `PanelList<T>` 替代，无滚动 |
| 多视图列表 | McpPanel (2视图), PluginPanel (4视图) | 每个视图持有一个 `PanelList<T>` |
| 固定选项 | ModelPanel (4项) | `PanelList<T>` 替代 cursor |
| 混合面板 | ConfigPanel (Browse/Edit), LoginPanel (Browse/Edit/New) | 列表部分用 `PanelList<T>`，表单部分保留自定义 |
| 非列表 | StatusPanel (Tab型) | 不迁移 |

#### 标准列表面板迁移示例（AgentPanel）

迁移前：

```rust
pub struct AgentPanel {
    cursor: usize,
    scroll_offset: u16,
    agents: Vec<AgentItem>,
}
// handle_key 中手动 cursor + ensure_cursor_visible
// handle_scroll 无实现
// handle_mouse 无实现
```

迁移后：

```rust
pub struct AgentPanel {
    list: PanelList<AgentItem>,
}

impl PanelComponent for AgentPanel {
    fn handle_key(&mut self, input: Input, ctx: &mut PanelContext<'_>) -> EventResult {
        match input.key_code() {
            Up => { self.list.move_cursor(-1); self.list.ensure_visible(10); }
            Down => { self.list.move_cursor(1); self.list.ensure_visible(10); }
            Enter => { /* 选择 agent 逻辑 */ }
            _ => {}
        }
        EventResult::Consumed
    }

    fn handle_scroll(&mut self, lines: i16, _ctx: &mut PanelContext<'_>) -> EventResult {
        self.list.handle_scroll(lines, 10);
        EventResult::Consumed
    }

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
}
```

#### 多视图面板迁移（McpPanel）

```rust
pub struct McpPanel {
    server_list: PanelList<ServerInfo>,    // ServerList 视图
    detail_cursor: usize,                  // ServerDetail 视图（操作菜单）
    view: McpPanelView,
}

impl PanelComponent for McpPanel {
    fn handle_scroll(&mut self, lines: i16, _ctx: &mut PanelContext<'_>) -> EventResult {
        match &self.view {
            McpPanelView::ServerList => {
                self.server_list.handle_scroll(lines, 10);
            }
            McpPanelView::ServerDetail { .. } => {
                // detail 视图的滚动（如果有）
            }
        }
        EventResult::Consumed
    }

    fn handle_mouse(&mut self, mouse: MouseEvent, area: Rect, ctx: &mut PanelContext<'_>) -> EventResult {
        match &self.view {
            McpPanelView::ServerList => {
                if self.server_list.handle_mouse_click(mouse.row, mouse.column, area, 2) {
                    return self.handle_key(
                        Input::from(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
                        ctx,
                    );
                }
            }
            McpPanelView::ServerDetail { .. } => { /* detail 视图点击 */ }
        }
        EventResult::Consumed
    }
}
```

#### 不迁移的面板

**StatusPanel**：Tab 型面板，←→ 切换 Tab，不涉及列表 cursor/scroll。鼠标点击 Tab 标签作为低优先级后续添加。

### ListOverlay 浮动容器（Widget 层新增）

新增 `peri-widgets/src/list_overlay.rs`，提供浮动列表渲染能力：

- `ListOverlay`：Clear 背景 + 边框 + SelectableList
- `ListOverlayState`：追踪最后渲染区域，供 TUI 层 hit test
- `Anchor`：锚点位置（Below/Above/Centered）
- `OverlayPosition`：弹出位置策略（Auto/Below/Above）

用于 Skills 提示浮层等需要浮动列表的场景。

### ensure_cursor_visible 统一

迁移完成后，`panel_ops.rs` 中的 `ensure_cursor_visible` 函数和 `agent_panel_move_up/down`、`hooks_panel_move_up/down` 等辅助函数将被移除，逻辑统一到 `PanelList::ensure_visible()` 中。

## 实现要点

### 关键技术决策

1. **PanelList 不含渲染**：纯状态管理，面板渲染函数通过 `list.cursor()`、`list.visible_range()`、`list.scroll_offset()` 获取数据自行渲染。原因：各面板渲染差异太大（搜索栏、详情展开、多视图），强行统一渲染会引入不必要的复杂度
2. **handle_mouse_click 返回 bool**：由面板决定点击后的行为（触发 Enter、仅移动 cursor、或忽略），不耦合具体操作
3. **Widget 层与 TUI 层独立**：`ListState` 的鼠标增强和 `PanelList` 是独立的能力。当前面板不用 `SelectableList` 渲染，但未来新面板可以直接用 widget 层的完整能力

### 依赖关系

- Phase 1（Widget 层）：Task 1-6，`peri-widgets` 内完成
- Phase 2（TUI 基础设施）：Task 7-9，`PanelList<T>` + trait 增强 + event.rs 重构
- Phase 3（面板迁移）：Task 10-17，所有面板一次性迁移到 `PanelList<T>`

### 改动范围

| 文件 | 变更 |
|------|------|
| `peri-widgets/src/list.rs` | `ListState` 增加鼠标字段/方法；`SelectableList` hover_style + 三态渲染 |
| `peri-widgets/src/list_overlay.rs` | **新增**：浮动容器 |
| `peri-widgets/src/lib.rs` | 导出 list_overlay 模块 |
| `peri-tui/src/app/panel_list.rs` | **新增**：`PanelList<T>` 统一状态管理器 |
| `peri-tui/src/app/panel_component.rs` | trait 新增 `handle_mouse` 默认方法 |
| `peri-tui/src/app/panel_manager.rs` | 新增 `dispatch_mouse` |
| `peri-tui/src/event.rs` | 鼠标事件统一路由 |
| `peri-tui/src/app/agent_panel.rs` | 迁移到 `PanelList<T>` |
| `peri-tui/src/app/hooks_panel.rs` | 迁移到 `PanelList<T>` |
| `peri-tui/src/app/cron_state.rs` | 迁移到 `PanelList<T>` |
| `peri-tui/src/app/memory_panel.rs` | 迁移到 `PanelList<T>` |
| `peri-tui/src/app/model_panel.rs` | 迁移到 `PanelList<T>` |
| `peri-tui/src/app/config_panel.rs` | 列表部分迁移到 `PanelList<T>` |
| `peri-tui/src/app/mcp_panel.rs` | 多视图各自迁移到 `PanelList<T>` |
| `peri-tui/src/app/plugin_panel.rs` | 多视图各自迁移到 `PanelList<T>` |
| `peri-tui/src/app/login_panel.rs` | 列表部分迁移到 `PanelList<T>` |
| `peri-tui/src/app/panel_ops.rs` | 移除 `ensure_cursor_visible` 及相关辅助函数 |

### 无新 crate 依赖

所有改动使用现有代码结构。`PanelList<T>` 是纯 Rust 泛型结构体，不引入新依赖。

## 约束一致性

本方案符合 `spec/global/constraints.md` 和 `spec/global/architecture.md` 中的约束：

- **Widget 独立 crate**：`ListState` 增强和 `ListOverlay` 在 `peri-widgets` 中，零内部依赖，仅依赖 ratatui
- **Workspace 依赖方向**：`PanelList<T>` 在 `peri-tui` 中，不违反分层约束
- **事件驱动 TUI**：鼠标事件通过 `dispatch_mouse` → `handle_mouse` 路由，符合现有事件分发模式
- **编码规范**：`PanelList<T>` 使用 `usize`/`u16` 类型，字符串截断用字符级操作

## 验收标准

- [ ] `PanelList<T>` 提供完整的列表状态管理（cursor、scroll、navigation、mouse）
- [ ] 所有 9 个面板（除 StatusPanel）迁移到 `PanelList<T>`，消除分散的 cursor/scroll_offset 字段
- [ ] 所有面板统一边界 clamp 导航行为
- [ ] 所有面板统一 `ensure_visible` 滚动跟随
- [ ] 鼠标点击选择：标准面板点击 item 移动 cursor 并触发对应操作
- [ ] 鼠标滚轮滚动：面板区域内滚轮滚动列表，不穿透到消息区
- [ ] `event.rs` 中 McpPanel/PluginPanel 的 ad-hoc 滚轮代码被移除，统一走 `dispatch_scroll`
- [ ] `panel_ops.rs` 中的 `ensure_cursor_visible` 及相关辅助函数被移除
- [ ] `cargo test -p peri-widgets` 全量通过
- [ ] `cargo build -p peri-tui` 编译通过
- [ ] 手动测试所有面板的键盘导航、鼠标点击、滚轮滚动行为正确
