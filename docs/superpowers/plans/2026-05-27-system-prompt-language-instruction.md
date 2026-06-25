# System Prompt Language Instruction Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Inject a `# Language` section into the system prompt so the LLM knows which language to respond in, matching the user's configured UI language preference.

**Architecture:** Add `language: Option<&str>` parameter to `build_system_prompt()`, inject a `# Language\nAlways respond in {language}...` paragraph in the dynamic section area. Freeze language at `session/new` time (like date/cwd). Propagate through `FrozenSessionData` and SubAgent `SystemPromptBuilder`.

**Tech Stack:** Rust (peri-acp, peri-tui), no new dependencies.

---

## File Structure

| File | Change | Purpose |
|------|--------|---------|
| `peri-acp/src/prompt/mod.rs` | Modify | Add `language` param, inject language paragraph |
| `peri-acp/src/session/executor.rs` | Modify | Add `language` to `FrozenSessionData`, pass through |
| `peri-acp/src/agent/builder.rs` | Modify | Capture language in SubAgent `SystemPromptBuilder` |
| `peri-tui/src/acp_server/mod.rs` | Modify | Add `frozen_language` to `SessionState` |
| `peri-tui/src/acp_server/requests.rs` | Modify | Read language from config, pass to `build_system_prompt` |
| `peri-tui/src/acp_server/prompt.rs` | Modify | Pass language to `FrozenSessionData` |
| `peri-tui/src/acp_stdio.rs` | Modify | Add `frozen_language` to `SessionInfo`, pass through |
| `peri-acp/src/prompt/prompt_test.rs` | Modify | Add language injection test |

---

### Task 1: Core — Add language parameter to build_system_prompt

**Files:**
- Modify: `peri-acp/src/prompt/mod.rs:105`

- [ ] **Step 1: Add `language` parameter and inject language section**

Append a `language: Option<&str>` parameter to `build_system_prompt()`. After the dynamic_sections loop and before the `overrides_block` processing, inject the language paragraph when `language` is `Some`.

```rust
pub fn build_system_prompt(
    overrides: Option<&AgentOverrides>,
    cwd: &str,
    features: PromptFeatures,
    extra_agent_dirs: &[std::path::PathBuf],
    frozen_date: Option<&str>,
    language: Option<&str>,  // 🆕
) -> String {
    // ... existing code ...
    
    // 动态段落
    let mut dynamic_sections: Vec<&str> = Vec::new();
    dynamic_sections.push(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../peri-tui/prompts/sections/07_env.md"
    )));
    // ... feature-gated sections (10-13) ...
    
    let overrides_block = overrides
        .map(build_agent_overrides_block)
        .unwrap_or_default();
    
    let mut result = String::new();
    for (i, section) in static_sections.iter().enumerate() {
        if i > 0 {
            result.push_str("\n\n");
        }
        result.push_str(section);
    }
    result.push_str("\n\n__SYSTEM_PROMPT_DYNAMIC_BOUNDARY__");
    if !overrides_block.is_empty() {
        result.push_str("\n\n");
        result.push_str(&overrides_block);
    }
    for section in &dynamic_sections {
        result.push_str("\n\n");
        result.push_str(section);
    }
    // 🆕 Language instruction (dynamic, after boundary to preserve cache prefix)
    if let Some(lang) = language {
        let lang_instruction = map_language_to_instruction(lang);
        result.push_str("\n\n# Language\n");
        result.push_str(&format!(
            "Always respond in {}. Use {} for all explanations, comments, and communications with the user. Technical terms and code identifiers should remain in their original form.",
            lang_instruction, lang_instruction
        ));
    }
    
    result
        .replace("{{cwd}}", &env.cwd)
        // ... rest of replacements ...
}

/// Map language code to human-readable instruction string.
fn map_language_to_instruction(lang: &str) -> &str {
    match lang {
        "zh-CN" | "zh" => "Simplified Chinese",
        "zh-TW" => "Traditional Chinese",
        "ja" => "Japanese",
        "ko" => "Korean",
        _ => lang, // fallback: use raw code (e.g., "en", "fr", "de")
    }
}
```

- [ ] **Step 2: Update existing test signatures to pass `None` for language**

Edit `peri-acp/src/prompt/prompt_test.rs` — all 21 test calls to `build_system_prompt` get an extra `None` at the end.

Each line goes from:
```rust
let result = build_system_prompt(None, "/tmp", PromptFeatures::none(), &[], None);
```
to:
```rust
let result = build_system_prompt(None, "/tmp", PromptFeatures::none(), &[], None, None);
```

There are 21 occurrences — use a global find-and-replace.

- [ ] **Step 3: Build and test the core change**

Run: `cargo test -p peri-acp --lib prompt::tests`
Expected: All existing tests pass, plus verify builds cleanly.

- [ ] **Step 4: Commit**

```bash
git add peri-acp/src/prompt/mod.rs peri-acp/src/prompt/prompt_test.rs
git commit -m "feat(prompt): add language parameter to build_system_prompt"
```

---

### Task 2: Add language to FrozenSessionData and propagate in executor

**Files:**
- Modify: `peri-acp/src/session/executor.rs:61-74`
- Modify: `peri-acp/src/session/executor.rs:309-323`

- [ ] **Step 1: Add `language` field to `FrozenSessionData`**

```rust
pub struct FrozenSessionData {
    pub system_prompt: String,
    pub claude_md: Option<String>,
    pub claude_local_md: Option<String>,
    pub skill_summary: Option<String>,
    pub date: String,
    pub is_git_repo: bool,
    /// Session creation language preference (e.g. "zh-CN", "en").
    /// None = auto-detect from user input (no explicit instruction).
    pub language: Option<String>,  // 🆕
}
```

- [ ] **Step 2: Pass language to legacy path in execute_prompt**

In `executor.rs`, before the `if let Some(ref f) = frozen` block, extract language. Then pass it in the legacy `build_system_prompt` call.

Replace lines 303-323:

```rust
    let language = frozen.as_ref().and_then(|f| f.language.clone())
        .or_else(|| peri_config.config.language.clone());

    let (
        system_prompt,
        frozen_claude_md,
        frozen_claude_local_md,
        frozen_skill_summary,
        frozen_date,
    ) = if let Some(ref f) = frozen {
        // 使用 session 创建时冻结的数据，跳过重建
        (
            f.system_prompt.clone(),
            f.claude_md.clone(),
            f.claude_local_md.clone(),
            f.skill_summary.clone(),
            Some(f.date.clone()),
        )
    } else {
        // Legacy: per-turn rebuild（子 Agent 等场景未提供 frozen 数据时使用）
        let features = PromptFeatures::detect();
        let sp = build_system_prompt(None, cwd, features, &plugin_agent_dirs, None, language.as_deref());
        (sp, None, None, None, None)
    };
```

- [ ] **Step 3: Commit**

```bash
git add peri-acp/src/session/executor.rs
git commit -m "feat(executor): add language field to FrozenSessionData"
```

---

### Task 3: Capture language in agent builder's SystemPromptBuilder

**Files:**
- Modify: `peri-acp/src/agent/builder.rs:249-260`

- [ ] **Step 1: Capture `language` in the closure**

Replace lines 249-260:

```rust
    let frozen_language_for_sub = peri_config.config.language.clone();
    let frozen_date_for_sub = frozen_date.clone();
    let system_builder: SystemPromptBuilder = Arc::new(move |overrides, cwd_dir| {
        let features = crate::prompt::PromptFeatures::detect();
        crate::prompt::build_system_prompt(
            overrides,
            cwd_dir,
            features,
            &[],
            frozen_date_for_sub.as_deref(),
            frozen_language_for_sub.as_deref(),
        )
    });
```

- [ ] **Step 2: Commit**

```bash
git add peri-acp/src/agent/builder.rs
git commit -m "feat(builder): capture language in SubAgent SystemPromptBuilder"
```

---

### Task 4: TUI — Add frozen_language to SessionState and pass through

**Files:**
- Modify: `peri-tui/src/acp_server/mod.rs:39-54`
- Modify: `peri-tui/src/acp_server/requests.rs:79-97`
- Modify: `peri-tui/src/acp_server/prompt.rs:116-125`

- [ ] **Step 1: Add `frozen_language` to `SessionState`**

```rust
pub(crate) struct SessionState {
    // ... existing fields ...
    pub(crate) frozen_date: Option<String>,
    /// Frozen language preference (e.g. "zh-CN", "en").
    pub(crate) frozen_language: Option<String>,  // 🆕
    pub(crate) recall_items: Vec<String>,
    // ...
}
```

- [ ] **Step 2: Read language from config and pass to build_system_prompt in session/new**

In `requests.rs`, after the `frozen_date` is set, read language from `cfg.peri_config`:

```rust
            // ── Freeze system prompt data at session creation ──
            let frozen_date = chrono::Local::now().format("%Y-%m-%d").to_string();
            
            let frozen_language = cfg.peri_config.read().config.language.clone();  // 🆕

            let (frozen_claude_md, frozen_claude_local_md) =
                peri_middlewares::AgentsMdMiddleware::read_frozen_content(&cwd);

            let frozen_skill_summary = peri_middlewares::SkillsMiddleware::build_frozen_summary(
                &cwd,
                &cfg.plugin_skill_dirs,
            );

            let features = peri_acp::prompt::PromptFeatures::detect();
            let system_prompt = peri_acp::prompt::build_system_prompt(
                None,
                &cwd,
                features,
                &cfg.plugin_agent_dirs,
                Some(&frozen_date),
                frozen_language.as_deref(),  // 🆕
            );

            let state = sessions.get_mut(&session_id).unwrap();
            state.frozen_system_prompt = Some(system_prompt);
            state.frozen_claude_md = frozen_claude_md;
            state.frozen_claude_local_md = frozen_claude_local_md;
            state.frozen_skill_summary = frozen_skill_summary;
            state.frozen_date = Some(frozen_date);
            state.frozen_language = frozen_language;  // 🆕
```

- [ ] **Step 3: Pass frozen_language to FrozenSessionData in prompt execution**

In `prompt.rs`, add `language` to the `FrozenSessionData` construction:

```rust
    let frozen = frozen_system_prompt.map(|sp| {
        // Read frozen_language from session state  // 🆕
        let frozen_lang = {
            let sessions = sessions.lock().await;
            sessions.get(&session_id)
                .and_then(|s| s.frozen_language.clone())
        };
        
        executor::FrozenSessionData {
            system_prompt: sp,
            claude_md: frozen_claude_md,
            claude_local_md: frozen_claude_local_md,
            skill_summary: frozen_skill_summary,
            date: frozen_date.unwrap_or_default(),
            is_git_repo: std::path::Path::new(&cwd).join(".git").exists(),
            language: frozen_lang,  // 🆕
        }
    });
```

Wait — looking at the prompt.rs code more carefully, the `frozen_system_prompt` and other frozen fields are already extracted from sessions before this block. Let me add `frozen_language` to the extraction:

In the extraction block (around lines 79-107), add `frozen_language`:

```rust
    let (
        cwd,
        history,
        is_empty,
        thread_id,
        frozen_system_prompt,
        frozen_claude_md,
        frozen_claude_local_md,
        frozen_skill_summary,
        frozen_date,
        frozen_language,  // 🆕
        incoming_recalls,
    ) = {
        let mut sessions = sessions.lock().await;
        let state = sessions
            .get_mut(&session_id)
            .ok_or_else(|| AcpError::new(-32602, "session not found"))?;
        (
            state.cwd.clone(),
            state.history.clone(),
            state.history.is_empty(),
            state.thread_id.clone(),
            state.frozen_system_prompt.clone(),
            state.frozen_claude_md.clone(),
            state.frozen_claude_local_md.clone(),
            state.frozen_skill_summary.clone(),
            state.frozen_date.clone(),
            state.frozen_language.clone(),  // 🆕
            std::mem::take(&mut state.recall_items),
        )
    };
```

Then use it:

```rust
    let frozen = frozen_system_prompt.map(|sp| executor::FrozenSessionData {
        system_prompt: sp,
        claude_md: frozen_claude_md,
        claude_local_md: frozen_claude_local_md,
        skill_summary: frozen_skill_summary,
        date: frozen_date.unwrap_or_default(),
        is_git_repo: std::path::Path::new(&cwd).join(".git").exists(),
        language: frozen_language,  // 🆕
    });
```

- [ ] **Step 4: Commit**

```bash
git add peri-tui/src/acp_server/mod.rs peri-tui/src/acp_server/requests.rs peri-tui/src/acp_server/prompt.rs
git commit -m "feat(tui): freeze and propagate language preference through ACP"
```

---

### Task 5: Stdio — Add frozen_language to SessionInfo and pass through

**Files:**
- Modify: `peri-tui/src/acp_stdio.rs` (SessionInfo struct, session/new, session/prompt)

- [ ] **Step 1: Add `frozen_language` to `SessionInfo`**

After `frozen_date` field:

```rust
    /// Session creation date (YYYY-MM-DD).
    frozen_date: Option<String>,
    /// Frozen language preference (e.g. "zh-CN", "en").  // 🆕
    frozen_language: Option<String>,  // 🆕
    /// Recall items from previous turn.
    recall_items: Vec<String>,
```

- [ ] **Step 2: Store language during session/new**

Find the session/new block (around line 278). Read language from config, pass to `build_system_prompt`, store in `SessionInfo`:

After the `frozen_date` line, add:

```rust
                    let frozen_date =
                        chrono::Local::now().format("%Y-%m-%d").to_string();
                    
                    let frozen_language = ctx.peri_config.read().config.language.clone();  // 🆕
```

Pass it to `build_system_prompt`:

```rust
                    let frozen_system_prompt = peri_acp::prompt::build_system_prompt(
                        None,
                        &cwd_str,
                        features,
                        &ctx.plugin_agent_dirs,
                        Some(&frozen_date),
                        frozen_language.as_deref(),  // 🆕
                    );
```

Store in `SessionInfo`:

```rust
                        sessions.insert(
                            sid.clone(),
                            SessionInfo {
                                session_id: sid.clone(),
                                thread_id: thread_id.clone(),
                                cwd: cwd_str,
                                history: Vec::new(),
                                cancel_token: None,
                                frozen_system_prompt: Some(frozen_system_prompt),
                                frozen_claude_md,
                                frozen_claude_local_md,
                                frozen_skill_summary,
                                frozen_date: Some(frozen_date),
                                frozen_language,  // 🆕
                                recall_items: Vec::new(),
                                agent_pool: peri_acp::session::agent_pool::AgentPool::new(),
                            },
                        );
```

- [ ] **Step 3: Pass frozen_language to FrozenSessionData during session/prompt**

In the FrozenSessionData construction (around line 402):

```rust
                                let frozen = s.frozen_system_prompt.as_ref().map(|sp| {
                                    executor::FrozenSessionData {
                                        system_prompt: sp.clone(),
                                        claude_md: s.frozen_claude_md.clone(),
                                        claude_local_md: s.frozen_claude_local_md.clone(),
                                        skill_summary: s.frozen_skill_summary.clone(),
                                        date: s.frozen_date.clone().unwrap_or_default(),
                                        is_git_repo: std::path::Path::new(&s.cwd)
                                            .join(".git")
                                            .exists(),
                                        language: s.frozen_language.clone(),  // 🆕
                                    }
                                });
```

- [ ] **Step 4: Commit**

```bash
git add peri-tui/src/acp_stdio.rs
git commit -m "feat(stdio): freeze and propagate language preference through stdio path"
```

---

### Task 6: Add language injection test

**Files:**
- Modify: `peri-acp/src/prompt/prompt_test.rs`

- [ ] **Step 1: Add test for language=Some("zh-CN")**

```rust
#[test]
fn test_language_simplified_chinese_injected() {
    let result = build_system_prompt(
        None,
        "/tmp",
        PromptFeatures::none(),
        &[],
        None,
        Some("zh-CN"),
    );
    assert!(
        result.contains("# Language"),
        "language=zh-CN 时应包含 # Language 标题"
    );
    assert!(
        result.contains("Simplified Chinese"),
        "zh-CN 应映射到 Simplified Chinese"
    );
    assert!(
        result.contains("Technical terms and code identifiers should remain in their original form"),
        "应包含技术术语保留原文指示"
    );
}
```

- [ ] **Step 2: Add test for language=None**

```rust
#[test]
fn test_language_none_no_injection() {
    let result = build_system_prompt(
        None,
        "/tmp",
        PromptFeatures::none(),
        &[],
        None,
        None,
    );
    assert!(
        !result.contains("\n# Language\n"),
        "language=None 时不应注入 Language 段落"
    );
}
```

- [ ] **Step 3: Add test for language section is after boundary marker**

```rust
#[test]
fn test_language_section_after_boundary_marker() {
    let result = build_system_prompt(
        None,
        "/tmp",
        PromptFeatures::none(),
        &[],
        None,
        Some("zh-CN"),
    );
    let boundary_pos = result.find("__SYSTEM_PROMPT_DYNAMIC_BOUNDARY__").unwrap();
    assert!(
        result[boundary_pos..].contains("# Language"),
        "Language 段落应在边界标记之后（动态区域，不破坏缓存前缀）"
    );
    assert!(
        !result[..boundary_pos].contains("# Language"),
        "Language 段落不应在边界标记之前（会破坏缓存前缀）"
    );
}
```

- [ ] **Step 5: Run tests**

```bash
cargo test -p peri-acp --lib prompt::tests
```
Expected: All tests pass including new language tests.

- [ ] **Step 6: Commit**

```bash
git add peri-acp/src/prompt/prompt_test.rs
git commit -m "test(prompt): add language injection tests"
```

---

### Task 7: Full build verification

- [ ] **Step 1: Build the entire workspace**

```bash
cargo build 2>&1
```
Expected: Compiles cleanly with zero errors, zero warnings.

- [ ] **Step 2: Run full test suite**

```bash
cargo test 2>&1
```
Expected: All tests pass.

- [ ] **Step 3: Check for any remaining build warnings**

```bash
cargo clippy -- -D warnings 2>&1
```
Expected: No warnings.

---

## Self-Review Checklist

1. **Spec coverage**: Issue #13 (中英文不稳定) — ✓ language section injected, ✓ frozen at session/new, ✓ propagated to SubAgents
2. **Placeholder scan**: No TBD/TODO, all code is exact
3. **Type consistency**: `Option<String>` in FrozenSessionData/SessionState/SessionInfo → `Option<&str>` in build_system_prompt → `as_deref()` at call sites

## Critical Invariants

- Language section is in the **dynamic area** (after `__SYSTEM_PROMPT_DYNAMIC_BOUNDARY__`), so changing language between sessions does NOT invalidate the Anthropic prompt cache prefix
- Language is **frozen at session/new** — never changes mid-session, satisfying "系统提示词稳定性是第一优先级"
- SubAgents **inherit** language preference via `SystemPromptBuilder` closure capture
- `None` language → no injection → LLM infers from user input (current behavior preserved as fallback)
- `peri-tui/src/prompt.rs` is dead code NOT modified (only its tests call it)
