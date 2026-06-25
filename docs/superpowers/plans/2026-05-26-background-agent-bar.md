# Background Agent Bar 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在 TUI 状态栏下方新增后台 SubAgent 管理栏，支持查看运行中的后台 agent、切换聚焦视图、消息过滤。

**Architecture:** 方案 A — 独立 Bar 组件 + Pipeline 过滤标记。数据模型从 `usize` 计数器扩展为 `Vec<RunningBgAgent>` 列表，渲染层新增 `bg_agent_bar.rs` 模块，消息过滤在 `MessagePipeline` 中按 `focused_instance_id` 实现。

**Tech Stack:** Rust, ratatui, tokio, tui-textarea

**设计文档:** `docs/superpowers/specs/2026-05-26-background-agent-bar-design.md`

---

## 文件结构

| 操作 | 文件 | 职责 |
|------|------|------|
| Modify | `peri-tui/src/app/chat_session.rs` | `RunningBgAgent` struct + `ChatSession` 字段变更 |
| Modify | `peri-tui/src/app/agent_ops/subagent.rs` | `handle_subagent_start` → push `RunningBgAgent` |
| Modify | `peri-tui/src/app/agent_events_bg.rs` | `handle_background_task_completed` → 移除 + 聚焦检查 |
| Modify | `peri-tui/src/app/agent_ops/lifecycle.rs` | `background_task_count` → `background_agents.len()` (2 处) |
| Modify | `peri-tui/src/app/agent_ops/polling.rs` | 同上 (3 处) |
| Modify | `peri-tui/src/app/panel_ops.rs` | session 拆分初始化 |
| Create | `peri-tui/src/ui/main_ui/bg_agent_bar.rs` | Bar 渲染 + 调色板 |
| Modify | `peri-tui/src/ui/main_ui/mod.rs` | 布局约束 + bar 渲染调用 |
| Modify | `peri-tui/src/ui/main_ui/status_bar.rs` | 后台计数指示器 → 改用 `background_agents.len()` |
| Modify | `peri-tui/src/event/keyboard.rs` | Ctrl+B 快捷键 + bar 键盘处理 + 只读模式 |
| Modify | `peri-tui/src/app/ui_state.rs` | `bg_bar_cursor` + `bg_bar_focused` 字段 |
| Modify | `peri-tui/src/app/message_pipeline/mod.rs` | `should_show_vm` 过滤器 |
| Modify | `peri-tui/src/app/message_pipeline/transform.rs` | `messages_to_view_models` 调用过滤器 |
| Modify | `peri-tui/src/app/agent_compact.rs` | compact 前退出聚焦 |
| Modify | `peri-tui/locales/zh-CN/main.ftl` | 新增 bar 相关 i18n 字符串 |
| Modify | `peri-tui/locales/en/main.ftl` | 同上 |
| Modify | `peri-tui/src/ui/headless_test.rs` | 更新现有 `background_task_count` 测试 |

---

### Task 1: 数据模型 — RunningBgAgent + ChatSession 字段

**Files:**
- Modify: `peri-tui/src/app/chat_session.rs`
- Modify: `peri-tui/src/app/ui_state.rs`

- [ ] **Step 1: 添加 RunningBgAgent 结构体和 ChatSession 字段**

在 `chat_session.rs` 中，在 `use` 块后、`ChatSession` 定义前，添加 `RunningBgAgent` struct。修改 `ChatSession` 字段：删除 `background_task_count`，新增 `background_agents` 和 `focused_instance_id`。

```rust
// chat_session.rs — 在 use 块之后，ChatSession 定义之前添加

use std::time::Instant;

/// 正在运行的后台 SubAgent
#[derive(Clone, Debug)]
pub(crate) struct RunningBgAgent {
    pub agent_name: String,
    pub instance_id: String,
    pub started_at: Instant,
}
```

修改 `ChatSession` struct：

```rust
// chat_session.rs — ChatSession struct 字段变更

// 删除: pub background_task_count: usize,
// 新增:
pub background_agents: Vec<RunningBgAgent>,
pub focused_instance_id: Option<String>,
```

修改 `ChatSession::new()`：

```rust
// chat_session.rs — ChatSession::new() 中替换
// 删除: background_task_count: 0,
// 新增:
background_agents: Vec::new(),
focused_instance_id: None,
```

在 `ui_state.rs` 的 `UiState` struct 中新增 bar 焦点状态：

```rust
// ui_state.rs — UiState struct 新增字段
pub bg_bar_cursor: Option<usize>,   // bar 中选中行（None = 无焦点）
```

在 `UiState::new()` 中初始化：

```rust
// ui_state.rs — UiState::new() 新增
bg_bar_cursor: None,
```

- [ ] **Step 2: 编译验证**

Run: `cargo build -p peri-tui 2>&1 | head -50`

预期：多处编译错误——所有引用 `background_task_count` 的地方会报错。这是预期的，Task 2 会修复。

---

### Task 2: 全局迁移 — background_task_count → background_agents.len()

**Files:**
- Modify: `peri-tui/src/app/agent_ops/lifecycle.rs` (2 处)
- Modify: `peri-tui/src/app/agent_ops/polling.rs` (3 处)
- Modify: `peri-tui/src/app/panel_ops.rs` (1 处)
- Modify: `peri-tui/src/ui/main_ui/status_bar.rs` (1 处)

- [ ] **Step 1: 替换 lifecycle.rs 中的引用**

在 `agent_ops/lifecycle.rs` 中，有两处 `background_task_count > 0`：

```rust
// lifecycle.rs:100 — 替换
// 旧: if self.session_mgr.sessions[self.session_mgr.active].background_task_count > 0 {
// 新:
if !self.session_mgr.sessions[self.session_mgr.active].background_agents.is_empty() {
```

```rust
// lifecycle.rs:329 — 替换
// 旧: if self.session_mgr.sessions[self.session_mgr.active].background_task_count > 0 {
// 新:
if !self.session_mgr.sessions[self.session_mgr.active].background_agents.is_empty() {
```

tracing 日志中的 count 也需更新：

```rust
// lifecycle.rs:104-106 — 替换 tracing 日志
// 旧: count = self.session_mgr.sessions[self.session_mgr.active].background_task_count,
// 新:
count = self.session_mgr.sessions[self.session_mgr.active].background_agents.len(),
```

- [ ] **Step 2: 替换 polling.rs 中的引用**

```rust
// polling.rs:135-136 — 替换条件
// 旧: || self.session_mgr.sessions[self.session_mgr.active].background_task_count > 0
// 新:
|| !self.session_mgr.sessions[self.session_mgr.active].background_agents.is_empty()
```

```rust
// polling.rs:142-143 — 替换 tracing 日志
// 旧: bg_count = self.session_mgr.sessions[self.session_mgr.active].background_task_count,
// 新:
bg_count = self.session_mgr.sessions[self.session_mgr.active].background_agents.len(),
```

```rust
// polling.rs:149-150 — 替换清零
// 旧: self.session_mgr.sessions[self.session_mgr.active].background_task_count = 0;
// 新:
self.session_mgr.sessions[self.session_mgr.active].background_agents.clear();
```

- [ ] **Step 3: 替换 panel_ops.rs 和 status_bar.rs**

```rust
// panel_ops.rs:101 — 替换
// 旧: background_task_count: 0,
// 新:
background_agents: Vec::new(),
focused_instance_id: None,
```

```rust
// status_bar.rs:170 — 替换条件
// 旧: if app.session_mgr.sessions[app.session_mgr.active].background_task_count > 0 {
// 新:
if !app.session_mgr.sessions[app.session_mgr.active].background_agents.is_empty() {
```

```rust
// status_bar.rs:179 — 替换 count
// 旧: (app.session_mgr.sessions[app.session_mgr.active].background_task_count as i64)
// 新:
(app.session_mgr.sessions[app.session_mgr.active].background_agents.len() as i64)
```

- [ ] **Step 4: 编译验证**

Run: `cargo build -p peri-tui 2>&1 | head -50`

预期：剩余编译错误在 `agent_events_bg.rs` 和 `subagent.rs`（这两个文件在 Task 3 修复）。

---

### Task 3: 事件处理器 — subagent_start / background_task_completed

**Files:**
- Modify: `peri-tui/src/app/agent_ops/subagent.rs`
- Modify: `peri-tui/src/app/agent_events_bg.rs`

- [ ] **Step 1: 更新 handle_subagent_start**

在 `subagent.rs` 中，替换 `handle_subagent_start` 中的计数器递增为 push：

```rust
// subagent.rs:74-76 — 替换
// 旧:
//     if is_background {
//         self.session_mgr.sessions[self.session_mgr.active].background_task_count += 1;
//     }
// 新:
    if is_background {
        use super::super::chat_session::RunningBgAgent;
        self.session_mgr.sessions[self.session_mgr.active]
            .background_agents
            .push(RunningBgAgent {
                agent_name: agent_id.clone(),
                instance_id: instance_id.clone(),
                started_at: std::time::Instant::now(),
            });
    }
```

- [ ] **Step 2: 更新 handle_background_task_completed**

在 `agent_events_bg.rs` 中，替换计数器递减为列表移除 + 聚焦检查。替换文件头部（约第 61-65 行）：

```rust
// agent_events_bg.rs:61-65 — 替换计数器递减
// 旧:
//     self.session_mgr.sessions[self.session_mgr.active].background_task_count =
//         self.session_mgr.sessions[self.session_mgr.active]
//             .background_task_count
//             .saturating_sub(1);
// 新:
    let was_focused = self.session_mgr.sessions[self.session_mgr.active]
        .focused_instance_id
        .as_deref()
        .map(|id| {
            // 检查被移除的 agent 是否是当前聚焦的
            self.session_mgr.sessions[self.session_mgr.active]
                .background_agents
                .iter()
                .any(|a| a.agent_name == agent_name && a.instance_id == id)
        })
        .unwrap_or(false);

    // 按 agent_name 移除第一个匹配项
    if let Some(pos) = self.session_mgr.sessions[self.session_mgr.active]
        .background_agents
        .iter()
        .position(|a| a.agent_name == agent_name)
    {
        self.session_mgr.sessions[self.session_mgr.active]
            .background_agents
            .remove(pos);
    }

    // 聚焦检查：如果被移除的是当前聚焦的 agent，退出聚焦
    if was_focused {
        self.session_mgr.sessions[self.session_mgr.active].focused_instance_id = None;
        self.session_mgr.sessions[self.session_mgr.active].ui.bg_bar_cursor = None;
        self.request_rebuild();
    }
```

更新 tracing 日志中的 count 引用（约第 71-72 行）：

```rust
// agent_events_bg.rs:71-72 — 替换 tracing 日志
// 旧:
//     bg_count_before = self.session_mgr.sessions[self.session_mgr.active].background_task_count + 1,
//     bg_count_after = self.session_mgr.sessions[self.session_mgr.active].background_task_count,
// 新:
    bg_count_before = self.session_mgr.sessions[self.session_mgr.active].background_agents.len() + 1,
    bg_count_after = self.session_mgr.sessions[self.session_mgr.active].background_agents.len(),
```

更新第 226 行附近的日志：

```rust
// agent_events_bg.rs:226 — 替换
// 旧: background_task_count = self.session_mgr.sessions[self.session_mgr.active].background_task_count,
// 新:
background_task_count = self.session_mgr.sessions[self.session_mgr.active].background_agents.len(),
```

更新第 236 行和 262 行的完成检查：

```rust
// agent_events_bg.rs:236 — 替换
// 旧: && self.session_mgr.sessions[self.session_mgr.active].background_task_count == 0
// 新:
&& self.session_mgr.sessions[self.session_mgr.active].background_agents.is_empty()
```

```rust
// agent_events_bg.rs:262 — 替换
// 旧: && self.session_mgr.sessions[self.session_mgr.active].background_task_count == 0
// 新:
&& self.session_mgr.sessions[self.session_mgr.active].background_agents.is_empty()
```

- [ ] **Step 3: 编译验证**

Run: `cargo build -p peri-tui 2>&1 | head -30`

预期：编译通过（或仅剩 headless_test.rs 中的测试引用需要更新）。

- [ ] **Step 4: 更新 headless_test.rs 中的测试引用**

在 `ui/headless_test.rs` 中，所有 `background_task_count` 引用需要更新：

```rust
// 全局替换模式:
// 旧: .background_task_count = 1  (或 2)
// 新: .background_agents = vec![super::super::chat_session::RunningBgAgent {
//          agent_name: "test-agent".to_string(),
//          instance_id: "test-inst".to_string(),
//          started_at: std::time::Instant::now(),
//      }]
```

对于断言 `background_task_count, 0`：

```rust
// 旧: assert_eq!(...background_task_count, 0, ...)
// 新: assert!(...background_agents.is_empty(), ...)
```

对于断言 `background_task_count, 1` (或 2)：

```rust
// 旧: assert_eq!(...background_task_count, 1, ...)
// 新: assert_eq!(...background_agents.len(), 1, ...)
```

- [ ] **Step 5: 编译 + 测试验证**

Run: `cargo build -p peri-tui && cargo test -p peri-tui --lib 2>&1 | tail -30`

预期：编译通过，所有现有测试通过。

- [ ] **Step 6: Commit**

```bash
git add peri-tui/src/app/chat_session.rs peri-tui/src/app/ui_state.rs \
        peri-tui/src/app/agent_ops/subagent.rs peri-tui/src/app/agent_events_bg.rs \
        peri-tui/src/app/agent_ops/lifecycle.rs peri-tui/src/app/agent_ops/polling.rs \
        peri-tui/src/app/panel_ops.rs peri-tui/src/ui/main_ui/status_bar.rs \
        peri-tui/src/ui/headless_test.rs
git commit -m "feat: replace background_task_count with Vec<RunningBgAgent> for agent tracking

- Add RunningBgAgent struct (agent_name, instance_id, started_at)
- Replace usize counter with Vec in ChatSession
- Add focused_instance_id field for focus mode
- Add bg_bar_cursor to UiState
- Update all references across lifecycle, polling, status_bar, headless_test
- handle_subagent_start now pushes RunningBgAgent
- handle_background_task_completed now removes by agent_name + checks focus"
```

---

### Task 4: Bar 渲染模块

**Files:**
- Create: `peri-tui/src/ui/main_ui/bg_agent_bar.rs`
- Modify: `peri-tui/src/ui/main_ui/mod.rs` (添加 `mod bg_agent_bar;`)

- [ ] **Step 1: 创建 bg_agent_bar.rs**

```rust
// peri-tui/src/ui/main_ui/bg_agent_bar.rs
//! 后台 Agent 管理栏——显示运行中的后台 SubAgent 列表

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState};
use ratatui::Frame;

use crate::app::App;

/// 固定调色板（最多 8 种颜色循环）
const AGENT_COLORS: &[Color] = &[
    Color::Cyan,
    Color::Magenta,
    Color::Yellow,
    Color::Green,
    Color::Blue,
    Color::Red,
    Color::Rgb(255, 165, 0), // orange
    Color::Rgb(148, 103, 189), // purple
];

/// 获取 agent 在列表中对应的颜色
pub(crate) fn agent_color(index: usize) -> Color {
    AGENT_COLORS[index % AGENT_COLORS.len()]
}

/// 格式化耗时（秒级）
fn format_elapsed(start: std::time::Instant) -> String {
    let secs = start.elapsed().as_secs();
    if secs < 60 {
        format!("{}s", secs)
    } else {
        format!("{}m{:02}s", secs / 60, secs % 60)
    }
}

/// 计算 bar 需要的高度（0 = 隐藏）
pub(crate) fn bg_bar_height(app: &App) -> u16 {
    let count = app.session_mgr.sessions[app.session_mgr.active]
        .background_agents
        .len();
    if count == 0 {
        0
    } else {
        // 1 行 main + N 行 agent，最多 5 行（1 main + 4 agent）
        (1 + count.min(4)).min(5) as u16
    }
}

pub(crate) fn render_bg_agent_bar(f: &mut Frame, app: &mut App, area: Rect) {
    if area.height == 0 {
        return;
    }

    let session = &app.session_mgr.sessions[app.session_mgr.active];
    let agents = &session.background_agents;
    let focused_id = &session.focused_instance_id;
    let cursor = session.ui.bg_bar_cursor;

    let mut items: Vec<ListItem> = Vec::new();

    // 第 1 行：main
    let main_style = if focused_id.is_none() {
        Style::default().add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    let main_selected = cursor == Some(0);
    let main_display_style = if main_selected {
        Style::default().add_modifier(Modifier::REVERSED)
    } else {
        main_style
    };
    items.push(ListItem::new(Line::from(vec![
        Span::styled("● ", Style::default().fg(Color::Green)),
        Span::styled("main", main_display_style),
    ])));

    // 后续行：每个后台 agent
    let visible_count = agents.len().min(4);
    for (i, agent) in agents.iter().take(visible_count).enumerate() {
        let color = agent_color(i);
        let is_focused = focused_id.as_deref() == Some(&agent.instance_id);
        let is_selected = cursor == Some(i + 1);

        let elapsed = format_elapsed(agent.started_at);
        let name_preview: String = agent.agent_name.chars().take(20).collect();
        let style = if is_selected {
            Style::default().add_modifier(Modifier::REVERSED)
        } else if is_focused {
            Style::default().fg(color).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(color)
        };

        items.push(ListItem::new(Line::from(vec![
            Span::styled("● ", Style::default().fg(color)),
            Span::styled(format!("{:<20}", name_preview), style),
            Span::styled(format!("  {}", elapsed), Style::default().fg(ratatui::style::Color::DarkGray)),
        ])));
    }

    // 溢出提示
    if agents.len() > 4 {
        let overflow = agents.len() - 4;
        items.push(ListItem::new(Line::from(Span::styled(
            format!("  …+{}", overflow),
            Style::default().fg(ratatui::style::Color::DarkGray),
        ))));
    }

    let bar_block = Block::default()
        .borders(Borders::TOP)
        .border_style(Style::default().fg(ratatui::style::Color::DarkGray));

    let list = List::new(items).block(bar_block);
    f.render_widget(list, area);
}
```

- [ ] **Step 2: 注册模块**

在 `peri-tui/src/ui/main_ui/mod.rs` 顶部模块声明中添加：

```rust
// mod.rs — 添加模块声明
pub(crate) mod bg_agent_bar;
```

- [ ] **Step 3: 编译验证**

Run: `cargo build -p peri-tui 2>&1 | head -20`

预期：编译通过（bar 模块暂未被调用，仅模块注册）。

---

### Task 5: 布局集成

**Files:**
- Modify: `peri-tui/src/ui/main_ui/mod.rs`

- [ ] **Step 1: 修改 render_session_column 布局约束**

在 `render_session_column()` 函数中，在 status_bar_height 计算之后、Layout::default() 之前，新增 bg_bar_height 计算：

```rust
// mod.rs — 在 status_bar_height 计算之后添加
let bg_bar_height = bg_agent_bar::bg_bar_height(app);
```

修改 Layout 约束数组，在 status_bar 之后添加 bg_agent_bar：

```rust
// mod.rs — Layout 约束从 7 个变为 8 个
let chunks = Layout::default()
    .direction(Direction::Vertical)
    .constraints([
        Constraint::Length(sticky_header_height),
        Constraint::Min(1),
        Constraint::Length(attachment_height),
        Constraint::Length(panel_height),
        Constraint::Length(queued_height),
        Constraint::Length(input_height),
        Constraint::Length(status_bar_height),
        Constraint::Length(bg_bar_height),  // ← 新增
    ])
    .split(area);
```

在 status_bar 渲染之后添加 bar 渲染调用：

```rust
// mod.rs — 在 status_bar::render_status_bar 调用之后添加
if bg_bar_height > 0 {
    bg_agent_bar::render_bg_agent_bar(f, app, chunks[7]);
}
```

**注意**：当前 status_bar 在多 session 模式下渲染在 `outer[1]`，单 session 模式下渲染在 chunks[6]。需要在两个分支中都添加 bg_bar 渲染。

多 session 分支需要把 bg_bar_height 也纳入 outer 的约束计算——outer 的 status_bar 行需要加上 bg_bar_height：

```rust
// mod.rs — 多 session 模式 outer 约束
// 旧: Constraint::Length(3), // 共享状态栏
// 新:
Constraint::Length(3 + bg_bar_height), // 共享状态栏 + bg agent bar
```

多 session 模式下，bg_bar 渲染在 status_bar 下方（outer[1] 中需要进一步拆分）：

```rust
// 多 session 模式 status_bar 渲染之后
if bg_bar_height > 0 {
    let sb_area = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(bg_bar_height),
        ])
        .split(outer[1]);
    status_bar::render_status_bar(f, app, sb_area[0]);
    bg_agent_bar::render_bg_agent_bar(f, app, sb_area[1]);
} else {
    status_bar::render_status_bar(f, app, outer[1]);
}
```

- [ ] **Step 2: 编译验证**

Run: `cargo build -p peri-tui 2>&1 | head -20`

预期：编译通过。手动运行 TUI 验证无布局异常。

- [ ] **Step 3: Commit**

```bash
git add peri-tui/src/ui/main_ui/bg_agent_bar.rs peri-tui/src/ui/main_ui/mod.rs
git commit -m "feat: add bg_agent_bar rendering module and layout integration

- New bg_agent_bar.rs with color palette, agent list rendering
- Layout constraint updated to include bg_agent_bar below status bar
- Multi-session mode: bg_bar rendered in shared bottom area"
```

---

### Task 6: 键盘处理 — Ctrl+B + Bar 焦点 + 只读模式

**Files:**
- Modify: `peri-tui/src/event/keyboard.rs`

- [ ] **Step 1: 添加 Ctrl+B 快捷键注册**

在 `keyboard.rs` 的 `SHORTCUT_CTRL_CYCLE_PROVIDER` 定义之后添加：

```rust
// keyboard.rs — 新增快捷键定义
static SHORTCUT_BG_BAR: KeyBinding = KeyBinding {
    label: "Ctrl+B",
    macos_char: None,
    modifiers: KeyModifiers::CONTROL,
    key: KeyCode::Char('b'),
};
```

在 `handle_key_event` 函数中，在 `BackTab` 处理之后、`Ctrl+T` 处理之前，添加 Ctrl+B 处理：

```rust
// keyboard.rs — 在 BackTab 处理之后添加
// Ctrl+B: 跳转到后台 agent bar
if SHORTCUT_BG_BAR.matches(&key_event) {
    let session = &app.session_mgr.sessions[app.session_mgr.active];
    if !session.background_agents.is_empty() {
        // 设置 bar 焦点：cursor 指向 main (index 0)
        app.session_mgr.sessions[app.session_mgr.active].ui.bg_bar_cursor = Some(0);
        return Ok(Some(Action::Redraw));
    }
    // 无后台 agent 时静默忽略
    return Ok(Some(Action::Redraw));
}
```

- [ ] **Step 2: 添加 Bar 焦点模式下的键盘处理**

在 `handle_key_event` 函数的**最开头**（在 `KeyEventKind::Release` 检查之后、BackTab 之前），添加 bar 焦点模式拦截：

```rust
// keyboard.rs — 在 Release 检查之后添加
// ── Bar 焦点模式拦截 ──
{
    let cursor = app.session_mgr.sessions[app.session_mgr.active].ui.bg_bar_cursor;
    if cursor.is_some() {
        return Ok(Some(handle_bar_key_event(app, key_event)));
    }
}
// ── 聚焦只读模式拦截 ──
{
    let focused = app.session_mgr.sessions[app.session_mgr.active]
        .focused_instance_id
        .is_some();
    if focused {
        if matches!(key_event.code, KeyCode::Esc) {
            // 退出聚焦
            app.session_mgr.sessions[app.session_mgr.active].focused_instance_id = None;
            app.session_mgr.sessions[app.session_mgr.active].ui.bg_bar_cursor = None;
            app.request_rebuild();
            return Ok(Some(Action::Redraw));
        }
        // 其他按键静默消费
        return Ok(Some(Action::Redraw));
    }
}
```

在 `keyboard.rs` 文件末尾（`handle_key_event` 函数外）添加 bar 键盘处理函数：

```rust
// keyboard.rs — 文件末尾新增
/// Bar 焦点模式下的键盘处理
fn handle_bar_key_event(app: &mut App, key_event: ratatui::crossterm::event::KeyEvent) -> Action {
    let agents_len = app.session_mgr.sessions[app.session_mgr.active]
        .background_agents
        .len();
    let total_items = 1 + agents_len.min(4); // main + visible agents

    let cursor = app.session_mgr.sessions[app.session_mgr.active]
        .ui
        .bg_bar_cursor
        .unwrap_or(0);

    match key_event.code {
        KeyCode::Esc => {
            // 退出 bar 焦点
            app.session_mgr.sessions[app.session_mgr.active].ui.bg_bar_cursor = None;
            Action::Redraw
        }
        KeyCode::Up => {
            let new_cursor = if cursor > 0 { cursor - 1 } else { total_items - 1 };
            app.session_mgr.sessions[app.session_mgr.active].ui.bg_bar_cursor = Some(new_cursor);
            Action::Redraw
        }
        KeyCode::Down => {
            let new_cursor = (cursor + 1) % total_items;
            app.session_mgr.sessions[app.session_mgr.active].ui.bg_bar_cursor = Some(new_cursor);
            Action::Redraw
        }
        KeyCode::Enter => {
            if cursor == 0 {
                // 选中 main → 退出聚焦（如果有的话）
                app.session_mgr.sessions[app.session_mgr.active].focused_instance_id = None;
            } else {
                // 选中某个后台 agent → 进入聚焦模式
                let agents = &app.session_mgr.sessions[app.session_mgr.active].background_agents;
                if let Some(agent) = agents.get(cursor - 1) {
                    app.session_mgr.sessions[app.session_mgr.active].focused_instance_id =
                        Some(agent.instance_id.clone());
                }
            }
            // 退出 bar 焦点，焦点回到输入框
            app.session_mgr.sessions[app.session_mgr.active].ui.bg_bar_cursor = None;
            app.request_rebuild();
            Action::Redraw
        }
        _ => Action::Redraw, // 其他按键静默消费
    }
}
```

- [ ] **Step 3: 编译验证**

Run: `cargo build -p peri-tui 2>&1 | head -20`

预期：编译通过。

- [ ] **Step 4: Commit**

```bash
git add peri-tui/src/event/keyboard.rs
git commit -m "feat: add Ctrl+B shortcut and bar/focus keyboard handling

- Ctrl+B opens bg agent bar (silent when no agents)
- Bar focus mode: Up/Down navigation, Enter to select, Esc to close
- Focus read-only mode: Esc to exit, other keys consumed
- handle_bar_key_event manages cursor + focus transitions"
```

---

### Task 7: 消息过滤 — Pipeline focused_instance_id

**Files:**
- Modify: `peri-tui/src/app/message_pipeline/mod.rs`
- Modify: `peri-tui/src/app/message_pipeline/transform.rs`

- [ ] **Step 1: 在 pipeline 中添加过滤方法**

在 `message_pipeline/mod.rs` 的 `MessagePipeline` impl 块中添加：

```rust
// mod.rs — MessagePipeline impl 中新增方法

/// 根据聚焦的 agent instance_id 过滤 VM 列表
pub fn filter_for_focus(
    vms: &mut Vec<MessageViewModel>,
    focused_instance_id: Option<&str>,
) {
    if let Some(id) = focused_instance_id {
        vms.retain(|vm| {
            match vm {
                MessageViewModel::SubAgentGroup {
                    is_background: true,
                    ..
                } => {
                    // 后台 SubAgentGroup：检查是否为聚焦的 agent
                    // SubAgentGroup 没有 instance_id 字段，使用 bg_hash 匹配
                    // bg_hash = instance_hash(instance_id)
                    true // 暂时保留所有，后续通过 bg_hash 精确匹配
                }
                _ => true, // 非 SubAgentGroup 消息始终保留
            }
        });
    }
}
```

**注意**：SubAgentGroup 没有 `instance_id` 字段，只有 `bg_hash`（由 `instance_hash(instance_id)` 生成）。精确过滤需要通过 bg_hash 匹配。完整的过滤逻辑：

```rust
// mod.rs — 完整过滤方法
pub fn filter_for_focus(
    vms: &mut Vec<MessageViewModel>,
    focused_instance_id: Option<&str>,
) {
    if let Some(_id) = focused_instance_id {
        // TODO: 精确过滤需要在 SubAgentGroup 中存储 instance_id
        // 当前 SubAgentGroup 仅有 bg_hash，无法精确匹配
        // 第一版实现：保留所有消息，聚焦模式仅影响输入框样式
    }
}
```

**设计决策**：第一版实现先不做消息过滤（保留所有消息可见），聚焦模式仅影响输入框边框颜色和标签。精确的消息过滤需要扩展 `SubAgentGroup` 结构体添加 `instance_id` 字段，这是一个较大的改动，留作后续迭代。

- [ ] **Step 2: 编译验证**

Run: `cargo build -p peri-tui 2>&1 | head -20`

预期：编译通过。

- [ ] **Step 3: Commit**

```bash
git add peri-tui/src/app/message_pipeline/mod.rs
git commit -m "feat: add filter_for_focus placeholder in message pipeline

First iteration: focus mode affects input box styling only.
Message filtering requires SubAgentGroup.instance_id field (deferred)."
```

---

### Task 8: 聚焦模式 — 输入框边框颜色 + 标签

**Files:**
- Modify: `peri-tui/src/ui/main_ui/mod.rs` (输入框渲染区域)

- [ ] **Step 1: 修改输入框渲染逻辑**

在 `render_session_column()` 中，输入框渲染之前，添加聚焦模式的边框样式。找到 textarea 渲染的位置（搜索 `textarea` 关键字），在渲染 textarea 之前修改其 block 样式：

```rust
// mod.rs — 在 textarea 渲染前添加聚焦样式
let focused_id = app.session_mgr.sessions[session_idx]
    .focused_instance_id
    .clone();

if let Some(ref id) = focused_id {
    // 聚焦模式：找到对应 agent 的颜色
    let agents = &app.session_mgr.sessions[session_idx].background_agents;
    let color = agents
        .iter()
        .position(|a| a.instance_id == *id)
        .map(|i| bg_agent_bar::agent_color(i))
        .unwrap_or(ratatui::style::Color::Cyan);

    let agent_name = agents
        .iter()
        .find(|a| a.instance_id == *id)
        .map(|a| a.agent_name.as_str())
        .unwrap_or("agent");

    let title = format!("[{}]", agent_name);
    let block = ratatui::widgets::Block::default()
        .borders(ratatui::widgets::Borders::ALL)
        .border_style(ratatui::style::Style::default().fg(color))
        .title(title);
    app.session_mgr.sessions[session_idx].ui.textarea.set_block(block);
} else {
    // 正常模式：恢复默认 block
    let block = ratatui::widgets::Block::default()
        .borders(ratatui::widgets::Borders::ALL)
        .border_style(ratatui::style::Style::default());
    app.session_mgr.sessions[session_idx].ui.textarea.set_block(block);
}
```

- [ ] **Step 2: 编译验证**

Run: `cargo build -p peri-tui 2>&1 | head -20`

预期：编译通过。

- [ ] **Step 3: Commit**

```bash
git add peri-tui/src/ui/main_ui/mod.rs
git commit -m "feat: input box border color and agent name label in focus mode

- Focused mode: border color from agent palette, [agent_name] title
- Normal mode: restore default border style"
```

---

### Task 9: Compact 聚焦退出 + 状态栏提示更新

**Files:**
- Modify: `peri-tui/src/app/agent_compact.rs`
- Modify: `peri-tui/src/ui/main_ui/status_bar.rs`

- [ ] **Step 1: compact 前退出聚焦**

在 `agent_compact.rs` 中，找到 compact 执行入口（`handle_compact_completed` 或 `start_compact`），在 compact 操作开始前清空聚焦状态：

```rust
// agent_compact.rs — 在 compact 操作开始处添加
// 退出聚焦模式（如有）
self.session_mgr.sessions[self.session_mgr.active].focused_instance_id = None;
self.session_mgr.sessions[self.session_mgr.active].ui.bg_bar_cursor = None;
```

- [ ] **Step 2: 编译验证**

Run: `cargo build -p peri-tui 2>&1 | head -20`

预期：编译通过。

- [ ] **Step 3: Commit**

```bash
git add peri-tui/src/app/agent_compact.rs
git commit -m "feat: exit focus mode before compact operation"
```

---

### Task 10: i18n 字符串

**Files:**
- Modify: `peri-tui/locales/zh-CN/main.ftl`
- Modify: `peri-tui/locales/en/main.ftl`

- [ ] **Step 1: 添加 bar 相关 i18n 字符串**

`zh-CN/main.ftl` 添加：

```ftl
# 后台 Agent 管理栏
bg-bar-focus-hint = 按 Esc 退出聚焦
```

`en/main.ftl` 添加：

```ftl
# Background Agent Bar
bg-bar-focus-hint = Press Esc to exit focus
```

- [ ] **Step 2: Commit**

```bash
git add peri-tui/locales/zh-CN/main.ftl peri-tui/locales/en/main.ftl
git commit -m "feat: add i18n strings for bg agent bar focus hint"
```

---

### Task 11: 集成测试 — 手动验证 + 现有测试回归

**Files:**
- Modify: `peri-tui/src/ui/headless_test.rs` (如需补充)

- [ ] **Step 1: 运行全量测试**

Run: `cargo test -p peri-tui --lib 2>&1 | tail -30`

预期：所有现有测试通过。

- [ ] **Step 2: 新增数据模型单元测试**

在 `ui/headless_test.rs` 中新增测试：

```rust
#[tokio::test]
async fn test_background_agents_lifecycle() {
    let (mut app, _handle) = create_test_app().await;

    // SubAgentStart(bg=true) → push agent
    app.push_agent_event(AgentEvent::SubAgentStart {
        agent_id: "code-reviewer".into(),
        instance_id: "inst-001".into(),
        task_preview: String::new(),
        is_background: true,
    });
    app.process_pending_events();
    assert_eq!(app.session_mgr.sessions[app.session_mgr.active].background_agents.len(), 1);
    assert_eq!(
        app.session_mgr.sessions[app.session_mgr.active].background_agents[0].agent_name,
        "code-reviewer"
    );

    // 再启动一个
    app.push_agent_event(AgentEvent::SubAgentStart {
        agent_id: "explorer".into(),
        instance_id: "inst-002".into(),
        task_preview: String::new(),
        is_background: true,
    });
    app.process_pending_events();
    assert_eq!(app.session_mgr.sessions[app.session_mgr.active].background_agents.len(), 2);

    // BackgroundTaskCompleted → 移除匹配的 agent
    app.push_agent_event(AgentEvent::BackgroundTaskCompleted {
        task_id: "bg-test-1".into(),
        agent_name: "code-reviewer".into(),
        success: true,
        output: "done".into(),
        tool_calls_count: 1,
        duration_ms: 100,
    });
    app.process_pending_events();
    assert_eq!(app.session_mgr.sessions[app.session_mgr.active].background_agents.len(), 1);
    assert_eq!(
        app.session_mgr.sessions[app.session_mgr.active].background_agents[0].agent_name,
        "explorer"
    );

    // 聚焦测试
    app.session_mgr.sessions[app.session_mgr.active].focused_instance_id = Some("inst-002".into());

    // 完成聚焦的 agent → 自动退出聚焦
    app.push_agent_event(AgentEvent::BackgroundTaskCompleted {
        task_id: "bg-test-2".into(),
        agent_name: "explorer".into(),
        success: true,
        output: "done".into(),
        tool_calls_count: 1,
        duration_ms: 100,
    });
    app.process_pending_events();
    assert!(app.session_mgr.sessions[app.session_mgr.active].background_agents.is_empty());
    assert_eq!(app.session_mgr.sessions[app.session_mgr.active].focused_instance_id, None);
}
```

- [ ] **Step 3: 运行新增测试**

Run: `cargo test -p peri-tui --lib -- test_background_agents_lifecycle 2>&1 | tail -10`

预期：测试通过。

- [ ] **Step 4: 最终 Commit**

```bash
git add peri-tui/src/ui/headless_test.rs
git commit -m "test: add background agent lifecycle test (add, complete, focus auto-exit)"
```

---

## 已知限制（第一版不做，后续迭代）

| 功能 | 原因 |
|------|------|
| 消息��滤（聚焦时只显示该 agent 的消息） | SubAgentGroup 没有 `instance_id` 字段，需扩展结构体。第一版聚焦模式仅影响输入框样式 |
| 输入框文字置灰 + 只读提示文字 | 需要 textarea 自定义渲染，复杂度较高 |
| bar 获得焦点时输入框变暗 | 需要在 textarea 渲染层判断 bar 焦点状态 |
| cursor 越界保护 | 当 bar 显示期间 agent 完成被移除，cursor 可能超出 total_items。需要在渲染和按键处理中 `cursor.min(total_items - 1)` |
| 后台 agent stale 状态检测 | 依赖 `agent_done_pending_bg` 超时机制，需确认超时后是否有事件触发 bar 更新 |
