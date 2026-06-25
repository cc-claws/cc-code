# Web Middleware Split Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Split `peri-middlewares/src/middleware/web.rs` (559 lines) into 4 files: `web.rs` (WebMiddleware glue), `web_common.rs` (SSRF guard + shared utils), `web_fetch.rs` (WebFetchTool), `web_search.rs` (WebSearchTool + Bing parsing).

**Architecture:** Pure mechanical extraction. `web_common.rs` holds SSRF validation and content processing utilities used by both tools. `web_fetch.rs` and `web_search.rs` each contain one tool's struct + `BaseTool` impl + description constant. `web.rs` retains only `WebMiddleware` + `Middleware` impl + tests. External callers use `WebMiddleware` — zero API breakage.

**Tech Stack:** Rust, `reqwest`, `serde_json`, `html2text`, `regex`, `base64`, `url`, `urlencoding`, `async_trait`.

---

## File Structure After Split

```
middleware/
├── mod.rs            # unchanged (pub mod web; pub use web::WebMiddleware;)
├── web.rs            # ~30 lines: WebMiddleware struct + Middleware impl + tests
├── web_common.rs     # ~80 lines: validate_url, html_to_text, truncate_content, constants
├── web_fetch.rs      # ~140 lines: WebFetchTool + BaseTool impl + WEB_FETCH_DESCRIPTION
└── web_search.rs     # ~350 lines: WebSearchTool + all Bing parsing helpers + constants
```

---

### Task 1: Create `web_common.rs` — extract shared utilities

**Files:**
- Create: `peri-middlewares/src/middleware/web_common.rs`
- Modify: `peri-middlewares/src/middleware/web.rs` (remove lines 1–79, 99–100)
- **⚠ Do NOT remove `use` statements needed by remaining code in `web.rs`**

- [ ] **Step 1: Create `web_common.rs`**

Create `/Users/konghayao/code/ai/perihelion/peri-middlewares/src/middleware/web_common.rs` with these items, copied verbatim from `web.rs`:

```
Lines to copy from web.rs:
  10–59   validate_url() function
  61–64   html_to_text() function
  66–75   truncate_content() function
  77–79   WEB_CREDIBILITY_WARNING constant
  99–100  MAX_RESPONSE_BYTES constant
```

Add imports at top:
```rust
use std::net::IpAddr;
use url::Url;
```

Change all function/const visibility to `pub(crate)`:
```rust
pub(crate) fn validate_url(url: &str) -> Result<Url, String> { ... }
pub(crate) fn html_to_text(html: &str) -> String { ... }
pub(crate) fn truncate_content(content: &str, max_lines: usize) -> String { ... }
pub(crate) const WEB_CREDIBILITY_WARNING: &str = ...;
pub(crate) const MAX_RESPONSE_BYTES: u64 = ...;
```

- [ ] **Step 2: Remove moved items from `web.rs`**

In `web.rs`, delete:
- Lines 10–59 (`validate_url` function)
- Lines 61–64 (`html_to_text`)
- Lines 66–75 (`truncate_content`)
- Lines 77–79 (`WEB_CREDIBILITY_WARNING`)
- Lines 99–100 (`MAX_RESPONSE_BYTES`)

Add import at top:
```rust
use super::web_common::{html_to_text, truncate_content, validate_url, MAX_RESPONSE_BYTES, WEB_CREDIBILITY_WARNING};
```

Also clean up now-unused imports in `web.rs`:
- `use std::net::IpAddr;` — only used by `validate_url` → **REMOVE**
- `use url::Url;` — only used by `validate_url` → **REMOVE**

- [ ] **Step 3: Build to verify**

```bash
cargo build -p peri-middlewares 2>&1
```
Expected: Success. If compilation errors:
- Check that `web_common.rs` imports `std::net::IpAddr` and `url::Url`
- Check that `web.rs` imports from `super::web_common::...`
- Verify unused import removal didn't break anything remaining

- [ ] **Step 4: Commit**

```bash
git add peri-middlewares/src/middleware/web_common.rs peri-middlewares/src/middleware/web.rs
git commit -m "refactor(web): extract web_common module from web.rs

Move validate_url, html_to_text, truncate_content, and shared
constants to web_common.rs. All items pub(crate).

Co-Authored-By: deepseek-v4-pro <deepseek-ai@claude-code.win>"
```

---

### Task 2: Create `web_fetch.rs` — extract WebFetchTool

**Files:**
- Create: `peri-middlewares/src/middleware/web_fetch.rs`
- Modify: `peri-middlewares/src/middleware/web.rs` (remove lines 82–213)

- [ ] **Step 1: Create `web_fetch.rs`**

Create `/Users/konghayao/code/ai/perihelion/peri-middlewares/src/middleware/web_fetch.rs` with these items, copied verbatim from `web.rs`:

```
Lines to copy from web.rs:
  82       pub struct WebFetchTool;
  84–97    WEB_FETCH_DESCRIPTION constant
 102–112   impl WebFetchTool { new() } + impl Default
 114–213   impl BaseTool for WebFetchTool
```

Add imports at top:
```rust
use async_trait::async_trait;
use peri_agent::tools::BaseTool;
use serde_json::Value;
use tokio::time::{timeout, Duration};

use super::web_common::{html_to_text, truncate_content, validate_url, MAX_RESPONSE_BYTES, WEB_CREDIBILITY_WARNING};
```

Note: The `WEB_CREDIBILITY_WARNING` is used in `WebFetchTool::invoke()` (line 206–208) — the import makes it accessible.

- [ ] **Step 2: Update `web.rs` — remove WebFetchTool, add import**

Delete lines 82–213 from `web.rs`. This removes:
- `WebFetchTool` struct
- `WEB_FETCH_DESCRIPTION`
- `impl WebFetchTool` + `impl Default`
- `impl BaseTool for WebFetchTool`

Add import at top of `web.rs`:
```rust
use super::web_fetch::WebFetchTool;
```

Also clean up now-unused imports in `web.rs`:
- `use peri_agent::tools::BaseTool;` — `WebMiddleware` doesn't use it directly, but `WebSearchTool` still does → **KEEP** (for now, until Task 3)
- `use serde_json::Value;` — same reasoning → **KEEP**
- `use async_trait::async_trait;` — same → **KEEP**
- `use tokio::time::{timeout, Duration};` — only used by `WebFetchTool` and `WebSearchTool` → **KEEP** (still needed by WebSearchTool)
- `use serde_json::Value;` → **KEEP**

- [ ] **Step 3: Build to verify**

```bash
cargo build -p peri-middlewares 2>&1
```
Expected: Success.

- [ ] **Step 4: Commit**

```bash
git add peri-middlewares/src/middleware/web_fetch.rs peri-middlewares/src/middleware/web.rs
git commit -m "refactor(web): extract web_fetch module from web.rs

Move WebFetchTool and its BaseTool impl to web_fetch.rs.

Co-Authored-By: deepseek-v4-pro <deepseek-ai@claude-code.win>"
```

---

### Task 3: Create `web_search.rs` — extract WebSearchTool + Bing helpers

**Files:**
- Create: `peri-middlewares/src/middleware/web_search.rs`
- Modify: `peri-middlewares/src/middleware/web.rs` (remove lines 215–525)

- [ ] **Step 1: Create `web_search.rs`**

Create `/Users/konghayao/code/ai/perihelion/peri-middlewares/src/middleware/web_search.rs` with these items, copied verbatim from `web.rs`:

```
Lines to copy from web.rs:
 215–220  SearchResult struct
 222–237  WEBSEARCH_DESCRIPTION constant
 240      MAX_RESULT_TEXT_CHARS constant
 243      pub struct WebSearchTool;
 249–263  BROWSER_HEADERS constant
 265–275  impl WebSearchTool + impl Default
 277–307  decode_html_entities()
 309–313  strip_html_tags()
 315–350  resolve_bing_url()
 352–392  extract_bing_results()
 394–428  extract_snippet()
 430–447  format_search_results()
 449–525  impl BaseTool for WebSearchTool
```

Add imports at top:
```rust
use async_trait::async_trait;
use peri_agent::tools::BaseTool;
use serde_json::Value;

use super::web_common::{WEB_CREDIBILITY_WARNING, MAX_RESULT_TEXT_CHARS};
```

Note: `WEB_CREDIBILITY_WARNING` is used in `format_search_results()` (lines 433–436).

Make private helpers pub(crate):
```rust
pub(crate) struct SearchResult { ... }
pub(crate) const BROWSER_HEADERS: &[(&str, &str)] = &[...];
```

Functions `decode_html_entities`, `strip_html_tags`, `resolve_bing_url`, `extract_bing_results`, `extract_snippet`, `format_search_results` can remain private (only used within `web_search.rs`).

- [ ] **Step 2: Remove all WebSearchTool code from `web.rs`**

Delete lines 215–525 from `web.rs`. Add import:
```rust
use super::web_search::WebSearchTool;
```

Now clean up unused imports from `web.rs`. After removing both `WebFetchTool` and `WebSearchTool`:

```rust
// REMOVE these (only used by the tool impls):
use async_trait::async_trait;
use peri_agent::tools::BaseTool;
use serde_json::Value;
use tokio::time::{timeout, Duration};

// KEEP these:
use peri_agent::agent::state::State;
use peri_agent::middleware::r#trait::Middleware;

// ADD import for tools:
use super::web_fetch::WebFetchTool;
use super::web_search::WebSearchTool;
```

Final `web.rs` imports should be:
```rust
use async_trait::async_trait;
use peri_agent::agent::state::State;
use peri_agent::middleware::r#trait::Middleware;
use peri_agent::tools::BaseTool;

use super::web_fetch::WebFetchTool;
use super::web_search::WebSearchTool;
```

Wait — `async_trait` is needed for `#[async_trait]` on `impl Middleware`. `BaseTool` is needed for the `Vec<Box<dyn BaseTool>>` return type. So keep both.

Final `web.rs` code (after all removals):
```rust
use async_trait::async_trait;
use peri_agent::agent::state::State;
use peri_agent::middleware::r#trait::Middleware;
use peri_agent::tools::BaseTool;

use super::web_fetch::WebFetchTool;
use super::web_search::WebSearchTool;

/// Web 中间件，提供 WebFetch 和 WebSearch 工具
pub struct WebMiddleware;

impl WebMiddleware {
    pub fn new() -> Self {
        Self
    }
}

impl Default for WebMiddleware {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl<S: State> Middleware<S> for WebMiddleware {
    fn name(&self) -> &str {
        "WebMiddleware"
    }

    fn collect_tools(&self, _cwd: &str) -> Vec<Box<dyn BaseTool>> {
        vec![
            Box::new(WebFetchTool::new()),
            Box::new(WebSearchTool::new()),
        ]
    }
}

#[cfg(test)]
#[path = "web_test.rs"]
mod tests;
```

- [ ] **Step 3: Build to verify**

```bash
cargo build -p peri-middlewares 2>&1
```
Expected: Success. Check for unused import warnings — they indicate something was missed.

- [ ] **Step 4: Commit**

```bash
git add peri-middlewares/src/middleware/web_search.rs peri-middlewares/src/middleware/web.rs
git commit -m "refactor(web): extract web_search module from web.rs

Move WebSearchTool, Bing parsing helpers, and related constants
to web_search.rs. web.rs now contains only WebMiddleware glue.

Co-Authored-By: deepseek-v4-pro <deepseek-ai@claude-code.win>"
```

---

### Task 4: Full verification

- [ ] **Step 1: Run full build**

```bash
cargo build 2>&1
```
Expected: All crates compile. No warnings (except pre-existing).

- [ ] **Step 2: Run web-related tests**

```bash
cargo test -p peri-middlewares web 2>&1
```
Expected: All tests in `web_test.rs` pass. No regressions.

- [ ] **Step 3: Verify `WebMiddleware::collect_tools()` returns both tools correctly**

```bash
cargo test -p peri-middlewares middleware 2>&1
```
Expected: All middleware tests pass.

- [ ] **Step 4: Run full test suite**

```bash
cargo test 2>&1
```
Expected: All tests pass.

- [ ] **Step 5: Verify file sizes**

```bash
wc -l peri-middlewares/src/middleware/web.rs peri-middlewares/src/middleware/web_common.rs peri-middlewares/src/middleware/web_fetch.rs peri-middlewares/src/middleware/web_search.rs
```
Expected approximate: `web.rs` ~30 lines, `web_common.rs` ~80 lines, `web_fetch.rs` ~140 lines, `web_search.rs` ~350 lines.

---

## Self-Review Checklist

1. **Spec coverage:** All files from the issue's target structure created ✓
2. **Placeholder scan:** No TODOs — every step has exact line ranges and complete import lists ✓
3. **Type consistency:** `WebMiddleware` unchanged, `collect_tools()` returns same `Vec<Box<dyn BaseTool>>` ✓
4. **Import cleanup:** Every removed import tracked — `web.rs` final import list explicitly shown ✓
5. **Constant visibility:** `WEB_CREDIBILITY_WARNING`, `MAX_RESPONSE_BYTES`, `BROWSER_HEADERS`, `MAX_RESULT_TEXT_CHARS` all `pub(crate)` for cross-module access ✓
6. **Test isolation:** `web_test.rs` untouched, referenced via `#[path = "web_test.rs"]` in `web.rs` ✓
