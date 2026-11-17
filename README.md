<div align="center">

**English** | [中文](README_ZH.md)

# cc-code

**Terminal coding agent powered by open-source models — Rust-built, `.claude/` ecosystem compatible**

DeepSeek-V4-Pro + Mimo-2.5Pro + GLM-5.1 driven, zero migration from `.claude/` config.

[![npm](https://img.shields.io/npm/v/@cc-claw/code)](https://www.npmjs.com/package/@cc-claw/code)
[![GitHub stars](https://img.shields.io/github/stars/cc-claws/cc-code?style=social)](https://github.com/cc-claws/cc-code/stargazers)
[![License](https://img.shields.io/badge/license-Apache%202.0-blue?style=flat-square)](LICENSE)
[![Website](https://img.shields.io/badge/website-cc--claw.com-orange?style=flat-square)](https://www.cc-claw.com)
[![GitHub last commit](https://img.shields.io/github/last-commit/cc-claws/cc-code?style=flat-square)](https://github.com/cc-claws/cc-code/commits/main)

<p align="center"><code>npm install -g @cc-claw/code</code></p>

### 🌐 Official Website: **[cc-claw.com](https://www.cc-claw.com)**

[Why cc-code](#why-cc-code) · [Auto Mode](#auto-mode-the-jev-semantic-gate) · [Core Capabilities](#core-capabilities) · [Install](#install) · [Nobody Coding](#how-we-built-cc-code-with-nobody-coding) · [Acknowledgments](#acknowledgments)

</div>

## ❤️ Sponsor

> [Want to appear here?](mailto:wismyzhizi2018@gmail.com)

<details open>
<summary>Click to collapse</summary>

[![Kimi K2.6](assets/partners/logos/kimi.png)](https://platform.moonshot.cn/console?aff=4D2GLR)

Kimi K2.6 is an open-source, native multimodal agentic model from Moonshot AI, built for long-horizon coding, coding-driven design, and swarm-based task orchestration. It handles complex end-to-end engineering work across front-end, DevOps, performance optimization, and full-stack workflows. Invite code: 4D2GLR. [Register here](https://platform.moonshot.cn/console?aff=4D2GLR)

---

<table>
<tr>
<td width="180"><a href="https://platform.xiaomimimo.com?ref=JBEYTF"><img src="assets/partners/logos/mimo.png" alt="Xiaomi MiMo" width="150"></a></td>
<td>Top-tier model MiMo V2.5 from Xiaomi. Register with invite code: both get ¥10 API credit + 10% off first order. Invite code: JBEYTF. <a href="https://platform.xiaomimimo.com?ref=JBEYTF">Register here</a> (auto-filled on registration · credit valid for 40 days)</td>
</tr>

<tr>
<td width="180"><a href="https://www.bigmodel.cn/glm-coding?ic=MR7BVITFAY"><img src="assets/partners/logos/glm.png" alt="GLM" width="150"></a></td>
<td>GLM Coding Plan from Zhipu AI — top-tier coding model in China, compatible with 20+ mainstream tools, best value. <a href="https://www.bigmodel.cn/glm-coding?ic=MR7BVITFAY">Join now</a></td>
</tr>
</table>

</details>

---

## Why cc-code?

| Comparison | Other Terminal Agents | cc-code |
|------------|----------------------|------|
| Runtime | Node.js / Bun, easily eats 1GB RAM | Rust native, fast startup, ~50MB memory |
| Model Lock-in | Locked to one LLM | Switch freely: Anthropic, OpenAI-compatible, DeepSeek, GLM |
| Prompt Cache | Recompute every turn, wasting tokens | Frozen system prompt, 95-99% cache hit rate |
| Tool Loading | All tools stuffed into every request | Core tools resident, rest lazy-loaded via Tool Search |
| IDE Integration | Terminal only | ACP protocol, Zed and other IDEs connect directly |
| `.claude/` Ecosystem | Incompatible | Use `.claude/` config, agents, skills, hooks, MCP directly |
| Safety Gate | Prompts you for every risky command | JEV semantic gate — a dedicated LLM judge that only prompts when it isn't sure |

---

## Auto Mode: The JEV Semantic Gate

`Auto` is the default permission mode (fail-closed). Instead of asking you about every call, it decides in two layers:

- **Deterministic layer** — your own rules, the built-in hard-deny list, and a read-only fast lane. Zero cost, no credentials required.
- **JEV semantic gate** — a dedicated LLM judge (`JEV_MODEL`, default `openrouter/typesafe/jev-1.13`) reached over its own endpoint (`JEV_ENDPOINT`) with its own key (`JEV_API_KEY`). It scores each call against a set of conditions and pops a single confirmation prompt only when the call lands in the uncertain band.

No credentials configured? The deterministic layer still runs and an LLM classifier takes over as fallback; when there is no confirmation channel (sub-agents, `-p`) the call is denied. The session model is never involved — judging is offloaded to JEV, so safety checks don't spend your context tokens. `Shift+Tab` toggles between `Auto` and `Bypass`.

> For deep-dive architecture, fallback classifiers, and troubleshooting, see the [Auto Mode & Permissions Guide](docs/auto-mode-guide.md).

---

## Core Capabilities

| Capability | Description |
|------------|-------------|
| **Rust Native** | Fast startup, low memory, zero runtime overhead |
| **Context Optimized** | System prompt frozen + dynamic content isolated, no token waste |
| **Multi-LLM Support** | Anthropic / OpenAI-compatible APIs, DeepSeek, GLM — switch freely |
| **`.claude/` Ecosystem Compatible** | `.claude/` config, agents, skills, hooks, MCP, sub-agents all reusable |
| **Streaming Markdown** | Code blocks, tables, diffs rendered in real-time |
| **ACP Protocol** | Connect to Zed and other IDEs, or build your own "Cloud Code" platform |
| **Auto Compact** | Long sessions auto-compressed, stays fast and cheap |
| **Sub-Agent Concurrency** | Background sub-agents run in parallel with fork and background modes |
| **Git Worktrees** | `--worktree` starts the session in an isolated checkout, `EnterWorktree` / `ExitWorktree` let the agent enter and leave one, and subsequent shell and file calls use that directory immediately. `isolation: "worktree"` gives sub-agents their own checkout |
| **HITL Approval** | Sensitive operations intercepted by the JEV semantic gate — a dedicated LLM judge that only prompts when it isn't sure |
| **Dual-Engine File Search** | Grep/Glob prefer the external ripgrep binary, falling back to the built-in Rust engine |
| **Monitor / Sleep Tools** | `Monitor` waits on a condition in the background with an output file, `Sleep` waits without occupying a shell — a foreground bare `sleep N` is rejected with guidance |

### v0.6.x New Features

**No more raw `<system-reminder>` leaking into hooks and the UI (v0.6.285):** The prompt handed to hooks used to carry the raw `<system-reminder>` tags that `cc-tui` uses to append background-task notifications, so external components such as the clawd desktop pet displayed the XML verbatim. `cc-middlewares` now excises the whole block by range before invoking `UserPromptSubmit`, and the UI distinguishes two shapes by whether any user text precedes the opening tag — a reminder appended after user input is cut together with its body (the notification is already shown as a system note), while a summary wrapped entirely in tags keeps its body and only loses the markup. See [CHANGELOG](./CHANGELOG.md).

**Foreground bare `sleep` now points at Monitor / Sleep (v0.6.285):** A model writing `sleep 30` blocks the whole shell — it can't be interrupted and produces no output. A foreground `Bash` call starting with `sleep N` (N ≥ 2 seconds) is now rejected outright with a hint to use `Monitor` for condition waiting or `Sleep` for a plain wait (the latter occupies no shell process and is Ctrl+C interruptible); sub-2-second rate-limit uses and background execution still pass. See [CHANGELOG](./CHANGELOG.md).

**Approval dialogs with descriptions and full-file diffs (v0.6.206):** When a tool call carries a `description`, the approval dialog shows it on its own line under the title. `Write`/`Edit` approvals now preview the full before/after file diff (new files get a numbered, syntax-highlighted code preview instead), falling back to the fragment diff when the file can't be read, is over the limit, or the fragment can't be located on disk. The dialog layout and colors follow the reference implementation: full-width solid rule on top, dashed rules around the body, numbered options and a fixed shortcut hint. See [CHANGELOG](./CHANGELOG.md).

**Grep / Glob aligned with the latest reference spec (v0.6.202):** `Grep` descriptions now stress integration with the permission-review UI and clickable `file_path:line_number` links, document ripgrep escaping and multi-line patterns, and add a `context` alias (equivalent to `-C`); `Glob` returns cwd-relative paths newest-first, rejects literal `"undefined"` / `"null"`, reports `No files found` uniformly on no match, and uses the standard truncation message. Both the ripgrep engine and the pure-Rust engine relativize paths, and the system prompt now steers Grep/Glob first instead of bare `grep`/`rg`/`find` in a shell. See [CHANGELOG](./CHANGELOG.md).

**Project hooks and user/plugin agent definitions (v0.6.199):** Hooks from the project's `.claude/settings.json` are now actually loaded (previously only global hooks were read and project hooks silently never ran); trust them explicitly with `cc-code trust-project` (or `CC_CODE_TRUST_PROJECT_HOOKS=1`). Agent definitions are now also found at the user level (`~/.claude/agents/`) and from plugins, with lookup order project > user > plugin > built-in. See [CHANGELOG](./CHANGELOG.md).

**Scrolling and input responsiveness (v0.6.201):** Input processing continues while the background renderer prepares a layout. Stable scrolling reuses the prepared width, and input, streaming output and background updates share one redraw schedule. Fullscreen transitions, resizing and selection copying keep their frame order. See [CHANGELOG](./CHANGELOG.md).

**Read-only Bash in tool groups (v0.6.197):** File-inspection Bash (`ls`, `cat`, `grep`, `rg`, `find`, `head`, `awk`, …) now folds into the same read-only group as `Read` / `Grep` / `Glob`, with the collapsed summary reading `ran N shell commands` beside `read` / `searched`. Classification is purely lexical on the frontend — the model no longer passes a `file_inspect` argument (removed from the tool schema) — and it tolerates `2>/dev/null` / `2>&1` / `>&2` as well as separators and `>` inside quotes. Thinking rows collapsed into one-line summaries: `∴ Thinking for Ns, …` while running, `  Thought for Ns, …` once frozen, with the body behind `Ctrl+O`. Auto-compact no longer clears the message area, and the stuck detector no longer mistakes consecutive edits for a loop. See [CHANGELOG](./CHANGELOG.md).

**Collapsed file-inspection Bash (v0.6.195):** Bash calls that only read files can now fold into the read-only tool group. The model flags them with the optional `file_inspect` argument (a single command counts too), the collapsed summary reads `ran N shell commands` beside `read` / `searched`, failed commands keep their exit code and stderr in the collapsed view, and `Ctrl+O` expands every command and its output. Running, backgrounded and explicitly-timed-out commands never fold, so live output and `Ctrl+B` stay untouched. See [CHANGELOG](./CHANGELOG.md).

**Image attachment links (v0.6.192):** Each sent image gets a `  ⎿  [Image #N]` line below your message. With a saved path and OSC 8 enabled, the reference opens the local image in supported terminals (kept in its own soft color, no artificial underline, never recolored link-blue). `DISABLE_HYPERLINKS=1` or a missing path leaves plain text; save failures do not block sending. Images use the system temp directory, and links are not guaranteed after restoring a session. Phase 1 does not send local paths to the model. See [CHANGELOG](./CHANGELOG.md) for the full scope.

**Skill workflows (v0.6.191):** Matching tasks use the core `Skill` tool, while `/skill-name args` loads instructions directly. Skill cards show a compact loading result; `context: fork` waits for an isolated agent, and active skill instructions survive compaction. See [skill behavior](./spec/global/domains/skill-trigger.md).

**Interface language:** TUI and CLI help support English and Simplified Chinese. Use `/lang en` or `/lang zh-CN` to save your preference. Without a saved language, startup detects `LC_ALL`, `LANGUAGE`, `LC_MESSAGES` and `LANG`; unsupported locales fall back to English. See [CHANGELOG](./CHANGELOG.md) for unreleased changes.

**Typing UX aligned with the reference implementation (v0.6.166):** Suggestion overlays (slash commands / skills / @ files) are now interactive while the agent is running — arrows/Tab navigate, Enter completes only a selected candidate, Esc dismisses the overlay first and interrupts the agent on a second press. "Send now" (steer) with a foreground command running waits a 2s grace period, then moves it to the background automatically. The status-bar permission label now shows a `⏵⏵` prefix. See [CHANGELOG](./CHANGELOG.md).

**Sub-agent cancellation (v0.6.139):** Just tell the main agent to stop a sub-task — it calls `Agent(action="stop", task_id="bg-…")` to cancel by exact `task_id` (**no fuzzy matching**, so look-alike sibling tasks are never stopped by mistake). The receipt carries the task summary so the agent can confirm what it stopped; the timeline prints `• Agent "x" was stopped by main Agent`, and the drawer row turns into a **red filled `●`** with `stopped by main Agent` (keyboard stops show `stopped by user`). While waiting, the main viewport shows `✻ Waiting for N background agents to finish`.

**Sub-agent prompt cards (v0.6.139):** Press `Ctrl+O` in the main viewport to switch detailed mode; the launch summary of background sub-tasks then turns from the compact tree into classic tool cards — `● Agent(task preview)` + `  └ Backgrounded agent` + `  └ Prompt:` followed by the **full** instruction text (6-column hanging indent, wrapped). Press `Ctrl+O` again to collapse. See [CHANGELOG](./CHANGELOG.md) for unreleased changes.

| Feature | Version | Description |
|---------|---------|-------------|
| **Message-Stream Colors Reverted to Plain INACTIVE and Block-Boundary Spacing Restored** | v0.6.289 | Two follow-up fixes to the v0.6.287 color pass. Thinking bodies go back to `theme::cur().inactive`: the derived `theme::thinking_body()` blended `inactive` 25% toward `thinking` and produced `#9B9DAC`, a grey with a blue-violet cast that clashed with the hook summary line sitting next to it, which uses plain `inactive`; the derivation is deleted and all three coloring points (thinking body, `Thought for Ns` summary, thinking lines inside `ToolCallGroup`) now use `inactive`, so each of the eight themes follows its own palette with no hue jump on one screen. The user bubble now turns white only when the agent actually starts working: it used to flip the moment Enter handed the message over, which only means "queued", so a turn failing halfway showed white text with nothing happening; `App::mark_turn_user_dispatched()` flips it on the agent's first real output (an `AssistantChunk` body text or a `ToolStart`) and rebuilds once from the tail, while pure `AiReasoning` does not count, queued messages are outside `view_messages`, and history snapshots stay `dispatched: true`. A block-boundary blank line now precedes the reasoning branch inside a single assistant bubble: the compression notice (`📦 [ACP] Compressed …`) arrives as a text block in the same bubble as the `Thought for Ns` summary, and `ContentBlockView::Text` ended without a separator while `Reasoning` only added one at its own tail, so a `Text` → `Reasoning` boundary inside one message had no gap; the line is pushed only when `lines` is non-empty and its last line carries non-whitespace content, leaving the thinking-first path (background continuation, AskUser bubbles) unchanged (#1082, #1084, #1083). |
| **PermissionRequest Hook Decisions Are Read, and Circuit-Breaker Skips Are No Longer Silent** | v0.6.288 | When an external approver (the clawd desktop pet, say) takes part in permission decisions over an HTTP hook, peri received the response and behaved as if it had never arrived: the only trace was a `1 PermissionRequest hook ran · 0ms` line, with neither the decision nor any failure reason. Two causes. First, the response field was never wired up — `SyncHookResponse.hook_specific_output` lacked a camelCase rename, so serde looked for `hook_specific_output` while the real JSON carries `hookSpecificOutput`, and the whole field was dropped silently; this hit `PermissionRequest` as well as `PreToolUse`'s `updatedInput` / `permissionDecision`, and the existing tests missed it because they constructed Rust structs rather than parsing real JSON. The field now carries `#[serde(default, rename = "hookSpecificOutput", alias = "hook_specific_output")]`, and a new `HookSpecificOutput::PermissionRequest` variant plus a `PermissionRequestDecision` type (`behavior` of `allow` / `deny`, `allow` optionally carrying `updatedInput`) parses the reply: allow becomes `PermissionOverride{decision: Allow}` and skips the approval dialog, deny becomes `PermissionOverride{decision: Deny}` and rejects on the spot with its reason — matching the reference implementation's `PermissionRequestHookSpecificOutputSchema`. Second, the circuit-breaker window was booked as a clean run: a failed HTTP hook connection trips the endpoint for 300 seconds, and while tripped peri short-circuits to `Allow` without sending a single request, logging that at debug level where the default `RUST_LOG=info` hides it — yet the run was still accounted as "ran=1, took 0ms", inventing a fake successful `PermissionRequest hook ran · 0ms` record. A measured log showed 37 consecutive `HTTP hook request failed, tripping circuit breaker for 300s` on one endpoint, every later trigger then showing 0ms. The short-circuit now returns `HookAction::Error` with the reason spelled out ("endpoint circuit breaker open (cooling down 300s)"), matching the reference implementation's approach of surfacing a visible `non_blocking_error` record for a failed hook without blocking the call. `HookAction::Error` still does not block the tool call — approval falls back to peri's own HITL dialog (#1078, #1079). |
| **Tool Names, Thinking Body and Pending User Messages Re-colored** | v0.6.287 | Three message-stream color fixes, all in the "make the fixed field easier to scan" direction, with no layout or interaction change. Tool names were `TEXT_SOFT` (#D0D0D0), the same shade as the argument summary on the same line, so two fields competed for attention; they are now `TEXT` (#FFFFFF), making the name the only emphasized fixed field in the row while the `●` indicator keeps carrying status (PR #914 had reverted this to `TEXT_SOFT`; the latest preference restores it), applied to both single `ToolBlock` and `ToolCallGroup` entries. Thinking bodies used `subtle` (#505050), nearly unreadable on a dark background; a new derived `theme::thinking_body()` blends `inactive` (#999999) 25% toward `thinking` (#A2A9E4), keeping the violet tint yet staying clearly below body text — derived rather than a new palette field, so all eight themes follow their own `inactive`/`thinking` and the field-locking `palettes_parity_test` is untouched, with the summary line lifted the same way while preserving "active and frozen share one color" so freezing shows no brightness jump. User message bodies had no foreground color (terminal default) and could not express whether the agent had picked the message up; `UserBubble` gains a `dispatched` flag rendering `MUTED` while queued and `TEXT` once sent, set by `mark_dispatched()` before `apply_pipeline_action`, always `true` for history snapshots and `false` for sub-session messages still awaiting delivery confirmation; the flag participates in `Hash`, otherwise the render thread serves a cached grey line (#1074, #1075). |
| **Raw `<system-reminder>` No Longer Leaks into Hooks or the UI** | v0.6.285 | Background-task notifications were appended to user input as `<system-reminder>` XML and handed verbatim to external components such as the clawd desktop pet, because `cc-tui` stored the concatenated text and `cc-middlewares` passed the whole Human message as the `UserPromptSubmit` hook prompt. The UI-side cleaner only stripped the tags at the leading and trailing edges, so a tag in the middle survived. Both sides now excise by range: `cc-middlewares` cuts the whole block whenever an opening tag appears (including everything after an unclosed tag), and `cc-tui-core` distinguishes two shapes — a reminder appended after user text is cut with its body (the notification is already rendered as a system note), while a summary wrapped entirely in tags keeps its body and only loses the markup. The background-notification check now runs after stripping, so a concatenated shape is not truncated into a bare system note on the way (#1071). |
| **Foreground Bare `sleep` Points at Monitor / Sleep** | v0.6.285 | A model writing `sleep 30` or `sleep 5 && check` blocks the whole shell: the process cannot be interrupted and produces no output. A foreground `Bash` call starting with `sleep N` (N ≥ 2 seconds, no following pipeline) is now rejected with a hint to use `Monitor` (condition waiting, returns a `taskId` and output file) or `Sleep` (`duration_seconds`, occupies no shell process, Ctrl+C interruptible). Sub-2-second rate-limit uses pass through, as do `run_in_background` and `Monitor` itself (#1070). |
| **First-Turn Session Title Is No Longer Polluted by Thinking Fragments** | v0.6.282 | After sending the first message in a fresh session, the title bar showed not the session topic but the model's thinking (e.g. "我们需要回答用户？…"), and the polluted title was persisted into `threads.title`. The title-extraction request hard-coded `max_tokens: 30` while inheriting the session's thinking configuration — with a reasoning model the thinking and the answer share one output budget, the 30 tokens were eaten entirely by thinking, the API returned a truncated result with `stop_reason = max_tokens`, and the `content` carried a thinking fragment instead of a title, which was rendered and stored verbatim. The hard-coded `max_tokens` is gone (it is a ceiling, not a reservation — relaxing it costs nothing extra) and a truncation guard now drops any response with `stop_reason == MaxTokens`, falling back to the local deterministic title; the same lesson had already been recorded on the rule-extraction path (#1064, #1065). |
| **Rapid Enter Presses Are No Longer Misread as a Multi-line Paste** | v0.6.281 | On Windows Terminal, pressing Enter repeatedly while the agent was running (loading) occasionally made a `[Pasted text #N +X lines]` placeholder appear in the input box out of nowhere, even though nothing had been pasted. The start window of the simulated-paste detector — which folds a rapid burst of per-character Key events into one `Event::Paste` on terminals without bracketed paste — was implemented with `pump.next(START_WINDOW = 1 ms)`, but the input pump returns already-buffered events immediately without honouring that deadline, so "is another event coming within 1 ms?" degraded into "does the pump happen to have a queued event right now?": rendering / streaming on the main thread backs the queue up, several Enter presses pile into it, they merge into `Event::Paste("\n\n")` and `paste_line_count > 1` folds them into the placeholder. Every event now carries the arrival instant stamped as the input thread returns from `event::read()`, and both windows are decided by the real arrival gap; events that are already buffered but lie outside the window are handed back to the queue and processed as separate key presses, so nothing is swallowed or reordered. The same change removes the mirror-image hazard of fast typing during a stalled frame being misread as a paste (#1060, #1061). The same release repairs `cc-code update` on Windows, where it had become a guaranteed failure at the extraction step: v0.6.279 moved overwrite semantics onto a pre-extract delete of the version directory, but the downloaded zip sat inside that very directory and was deleted along with it, so `ExtractToDirectory` reported a missing source path. The archive is now written to the system temp directory and is still removed by variable at the end, leaving no residue behind, while overwrite semantics and the failure-path cleanups are unchanged. Versions 0.6.279 and 0.6.280 embed a script reference pinned to the defective commit and therefore cannot self-update — they are the last ones needing a manual `irm … \| iex`; from 0.6.281 on, `cc-code update` works again (#1058, #1059). |
| **Failed-Turn Summary Line Survives the Trailing `Done`** | v0.6.280 | When a turn ended with an LLM error (an unreachable provider endpoint, say), the area under the `Agent Error` card went blank: the summary line you normally see on success — `✻ Charged for 36s · done 20:29` — disappeared, so the turn's elapsed time and completion stamp were lost. The executor always emits `Error` first and then `Done`, so `handle_error` wrote the failed summary and the trailing `handle_done` cleared it right away, because `reconcile_already_done` still marked the turn as abnormally terminated; that also overwrote `persist_last_task_summary()` with `None`, losing the elapsed time on `-c` / `-r` restore. The clearing branch is now guarded by `last_summary_is_error()`, so a settled failed summary is preserved and the error card reads `✻ Failed after Xs · HH:MM`; interrupt and cancel still clear it as before. The same release unblocks the Clippy gate, which the 1.100.0 toolchain had turned red on `cc-widgets` test code (#1055, #1056). |
| **Worktree Status Refresh and Reliable Task Completion** | v0.6.275 | The status bar follows EnterWorktree / ExitWorktree during the current prompt, including ExecuteExtraTool calls, and discards stale Git queries. Finished, cancelled and failed executions explicitly stop the main event pump, drain queued events and send one completion notification, fixing the UI remaining on Retrieving when cached worktree tools retain a channel sender. Thirteen regression tests cover directory notifications, pump shutdown and TUI state (#1039, #1040). |
| **Git Worktree Isolation: Startup Flag, Two Agent Tools, Four Hook Events and Sub-Agent Isolation** | v0.6.274 | Worktree used to exist only as a promise in the prompt text — nothing executable behind it. Isolation is now real: `--worktree [NAME]` starts the TUI inside an isolated checkout (stored under `~/.cc-code/worktrees`, branch `worktree/<name>`, random name when omitted, TUI only — the `acp` / `plugin` / `update` subcommands are unaffected), and on exit every still-open checkout is offered as keep / remove, where removal happens only after an explicit yes. Two agent tools come with it: `EnterWorktree` creates a `worktree/<name>` branch checkout off HEAD and switches the session's working directory into it (effective from the next turn), preferring the in-repo `.claude/worktrees` managed root (written to `.git/info/exclude` on creation, so `git status` stays clean) and falling back to `~/.cc-code/worktrees` outside a Git repo; names are validated per segment (whitelist, 64 chars) and a directory of the same name is reused instead of failing. `ExitWorktree` closes it out with keep / remove and refuses removal while uncommitted files or new commits exist unless `discard_changes` is set — and it only ever touches worktrees created by this session, never another session's or a manually added `git worktree add` directory. Four hook events ship alongside: `WorktreeCreate` / `WorktreeRemove` / `PermissionDenied` / `CwdChanged`; the hook runtime is now shared across components so tools outside the middleware chain can fire hooks, and a `WorktreeCreate` hook may veto the creation or hand out a directory of its own (in which case git is not called at all). Hook `permissionDecision` finally takes effect — allow skips the approval dialog, deny rejects on the spot. The session directory became a shared handle: ACP reads the cwd from it every turn and emits `notifications/peri/cwd_changed`, the TUI status bar follows that directory for its git detection, and a directory change appends a cwd reminder to the conversation and re-reads CLAUDE.md / AGENTS.md for the new location (the session-frozen system prompt is untouched). Sub-agent isolation works too: `isolation: "worktree"` runs the sub-agent in its own checkout, keeps the directory and writes its path back into the result when there are changes, deletes it when there are none, and keeps it when the change count cannot be determined (#1034). |
> Older releases (v0.6.0 – v0.6.273) are listed in the [CHANGELOG](./CHANGELOG.md).

---

## Install

Binaries available for macOS (x86_64 / Apple Silicon), Linux (x86_64 / aarch64), and Windows (x86_64).

### npm (Recommended)

```bash
npm install -g @cc-claw/code
```

No install script runs: the binary ships as a platform-specific optional
dependency (`@cc-claw/code-<platform>-<arch>`), and npm picks the one that
matches your OS and architecture. That means plain installs, `--ignore-scripts`,
pnpm and bun all work, and nothing is fetched from GitHub at install time.

> [!NOTE]
> Installing with `--omit=optional` / `--no-optional` leaves the package without
> its binary; `cc-code` then exits with a message explaining how to reinstall.

Windows on ARM has no native build yet — install the x64 build explicitly with
`npm install -g --arch=x64 @cc-claw/code`.

### Upgrade

```bash
npm update -g @cc-claw/code
```

### macOS / Linux (Script)

```bash
curl -fsSL https://raw.githubusercontent.com/cc-claws/cc-code/main/scripts/install.sh | bash
```

### Windows (PowerShell)

```powershell
irm https://raw.githubusercontent.com/cc-claws/cc-code/main/scripts/install.ps1 | iex
```

---

## How We Built cc-code with Nobody Coding

**Nobody Coding** means exactly what it sounds like. No human wrote a single line of cc-code — not the architecture, not the TUI, not the harness tuning that makes open-source models reliable in an Agent loop. Humans decide *what*. AI figures out *how*. You're not pair programming — you're product managing an engineer that never sleeps. 99% of cc-code was built this way.

> Recent commits are almost entirely DeepSeek, Mimo, and GLM. Claude was just there in the beginning.

### Typical Workflow

| When you... | Pipeline kicks off |
|---|---|

---

## Repository Structure

```text
cc-code/
├── cc-agent/                # Core: Agent loop, tool system, persistence, telemetry
│   └── README.md              # Agent framework guide
├── cc-tools/                # Leaf tool crate: stateless core tools and cross-platform process wrapper
│   └── README.md              # Tool catalog and contracts
├── cc-terminal/             # Cross-platform terminal layer: backend, viewport, clipboard, ConPTY
│   └── README.md              # Terminal backend and capabilities
├── cc-middlewares/           # Middleware: HITL, SubAgent, Skills, MCP, Hooks (file/terminal tools live in cc-tools / cc-terminal)
│   ├── README.md              # Middleware overview
│   └── CLAUDE.md              # Development guide and traps
├── cc-tui-core/             # Session core & display model: i18n, theme, markdown, MessageViewModel
│   └── README.md              # Display model and core state
├── cc-tui/                  # TUI application (Ratatui)
│   ├── README.md              # TUI usage guide
│   └── CLAUDE.md              # Development guide and traps
├── cc-acp/                  # ACP service layer: bridges TUI/IDE with Agent
│   └── README.md              # ACP architecture and data flow
├── cc-widgets/              # Widget component library
│   └── README.md              # Component list and examples
├── cc-compress/            # Leaf crate: CLI output compression filters (ANSI, token guard, dedup)
│   └── README.md              # Compression filters and contracts
├── cc-lsp/                  # LSP client library
│   └── README.md              # LSP operations and config
├── langfuse-client/           # Langfuse telemetry client
│   └── README.md              # Telemetry config and usage
├── npm/                       # npm package: platform optional dependencies + bin launcher
│   └── README.md              # npm package docs
├── scripts/
│   ├── install.sh             # macOS / Linux installer
│   └── install.ps1            # Windows installer
├── spec/                      # Design docs and specs
│   ├── global/                # Global architecture docs
│   ├── issues/                # Issue analysis docs
│   └── archive/               # Archived feature specs
├── docs/                      # Long-form docs (prd, adr, designs, acp)
├── human/                     # Manual walkthrough checklists
├── CLAUDE.md                  # Project development guide
├── CHANGELOG.md               # Version changelog
├── CONTRIBUTING.md            # Contribution guide
├── DEVLOG.md                  # Development log (milestones)
├── README.md
├── README_ZH.md
├── TUI-STYLE.md               # TUI command / shortcut / interaction reference
└── LICENSE                    # Apache 2.0
```

---

## Acknowledgments

| Project | Description |
|---------|-------------|
| [Peri (KonghaYao)](https://github.com/KonghaYao/peri) | This project is forked from Peri, distributed under Apache 2.0. Credit to the original author. |
| [Superpowers](https://github.com/obra/superpowers) & [Matt Pocock's Skills](https://github.com/mattpocock/skills) | Skill suites driving cc-code's AI engineering workflow |
| [ACP](https://agentclientprotocol.com/) | Open protocol for agent-IDE communication |
| [rmcp](https://github.com/anthropics/rmcp) | Rust MCP client library |
| [Ratatui](https://ratatui.rs) & [Tokio](https://tokio.rs) | TUI framework and async runtime |
| [Langfuse](https://langfuse.com) | LLM observability |
| [Zed](https://zed.dev) | First ACP-compatible IDE, proved the protocol works |

---

## Star History

[![Star History Chart](https://api.star-history.com/svg?repos=cc-claws/cc-code&type=Date)](https://www.star-history.com/#cc-claws/cc-code&Date)

---

## License

[Apache License 2.0](LICENSE) — free to use, modify, and distribute, including commercial use.
