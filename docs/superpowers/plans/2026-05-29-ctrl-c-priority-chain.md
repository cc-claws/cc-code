# Ctrl+C 优先级链 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将 Ctrl+C 行为改为优先级链：清空输入框 → 中断 Agent → 进入 quit-pending → 退出程序。

**Architecture:** 修改 `handle_ctrl_c` 函数，将当前的两层 if-else（loading / quit-pending）改为三步优先级链。最高优先级检查 textarea 是否有内容（`lines()` 非空），有则调用 `select_all()` + `cut()` 清空。`interrupt()` 和 quit-pending 逻辑不变，仅在输入框为空时执行。

**Tech Stack:** Rust, tui-textarea-2 (`TextArea::select_all()`, `TextArea::cut()`, `TextArea::lines()`)

---

### Task 1: 重写 handle_ctrl_c 为优先级链

**Files:**
- Modify: `peri-tui/src/event/keyboard/normal_keys.rs:366-384`

- [ ] **Step 1: 重写 handle_ctrl_c 函数**

将 `normal_keys.rs:366-384` 的 `handle_ctrl_c` 函数替换为：

```rust
fn handle_ctrl_c(app: &mut App) -> Option<Action> {
    let session = &mut app.session_mgr.sessions[app.session_mgr.active];

    // 优先级 1: 输入框有内容 → 清空输入框
    if session
        .ui
        .textarea
        .lines()
        .iter()
        .any(|l| !l.is_empty())
    {
        session.ui.textarea.move_cursor(tui_textarea::CursorMove::Head);
        session.ui.textarea.select_all();
        session.ui.textarea.cut();
        app.global_ui.quit_pending_since = None;
        return None;
    }

    // 优先级 2: Agent 运行中 → 中断 agent
    if session.ui.loading {
        app.interrupt();
        app.global_ui.quit_pending_since = None;
        return None;
    }

    // 优先级 3: Agent 未运行 → quit-pending 逻辑
    if let Some(since) = app.global_ui.quit_pending_since {
        if since.elapsed() < std::time::Duration::from_secs(2) {
            return Some(Action::Quit);
        } else {
            app.global_ui.quit_pending_since = Some(std::time::Instant::now());
        }
    } else {
        app.global_ui.quit_pending_since = Some(std::time::Instant::now());
    }
    None
}
```

**设计决策**：用 `select_all()` + `cut()` 而非 `clear()`——这样用户可以通过 Ctrl+V（paste）恢复误清空的内容。

**注意**：`tui_textarea::CursorMove` 路径需要确认——当前文件导入是 `use tui_textarea::{Input, Key}`，`CursorMove` 通过完整路径 `tui_textarea::CursorMove::Head` 引用即可，无需额外 import。

- [ ] **Step 2: 编译验证**

Run: `cargo build -p peri-tui 2>&1 | head -30`
Expected: 编译通过，无错误

- [ ] **Step 3: Commit**

```bash
git add peri-tui/src/event/keyboard/normal_keys.rs
git commit -m "feat(tui): Ctrl+C priority chain - clear input first, then interrupt, then quit"
```

---

### Task 2: 编写单元测试

**Files:**
- Modify: `peri-tui/src/event/keyboard/normal_keys.rs`（末尾添加 `#[cfg(test)]` 模块）

- [ ] **Step 1: 在 normal_keys.rs 末尾添加测试模块**

`handle_ctrl_c` 是私有函数，测试必须放在同文件的 `#[cfg(test)]` 模块中。在 `normal_keys.rs` 末尾添加：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::build_textarea;
    use crate::event::Action;

    /// 辅助函数：创建 headless App 并重置 textarea
    async fn make_app() -> crate::app::App {
        let (app, _) = crate::app::App::new_headless(80, 24).await;
        app
    }

    #[tokio::test]
    async fn test_ctrl_c_clears_textarea_when_has_content() {
        let mut app = make_app().await;
        app.session_mgr.sessions[app.session_mgr.active].ui.textarea =
            build_textarea(false);
        app.session_mgr.sessions[app.session_mgr.active]
            .ui
            .textarea
            .insert_str("hello world");

        let result = handle_ctrl_c(&mut app);

        assert!(result.is_none(), "有内容时 Ctrl+C 不应返回 Quit");
        let lines = app.session_mgr.sessions[app.session_mgr.active]
            .ui
            .textarea
            .lines()
            .to_vec();
        assert!(
            lines.iter().all(|l| l.is_empty()),
            "清空后 textarea 应为空，实际: {:?}",
            lines
        );
        assert!(
            app.global_ui.quit_pending_since.is_none(),
            "清空输入框不应进入 quit-pending"
        );
    }

    #[tokio::test]
    async fn test_ctrl_c_interrupts_agent_when_textarea_empty() {
        let mut app = make_app().await;
        // textarea 默认为空，设置 loading 模拟 agent 运行
        app.set_loading(true);

        let result = handle_ctrl_c(&mut app);

        assert!(result.is_none(), "中断 agent 不应返回 Quit");
        // loading 状态由 interrupt() 异步清理，此处仅验证不触发 quit-pending
        assert!(
            app.global_ui.quit_pending_since.is_none(),
            "中断 agent 不应进入 quit-pending"
        );
    }

    #[tokio::test]
    async fn test_ctrl_c_enters_quit_pending_when_idle_and_empty() {
        let mut app = make_app().await;
        // textarea 为空，loading 为 false

        let result = handle_ctrl_c(&mut app);

        assert!(result.is_none(), "第一次 Ctrl+C 不应返回 Quit");
        assert!(
            app.global_ui.quit_pending_since.is_some(),
            "空闲时应进入 quit-pending"
        );

        // 第二次立即按下 → 退出
        let result = handle_ctrl_c(&mut app);
        assert!(
            matches!(result, Some(Action::Quit)),
            "2 秒内第二次 Ctrl+C 应返回 Quit"
        );
    }

    #[tokio::test]
    async fn test_ctrl_c_does_not_quit_when_textarea_has_content() {
        let mut app = make_app().await;
        // 先进入 quit-pending
        let _ = handle_ctrl_c(&mut app);
        assert!(app.global_ui.quit_pending_since.is_some());

        // 输入内容后再按 Ctrl+C
        app.session_mgr.sessions[app.session_mgr.active]
            .ui
            .textarea
            .insert_str("some text");
        let result = handle_ctrl_c(&mut app);

        assert!(result.is_none(), "有内容时不应退出");
        assert!(
            app.global_ui.quit_pending_since.is_none(),
            "清空输入框应重置 quit-pending"
        );
    }
}
```

- [ ] **Step 2: 运行测试验证**

Run: `cargo test -p peri-tui --lib -- tests:: 2>&1 | tail -20`
Expected: 4 个新测试全部 PASS

- [ ] **Step 3: Commit**

```bash
git add peri-tui/src/event/keyboard/normal_keys.rs
git commit -m "test(tui): add unit tests for Ctrl+C priority chain behavior"
```

---

### Task 3: 更新 i18n 状态栏提示

**Files:**
- Modify: `peri-tui/src/ui/main_ui/status_bar.rs`

- [ ] **Step 1: 检查状态栏 Ctrl+C 提示文案**

当前状态栏在空闲时显示 `Ctrl+C` → "关闭"（来自 `status_bar.rs:427`）。由于行为已变更（空闲时第一次 Ctrl+C 不再是"关闭"，而是"进入 quit-pending"），需要确认是否需要更新提示文案。

Run: `grep -n "key-close\|Ctrl.*C" peri-tui/src/ui/main_ui/status_bar.rs`

如果提示文案使用了 i18n key `key-close`，检查 `peri-tui/src/i18n/locales/` 中的翻译是否仍然准确。如果文案暗示"单次 Ctrl+C 关闭"，需要改为更准确的描述（如"按两次退出"或保持不变，因为 quit-pending 模式下已有倒计时提示）。

**注意**：这只是文案审查，可能不需要修改。

- [ ] **Step 2: Commit（如有修改）**

```bash
git add peri-tui/src/ui/main_ui/status_bar.rs
git commit -m "docs(tui): update Ctrl+C hint in status bar"
```
