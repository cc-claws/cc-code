# Plugin Panel 拆分 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将 3 个超阈值文件（panels/plugin.rs 1167行、plugin_panel/handlers.rs 841行、plugin_panel/mod.rs 812行）拆分为聚焦子模块，所有文件控制在 500 行以内。

**Architecture:** panels/plugin.rs（9 个渲染函数）按视图类型拆分为 `plugin_render/` 子目录。handlers.rs（14 个方法）按操作流程（delete/discover/installed/marketplace/install/persistence）拆分为 `plugin_handlers/`。mod.rs 的 helper methods 随 handlers 一并移动。零行为改变——纯文件拆分 + re-export。

**Tech Stack:** Rust 2021, ratatui, no new dependencies.

---

## 文件结构总览

**Create:**
```
peri-tui/src/ui/main_ui/panels/plugin_render/
├── mod.rs          (~60 lines:  re-exports + detail_kv_line + truncate_display)
├── list.rs         (~420 lines: render_list)
├── detail.rs       (~170 lines: render_detail)
├── discover_detail.rs (~135 lines: render_discover_detail)
├── discover_search.rs (~45 lines: render_discover_search_box)
├── discover_list.rs  (~235 lines: render_discover_list)
└── add_marketplace.rs (~110 lines: render_add_marketplace)

peri-tui/src/app/plugin_panel/plugin_handlers/
├── mod.rs           (~20 lines: re-exports)
├── delete.rs        (~130 lines: handle_confirm_delete + handle_marketplace_confirm_delete)
├── discover_search.rs (~95 lines: handle_discover_searching)
├── discover_detail.rs (~115 lines: handle_discover_detail)
├── installed_detail.rs (~40 lines: handle_installed_detail)
├── installed_list.rs  (~55 lines: handle_installed_list)
├── discover_list.rs   (~55 lines: handle_discover_list)
├── marketplace.rs     (~170 lines: handle_marketplaces_list + handle_marketplace_add)
├── install.rs         (~125 lines: spawn_install_current + do_detail_action)
└── persistence.rs     (~120 lines: persist_enabled_state + persist_marketplace_delete + persist_marketplace_add)
```

**Modify:**
- `peri-tui/src/ui/main_ui/panels/plugin.rs` → 缩减为仅 re-export（~15 lines）
- `peri-tui/src/app/plugin_panel/handlers.rs` → 缩减为仅 re-export（~17 lines）
- `peri-tui/src/app/plugin_panel/mod.rs` → 移出 handlers 中的 helper 方法，更新 imports（-400 lines → ~410 lines）

---

### Task 1: 拆分 panels/plugin.rs — list.rs + detail.rs

**Files:**
- Create: `peri-tui/src/ui/main_ui/panels/plugin_render/mod.rs`
- Create: `peri-tui/src/ui/main_ui/panels/plugin_render/list.rs`
- Create: `peri-tui/src/ui/main_ui/panels/plugin_render/detail.rs`
- Modify: `peri-tui/src/ui/main_ui/panels/plugin.rs`

- [ ] **Step 1: 创建 plugin_render/mod.rs — 占位 re-export**

```rust
// peri-tui/src/ui/main_ui/panels/plugin_render/mod.rs
pub mod list;
pub mod detail;

// Shared helpers
use crate::app::plugin_panel::PluginPanel;
use ratatui::text::Line;

pub(crate) fn detail_kv_line<'a>(key: &str, value: &str) -> Line<'a> {
    use ratatui::text::Span;
    use ratatui::style::{Style, Modifier};
    Line::from(vec![
        Span::styled(format!("{key}: "), Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(value.to_string()),
    ])
}

pub(crate) fn truncate_display(s: &str, max_width: usize) -> String {
    if s.chars().count() > max_width {
        let truncated: String = s.chars().take(max_width.saturating_sub(3)).collect();
        format!("{truncated}...")
    } else {
        s.to_string()
    }
}
```

- [ ] **Step 2: 创建 plugin_render/list.rs — 复制 render_list (lines 48-463)**

Read `peri-tui/src/ui/main_ui/panels/plugin.rs` lines 48-463, extract `fn render_list` and its complete body. This function handles Installed/Enabled/Disabled list views. Write it to `plugin_render/list.rs` with proper imports:

```rust
// peri-tui/src/ui/main_ui/panels/plugin_render/list.rs
use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, BorderType, Borders, Paragraph},
    Frame,
};
use peri_widgets::{BorderedPanel, ScrollState, ScrollableArea};
use crate::app::plugin_panel::{PluginPanel, PluginPanelView};
use crate::app::App;
use crate::ui::theme;

use super::PluginPanel;

pub(crate) fn render_list(f: &mut Frame, panel: &PluginPanel, app: &mut App, area: Rect) {
    // ... entire function body verbatim
}
```

- [ ] **Step 3: 创建 plugin_render/detail.rs — 复制 render_detail (lines 464-636)**

```rust
// peri-tui/src/ui/main_ui/panels/plugin_render/detail.rs
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph},
    Frame,
};
use peri_widgets::{BorderedPanel, ScrollableArea};
use crate::app::plugin_panel::PluginPanel;
use crate::app::App;
use crate::ui::theme;

use super::detail_kv_line;

pub(crate) fn render_detail(f: &mut Frame, panel: &PluginPanel, app: &mut App, area: Rect) {
    // ... entire function body verbatim
}
```

- [ ] **Step 4: 修改 plugin.rs — 裁剪为 dispatcher + 保留 discover 渲染（暂不移动）**

将 `plugin.rs` 中的 `render_list` 和 `render_detail` 函数体替换为对 `plugin_render` 的转发：

```rust
// peri-tui/src/ui/main_ui/panels/plugin.rs
// (保留顶部 import，删除 render_list 和 render_detail 函数)

mod plugin_render;
use plugin_render::{list, detail};

// In render_plugin_panel dispatcher's render_list branch:
// Change: render_list(f, panel, app, area);
// To:     plugin_render::list::render_list(f, panel, app, area);

// In render_plugin_panel's render_detail branch:
// Change: render_detail(f, panel, app, area);
// To:     plugin_render::detail::render_detail(f, panel, app, area);
```

- [ ] **Step 5: Build and verify**

```bash
cargo build -p peri-tui 2>&1
```
Expected: Build succeeds, zero errors.

- [ ] **Step 6: Commit**

```bash
git add peri-tui/src/ui/main_ui/panels/plugin_render/ peri-tui/src/ui/main_ui/panels/plugin.rs
git commit -m "refactor(plugin): extract render_list + render_detail to plugin_render/

Split panels/plugin.rs — moves list and detail render functions to
plugin_render/{list, detail}.rs. panel/plugin.rs reduced from ~1167 to ~750 lines."
```

---

### Task 2: 拆分 panels/plugin.rs — discover 子页面 + add_marketplace

**Files:**
- Create: `peri-tui/src/ui/main_ui/panels/plugin_render/discover_detail.rs`
- Create: `peri-tui/src/ui/main_ui/panels/plugin_render/discover_search.rs`
- Create: `peri-tui/src/ui/main_ui/panels/plugin_render/discover_list.rs`
- Create: `peri-tui/src/ui/main_ui/panels/plugin_render/add_marketplace.rs`
- Modify: `peri-tui/src/ui/main_ui/panels/plugin_render/mod.rs`
- Modify: `peri-tui/src/ui/main_ui/panels/plugin.rs`

- [ ] **Step 1: 创建 discover_detail.rs — 复制 render_discover_detail (lines 637-770)**

```rust
// peri-tui/src/ui/main_ui/panels/plugin_render/discover_detail.rs
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};
use peri_widgets::{BorderedPanel, ScrollableArea};
use crate::app::plugin_panel::{DiscoverDetailAction, PluginPanel};
use crate::app::App;
use crate::ui::theme;
use super::detail_kv_line;

pub(crate) fn render_discover_detail(f: &mut Frame, panel: &PluginPanel, app: &mut App, area: Rect) {
    // ... entire function body verbatim
}
```

- [ ] **Step 2: 创建 discover_search.rs — 复制 render_discover_search_box (lines 771-812)**

- [ ] **Step 3: 创建 discover_list.rs — 复制 render_discover_list (lines 813-1041)**

- [ ] **Step 4: 创建 add_marketplace.rs — 复制 render_add_marketplace (lines 1061-1167)**

- [ ] **Step 5: 更新 plugin_render/mod.rs — 添加新子模块声明**

```rust
pub mod list;
pub mod detail;
pub mod discover_detail;
pub mod discover_search;
pub mod discover_list;
pub mod add_marketplace;
// ... keep shared helpers
```

- [ ] **Step 6: 更新 plugin.rs — 替换剩余函数为转发调用，删除原地定义**

将 `plugin.rs` 更新为仅保留 dispatcher `render_plugin_panel`，所有内部渲染函数替换为 `plugin_render::` 路径调用。删除原 `detail_kv_line`、`truncate_display`（已移至 plugin_render/mod.rs）。

`plugin.rs` 最终状态（~15 行）：
```rust
use ratatui::{layout::Rect, Frame};
use crate::app::plugin_panel::PluginPanel;
use crate::app::App;

mod plugin_render;

pub fn render_plugin_panel(f: &mut Frame, panel: &PluginPanel, app: &mut App, area: Rect) {
    if panel.add_marketplace_active {
        plugin_render::add_marketplace::render_add_marketplace(f, panel, app, area);
        return;
    }
    if panel.discover_detail_index.is_some() {
        plugin_render::discover_detail::render_discover_detail(f, panel, app, area);
    } else if panel.is_detail() {
        plugin_render::detail::render_detail(f, panel, app, area);
    } else if panel.view == crate::app::plugin_panel::PluginPanelView::Discover {
        plugin_render::discover_list::render_discover_list(f, panel, app, area);
    } else {
        plugin_render::list::render_list(f, panel, app, area);
    }
}
```

- [ ] **Step 7: Build and verify**

```bash
cargo build -p peri-tui 2>&1
```
Expected: Build succeeds.

- [ ] **Step 8: Commit**

```bash
git add peri-tui/src/ui/main_ui/panels/plugin_render/ peri-tui/src/ui/main_ui/panels/plugin.rs
git commit -m "refactor(plugin): extract remaining render functions to plugin_render/

Finalizes panels/plugin.rs split — discover/search/marketplace render
functions moved to plugin_render/{discover_detail,discover_search,discover_list,add_marketplace}.rs.
panels/plugin.rs: 1167 → ~15 lines."
```

---

### Task 3: 拆分 plugin_panel/handlers.rs — delete + discover

**Files:**
- Create: `peri-tui/src/app/plugin_panel/plugin_handlers/mod.rs`
- Create: `peri-tui/src/app/plugin_panel/plugin_handlers/delete.rs`
- Create: `peri-tui/src/app/plugin_panel/plugin_handlers/discover_search.rs`
- Create: `peri-tui/src/app/plugin_panel/plugin_handlers/discover_detail.rs`
- Create: `peri-tui/src/app/plugin_panel/plugin_handlers/discover_list.rs`
- Modify: `peri-tui/src/app/plugin_panel/handlers.rs`
- Modify: `peri-tui/src/app/plugin_panel/mod.rs`

- [ ] **Step 1: 创建 plugin_handlers/mod.rs**

```rust
// peri-tui/src/app/plugin_panel/plugin_handlers/mod.rs
pub mod delete;
pub mod discover_search;
pub mod discover_detail;
pub mod discover_list;
```

- [ ] **Step 2: 创建 delete.rs — 复制 handle_confirm_delete (lines 12-67) + handle_marketplace_confirm_delete (lines 539-575)**

```rust
// peri-tui/src/app/plugin_panel/plugin_handlers/delete.rs
use tui_textarea::{Input, Key};
use peri_middlewares::plugin::InstallScope;
use super::super::super::panel_manager::{EventResult, PanelContext};
use super::super::types::*;
use super::super::PluginPanel;

impl PluginPanel {
    pub(super) fn handle_confirm_delete(
        &mut self,
        input: Input,
        ctx: &mut PanelContext<'_>,
    ) -> EventResult {
        // ... verbatim from handlers.rs:12-67
    }

    pub(super) fn handle_marketplace_confirm_delete(
        &mut self,
        input: Input,
        ctx: &mut PanelContext<'_>,
    ) -> EventResult {
        // ... verbatim from handlers.rs:539-575
    }
}
```

- [ ] **Step 3: 创建 discover_search.rs — handle_discover_searching (lines 68-161)**

- [ ] **Step 4: 创建 discover_detail.rs — handle_discover_detail (lines 162-273)**

- [ ] **Step 5: 创建 discover_list.rs — handle_discover_list (lines 361-411)**

- [ ] **Step 6: 更新 handlers.rs — 替换为 re-export，删除 relocated impl blocks**

```rust
// peri-tui/src/app/plugin_panel/handlers.rs
// — after handlers became a directory, this becomes the mod.rs entry
// Replace all content with:
pub mod plugin_handlers;
pub use plugin_handlers::*;

// Keep remaining impl PluginPanel methods NOT moved yet:
// - handle_installed_detail, handle_installed_list
// - handle_marketplaces_list, handle_marketplace_add
// - spawn_install_current, do_detail_action
// - persist_enabled_state, persist_marketplace_delete, persist_marketplace_add
```

Wait — the current `handlers.rs` is a file, not a directory. We need to convert it to `handlers/mod.rs` + sub-files. Let me revise: since the project uses the `mod.rs` convention, we convert:

```
Before:
  plugin_panel/handlers.rs  (841 lines)

After:
  plugin_panel/handlers/
  ├── mod.rs                (remaining impl PluginPanel methods, ~530 lines)
  ├── plugin_handlers/
  │   ├── mod.rs            (re-exports)
  │   ├── delete.rs
  │   ├── discover_search.rs
  │   ├── discover_detail.rs
  │   └── discover_list.rs
```

Actually, the project uses `plugin_panel/handlers.rs` as a module file. To add sub-modules, we need to convert to directory. But that changes the import path for everything that does `use super::handlers`. Let me check what imports from handlers.rs.

The cleaner approach: Move methods TO handlers.rs as a flat file, or use `#[path]` attribute. Actually the simplest approach without breaking imports:

1. Create `plugin_panel/handlers/` directory
2. Move current `handlers.rs` → `handlers/mod.rs` (rename)
3. Add sub-file declarations in `handlers/mod.rs`
4. No import changes needed — `mod handlers;` in `plugin_panel/mod.rs` auto-discovers `handlers/mod.rs`

But `mod handlers;` exists in `plugin_panel/mod.rs:16`. This would work fine with the directory approach.

Let me restructure the plan. We should do directory conversion first.

- [ ] **Step 1a: Convert handlers.rs to handlers/ directory**

```bash
mkdir peri-tui/src/app/plugin_panel/handlers
mv peri-tui/src/app/plugin_panel/handlers.rs peri-tui/src/app/plugin_panel/handlers/mod.rs
```

- [ ] **Step 1b: Verify build**

```bash
cargo build -p peri-tui 2>&1
```
Rust module system auto-discovers `handlers/mod.rs` when `mod handlers;` is declared. Build should succeed with zero changes.

- [ ] **Step 2: Create plugin_handlers/ sub-directory with first batch**

```bash
mkdir peri-tui/src/app/plugin_panel/handlers/plugin_handlers
```

Create `handlers/plugin_handlers/mod.rs`:
```rust
pub mod delete;
pub mod discover_search;
pub mod discover_detail;
pub mod discover_list;
```

Create each sub-file, extract the corresponding methods from `handlers/mod.rs`, remove their definitions from `handlers/mod.rs`, replace with `pub mod plugin_handlers;` declaration.

**Note:** The `impl PluginPanel` in handlers/mod.rs currently has these methods. Each extracted file must contain its own `impl PluginPanel { ... }` block. Rust allows multiple `impl` blocks for the same type across files as long as they're in the same crate.

- [ ] **Step 3: Build and verify after each file creation**

```bash
cargo build -p peri-tui 2>&1
```

- [ ] **Step 4: Commit**

```bash
git add peri-tui/src/app/plugin_panel/handlers/
git rm peri-tui/src/app/plugin_panel/handlers.rs
git commit -m "refactor(plugin): extract delete+discover handlers to plugin_handlers/

Convert handlers.rs → handlers/ directory with plugin_handlers/ sub-directory.
Extract: delete (confirm_delete + marketplace_confirm_delete),
discover search/detail/list handlers."
```

---

### Task 4: 拆分 handlers/plugin_handlers — installed + marketplace + persistence

**Files:**
- Create: `peri-tui/src/app/plugin_panel/handlers/plugin_handlers/installed_detail.rs`
- Create: `peri-tui/src/app/plugin_panel/handlers/plugin_handlers/installed_list.rs`
- Create: `peri-tui/src/app/plugin_panel/handlers/plugin_handlers/marketplace.rs`
- Create: `peri-tui/src/app/plugin_panel/handlers/plugin_handlers/install.rs`
- Create: `peri-tui/src/app/plugin_panel/handlers/plugin_handlers/persistence.rs`
- Modify: `peri-tui/src/app/plugin_panel/handlers/plugin_handlers/mod.rs`
- Modify: `peri-tui/src/app/plugin_panel/handlers/mod.rs`

- [ ] **Step 1: Create remaining handler files**

For each handler group, extract the method from `handlers/mod.rs` into its own file. Each file wraps methods in `impl PluginPanel { ... }`:

**installed_detail.rs** — `handle_installed_detail` (lines 274-307 in original)

**installed_list.rs** — `handle_installed_list` (lines 308-360 in original)

**marketplace.rs** — `handle_marketplaces_list` (lines 412-538) + `handle_marketplace_add` (lines 576-620)

**install.rs** — `spawn_install_current` (lines 621-656) + `do_detail_action` (lines 657-685)

**persistence.rs** — `persist_enabled_state` (lines 686-700) + `persist_marketplace_delete` (lines 701-744) + `persist_marketplace_add` (lines 745+)

- [ ] **Step 2: Update plugin_handlers/mod.rs**

```rust
pub mod delete;
pub mod discover_search;
pub mod discover_detail;
pub mod discover_list;
pub mod installed_detail;
pub mod installed_list;
pub mod marketplace;
pub mod install;
pub mod persistence;
```

- [ ] **Step 3: Build and verify**

```bash
cargo build -p peri-tui 2>&1
```

- [ ] **Step 4: Final check — handlers/mod.rs line count**

```bash
wc -l peri-tui/src/app/plugin_panel/handlers/mod.rs
```
Expected: < 30 lines (just `pub mod plugin_handlers;` + declaration).

- [ ] **Step 5: Verify plugin_panel/mod.rs still < 500 lines by moving helper methods**

Check if `plugin_panel/mod.rs` has helper methods that belong in handler files. Move any remaining `impl PluginPanel` helper methods from `mod.rs` to appropriate handler sub-files.

- [ ] **Step 6: Commit**

```bash
git add peri-tui/src/app/plugin_panel/handlers/
git commit -m "refactor(plugin): extract installed/marketplace/install/persistence handlers

Extract remaining handler methods from handlers/mod.rs to plugin_handlers/:
installed_detail, installed_list, marketplace, install, persistence.
handlers/mod.rs: 841 → ~28 lines."
```

---

### Task 5: Verify & run pre-commit

**Files:** None created. Verification only.

- [ ] **Step 1: Full workspace build**

```bash
cargo build --workspace 2>&1
```
Expected: All crates compile clean.

- [ ] **Step 2: Run tests**

```bash
cargo test -p peri-tui --lib 2>&1 | tail -20
```
Expected: All tests pass (especially `plugin_panel_test`).

- [ ] **Step 3: Run clippy**

```bash
cargo clippy -p peri-tui -- -D warnings 2>&1
```
Expected: No new warnings.

- [ ] **Step 4: Run rustfmt**

```bash
cargo fmt --all
```

- [ ] **Step 5: Line count verification**

```bash
echo "=== Plugin Panel final sizes ==="
wc -l peri-tui/src/ui/main_ui/panels/plugin.rs
wc -l peri-tui/src/app/plugin_panel/handlers/mod.rs
wc -l peri-tui/src/app/plugin_panel/mod.rs
echo "--- plugin_render/ ---"
wc -l peri-tui/src/ui/main_ui/panels/plugin_render/*.rs
echo "--- plugin_handlers/ ---"
wc -l peri-tui/src/app/plugin_panel/handlers/plugin_handlers/*.rs
```

Expected: All files ≤ 500 lines, mod.rs files ≤ 500 lines.

- [ ] **Step 6: Final commit**

```bash
git add -A
git commit -m "refactor(plugin): finalize plugin panel split

Summary:
- panels/plugin.rs: 1167 → 15 lines (dispatcher only)
- plugin_render/: 7 files, max 420 lines
- handlers/: 841 → 28 lines (re-export only)
- plugin_handlers/: 9 files, max 170 lines
- plugin_panel/mod.rs: 812 → ~410 lines after helper moves

All tests pass, zero behavior change."
```
