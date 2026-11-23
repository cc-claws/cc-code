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
| **OpenCode Go Gateway Session Header and Auth** | v0.6.300 | Pointing a provider's base URL at the OpenCode Go (`opencode.ai`) inference endpoint used to be rejected with 400 `Request is missing x-opencode-session and cannot be routed efficiently` — the gateway routes and aggregates prompt cache per session, so without a stable session header the endpoint was unusable, and the Anthropic endpoint additionally requires Bearer auth. `apply_opencode_session_header()` (`cc-agent/src/llm/mod.rs`) injects `x-opencode-session` from the ACP session ID (stable across turns); the header is deliberately not filtered by host, since a base URL may reach OpenCode through a self-hosted relay and unrecognized providers simply ignore the extra header, mirroring the existing unfiltered `x-session-id` passthrough. `is_opencode_endpoint()` (`url_util.rs`) matches `opencode.ai` / `*.opencode.ai` by host (with an `opencode.ai.evil.com` forgery test case); the Anthropic endpoint switches to `bearer_auth` while keeping `x-api-key`, and both OpenAI non-streaming and streaming paths inject the header. `build_reqwest_client()` also declares `User-Agent: cc-code/<version>` so the gateway no longer classifies the traffic as a generic SDK (#1110, #1111, contributed by ywsy). |
| **Screen Selection Follows the Message Area When a Dialog Opens** | v0.6.298 | After dragging over some text without moving the mouse, an agent-opened ask/approval dialog grows the bottom panels and pushes the message area up by a few rows. `ScreenSelection` stores absolute screen coordinates and only recomputed on mouse events, so the highlight stayed on the stale rows — painting over the dialog, and copying whatever text sat at the old rows (i.e. the content underneath the popup). A new `ScreenSelection::shift_rows(rows, remap)` runs right after `Layout::split` in `cc-tui/src/ui/main_ui/mod.rs`, before anything is drawn: both endpoints of the selection are moved by the delta of the message area's `y`, and when `remap` returns `None` (row occluded or clipped) the whole selection is dropped via `clear`, since keeping half a highlight would make the copied text disagree with what is shown. The in-list `TextSelection` re-derives rows from content anchors (`anchor_start`/`anchor_end`) every frame and is unaffected (#1108). |
| **Residual Result Bodies Use `text_soft`, and the Background-Sleep Rejection Message Was Rewritten** | v0.6.297, v0.6.298 | Continuing the one-screen-two-greys fix, the `(No output)` fallback in `render_group_bash_body`, the `(No output)` / `Done` states of `ToolBlock`, and the generic result bodies (Search matches etc.) in detailed `ToolCallGroup` mode were still `inactive`; they now use `text_soft`. The `⎿` elbow keeps its original colour and predicate (still `error`/`inactive` by `entry.is_error`) — only the body changed. `blocked_sleep_message` in `cc-tools/src/bash/mod.rs` now states the full reason, makes clear this is a `return Err` rejection rather than a warning, and no longer points at until-loop / `run_in_background` / "chain shorter sleeps", which are either blocked elsewhere or not the hint's call; the text is model-facing and stays English (cc-tools is a leaf crate without `cc-tui-core::i18n`) (#1105, #1107). |
| **Tool Summary Rows Share the Body's Grey** | v0.6.297 | The Read tool's collapsed summary `⎿ Read 32 lines` and the Glob summary took `inactive` (`#999999` in the default theme) while the adjacent Bash output body took `text_soft` (`#D0D0D0`), putting two different greys on one screen and reading as inconsistent rendering. Three summary sites in `cc-tui/src/ui/message_render/mod.rs` now use `text_soft`: the collapsed Read summary in `AssistantBubble`, the in-group Read summary, and the Glob summary plus its content lines in `ToolCallGroup`. The `⎿` elbow prefix stays `inactive`, so the "dim prefix, bright content" hierarchy is preserved; expanded bodies, Bash output colors and the theme palettes are untouched, and every other `inactive` consumer (status bar, placeholders, separators, collapse hints) is unaffected (#1105). |
| **Thinking Summary Titles Match the Status Bar's MUTED** | v0.6.295 | The message-stream summary row `Thought for 15s, read 2 files, ran 4 shell commands (ctrl+o to expand)` was colored by `theme::thinking_body()` (`inactive` blended 0.7 toward `subtle`, giving `#666666` in the default theme), while the status bar's hint row of the same semantic level used `MUTED` (`#999999`); one step of brightness apart, the title read heavier than the status bar beneath it. Both summary rows — the `AssistantBubble` `Reasoning` branch and the `ToolCallGroup` `thinking` branch — now take `theme::cur().inactive` instead: exactly `#999999` in the default theme, matching `MUTED`, and each of the other themes following its own palette. Thinking bodies are untouched and keep the darker `theme::thinking_body()`, so the title still sits one step above the content it heads, and `dim_markdown_lines` / `push_thinking_body` keep their existing colors (#1101, #1102). |
| **Long-Log Scrolling Stops Building Whole-Message Render Products** | v0.6.294 | Virtual rendering coupled measurement with publication: scrolling into the middle of a long message first built a render product for the entire message — lines, wrap map, links — only to throw it away, then copied the whole thing again when publishing the viewport; fast paging across several messages also accumulated loaded segments until the publish stage began evicting, lifting the process working set. Measurement and publication are now separate: `render_layout` gains `measure_viewport`, which resolves relative scroll positioning and heights without copying render lines, and `render` publishes only the viewport range — each segment contributes its visible lines plus one screen of measured buffer on each side, so a 40-row viewport scrolled into a 10,001-line log publishes 120 lines, row-for-row equivalent to the full render (the layout test compares against the same region of a reference render), with scroll anchors and link hits still exact. Segment residency moved to the `lru` crate's `LruCache` (a workspace dependency), evicting by bytes on every segment load, so ordinary segments share a 16 MiB estimated budget while a single oversized segment may hold the cache alone; the previous `evict` only ran at the end of publishing and picked victims by a `used_at` timestamp. `/gc` now counts virtual resident segments in the render-memory total, while the published window is still estimated separately to avoid double counting. `RenderWorker` caches `has_live_sources` instead of rescanning every source message each round to spot running tools (#1098, #1099). |
| **SessionStart Hook Rows Stay with the First User Message** | v0.6.293 | The summary and failures appear directly below the user message that started the session, including asynchronous failures that arrive in a later turn (#1096). |
| **Detail-Mode Thinking Rendered as a Single `∴` Line, with a Thinking Display Toggle** | v0.6.291 | Under `Ctrl+O` detail mode, thinking used to render on two lines — a `Thought for Ns` summary plus a `⎿` body — while the same thinking read `∴ Thinking for N chars` while active inside a tool group, so freezing it into detail mode swapped in a different shape; with the summary sitting beside the body, the elapsed time was also duplicated information. Detail mode now folds thinking into one continuous block led by `∴ ` (`THINKING_PREFIX`), continuation lines hanging-indented two columns (`THINKING_HANG`), width computed by `thinking_content_width(width)`, colored by `theme::thinking_body()` to match the summary row; the `AssistantBubble` `Reasoning` branch and the `ToolCallGroup` `thinking` branch render the body and `continue`, dropping the summary line entirely. A new `AppConfig.thinking_ui: Option<bool>` (`thinking_ui_enabled()` defaults to `true`, overridable per workspace via `merge_overrides`) mirrors the existing `hooks_ui` naming — it deliberately does not reuse `ThinkingConfig`, which carries the LLM reasoning parameters. `/config` gains a "Thinking Display" row (`ROW_THINKING_DISPLAY`, right after "Hooks Display", later row numbers shifted), wired through `RenderTask.show_thinking` + `RenderEvent::ToggleThinking(bool)` + `thinking_sensitive()` mixed into `render_key`, so a save takes effect immediately the same way `ToggleHookRows` does. On (default): detail mode shows the `∴` body with no summary row, normal mode keeps `Thought for Ns (ctrl+o to expand)`. Off: detail mode renders no thinking block at all, and normal mode keeps the summary but drops the `(ctrl+o to expand)` tail — expanding would show nothing, so the hint would contradict itself. Rendering only; thinking data always stays in the VM, so toggling back restores history (#1091, #1092). |
| **Monitor / Sleep Tools Removed, Blocking `sleep` Guard Kept** | v0.6.290 | v0.6.285 added `MonitorTool` and `SleepTool` plus a foreground `sleep` guard for the goal of keeping a `sleep N` out of the shell. In practice the two tools cost more than they earned — waiting on a condition is `run_in_background`'s job, and extra tools only lengthened the list and invited wrong picks. All four files (`monitor`, `monitor_test`, `sleep`, `sleep_test`) are gone, `TerminalMiddleware::build_tools` returns only `Bash` again, `tool_names()` returns `vec!["Bash"]`, and `cc-tools/src/lib.rs` drops the exports. The guard and its alternatives stay: `BLOCKED_SLEEP_MIN_SECONDS = 2`, `split_shell_subcommands`, `parse_sleep_seconds`, `detect_blocked_sleep_pattern` and the invoke-site check are untouched; `blocked_sleep_message()` only loses the "use the Sleep tool" sentence and keeps condition-waiting via `run_in_background: true` plus the until-loop. The `… +N lines (ctrl+o to expand)` hint for Write **new files** now matches the `⎿` prefix color — that path goes through `render_new_file_code` in `cc-widgets/src/diff/renderer.rs` rather than `push_visual_capped_output`, and hard-coded `theme.dim()` (`subtle`, #505050); the function takes a new `hint_color: Option<Color>` (cc-tui passes `inactive`), with `None` where no truncation hint can appear (#1089, #1090). |
| **Tool Names Down to #F2F2F2, Thinking Bodies Down to #666666** | v0.6.290 | v0.6.287 pushed tool names to pure white `#FFFFFF` and handed thinking bodies to `inactive` (`#999999`); in use the former was glaring and the latter still too bright on a dark background, both competing with body text. Two derived helpers follow the "derive rather than add a palette field" pattern (sidestepping the field-locking `palettes_parity_test`, all eight themes following their own palette): `theme::tool_name()` subtracts `TOOL_NAME_DIM = 13` per channel from `cur().text` with saturation, landing exactly on `#F2F2F2` in the default theme and returning named colors unchanged; `theme::thinking_body()` blends `inactive` toward `subtle` at 0.7, giving `153×0.3 + 80×0.7 ≈ 102` per channel — `#666666` (0.7 rather than 0.5 because thinking arrives in paragraphs, so half a step down stays too bright; 0.7 is what separates it from `inactive` elements such as hook rows and the queue preview). Coloring points: the tool-name span in `push_group_entry_header`, `ToolCallState::new(display_name, …)` and `name_style` in `ToolCallGroup` go through `tool_name()`; the unstyled spans in `dim_markdown_lines`, `push_thinking_body`'s `prefix_style`, the `AssistantBubble` thinking summary and the `ToolCallGroup` summary all move from `theme::cur().inactive` to `thinking_body()`. `inactive` itself is untouched — 60-odd other sites (hook summary rows, the queue preview, the Bash `⎿` prefix) still share it (#1088). |
> Older release notes are listed in the [CHANGELOG](./CHANGELOG.md).

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
