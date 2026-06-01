# 交互弹窗激活时底部输入框失效

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 当 AskUser / HITL / OAuth 交互弹窗激活时，底部常驻输入框视觉上失效（光标隐藏、样式变暗），且 Paste/Mouse 点击事件不向 textarea 写入。

**Architecture:** 在 `App` 上新增 `is_interaction_popup_active()` 辅助方法（复用已有 `interaction_prompt` + `oauth_prompt` 检测）；在 `event/mod.rs` 的 `Event::Paste` 和 `MouseEventKind::Down(Left)` 处理路径中增加弹窗守卫；在 `ui/main_ui/mod.rs` 的 textarea 渲染中增加弹窗条件以隐藏光标和变暗样式。

**Tech Stack:** Rust + ratatui + tui_textarea

**Spec:** `spec/issues/2026-05-31-interaction-popup-textarea-not-disabled.md`

---

### Task 1: 添加 `App::is_interaction_popup_active()` 辅助方法

**Files:**
- Modify: `peri-tui/src/app/mod.rs`

- [ ] **Step 1: 在 `impl App` 块中添加辅助方法**

在 `mod.rs:648`（`impl App` 块的结尾 `}` 前）插入以下方法。放在 `get_compact_config()` 之后、impl 块结束之前。

```rust
/// 检查是否有任何交互弹窗处于激活状态（AskUser / HITL / OAuth）。
/// 弹窗激活时，底部 textarea 应失效——隐藏光标、禁止输入、视觉变暗。
pub fn is_interaction_popup_active(&self) -> bool {
    self.global_ui.oauth_prompt.is_some()
        || self.session_mgr.sessions[self.session_mgr.active]
            .agent
            .interaction_prompt
            .is_some()
}
```

- [ ] **Step 2: 编译检查**

```bash
cargo build -p peri-tui 2>&1 | head -20
```
Expected: 编译成功，无新增警告。

- [ ] **Step 3: Commit**

```bash
git add peri-tui/src/app/mod.rs
git commit -m "feat: add App::is_interaction_popup_active() helper

用于统一检测 AskUser/HITL/OAuth 弹窗是否激活，供事件处理和渲染复用。

Co-Authored-By: deepseek-v4-pro <deepseek-ai@claude-code-best.win>"
```

---

### Task 2: 修复 Paste 事件绕过弹窗

**Files:**
- Modify: `peri-tui/src/event/mod.rs:306-310`

- [ ] **Step 1: 在 Paste 事件 fallback 前增加弹窗守卫**

将 `event/mod.rs` 第 306-310 行的 fallback 代码：

```rust
            // Fallback: paste into textarea
            app.session_mgr.sessions[app.session_mgr.active]
                .ui
                .textarea
                .insert_str(&text);
```

替换为：

```rust
            // Fallback: paste into textarea
            // 弹窗激活时不写入 textarea——用户应通过弹窗 UI 交互
            if !app.is_interaction_popup_active() {
                app.session_mgr.sessions[app.session_mgr.active]
                    .ui
                    .textarea
                    .insert_str(&text);
            }
```

- [ ] **Step 2: 编译检查**

```bash
cargo build -p peri-tui 2>&1 | head -20
```
Expected: 编译成功，无新增警告。

- [ ] **Step 3: Commit**

```bash
git add peri-tui/src/event/mod.rs
git commit -m "fix: block paste into textarea when interaction popup is active

之前 Event::Paste 不检查 interaction_prompt/oauth_prompt，
弹窗激活时粘贴的文本直接进入底部 textarea 而非弹窗。

Co-Authored-By: deepseek-v4-pro <deepseek-ai@claude-code-best.win>"
```

---

### Task 3: 修复鼠标点击 textarea 绕过弹窗

**Files:**
- Modify: `peri-tui/src/event/mod.rs:655-675`

- [ ] **Step 1: 在 Mouse Down(Left) 的 textarea 点击路径前增加弹窗守卫**

将 `event/mod.rs` 第 655-675 行的代码：

```rust
                // Textarea area: start textarea selection
                if let Some(area) = app.session_mgr.sessions[app.session_mgr.active]
                    .ui
                    .textarea_area
                {
                    if mouse.row >= area.y
                        && mouse.row < area.y + area.height
                        && mouse.column >= area.x
                        && mouse.column < area.x + area.width
                    {
                        let session = &app.session_mgr.sessions[app.session_mgr.active];
                        let (row, col) =
                            mouse::textarea_mouse_to_cursor(&session.ui.textarea, area, &mouse);
                        app.session_mgr.sessions[app.session_mgr.active]
                            .ui
                            .textarea
                            .move_cursor(tui_textarea::CursorMove::Jump(row as u16, col as u16));
                        app.session_mgr.sessions[app.session_mgr.active]
                            .ui
                            .textarea
                            .start_selection();
```

替换为：

```rust
                // Textarea area: start textarea selection
                // 弹窗激活时跳过——光标不应移到 textarea 内
                if !app.is_interaction_popup_active() {
                    if let Some(area) = app.session_mgr.sessions[app.session_mgr.active]
                        .ui
                        .textarea_area
                    {
                        if mouse.row >= area.y
                            && mouse.row < area.y + area.height
                            && mouse.column >= area.x
                            && mouse.column < area.x + area.width
                        {
                            let session = &app.session_mgr.sessions[app.session_mgr.active];
                            let (row, col) =
                                mouse::textarea_mouse_to_cursor(&session.ui.textarea, area, &mouse);
                            app.session_mgr.sessions[app.session_mgr.active]
                                .ui
                                .textarea
                                .move_cursor(tui_textarea::CursorMove::Jump(row as u16, col as u16));
                            app.session_mgr.sessions[app.session_mgr.active]
                                .ui
                                .textarea
                                .start_selection();
```

注意：仅增加外层 `if !app.is_interaction_popup_active()` 包裹，内部逻辑不变。

- [ ] **Step 2: 编译检查**

```bash
cargo build -p peri-tui 2>&1 | head -20
```
Expected: 编译成功，无新增警告。

- [ ] **Step 3: Commit**

```bash
git add peri-tui/src/event/mod.rs
git commit -m "fix: block mouse click on textarea when interaction popup is active

弹窗激活时鼠标点击 textarea 区域不应移动光标或开始选区。

Co-Authored-By: deepseek-v4-pro <deepseek-ai@claude-code-best.win>"
```

---

### Task 4: 修复渲染——弹窗激活时 textarea 光标隐藏 + 样式变暗

**Files:**
- Modify: `peri-tui/src/ui/main_ui/mod.rs`（两处：block 样式 + cursor 隐藏）

- [ ] **Step 1: 在 textarea block 样式判断中增加弹窗条件**

找出 `render_session_column` 函数中的三个 block 样式分支（约 L281-340）。`bar_focused` 分支已实现 DarkGray 变暗效果，将弹窗激活也加入该分支。

将 `bar_focused` 条件：

```rust
    if bar_focused {
```

替换为：

```rust
    let popup_active = app.global_ui.oauth_prompt.is_some()
        || app.session_mgr.sessions[session_idx]
            .agent
            .interaction_prompt
            .is_some();

    if bar_focused || popup_active {
```

（`bar_focused` 分支的 block 样式已经使用 `Color::DarkGray` border + 默认 text style，弹窗激活时复用此效果即可。后续 `focused_id` 只读分支和 `else` 正常分支保持不变。）

- [ ] **Step 2: 在 cursor 隐藏判断中增加弹窗条件**

将 `should_hide_cursor` 判断（约 L345）：

```rust
    let should_hide_cursor = !app.focused || !is_active;
```

替换为：

```rust
    let should_hide_cursor = !app.focused || !is_active || popup_active;
```

（`popup_active` 变量已在 Step 1 中定义，可直接复用。）

- [ ] **Step 3: 编译检查**

```bash
cargo build -p peri-tui 2>&1 | head -20
```
Expected: 编译成功，无新增警告。

- [ ] **Step 4: Commit**

```bash
git add peri-tui/src/ui/main_ui/mod.rs
git commit -m "fix: hide cursor and dim textarea when interaction popup is active

弹窗激活时：
- textarea 边框变 DarkGray（复用 bar_focused 样式）
- 光标隐藏（should_hide_cursor 新增 popup_active 条件）

Co-Authored-By: deepseek-v4-pro <deepseek-ai@claude-code-best.win>"
```

---

### Task 5: 手动验证

- [ ] **Step 1: 启动 TUI 并触发 AskUser 弹窗**

```bash
cargo run -p peri-tui
```

在 Bypass 权限模式下输入任意需要 AskUser 触发的指令（如某些模型切换操作或在其他 TUI 版本中测试），或使用 issue-create skill 触发多轮提问。

- [ ] **Step 2: 验证 Paste 不再泄漏**

弹窗激活后，Cmd+V 粘贴一段文本。预期：文本不出现于底部 textarea（弹窗本身不提供粘贴功能时可静默丢弃）。

- [ ] **Step 3: 验证鼠标点击 textarea 无效**

弹窗激活后，鼠标点击底部 textarea 区域。预期：光标不移动，textarea 不进入选区模式。

- [ ] **Step 4: 验证光标已隐藏**

弹窗激活后观察底部 textarea。预期：无闪烁光标，输入框边框变暗灰色，视觉上明显"禁用"。

- [ ] **Step 5: 验证弹窗关闭后 textarea 恢复正常**

按 Enter 完成弹窗交互后。预期：底部 textarea 恢复正常光标、正常边框颜色、可正常输入和粘贴。

---

## Self-Review

1. **Spec coverage:** 覆盖 issue 中全部 5 项症状（Task 2=粘贴泄漏、Task 3=鼠标点击泄漏、Task 4=光标+样式渲染、Task 1=基础设施）。键盘输入泄漏（用户报告但代码确认 `handle_popups` 已拦截）通过 `handle_popups` 拦截 + cursor 隐藏 + dimming 三重保障缓解——即使有未发现的泄漏路径，视觉反馈也会明确告诉用户输入无效。
2. **Placeholder scan:** 无 TBD/TODO/placeholder。所有代码修改完整、可编译。
3. **Type consistency:** `is_interaction_popup_active()` 返回值 `bool`，在 Task 1 定义、Task 2-3 调用，签名一致。
