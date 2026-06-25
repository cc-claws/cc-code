# Architecture Cleanup — mod.rs 分组 + 残余拆分 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 重组 app/mod.rs（56 子模块，814 行）将模块声明分组为分类子模块降低认知负担；从 app/mod.rs 提取公共编辑工具函数；从 event/mod.rs 提取宏定义到独立文件。

**Architecture:** 创建 `app/modules.rs` 作为分类重导出枢纽：所有 56 个子模块按职责分为 panels/state/agent/ops 四组，每组一个 `modules_*.rs` 文件声明 `pub mod`，`modules.rs` 统一重导出。app/mod.rs 仅保留 `mod modules;` + App 结构体 + impl App + 工具函数。event/mod.rs 的宏提取到 `event/macros.rs`。

**Tech Stack:** Rust 2021, no new dependencies.

---

## 文件结构总览

**Create:**
```
peri-tui/src/app/
├── modules.rs          (~25 lines: unified re-exports from all groups)
├── modules_panels.rs   (~20 lines: 14 panel module declarations)
├── modules_state.rs    (~12 lines: 5 state module declarations)
├── modules_agent.rs    (~18 lines: 16 agent/ops module declarations)
├── modules_system.rs   (~12 lines: 6 system module declarations)
└── edit_utils.rs       (~195 lines: handle_edit_key + edit_display_parts + build_textarea*)

peri-tui/src/event/
└── macros.rs           (~55 lines: with_global_panels! + with_session_panels!)
```

**Modify:**
- `peri-tui/src/app/mod.rs` → module declarations replaced with `mod modules; pub use modules::*;` (814 → ~620 lines)
- `peri-tui/src/event/mod.rs` → macros moved to `macros.rs` (576 → ~520 lines)

---

### Task 1: 创建分类模块重导出 — modules_panels.rs + modules_state.rs

**Files:**
- Create: `peri-tui/src/app/modules_panels.rs`
- Create: `peri-tui/src/app/modules_state.rs`
- Modify: `peri-tui/src/app/mod.rs`

- [ ] **Step 1: 创建 modules_panels.rs — 所有 panel 子模块**

```rust
// peri-tui/src/app/modules_panels.rs
// Panel modules — all UI panel definitions
pub mod agent_panel;
pub mod config_panel;
pub mod hooks_panel;
pub mod login_panel;
pub mod memory_panel;
pub mod model_panel;
pub mod plugin_panel;
pub mod setup_wizard;
pub mod status_panel;
pub mod mcp_panel;
pub mod panel_component;
pub mod panel_list;
pub mod panel_manager;
pub mod panel_plugin;

// Panel private modules
mod panel_agent;
mod panel_config;
mod panel_hooks;
mod panel_login;
mod panel_memory;
mod panel_model;
mod panel_ops;
mod panel_status;
```

- [ ] **Step 2: 创建 modules_state.rs — 状态管理模块**

```rust
// peri-tui/src/app/modules_state.rs
// State management modules
mod global_ui_state;
mod service_registry;
pub use global_ui_state::GlobalUiState;
pub use service_registry::ServiceRegistry;

mod session_manager;
pub use session_manager::SessionManager;

mod ui_state;
pub use ui_state::UiState;

mod message_state;
pub use message_state::MessageState;
```

- [ ] **Step 3: 验证编译**

```bash
cargo build -p peri-tui 2>&1
```
Expected: Unused module warning for new files (not yet imported). Verify the content compiles when imported.

---

### Task 2: 创建 modules_agent.rs + modules_system.rs

- [ ] **Step 1: 创建 modules_agent.rs — Agent 操作模块**

```rust
// peri-tui/src/app/modules_agent.rs
// Agent communication and operation modules
mod agent_comm;
mod agent_compact;
mod agent_events_bg;
mod agent_events_oauth;
mod agent_events_plugin;
mod agent_ops;
mod agent_ops_interaction;
mod agent_render;
mod agent_submit;
mod ask_user_ops;
mod ask_user_prompt;
mod cron_ops;
mod cron_state;
mod hint_ops;
mod history_ops;
mod hitl_ops;
mod hitl_prompt;
```

- [ ] **Step 2: 创建 modules_system.rs — 系统/基础设施模块**

```rust
// peri-tui/src/app/modules_system.rs
// System infrastructure modules
mod chat_session;
mod command_system;
mod session_metadata;
pub use chat_session::ChatSession;
pub use command_system::CommandSystem;
pub use session_metadata::SessionMetadata;

mod langfuse_state;
mod oauth_prompt;
mod thread_ops;
```

- [ ] **Step 3: 创建 modules.rs — 统一重导出枢纽**

```rust
// peri-tui/src/app/modules.rs
// Unified module declarations — groups all 56 sub-modules by category.
// Each modules_*.rs declares its own set of siblings via `pub mod` or `mod`.

mod modules_panels;
mod modules_state;
mod modules_agent;
mod modules_system;

// Re-export panel types that are `pub mod` in modules_panels.rs
pub use modules_panels::{
    agent_panel, config_panel, hooks_panel, login_panel, memory_panel, model_panel,
    panel_component, panel_list, panel_manager, panel_plugin, plugin_panel, setup_wizard,
    status_panel, mcp_panel,
};

// Re-export state types
pub use modules_state::{GlobalUiState, ServiceRegistry, SessionManager, UiState, MessageState};

// Re-export system types
pub use modules_system::{ChatSession, CommandSystem, SessionMetadata};
```

---

### Task 3: 更新 app/mod.rs — 替换 56 个模块声明为单行

- [ ] **Step 1: 替换 app/mod.rs 中的模块声明区域**

In `app/mod.rs`, replace lines 1-70 (all `pub mod` and `mod` declarations and their inline `pub use`) with:

```rust
mod modules;
pub use modules::*;

// Remaining direct re-exports that don't fit the categorical grouping
pub mod agent;
pub mod events;
pub mod text_selection;
pub mod tool_display;
pub mod message_pipeline;

pub use events::AgentEvent;

// Prompt types
pub use ask_user_prompt::AskUserBatchPrompt;
pub use hitl_prompt::{HitlBatchPrompt, PendingAttachment};
pub use oauth_prompt::OAuthPrompt;
```

Wait — the modules_panels/state/agent/system files use `pub mod` and `mod` directly. But in Rust, only the parent module (`mod.rs`) can declare child modules. The `modules_*.rs` files ARE siblings of the panel files, so they CAN declare them.

Let me verify: `app/mod.rs` declares `mod modules;`. Then `app/modules.rs` declares `mod modules_panels;`. Then `app/modules_panels.rs` declares `pub mod agent_panel;`. The compiler resolves `agent_panel` as `app/agent_panel.rs` (sibling of `app/modules_panels.rs`). This works because Rust resolves relative to the file declaring `mod`, not the crate root. So `app/modules_panels.rs` declaring `pub mod agent_panel;` resolves to `app/agent_panel.rs`. ✓

- [ ] **Step 2: Remove individual re-exports that moved to modules.rs**

Remove lines that duplicate what `modules.rs` now re-exports:
```rust
// REMOVE these from app/mod.rs:
pub use global_ui_state::GlobalUiState;
pub use service_registry::ServiceRegistry;
pub use session_manager::SessionManager;
pub use ui_state::UiState;
pub use message_state::MessageState;
pub use command_system::CommandSystem;
pub use session_metadata::SessionMetadata;
pub use chat_session::ChatSession;
```

But KEEP the `pub use` for prompt types and other types that aren't in the categorical re-export groups.

- [ ] **Step 3: Build and fix import errors**

```bash
cargo build -p peri-tui 2>&1
```

Expected: Build may fail with "unresolved import" errors if some types were accessed via `crate::app::SomeType` but now the path changed. Fix by ensuring `modules.rs` re-exports all previously `pub` types.

If the `pub use` in `modules.rs` doesn't propagate correctly (Rust 2021 has specific rules about re-export visibility), add explicit `pub use modules_panels::agent_panel;` etc. in `app/mod.rs` itself.

**If re-export propagation is complex**, use a simpler approach: keep all `pub use` statements in `app/mod.rs` and only group the `mod` declarations into `modules_*.rs`. This is less clean but avoids breaking import paths:

```rust
// app/mod.rs — simplified approach
mod modules; // Only contains: mod modules_panels; mod modules_state; etc.

// Keep ALL existing pub use statements here unchanged
pub use global_ui_state::GlobalUiState;
// ... etc
```

- [ ] **Step 4: Commit**

```bash
git add peri-tui/src/app/modules*.rs peri-tui/src/app/mod.rs
git commit -m "refactor(app): group 56 module declarations into categorical modules_*.rs

app/mod.rs module declarations: 56 lines → 5 files grouped by category.
Categories: modules_panels.rs (14), modules_state.rs (5),
modules_agent.rs (16), modules_system.rs (6).
Reduces cognitive load of app/mod.rs top section."
```

---

### Task 4: 提取 edit_utils.rs — 从 app/mod.rs 分离公共工具函数

**Files:**
- Create: `peri-tui/src/app/edit_utils.rs`
- Modify: `peri-tui/src/app/mod.rs`

- [ ] **Step 1: 创建 edit_utils.rs**

Extract lines 621-814 from `app/mod.rs` (functions `ensure_cursor_visible`, `handle_edit_key`, `edit_display_parts`, `build_textarea`, `build_textarea_with_hint`) into `app/edit_utils.rs`.

```rust
// peri-tui/src/app/edit_utils.rs
use ratatui::style::Style;
use ratatui::text::Span;
use tui_textarea::TextArea;
use crate::ui::theme;

/// 确保光标在滚动视口内可见，返回调整后的 scroll_offset
pub fn ensure_cursor_visible(cursor_row: u16, scroll_offset: u16, visible_height: u16) -> u16 {
    // ... verbatim from app/mod.rs:621-632
}

/// 对单行 `String` + 光标位置统一处理编辑按键。
/// 返回 `true` 表示该按键已被消费（调用方应停止 match）。
///
/// 支持的按键：Char、Backspace、Delete、Left、Right、Home、End、
/// Ctrl+A(Home)、Ctrl+E(End)、Ctrl+K(kill to end)、Ctrl+U(kill to start)
pub fn handle_edit_key(buf: &mut String, cursor: &mut usize, input: tui_textarea::Input) -> bool {
    // ... verbatim from app/mod.rs:641-778
}

/// 将 `(buf, cursor)` 渲染为带光标块的字符串元组 `(before_cursor, after_cursor)`。
pub fn edit_display_parts(buf: &str, cursor: usize) -> (String, String) {
    // ... verbatim from app/mod.rs:782-788
}

pub fn build_textarea(disabled: bool) -> TextArea<'static> {
    // ... verbatim from app/mod.rs:790-792
}

fn build_textarea_with_hint(_disabled: bool, hint: &str) -> TextArea<'static> {
    // ... verbatim from app/mod.rs:794-814
}
```

- [ ] **Step 2: 更新 app/mod.rs**

Add `pub mod edit_utils;` to module declarations and remove the inline function definitions (lines 621-814).

Update `use` statements in `app/mod.rs` — `build_textarea` is called in `new()` and `interrupt()`. After extraction, these calls need to be `edit_utils::build_textarea(false)` or add `use edit_utils::build_textarea;`.

- [ ] **Step 3: 查找并更新所有调用点**

```bash
grep -rn 'build_textarea\|handle_edit_key\|edit_display_parts\|ensure_cursor_visible' peri-tui/src/ --include='*.rs' | grep -v edit_utils.rs | grep -v '_test.rs'
```

Update each call site to use `edit_utils::function_name` or add local `use` import.

Common call sites:
- `app/mod.rs`: `build_textarea` calls in `new()` and `set_loading()` → use `edit_utils::build_textarea`
- `app/mod.rs`: `handle_edit_key`, `edit_display_parts` → use `edit_utils::` prefix
- `agent_ops_interaction.rs` or wherever text edit functions are used

- [ ] **Step 4: Build and test**

```bash
cargo build -p peri-tui 2>&1
cargo test -p peri-tui --lib 2>&1 | tail -10
```
Expected: Build succeeds, tests pass.

- [ ] **Step 5: Commit**

```bash
git add peri-tui/src/app/edit_utils.rs peri-tui/src/app/mod.rs
# Also git add any files that had import updates
git commit -m "refactor(app): extract edit utilities to edit_utils.rs

app/mod.rs: 814 → ~620 lines. Extract: handle_edit_key,
edit_display_parts, build_textarea, ensure_cursor_visible."
```

---

### Task 5: 提取 event/macros.rs — 从 event/mod.rs 分离宏

**Files:**
- Create: `peri-tui/src/event/macros.rs`
- Modify: `peri-tui/src/event/mod.rs`

- [ ] **Step 1: 创建 event/macros.rs**

Extract lines 20-55 from `event/mod.rs` (`with_global_panels!` and `with_session_panels!` macros) into `event/macros.rs`.

```rust
// peri-tui/src/event/macros.rs

/// Executes a panel dispatch on `global_panels`, automatically handling
/// `mem::take` borrow avoidance.
#[macro_export]
macro_rules! with_global_panels {
    ($app:expr, |$pm:ident, $ctx:ident| $body:expr) => {{
        let mut $pm = std::mem::take(&mut $app.global_panels);
        let mut $ctx = $crate::app::panel_manager::PanelContext {
            services: &mut $app.services,
            session_mgr: &mut $app.session_mgr,
        };
        let result = { $body };
        $app.global_panels = $pm;
        result
    }};
}

/// Executes a panel dispatch on the active session's `session_panels`.
#[macro_export]
macro_rules! with_session_panels {
    ($app:expr, |$sp:ident, $ctx:ident| $body:expr) => {{
        let active_idx = $app.session_mgr.active;
        let mut $sp = std::mem::take(&mut $app.session_mgr.sessions[active_idx].session_panels);
        let mut $ctx = $crate::app::panel_manager::PanelContext {
            services: &mut $app.services,
            session_mgr: &mut $app.session_mgr,
        };
        let result = { $body };
        $app.session_mgr.sessions[active_idx].session_panels = $sp;
        result
    }};
}
```

- [ ] **Step 2: 更新 event/mod.rs**

Remove the macro definitions (lines 20-55), add:
```rust
mod macros;
```
(Because macros have `#[macro_export]`, they're available crate-wide regardless of `mod` visibility.)

- [ ] **Step 3: Build and test**

```bash
cargo build -p peri-tui 2>&1
```
Expected: Build succeeds. Macros are exported at crate root, `mod macros;` ensures the file is compiled.

- [ ] **Step 4: Commit**

```bash
git add peri-tui/src/event/macros.rs peri-tui/src/event/mod.rs
git commit -m "refactor(event): extract panel dispatch macros to macros.rs

event/mod.rs: 576 → ~520 lines. Extract: with_global_panels!,
with_session_panels!."
```

---

### Task 6: Verify & run pre-commit

**Files:** None created. Verification only.

- [ ] **Step 1: Full workspace build**

```bash
cargo build --workspace 2>&1
```
Expected: All crates compile clean.

- [ ] **Step 2: Run full test suite**

```bash
cargo test --workspace 2>&1 | tail -20
```
Expected: All tests pass.

- [ ] **Step 3: Run clippy + fmt**

```bash
cargo clippy --workspace -- -D warnings 2>&1
cargo fmt --all
```

- [ ] **Step 4: Verify app/mod.rs module count**

```bash
grep -c '^pub mod \|^mod ' peri-tui/src/app/mod.rs
```
Expected: ~8 direct module declarations (modules + remaining stragglers).

```bash
grep -c '^pub mod \|^mod ' peri-tui/src/app/modules_panels.rs peri-tui/src/app/modules_state.rs peri-tui/src/app/modules_agent.rs peri-tui/src/app/modules_system.rs
```
Expected: 14 + 5 + 16 + 6 = 41 across the group files.

- [ ] **Step 5: Line count summary**

```bash
echo "=== app/mod.rs ===" && wc -l peri-tui/src/app/mod.rs
echo "=== modules_*.rs ===" && wc -l peri-tui/src/app/modules*.rs
echo "=== edit_utils.rs ===" && wc -l peri-tui/src/app/edit_utils.rs
echo "=== event/* ===" && wc -l peri-tui/src/event/*.rs
```

- [ ] **Step 6: Final commit**

```bash
git add -A
git commit -m "refactor: architecture cleanup — module grouping + utils extraction

app/mod.rs: 814 → ~620 lines (+5 group files, +edit_utils.rs).
- Module declarations: 56 → 4 categorized group files
- Edit utilities extracted to edit_utils.rs (195 lines)
event/mod.rs: 576 → ~520 lines
- Macros extracted to macros.rs

All tests pass, zero behavior change."
```
