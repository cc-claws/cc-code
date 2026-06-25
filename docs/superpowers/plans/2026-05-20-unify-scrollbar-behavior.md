# 统一滚动条行为 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将所有面板（panels）的滚动条行为统一到消息区（message area）已有风格：▲/▼ 按钮 + 鼠标点击跳转 + 鼠标拖拽定位。

**Architecture:** 增强 `peri-widgets` 的 `ScrollableArea` widget，使其在渲染滚动条时可选绘制 ▲/▼ 按钮，并返回 `ScrollbarMetrics`（包含按钮和滚动条列的几何信息）。事件循环利用这些几何信息在面板区域检测点击/拖拽，通过 `PanelComponent` trait 新增的 `set_scroll_offset` 方法同步滚动位置。

**Tech Stack:** Rust + ratatui + peri-widgets + peri-tui

---

### Task 1: `ScrollableArea` 返回 `ScrollbarMetrics` + 渲染 ▲/▼

**Files:**
- Modify: `peri-widgets/src/scrollable.rs:75-148`

- [ ] **Step 1: 定义 `ScrollbarMetrics` 结构体，修改 `render()` 签名**

在 `ScrollState` 和 `ScrollableArea` 之间插入：

```rust
/// 滚动条几何信息，供事件循环用于鼠标交互检测
#[derive(Debug, Clone, Copy)]
pub struct ScrollbarMetrics {
    /// 滚动条所在列区域（宽 1，跨整个 panel 高度）
    pub bar_area: ratatui::layout::Rect,
    /// 最大滚动偏移量
    pub max_offset: u16,
    /// ▲ 按钮区域（offset > 0 时存在）
    pub up_btn_area: Option<ratatui::layout::Rect>,
    /// ▼ 按钮区域（offset < max_offset 时存在）
    pub down_btn_area: Option<ratatui::layout::Rect>,
}
```

将 `render()` 签名从 `pub fn render(self, f: &mut Frame, area: Rect, state: &mut ScrollState)` 改为：

```rust
pub fn render(self, f: &mut Frame, area: Rect, state: &mut ScrollState) -> Option<ScrollbarMetrics>
```

- [ ] **Step 2: 修改 `render()` 内部逻辑，返回 `ScrollbarMetrics`**

在 `render()` 末尾，将当前的 `if needs_scrollbar { ... }` 块改为同时渲染 ▲/▼ 按钮并构造 `ScrollbarMetrics`：

```rust
if needs_scrollbar {
    let bar_area = Rect {
        x: area.right().saturating_sub(1),
        y: area.y,
        width: 1,
        height: area.height,
    };

    // 渲染滚动条 track + thumb
    let viewport = if let Some(max_thumb) = self.max_thumb_length {
        visible_height.min(max_thumb)
    } else {
        0
    };
    let mut scrollbar_state = ScrollbarState::new(max_scroll as usize)
        .viewport_content_length(viewport as usize)
        .position(state.offset as usize);
    let scrollbar = unified_vertical_scrollbar().style(self.scrollbar_style);
    f.render_stateful_widget(scrollbar, area, &mut scrollbar_state);

    // ▲ 按钮（offset > 0 时）
    let up_btn_area = if state.offset > 0 {
        let area = Rect {
            x: bar_area.x,
            y: bar_area.y,
            width: 1,
            height: 1,
        };
        let arrow = Paragraph::new(Text::from(Span::styled(
            "▲",
            self.scrollbar_style.add_modifier(Modifier::BOLD),
        )));
        f.render_widget(arrow, area);
        Some(area)
    } else {
        None
    };

    // ▼ 按钮（offset < max_scroll 时）
    let down_btn_area = if state.offset < max_scroll {
        let area = Rect {
            x: bar_area.x,
            y: bar_area.bottom().saturating_sub(1),
            width: 1,
            height: 1,
        };
        let arrow = Paragraph::new(Text::from(Span::styled(
            "▼",
            self.scrollbar_style.add_modifier(Modifier::BOLD),
        )));
        f.render_widget(arrow, area);
        Some(area)
    } else {
        None
    };

    Some(ScrollbarMetrics {
        bar_area,
        max_offset: max_scroll,
        up_btn_area,
        down_btn_area,
    })
} else {
    None
}
```

需要新增 use：`text::Span`、`widgets::Paragraph`、`style::Modifier`。

- [ ] **Step 3: 编译检查 `peri-widgets`**

Run: `cargo build -p peri-widgets 2>&1 | head -30`

预期：编译通过，`render()` 返回值变化会导致调用方报错（下一 Task 修复）。

---

### Task 2: 导出 `ScrollbarMetrics`

**Files:**
- Modify: `peri-widgets/src/lib.rs`

- [ ] **Step 1: 导出 `ScrollbarMetrics`**

找到 lib.rs 中 `ScrollableArea`、`ScrollState` 的导出行，在其附近添加 `ScrollbarMetrics`：

```rust
pub use scrollable::{ScrollState, ScrollableArea, ScrollbarMetrics, unified_vertical_scrollbar};
```

- [ ] **Step 2: 编译检查**

Run: `cargo build -p peri-widgets 2>&1 | head -10`

预期：PASS。

---

### Task 3: `UiState` 新增面板滚动条状态字段

**Files:**
- Modify: `peri-tui/src/app/ui_state.rs:1-56`

- [ ] **Step 1: 添加 import**

```rust
use peri_widgets::ScrollbarMetrics;
```

- [ ] **Step 2: 新增字段**

在 `UiState` 结构体的 `scrollbar_max_offset` 后添加：

```rust
/// 面板滚动条几何信息（鼠标交互用）
pub panel_scrollbar_metrics: Option<ScrollbarMetrics>,
/// 用户是否正在拖拽面板滚动条
pub panel_scrollbar_dragging: bool,
```

- [ ] **Step 3: 在 `UiState::new()` 中初始化**

```rust
panel_scrollbar_metrics: None,
panel_scrollbar_dragging: false,
```

- [ ] **Step 4: 编译检查**

Run: `cargo build -p peri-tui 2>&1 | head -10`

---

### Task 4: `PanelComponent` trait 新增 `set_scroll_offset` 方法

**Files:**
- Modify: `peri-tui/src/app/panel_component.rs:14-50`

- [ ] **Step 1: 在 trait 中添加默认空实现方法**

在 `handle_scroll` 方法后添加：

```rust
/// 直接设置滚动偏移量（用于滚动条拖拽）
fn set_scroll_offset(&mut self, _offset: u16) {}
```

---

### Task 5: `PanelManager` 新增 `dispatch_set_scroll_offset` 方法

**Files:**
- Modify: `peri-tui/src/app/panel_manager.rs:383-401`

- [ ] **Step 1: 添加 dispatch 方法**

在 `dispatch_scroll` 方法后添加：

```rust
/// 分发绝对滚动偏移量到当前激活面板
pub fn dispatch_set_scroll_offset(&mut self, offset: u16, ctx: &mut PanelContext<'_>) {
    use super::panel_component::PanelComponent;
    let Some(state) = self.active.as_mut() else { return };
    match state {
        PanelState::Model(p) => p.set_scroll_offset(offset),
        PanelState::Agent(p) => p.set_scroll_offset(offset),
        PanelState::Hooks(p) => p.set_scroll_offset(offset),
        PanelState::Status(p) => p.set_scroll_offset(offset),
        PanelState::Memory(p) => p.set_scroll_offset(offset),
        PanelState::Login(p) => p.set_scroll_offset(offset),
        PanelState::Config(p) => p.set_scroll_offset(offset),
        PanelState::ThreadBrowser(p) => p.set_scroll_offset(offset),
        PanelState::Mcp(p) => p.set_scroll_offset(offset),
        PanelState::Cron(p) => p.set_scroll_offset(offset),
        PanelState::Plugin(p) => p.set_scroll_offset(offset),
    }
}
```

---

### Task 6: 所有面板实现 `set_scroll_offset`

**Files:**
- Modify: `peri-tui/src/app/memory_panel.rs`
- Modify: `peri-tui/src/app/hooks_panel.rs`
- Modify: `peri-tui/src/app/agent_panel.rs`
- Modify: `peri-tui/src/app/cron_state.rs`
- Modify: `peri-tui/src/thread/browser.rs`
- Modify: `peri-tui/src/app/mcp_panel/mod.rs`
- Modify: `peri-tui/src/app/plugin_panel/mod.rs`
- Modify: `peri-tui/src/app/login_panel/component.rs`
- Modify: `peri-tui/src/app/config_panel.rs`

静态分析确认：model_panel / status_panel 无 `handle_scroll`，不需要实现（默认 no-op）。

- [ ] **Step 1: memory_panel.rs — 在 `handle_scroll` 方法后添加**

```rust
fn set_scroll_offset(&mut self, offset: u16) {
    self.list.set_scroll_offset(offset);
}
```

- [ ] **Step 2: hooks_panel.rs — 同上模式**

```rust
fn set_scroll_offset(&mut self, offset: u16) {
    self.list.set_scroll_offset(offset);
}
```

- [ ] **Step 3: agent_panel.rs — 同上模式**

```rust
fn set_scroll_offset(&mut self, offset: u16) {
    self.list.set_scroll_offset(offset);
}
```

- [ ] **Step 4: cron_state.rs — 同上模式**

```rust
fn set_scroll_offset(&mut self, offset: u16) {
    self.list.set_scroll_offset(offset);
}
```

- [ ] **Step 5: thread/browser.rs — `ThreadBrowser` 有自有 `scroll_offset` 字段**

在 `handle_scroll` 方法后添加：

```rust
fn set_scroll_offset(&mut self, offset: u16) {
    self.scroll_offset = offset;
}
```

- [ ] **Step 6: mcp_panel/mod.rs — `McpPanel` 根据视图分发**

在 `scroll_offset()` 方法后添加：

```rust
pub fn set_scroll_offset(&mut self, offset: u16) {
    match &mut self.view {
        McpPanelView::ServerList => self.server_list.set_scroll_offset(offset),
        McpPanelView::ServerDetail { .. } => self.detail_scroll_offset = offset,
    }
}
```

同时在 `component.rs` 的 `impl PanelComponent for McpPanel` 中添加 trait override：

```rust
fn set_scroll_offset(&mut self, offset: u16) {
    self.set_scroll_offset(offset);
}
```

- [ ] **Step 7: plugin_panel/mod.rs — 已有 `set_scroll_offset`，但需按 view 分发更精确**

检查现有实现，替换为：

```rust
pub fn set_scroll_offset(&mut self, offset: u16) {
    match self.view {
        PluginPanelView::Installed => self.installed_list.set_scroll_offset(offset),
        PluginPanelView::Discover => self.discover_list.set_scroll_offset(offset),
        PluginPanelView::Marketplaces => self.marketplace_list.set_scroll_offset(offset),
        PluginPanelView::Errors => self.installed_list.set_scroll_offset(offset),
        PluginPanelView::DiscoverDetail { .. }
        | PluginPanelView::AddMarketplace { .. }
        | PluginPanelView::InstalledDetail { .. }
        | PluginPanelView::MarketplaceDetail { .. } => {
            self.set_scroll_offset(offset);
        }
    }
}
```

Wait, plugin_panel 已有一个 `set_scroll_offset` — 让我检查它的签名和实现。如果是 `self.scroll_offset = offset`，那就保持了。

- [ ] **Step 8: login_panel/component.rs — Browse 模式下设置列表偏移**

```rust
fn set_scroll_offset(&mut self, offset: u16) {
    if matches!(self.mode, LoginPanelMode::Browse) {
        self.browse_list.set_scroll_offset(offset);
    }
}
```

- [ ] **Step 9: config_panel.rs — Browse 模式下设置列表偏移**

```rust
fn set_scroll_offset(&mut self, offset: u16) {
    if matches!(self.mode, ConfigPanelMode::Browse) {
        self.browse_list.set_scroll_offset(offset);
    }
}
```

- [ ] **Step 10: 编译检查**

Run: `cargo build -p peri-tui 2>&1 | grep error | head -20`

---

### Task 7: 事件循环添加面板滚动条交互

**Files:**
- Modify: `peri-tui/src/event/mod.rs:208-480`

- [ ] **Step 1: `MouseEventKind::Down` — 面板区域增加 ▲/▼/滚动条点击检测**

在现有 panel_area 鼠标 dispatch 之后（`if click_consumed { return ... }` 之后，约 line 240），插入滚动条检测：

```rust
// Panel scrollbar: ▲/▼ buttons and bar click/drag
if let Some(metrics) = app.session_mgr.sessions[app.session_mgr.active]
    .ui
    .panel_scrollbar_metrics
{
    // ▼ button click (bottom)
    if let Some(btn) = metrics.down_btn_area {
        if mouse.column >= btn.x
            && mouse.column < btn.x + btn.width
            && mouse.row >= btn.y
            && mouse.row < btn.y + btn.height
        {
            let session = &mut app.session_mgr.sessions[app.session_mgr.active];
            let new_offset = metrics.max_offset;
            with_session_panels!(app, |sp, ctx| {
                sp.dispatch_set_scroll_offset(new_offset, &mut ctx);
            });
            session.ui.panel_scroll_offset = new_offset;
            return Ok(Some(Action::Redraw));
        }
    }
    // ▲ button click (top)
    if let Some(btn) = metrics.up_btn_area {
        if mouse.column >= btn.x
            && mouse.column < btn.x + btn.width
            && mouse.row >= btn.y
            && mouse.row < btn.y + btn.height
        {
            let session = &mut app.session_mgr.sessions[app.session_mgr.active];
            with_session_panels!(app, |sp, ctx| {
                sp.dispatch_set_scroll_offset(0, &mut ctx);
            });
            session.ui.panel_scroll_offset = 0;
            return Ok(Some(Action::Redraw));
        }
    }
    // Scrollbar bar click (proportional jump + start drag)
    if mouse.column == metrics.bar_area.x
        && mouse.row >= metrics.bar_area.y
        && mouse.row < metrics.bar_area.bottom()
        && metrics.max_offset > 0
    {
        let bar_inner_height = metrics.bar_area.height.saturating_sub(2);
        if bar_inner_height > 0 {
            let rel_y = (mouse.row.saturating_sub(metrics.bar_area.y + 1)).min(bar_inner_height);
            let new_offset =
                ((rel_y as f64 / bar_inner_height as f64) * metrics.max_offset as f64) as u16;
            let session = &mut app.session_mgr.sessions[app.session_mgr.active];
            with_session_panels!(app, |sp, ctx| {
                sp.dispatch_set_scroll_offset(new_offset, &mut ctx);
            });
            session.ui.panel_scroll_offset = new_offset.min(metrics.max_offset);
            session.ui.panel_scrollbar_dragging = true;
        }
        return Ok(Some(Action::Redraw));
    }
}
```

- [ ] **Step 2: `MouseEventKind::Drag` — 面板滚动条拖拽更新**

在消息区滚动条拖拽块之后（约 line 409），插入面板滚动条拖拽：

```rust
// Panel scrollbar drag: update panel scroll offset from mouse Y
if app.session_mgr.sessions[app.session_mgr.active]
    .ui
    .panel_scrollbar_dragging
{
    if let (Some(area), Some(metrics)) = (
        app.session_mgr.sessions[app.session_mgr.active].ui.panel_area,
        app.session_mgr.sessions[app.session_mgr.active]
            .ui
            .panel_scrollbar_metrics,
    ) {
        let bar_inner_height = metrics.bar_area.height.saturating_sub(2);
        if bar_inner_height > 0 {
            let rel_y = (mouse.row.saturating_sub(metrics.bar_area.y + 1)).min(bar_inner_height);
            let new_offset =
                ((rel_y as f64 / bar_inner_height as f64) * metrics.max_offset as f64) as u16;
            let session = &mut app.session_mgr.sessions[app.session_mgr.active];
            with_session_panels!(app, |sp, ctx| {
                sp.dispatch_set_scroll_offset(new_offset, &mut ctx);
            });
            session.ui.panel_scroll_offset = new_offset.min(metrics.max_offset);
        }
    }
}
```

- [ ] **Step 3: `MouseEventKind::Up` — 结束面板滚动条拖拽**

在消息区滚动条拖拽结束之后（约 line 481），插入：

```rust
// End panel scrollbar drag
app.session_mgr.sessions[app.session_mgr.active]
    .ui
    .panel_scrollbar_dragging = false;
```

- [ ] **Step 4: 编译检查**

Run: `cargo build -p peri-tui 2>&1 | grep error | head -20`

---

### Task 8: 所有面板渲染接收 `ScrollbarMetrics` 并写入 `UiState`

**Files:**
- Modify: `peri-tui/src/ui/main_ui/panels/memory.rs:88-92`
- Modify: `peri-tui/src/ui/main_ui/panels/hooks.rs:147-153`
- Modify: `peri-tui/src/ui/main_ui/panels/agent.rs:170-176`
- Modify: `peri-tui/src/ui/main_ui/panels/cron.rs:140-146`
- Modify: `peri-tui/src/ui/main_ui/panels/thread_browser.rs:315-321`
- Modify: `peri-tui/src/ui/main_ui/panels/mcp.rs:130-136 + 422-428` (2 处)
- Modify: `peri-tui/src/ui/main_ui/popups/ask_user.rs:169-175`
- Modify: `peri-tui/src/ui/main_ui/panels/plugin/plugin_render/detail.rs:174-180`
- Modify: `peri-tui/src/ui/main_ui/panels/plugin/plugin_render/discover_list.rs:228-234`
- Modify: `peri-tui/src/ui/main_ui/panels/plugin/plugin_render/discover_detail.rs:136-142`
- Modify: `peri-tui/src/ui/main_ui/panels/plugin/plugin_render/list.rs:407-413`

- [ ] **Step 1: 批量修改面板渲染处的 `ScrollableArea` 调用**

每处当前代码模式为：

```rust
let mut scroll_state = ScrollState::with_offset(panel.scroll_offset());
ScrollableArea::new(Text::from(lines))
    .scrollbar_style(Style::default().fg(theme::MUTED))
    .render(f, inner, &mut scroll_state);
```

改为：

```rust
let mut scroll_state = ScrollState::with_offset(panel.scroll_offset());
let metrics = ScrollableArea::new(Text::from(lines))
    .scrollbar_style(Style::default().fg(theme::MUTED))
    .render(f, inner, &mut scroll_state);
app.session_mgr.sessions[app.session_mgr.active]
    .ui
    .panel_scrollbar_metrics = metrics;
```

对于每个面板，找到 `ScrollableArea::new(` 调用并修改。

- [ ] **Step 2: 编译检查**

Run: `cargo build -p peri-tui 2>&1 | head -20`

预期：PASS（所有调用方已更新）。

---

### Task 9: 全量测试 + pre-commit

**Files:** 无新文件

- [ ] **Step 1: 运行全量测试**

```bash
cargo test 2>&1 | tail -20
```

预期：全部通过。

- [ ] **Step 2: 运行 pre-commit hooks**

```bash
lefthook run pre-commit 2>&1
```

预期：fmt、clippy、check 全部通过。

- [ ] **Step 3: Commit**

```bash
git add -A
git commit -m "feat: unify panel scrollbar behavior with ▲/▼ buttons and mouse drag support

- ScrollableArea::render() now returns Option<ScrollbarMetrics> with ▲/▼ button areas
- PanelComponent trait gains set_scroll_offset() for absolute scroll positioning
- Event loop handles panel scrollbar click/drag using ScrollbarMetrics
- All panel render call sites updated to capture metrics

Co-Authored-By: deepseek-v4-pro <deepseek-ai@claude-code-best.win>"
```

