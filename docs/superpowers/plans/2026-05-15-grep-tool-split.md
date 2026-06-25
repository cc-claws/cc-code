# Grep Tool Split Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Split `peri-middlewares/src/tools/filesystem/grep.rs` (677 lines) into 3 files: `grep.rs` (tool definition + core search), `grep_args.rs` (parameter parsing), `grep_format.rs` (SearchSink formatting).

**Architecture:** Pure mechanical extraction. `grep_args.rs` extracts `ParsedArgs`, `OutputMode`, `GrepInput`, `to_parsed_args()`, and `type_to_glob()`. `grep_format.rs` extracts `SearchSink` and its `Sink` impl. `grep.rs` retains `GrepTool` + `BaseTool` impl + `execute_search()` + tests. All items are `pub(crate)` within the `filesystem` module.

**Tech Stack:** Rust, `ripgrep` crates (`grep-regex`, `grep-searcher`, `ignore`), `serde_json`, `tokio`, `glob`.

---

## Problem Summary

`grep.rs` has 3 distinct layers in one file:

| Layer | Lines | Items |
|-------|-------|-------|
| Tool interface | 1–24, 26–56, 481–672 | `GrepTool` struct, description, `BaseTool` impl, `invoke` |
| Parameter parsing | 58–191 | `ParsedArgs`, `OutputMode`, `GrepInput`, `to_parsed_args()`, `type_to_glob()` |
| Formatting + core search | 194–479 | `SearchSink` + `Sink` impl, `execute_search()` |

Test file (`grep_test.rs`, 489 lines) is already separate and included via `#[path = "grep_test.rs"]` at lines 674–676. No test changes needed — all 24 tests exercise the public `GrepTool::invoke()` API.

---

## File Structure After Split

```
tools/filesystem/
├── mod.rs             # unchanged (pub mod grep; pub use grep::GrepTool;)
├── grep.rs            # ~340 lines: GrepTool + BaseTool impl + execute_search() + tests
├── grep_args.rs       # ~140 lines: ParsedArgs + OutputMode + GrepInput + type_to_glob()
└── grep_format.rs     # ~110 lines: SearchSink + Sink impl
```

---

### Task 1: Create `grep_args.rs` — extract parameter parsing types

**Files:**
- Create: `peri-middlewares/src/tools/filesystem/grep_args.rs`
- Modify: `peri-middlewares/src/tools/filesystem/grep.rs` (remove lines 58–191)

- [ ] **Step 1: Create `grep_args.rs`**

Create `/Users/konghayao/code/ai/perihelion/peri-middlewares/src/tools/filesystem/grep_args.rs` with these items, copied verbatim from `grep.rs`:

```
Lines to copy from grep.rs:
  58–75   ParsedArgs struct
  77–83   OutputMode enum
  85–104  GrepInput struct
 106–130  type_to_glob() function
 132–191  impl GrepInput { to_parsed_args() }
```

No import changes needed — these types only use `std` types. Add `use std::cell::Cell;` at top if any item references `Cell` (they don't — ParsedArgs uses String/Path, GrepInput uses String/bool).

- [ ] **Step 2: Add `pub(crate)` visibility in `grep_args.rs`**

Change all declarations from private to `pub(crate)` so `grep.rs` can import them:

```rust
pub(crate) struct ParsedArgs { ... }
pub(crate) enum OutputMode { ... }
pub(crate) struct GrepInput { ... }
pub(crate) fn type_to_glob(type_name: &str) -> Vec<&'static str> { ... }
// impl GrepInput stays, to_parsed_args is pub(crate)
```

- [ ] **Step 3: Remove parameter types from `grep.rs`**

Delete lines 58–191 from `grep.rs`. Add import at the top:

```rust
use super::grep_args::{GrepInput, OutputMode, ParsedArgs, type_to_glob};
```

- [ ] **Step 4: Build to verify**

```bash
cargo build -p peri-middlewares 2>&1
```
Expected: Success. If "unresolved import" errors appear, verify `pub(crate)` on all items in `grep_args.rs`.

- [ ] **Step 5: Commit**

```bash
git add peri-middlewares/src/tools/filesystem/grep_args.rs peri-middlewares/src/tools/filesystem/grep.rs
git commit -m "refactor(filesystem): extract grep_args module from grep.rs

Move ParsedArgs, OutputMode, GrepInput, type_to_glob(), and
to_parsed_args() to grep_args.rs. All items pub(crate).

Co-Authored-By: deepseek-v4-pro <deepseek-ai@claude-code.win>"
```

---

### Task 2: Create `grep_format.rs` — extract SearchSink formatting

**Files:**
- Create: `peri-middlewares/src/tools/filesystem/grep_format.rs`
- Modify: `peri-middlewares/src/tools/filesystem/grep.rs` (remove lines 194–296)

- [ ] **Step 1: Create `grep_format.rs`**

Create `/Users/konghayao/code/ai/perihelion/peri-middlewares/src/tools/filesystem/grep_format.rs` with these items, copied verbatim from `grep.rs`:

```
Lines to copy from grep.rs:
 194–207  SearchSink struct
 209–296  impl Sink for SearchSink { matched(), context() }
```

Add imports at top:
```rust
use std::cell::Cell;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use grep::searcher::{Searcher, Sink, SinkContext, SinkContextKind, SinkMatch};

use super::grep_args::OutputMode;
```

- [ ] **Step 2: Change `SearchSink` visibility to `pub(crate)`**

```rust
pub(crate) struct SearchSink { ... }
```

- [ ] **Step 3: Remove SearchSink from `grep.rs`**

Delete lines 194–296 from `grep.rs`. Add import:
```rust
use super::grep_format::SearchSink;
```

Also check: the `execute_search()` function in `grep.rs` constructs `SearchSink` directly (line 420–432). Verify the type is accessible.

- [ ] **Step 4: Clean up unused imports in `grep.rs`**

After removing `SearchSink` and `Sink` impl, the following imports from `grep.rs` may become unused:
```rust
use std::cell::Cell;                           // only used by SearchSink → REMOVE
use std::sync::atomic::{..., Ordering};        // still used by execute_search
use std::sync::{Arc, Mutex};                   // still used by execute_search
use grep::searcher::{Sink, SinkContext, SinkContextKind, SinkMatch};  // only used by SearchSink → REMOVE
```

Remove: `use std::cell::Cell;`, `Sink`, `SinkContext`, `SinkContextKind`, `SinkMatch` from the grep/searcher import.

- [ ] **Step 5: Build to verify**

```bash
cargo build -p peri-middlewares 2>&1
```
Expected: Success. If compilation fails, check:
- Unused import warnings → remove the unused imports
- "cannot find type SearchSink" → verify `use super::grep_format::SearchSink` is present

- [ ] **Step 6: Commit**

```bash
git add peri-middlewares/src/tools/filesystem/grep_format.rs peri-middlewares/src/tools/filesystem/grep.rs
git commit -m "refactor(filesystem): extract grep_format module from grep.rs

Move SearchSink and its Sink impl to grep_format.rs.
Clean up now-unused imports in grep.rs.

Co-Authored-By: deepseek-v4-pro <deepseek-ai@claude-code.win>"
```

---

### Task 3: Final verification

- [ ] **Step 1: Run full build**

```bash
cargo build 2>&1
```
Expected: All crates compile.

- [ ] **Step 2: Run grep tests**

```bash
cargo test -p peri-middlewares grep 2>&1
```
Expected: All 24 grep tests pass. No regressions.

- [ ] **Step 3: Run full test suite**

```bash
cargo test 2>&1
```
Expected: All tests pass.

- [ ] **Step 4: Verify file sizes**

```bash
wc -l peri-middlewares/src/tools/filesystem/grep.rs peri-middlewares/src/tools/filesystem/grep_args.rs peri-middlewares/src/tools/filesystem/grep_format.rs
```
Expected approximate: `grep.rs` ~340 lines, `grep_args.rs` ~140 lines, `grep_format.rs` ~110 lines.

---

## Self-Review Checklist

1. **Spec coverage:** All 3 files from the issue's target structure created ✓
2. **Placeholder scan:** No TODOs — every file has exact copy regions and import lists ✓
3. **Type consistency:** `GrepTool` public API unchanged; `GrepInput`, `ParsedArgs`, `SearchSink` are `pub(crate)` within `filesystem` module ✓
4. **Import cleanup:** All unused imports specifically listed for removal ✓
5. **Test isolation:** Test file (`grep_test.rs`) untouched, tests exercise public `invoke()` API only ✓
