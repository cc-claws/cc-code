# AskUser 弹窗修复：选项描述丢失 + 滚动不可交互

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 修复 AskUser 弹窗的三个问题：ACP 路径��项 description 被丢弃、内容溢出面板高度不够、滚动条渲染但不可交互。

**Architecture:** 三个独立修复点：(1) 在 TUI 解析 Elicitation JSON 时从原始 JSON 中提取每个选项的 `description` 字段；(2) 增大 AskUser 面板高度上限从 60% 到 75%；(3) 在 ask_user 键盘处理中添加 Ctrl+U/Ctrl+D 滚动支持，在鼠标处理中添加滚轮/拖拽滚动条支持。

**Tech Stack:** Rust, ratatui, peri-widgets (ScrollableArea/ScrollState/ScrollbarMetrics)

**Issue:** `spec/issues/2026-05-23-ask-user-overflow-and-description-missing.md`

---

## File Structure

| 文件 | 变更 | 职责 |
|------|------|------|
| `peri-tui/src/app/agent_ops_interaction.rs` | 修改 | Task 1: 从原始 JSON 提取选项 description |
| `peri-tui/src/ui/main_ui/mod.rs` | 修改 | Task 2: 增大 AskUser 面板高度上限 |
| `peri-tui/src/event/keyboard.rs` | 修改 | Task 3: 添加 Ctrl+U/Ctrl+D 滚动快捷键 |
| `peri-tui/src/app/ask_user_ops.rs` | 修改 | Task 3: 添加滚动操作方法 |
| `peri-tui/src/ui/main_ui/popups/ask_user.rs` | 修改 | Task 3: 将 ScrollbarMetrics 存储到 prompt 状态 |
| `peri-tui/src/app/ask_user_prompt.rs` | 修改 | Task 3: 添加 scrollbar_metrics 字段 |
| `peri-tui/src/event/mouse.rs` | 修改 | Task 4: 添加 ask_user 弹窗鼠标滚轮/拖拽支持 |

---

### Task 1: ACP 路径选项 description 提取

**Files:**
- Modify: `peri-tui/src/app/agent_ops_interaction.rs:75-120`

**背景：** `peri-acp/src/broker/transport_broker.rs` 的 `inject_option_descriptions` 已在 JSON 的 `oneOf`/`anyOf` 数组中注入了 `description` 字段。但 TUI 侧 `handle_acp_elicitation` 先用 `serde_json::from_value::<CreateElicitationRequest>(params)` 反序列化（`EnumOption` 无 description 字段，丢失），然后硬编码 `description: None`。

**修复策略：** 反序列化后，再从原始 `params` JSON 中读取每个属性的 `oneOf`/`anyOf` 数组里每个选项的 `description` 字段，合并到已构建的 `AskUserOption` 中。

- [ ] **Step 1: 实现 description 提取辅助函数**

在 `peri-tui/src/app/agent_ops_interaction.rs` 文件底部（`handle_interaction_request` 方法之后，模块末尾之前）添加独立辅助函数：

```rust
/// 从 Elicitation JSON 中提取每个属性的选项 description。
/// `inject_option_descriptions` (transport_broker) 在 JSON 层面注入了 description，
/// 但 `EnumOption` 结构体无此字段，反序列化后丢失。
fn extract_option_descriptions(
    params: &serde_json::Value,
    prop_id: &str,
    is_multi: bool,
) -> Vec<Option<String>> {
    let container_key = if is_multi { "anyOf" } else { "oneOf" };
    let Some(arr) = params
        .get("mode")
        .and_then(|m| m.get("requestedSchema"))
        .and_then(|s| s.get("properties"))
        .and_then(|p| p.get(prop_id))
        .and_then(|prop| {
            // multi-select: description 在 items 下面；single: description 在 prop 下面
            if is_multi {
                prop.get("items")
            } else {
                Some(prop)
            }
        })
        .and_then(|p| p.get(container_key))
        .and_then(|v| v.as_array())
    else {
        return vec![];
    };
    arr.iter()
        .map(|opt| {
            opt.get("description")
                .and_then(|d| d.as_str())
                .map(|s| s.to_string())
        })
        .collect()
}
```

- [ ] **Step 2: 在 handle_acp_elicitation 中调用提取函数**

修改 `handle_acp_elicitation` 方法（`agent_ops_interaction.rs:75-120`），在构建完 `questions` 向量后、创建 `AskUserBatchRequest` 之前，注入 description：

将当前代码（约 73-120 行）：
```rust
let mut questions = Vec::new();

if let ElicitationMode::Form(form) = req.mode {
    for (prop_id, prop) in &form.requested_schema.properties {
        let (title, description, is_multi, options) = match prop {
            // ...
        };
        questions.push(AskUserQuestionData {
            tool_call_id: prop_id.clone(),
            question: description.unwrap_or_default(),
            header: title.unwrap_or_default(),
            multi_select: is_multi,
            options,
        });
    }
}
```

改为：
```rust
let mut questions = Vec::new();

if let ElicitationMode::Form(form) = req.mode {
    for (prop_id, prop) in &form.requested_schema.properties {
        let (title, description, is_multi, mut options) = match prop {
            agent_client_protocol_schema::ElicitationPropertySchema::String(s) => (
                s.title.clone(),
                s.description.clone(),
                false,
                s.one_of
                    .as_ref()
                    .map(|opts| {
                        opts.iter()
                            .map(|o| AskUserOption {
                                label: o.title.clone(),
                                description: None,
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
            ),
            agent_client_protocol_schema::ElicitationPropertySchema::Array(a) => (
                a.title.clone(),
                a.description.clone(),
                true,
                match &a.items {
                    agent_client_protocol_schema::MultiSelectItems::Titled(t) => t
                        .options
                        .iter()
                        .map(|o| AskUserOption {
                            label: o.title.clone(),
                            description: None,
                        })
                        .collect(),
                    _ => vec![],
                },
            ),
            _ => continue,
        };
        // 从原始 JSON 中提取被 EnumOption 丢弃的 description
        let opt_descs = extract_option_descriptions(&params, prop_id, is_multi);
        for (i, desc) in opt_descs.into_iter().enumerate() {
            if let Some(opt) = options.get_mut(i) {
                if opt.description.is_none() {
                    opt.description = desc;
                }
            }
        }
        questions.push(AskUserQuestionData {
            tool_call_id: prop_id.clone(),
            question: description.unwrap_or_default(),
            header: title.unwrap_or_default(),
            multi_select: is_multi,
            options,
        });
    }
}
```

注意：`params` 参数已存在于函数签名中（`handle_acp_elicitation(&mut self, id: RequestId, params: serde_json::Value)`）。但 `params` 在 Step 1 会被 `serde_json::from_value` consume（move）。需要把 `serde_json::from_value::<CreateElicitationRequest>(params)` 改为先 clone：

```rust
let req = match serde_json::from_value::<CreateElicitationRequest>(params.clone()) {
```

- [ ] **Step 3: 编译验证**

Run: `cargo build -p peri-tui 2>&1 | head -30`
Expected: BUILD SUCCEEDED（无类型错误）

- [ ] **Step 4: Commit**

```bash
git add peri-tui/src/app/agent_ops_interaction.rs
git commit -m "fix: extract option descriptions from Elicitation JSON in ACP path

EnumOption struct has no description field, so inject_option_descriptions
(transport_broker) patches the JSON post-serialization. The TUI side now
reads these injected descriptions from the raw params JSON."
```

---

### Task 2: 增大 AskUser 面板高度上限

**Files:**
- Modify: `peri-tui/src/ui/main_ui/mod.rs:310-376`

**背景：** `active_panel_height` 中 AskUser 面板最大占屏幕 60%（`screen_height * 3 / 5`），和 plugin 面板的 70% 相比太小。选项多/文字长时内容溢出。

- [ ] **Step 1: 将 AskUser 的最大高度从 60% 提升到 75%**

修改 `active_panel_height` 函数（`mod.rs:310-317`）：

```rust
fn active_panel_height(app: &App, screen_height: u16, screen_width: u16) -> u16 {
    // plugin 面板可以占 70%，其他面板最多 60%
    let is_plugin_panel = app.global_panels.is_active(crate::app::PanelKind::Plugin);
    // AskUser 弹窗选项多时需要更多空间，允许 75%
    let has_ask_user = matches!(
        &app.session_mgr.sessions[app.session_mgr.active].agent.interaction_prompt,
        Some(crate::app::InteractionPrompt::Questions(_))
    );
    let max_h = if is_plugin_panel {
        screen_height * 70 / 100
    } else if has_ask_user {
        screen_height * 3 / 4
    } else {
        screen_height * 3 / 5
    };
```

- [ ] **Step 2: 编译验证**

Run: `cargo build -p peri-tui 2>&1 | head -20`
Expected: BUILD SUCCEEDED

- [ ] **Step 3: Commit**

```bash
git add peri-tui/src/ui/main_ui/mod.rs
git commit -m "fix: increase AskUser popup max height from 60% to 75%

Options with long text or many choices need more vertical space.
Prevents content overflow and reduces scrollbar dependency."
```

---

### Task 3: 添加键盘滚动支持 (Ctrl+U/Ctrl+D)

**Files:**
- Modify: `peri-tui/src/app/ask_user_prompt.rs` — 添加 `scrollbar_metrics` 字段
- Modify: `peri-tui/src/ui/main_ui/popups/ask_user.rs` — 存储 ScrollbarMetrics 到 prompt
- Modify: `peri-tui/src/app/ask_user_ops.rs` — 添加 `ask_user_scroll` 方法
- Modify: `peri-tui/src/event/keyboard.rs` — 添加 Ctrl+U/Ctrl+D 绑定

**背景：** 当前 ask_user 弹窗的滚动仅通过上下键移动光标时自动跟随（`ask_user_move` 中调用 `ensure_cursor_visible`），但没有独立的页面级滚动。`ScrollState` 渲染了滚动条但没有键盘/鼠标交互。需要：(a) 将渲染时得到的 `ScrollbarMetrics` 存下来供事件处理用；(b) 添加页面级滚动方法；(c) 绑定快捷键。

- [ ] **Step 1: 在 AskUserBatchPrompt 添加 scrollbar_metrics 字段**

修改 `peri-tui/src/app/ask_user_prompt.rs`，在 `AskUserBatchPrompt` 结构体中添加字段：

```rust
pub struct AskUserBatchPrompt {
    pub questions: Vec<QuestionState>,
    pub active_tab: usize,
    pub confirmed: Vec<bool>,
    pub response_tx: tokio::sync::oneshot::Sender<Vec<String>>,
    /// 内容滚动偏移
    pub scroll_offset: u16,
    /// 渲染时存储的滚动条几何信息，供鼠标交互使用
    pub scrollbar_metrics: Option<peri_widgets::ScrollbarMetrics>,
}
```

在 `from_request` 中初始化：
```rust
scrollbar_metrics: None,
```

- [ ] **Step 2: 在 render_ask_user_popup 中存储 ScrollbarMetrics**

修改 `peri-tui/src/ui/main_ui/popups/ask_user.rs` 底部的渲染代码（约 176-181 行）：

当前代码：
```rust
let mut scroll_state = ScrollState::with_offset(prompt.scroll_offset);
app.session_mgr.sessions[app.session_mgr.active]
    .ui
    .panel_scrollbar_metrics = ScrollableArea::new(Text::from(lines))
    .scrollbar_style(Style::default().fg(theme::MUTED))
    .render(f, content_area, &mut scroll_state);
```

改为：
```rust
let mut scroll_state = ScrollState::with_offset(prompt.scroll_offset);
let metrics = ScrollableArea::new(Text::from(lines))
    .scrollbar_style(Style::default().fg(theme::MUTED))
    .render(f, content_area, &mut scroll_state);
// 存储到 prompt 状态供事件交互使用
app.session_mgr.sessions[app.session_mgr.active]
    .ui
    .panel_scrollbar_metrics = metrics;
if let Some(InteractionPrompt::Questions(p)) = app.session_mgr.sessions
    [app.session_mgr.active]
    .agent
    .interaction_prompt
    .as_mut()
{
    p.scrollbar_metrics = metrics;
}
```

- [ ] **Step 3: 在 ask_user_ops.rs 中添加页面级滚动方法**

在 `impl App` 的 ask_user 方法块中（`ask_user_ops.rs`）添加：

```rust
/// 页面级滚动（Ctrl+U 上翻 / Ctrl+D 下翻）
pub fn ask_user_scroll(&mut self, lines: i16) {
    if let Some(InteractionPrompt::Questions(p)) = self.session_mgr.sessions
        [self.session_mgr.active]
        .agent
        .interaction_prompt
        .as_mut()
    {
        let visible = p
            .scrollbar_metrics
            .map(|m| {
                // 可见高度 = bar_area 高度
                m.bar_area.height
            })
            .unwrap_or(10);
        if lines > 0 {
            p.scroll_offset = p.scroll_offset.saturating_add(lines as u16);
        } else {
            p.scroll_offset = p.scroll_offset.saturating_sub((-lines) as u16);
        }
        // 钳位到最大偏移
        if let Some(m) = p.scrollbar_metrics {
            p.scroll_offset = p.scroll_offset.min(m.max_offset);
        }
        // 同步光标位置到可见区域
        let cursor_row = p.current().option_cursor.max(0) as u16;
        if cursor_row < p.scroll_offset {
            p.current().option_cursor = (p.scroll_offset as isize).min(p.current().total_rows() - 1);
        } else if let Some(m) = p.scrollbar_metrics {
            let visible_h = m.bar_area.height;
            if cursor_row >= p.scroll_offset + visible_h {
                let new_cursor = (p.scroll_offset + visible_h - 1) as isize;
                p.current().option_cursor = new_cursor.min(p.current().total_rows() - 1);
            }
        }
    }
}
```

- [ ] **Step 4: 在 keyboard.rs 中绑定 Ctrl+U/Ctrl+D**

修改 `peri-tui/src/event/keyboard.rs` 中 ask_user 分支（约 340 行附近），在 `KeyCode::Up`/`KeyCode::Down` 之前添加：

```rust
// Ctrl+U / Ctrl+D 页面滚动（与消息区一致）
Input {
    key: Key::Char('u'),
    ctrl: true,
    ..
} => app.ask_user_scroll(-10),
Input {
    key: Key::Char('d'),
    ctrl: true,
    ..
} => app.ask_user_scroll(10),
```

注意：根据 CLAUDE.md 的编码规范，不使用 PageUp/PageDown。Ctrl+U/Ctrl+D 在 textarea 空时做滚动，这里 ask_user 弹窗没有 textarea，所以直接绑定即可。

- [ ] **Step 5: 编译验证**

Run: `cargo build -p peri-tui 2>&1 | head -20`
Expected: BUILD SUCCEEDED

- [ ] **Step 6: Commit**

```bash
git add peri-tui/src/app/ask_user_prompt.rs peri-tui/src/app/ask_user_ops.rs peri-tui/src/ui/main_ui/popups/ask_user.rs peri-tui/src/event/keyboard.rs
git commit -m "feat: add keyboard scrolling (Ctrl+U/Ctrl+D) for AskUser popup

Store ScrollbarMetrics in prompt state for scroll bounds.
Sync cursor position after page-level scroll."
```

---

### Task 4: 添加鼠标滚轮/滚动条交互支持

**Files:**
- Modify: `peri-tui/src/event/mouse.rs` — 添加 ask_user 弹窗鼠标处理分支

**背景：** 当前鼠标事件处理没有 ask_user 弹窗的分支。需要在 `handle_mouse_event` 中检测当前有 ask_user 弹窗打开时，处理滚轮（ScrollUp/ScrollDown）和滚动条拖拽。

- [ ] **Step 1: 找到鼠标事件分发入口，添加 ask_user 分支**

先阅读 `peri-tui/src/event/mouse.rs`，找到面板区域的鼠标处理逻辑。其他面板（如 hooks_panel、mcp_panel）已通过 `PanelComponent` trait 的 `handle_mouse` 处理鼠标。但 ask_user 弹窗不经过 PanelComponent，需要独立处理。

在鼠标事件处理中，当 `interaction_prompt` 为 `Questions` 时，拦截面板区域的滚轮和拖拽事件：

```rust
// AskUser 弹窗鼠标交互
if let Some(InteractionPrompt::Questions(ref mut p)) = sessions[active]
    .agent
    .interaction_prompt
{
    // 滚轮滚动
    if let MouseEventKind::ScrollUp | MouseEventKind::ScrollDown = event.kind {
        // 检查是否在面板区域内
        if let Some(panel_area) = sessions[active].ui.panel_area {
            if panel_area.contains((event.column, event.row).into()) {
                let delta = match event.kind {
                    MouseEventKind::ScrollUp => -3i16,
                    MouseEventKind::ScrollDown => 3i16,
                    _ => 0,
                };
                app.ask_user_scroll(delta);
                return EventResult::Consumed;
            }
        }
    }
    // 滚动条拖拽
    if let Some(metrics) = p.scrollbar_metrics {
        if let MouseEventKind::Down(_button) = event.kind {
            if metrics.bar_area.contains((event.column, event.row).into()) {
                // 点击滚动条区域：根据点击位置计算偏移
                let click_y = event.row;
                let ratio = (click_y - metrics.bar_area.y) as f32
                    / metrics.bar_area.height as f32;
                let new_offset = (ratio * metrics.max_offset as f32) as u16;
                p.scroll_offset = new_offset.min(metrics.max_offset);
                return EventResult::Consumed;
            }
        }
    }
}
```

注意：具体代码位置和 `sessions` 变量名需要参考 `mouse.rs` 的上下文。核心逻辑是：
1. 检查当前是否有 ask_user 弹窗打开
2. 检查鼠标坐标是否在面板区域内（用 `panel_area`）
3. 滚轮 → 调用 `ask_user_scroll(±3)`
4. 点击滚动条 → 计算比例跳转

- [ ] **Step 2: 编译验证**

Run: `cargo build -p peri-tui 2>&1 | head -20`
Expected: BUILD SUCCEEDED

- [ ] **Step 3: Commit**

```bash
git add peri-tui/src/event/mouse.rs
git commit -m "feat: add mouse scroll/drag support for AskUser popup

Mouse wheel scrolls ±3 lines, clicking scrollbar jumps to position.
Consumes events when inside the AskUser panel area."
```

---

## Self-Review

### Spec Coverage

| Issue 要求 | Task 覆盖 |
|-----------|----------|
| 弹窗高度不够 | Task 2: 60% → 75% |
| 滚动条不可交互（键盘） | Task 3: Ctrl+U/Ctrl+D |
| 滚动条不可交互（鼠标） | Task 4: 滚轮 + 点击滚动条 |
| 选项 description 未显示 | Task 1: 从 JSON 提取 description |

### Placeholder Scan

无 TBD/TODO/placeholder。所有代码步骤包含完整实现代码。

### Type Consistency

- `AskUserBatchPrompt.scrollbar_metrics: Option<ScrollbarMetrics>` — Task 3 Step 1 定义，Step 2 写入，Step 3/4 和 Task 4 读取
- `ask_user_scroll(lines: i16)` — Task 3 Step 3 定义，Step 4 和 Task 4 调用
- `extract_option_descriptions` 返回 `Vec<Option<String>>` — Task 1 Step 1 定义，Step 2 消费
- `params: serde_json::Value` — 函数签名已有，Step 2 需要 clone 避免被 from_value 消费
