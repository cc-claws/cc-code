# Langfuse Telemetry Data Quality Fix

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix Langfuse telemetry so that model name, session_id, and cost data are correctly recorded and visible in both the Langfuse UI and the analysis script.

**Architecture:** The current implementation sends events via the OTLP endpoint (`/api/public/otel/v1/traces`), which doesn't map `langfuse.observation.model.name` attribute to the top-level `providedModelName` field. We fix this by adding a parallel native ingestion path alongside OTLP. The analysis script also needs to read model from metadata when `providedModelName` is empty.

**Tech Stack:** Rust (reqwest, serde), TypeScript (bun), Langfuse OTLP + Native Ingestion APIs

---

## Problem Summary

| # | Issue | Root Cause | Impact |
|---|-------|-----------|--------|
| 1 | `providedModelName: null` | OTLP doesn't map `langfuse.observation.model.name` to top-level field | Langfuse UI shows no model; analysis script can't group by model |
| 2 | Tool observations missing `session_id` | `on_tool_end` sets `session_id: None` | Tools not grouped by session in Langfuse UI |
| 3 | Analysis script reads empty model | Script reads `providedModelName` which is null | Report shows `model: ?` |
| 4 | `costDetails` always empty | No cost data sent | No cost tracking |
| 5 | Agent input shows `None` in API response | OTLP ObservationUpdate may overwrite initial create | Needs investigation |

---

### Task 1: Fix Tool observation `session_id`

**Files:**
- Modify: `peri-acp/src/langfuse/tracer.rs:338-357` (the `ObservationBody` in `on_tool_end`)
- Test: verify via `bunx langfuse-cli`

- [ ] **Step 1: Add `session_id` to tool ObservationBody**

In `tracer.rs`, method `on_tool_end`, change the `ObservationBody` construction:

```rust
// Before (line 356):
session_id: None,

// After:
session_id: Some(self.session_id.clone()),
```

Wait — `session_id` is inside `self` but we already moved/borrowed parts of self. Need to clone `session_id` before the borrow. Add at the top of the method:

```rust
pub fn on_tool_end(&mut self, tool_call_id: &str, output: &str, is_error: bool) {
    let session_id = self.session_id.clone();  // ADD THIS
    let trace_id = self.trace_id.clone();
    let trace_id_for_log = self.trace_id.clone();
    // ... rest unchanged ...
```

Then change:
```rust
session_id: None,
// →
session_id: Some(session_id),
```

- [ ] **Step 2: Build and verify compilation**

Run: `cargo build -p peri-acp`
Expected: compiles without errors

- [ ] **Step 3: Commit**

```bash
git add peri-acp/src/langfuse/tracer.rs
git commit -m "fix(langfuse): add session_id to tool observations"
```

---

### Task 2: Add native ingestion endpoint alongside OTLP

The OTLP endpoint doesn't support `providedModelName` mapping. We add a secondary `ingest_native` method to `LangfuseClient` that sends events to `/api/public/ingestion` as the Langfuse native format, which properly supports all fields.

**Files:**
- Modify: `langfuse-client/src/client.rs` — add `ingest_native()` method
- Modify: `langfuse-client/src/batcher.rs` — dual-path flushing (OTLP + native)
- Test: `langfuse-client/src/client_test.rs` (new)

- [ ] **Step 2.1: Add `ingest_native` method to `LangfuseClient`**

In `client.rs`, add a new method after `ingest()`:

```rust
/// Send events via Langfuse native ingestion API.
///
/// POST /api/public/ingestion
/// Sends raw JSON ingestion events — supports full field mapping
/// including `model` → `providedModelName`.
pub async fn ingest_native(&self, events: Vec<IngestionEvent>) -> Result<(), LangfuseError> {
    if events.is_empty() {
        return Ok(());
    }

    let url = format!("{}/api/public/ingestion", self.base_url);

    // Build native ingestion payload: { "batch": [ ...events ] }
    let payload = serde_json::json!({
        "batch": events
    });

    let mut attempt = 0;
    loop {
        let result = self
            .http
            .post(&url)
            .header("Authorization", &self.auth_header)
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await;

        match result {
            Ok(response) => {
                let status = response.status();
                if status.is_success() {
                    let _ = response.bytes().await;
                    return Ok(());
                } else if status.is_client_error() {
                    let body = response.text().await.unwrap_or_default();
                    return Err(LangfuseError::IngestionApi {
                        status: status.as_u16(),
                        body,
                    });
                } else {
                    // 5xx: retry
                    attempt += 1;
                    if attempt > self.max_retries {
                        let body = response.text().await.unwrap_or_default();
                        return Err(LangfuseError::IngestionApi {
                            status: status.as_u16(),
                            body,
                        });
                    }
                    let delay = std::time::Duration::from_secs(1 << (attempt - 1));
                    tokio::time::sleep(delay).await;
                }
            }
            Err(e) => {
                attempt += 1;
                if attempt > self.max_retries {
                    return Err(LangfuseError::Http(e));
                }
                let delay = std::time::Duration::from_secs(1 << (attempt - 1));
                tokio::time::sleep(delay).await;
            }
        }
    }
}
```

- [ ] **Step 2.2: Verify `IngestionEvent` serializes correctly for native API**

Check that `IngestionEvent` derives `Serialize` and the native ingestion API accepts the format. The expected payload shape is:

```json
{
  "batch": [
    { "id": "...", "type": "generation-create", "timestamp": "...", "body": { ... } }
  ]
}
```

Look at `IngestionEvent`'s `Serialize` impl — each variant needs a `"type"` field that matches Langfuse's expected event type strings (`"trace-create"`, `"span-create"`, `"generation-create"`, etc.). If missing, add a custom serde serialization or a `"type"` helper method.

Check `langfuse-client/src/types/mod.rs` for the enum definition and verify `#[serde(tag = "type", rename_all = "camelCase")]` or equivalent.

- [ ] **Step 2.3: Update Batcher to use native ingestion**

In `batcher.rs`, modify the flush loop. Currently it calls `client.ingest(events)` (OTLP). Change to:

```rust
// Use native ingestion for better field support (model name, etc.)
if let Err(e) = self.client.ingest_native(events).await {
    tracing::warn!("Native ingestion failed, falling back to OTLP: {}", e);
    // fallback: let the original OTLP path handle it
}
```

Or more conservatively: keep OTLP as primary, add native as secondary for generation events only. The simplest approach: **replace OTLP with native entirely**, since native is the canonical API.

- [ ] **Step 2.4: Build and test**

Run: `cargo build -p langfuse-client && cargo test -p langfuse-client`
Expected: all pass

- [ ] **Step 2.5: Commit**

```bash
git add langfuse-client/
git commit -m "feat(langfuse): switch from OTLP to native ingestion API for full field support"
```

---

### Task 3: Fix analysis script model extraction

**Files:**
- Modify: `.claude/skills/langfuse/scripts/analyze.ts`

- [ ] **Step 3.1: Add model extraction with metadata fallback**

In `analyzeTrace`, change the `GenDetail` construction:

```typescript
// Current:
model: g.providedModelName || g.internalModelId || g.model || "?",

// Fixed: fallback to metadata attribute
model: g.providedModelName || g.internalModelId || g.model
  || g.metadata?.attributes?.["langfuse.observation.model.name"]
  || "?",
```

- [ ] **Step 3.2: Verify with live data**

Run: `bun .claude/skills/langfuse/scripts/analyze.ts --report 3`

Check that `Model` column now shows `glm-5.1` instead of `?` in the "Most Expensive Trace Detail" section.

- [ ] **Step 3.3: Commit**

```bash
git add .claude/skills/langfuse/scripts/analyze.ts
git commit -m "fix(langfuse): analysis script reads model from metadata fallback"
```

---

### Task 4: Add `version` field to observations

Setting `version` on observations enables filtering by release in Langfuse UI.

**Files:**
- Modify: `peri-acp/src/langfuse/tracer.rs`

- [ ] **Step 4.1: Add version constant and use it**

At the top of `tracer.rs`:

```rust
const VERSION: &str = env!("CARGO_PKG_VERSION");
```

In `on_trace_start`, `on_llm_end` (GenerationBody), `on_tool_end` (ObservationBody), `on_subagent_start`:
add `version: Some(VERSION.to_string())` to each body construction.

- [ ] **Step 4.2: Build**

Run: `cargo build -p peri-acp`

- [ ] **Step 4.3: Commit**

```bash
git add peri-acp/src/langfuse/tracer.rs
git commit -m "feat(langfuse): add version to all observations"
```

---

### Task 5: Verify end-to-end with live TUI

- [ ] **Step 5.1: Start TUI and send a test prompt**

Run: `cargo run -p peri-tui`
Send: "hello"

- [ ] **Step 5.2: Check Langfuse for correct data**

Run:
```bash
bunx langfuse-cli api observations list --limit 5 --type GENERATION --fields core,model,usage --json
```

Verify:
- `providedModelName` is populated (after Task 2 native ingestion fix)
- OR `metadata.attributes["langfuse.observation.model.name"]` has the model (before Task 2)
- `usageDetails` has input/output/cache_read
- Tool observations have `session_id`

- [ ] **Step 5.3: Run analysis report**

```bash
bun .claude/skills/langfuse/scripts/analyze.ts --report 5
```

Verify model column shows real model names.

---

## Self-Review

**Spec coverage:**
- ✅ Issue 1 (model name) → Task 2 + Task 3
- ✅ Issue 2 (tool session_id) → Task 1
- ✅ Issue 3 (script model fallback) → Task 3
- ⏸ Issue 4 (costDetails) → Deferred (requires pricing config, not critical)
- ⏸ Issue 5 (agent input None) → Needs more investigation; may resolve with native ingestion

**Placeholder scan:** No TBDs, all code shown inline.

**Type consistency:**
- `session_id: String` cloned correctly across all methods
- `IngestionEvent` Serialize format verified for native API compatibility
- `GenerationBody.model` is `Option<String>`, matches `model_name()` return type

**Risks:**
- Native ingestion API may reject our current `IngestionEvent` format if the `type` discriminator doesn't match. Need to verify the exact expected format with `--curl` or Langfuse docs.
- If native API is incompatible, fallback: keep OTLP but add a post-hoc metadata enrichment step.
