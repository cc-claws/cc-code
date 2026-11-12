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
| **Git Worktrees** | `--worktree` starts the session in an isolated checkout, `EnterWorktree` / `ExitWorktree` let the agent enter and leave one, and `isolation: "worktree"` isolates sub-agents — the session's working directory follows along |
| **HITL Approval** | Sensitive operations intercepted by the JEV semantic gate — a dedicated LLM judge that only prompts when it isn't sure |
| **Dual-Engine File Search** | Grep/Glob prefer the external ripgrep binary, falling back to the built-in Rust engine |

### v0.6.x New Features

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
| **Git Worktree Isolation: Startup Flag, Two Agent Tools, Four Hook Events and Sub-Agent Isolation** | v0.6.274 | Worktree used to exist only as a promise in the prompt text — nothing executable behind it. Isolation is now real: `--worktree [NAME]` starts the TUI inside an isolated checkout (stored under `~/.cc-code/worktrees`, branch `worktree/<name>`, random name when omitted, TUI only — the `acp` / `plugin` / `update` subcommands are unaffected), and on exit every still-open checkout is offered as keep / remove, where removal happens only after an explicit yes. Two agent tools come with it: `EnterWorktree` creates a `worktree/<name>` branch checkout off HEAD and switches the session's working directory into it (effective from the next turn), preferring the in-repo `.claude/worktrees` managed root (written to `.git/info/exclude` on creation, so `git status` stays clean) and falling back to `~/.cc-code/worktrees` outside a Git repo; names are validated per segment (whitelist, 64 chars) and a directory of the same name is reused instead of failing. `ExitWorktree` closes it out with keep / remove and refuses removal while uncommitted files or new commits exist unless `discard_changes` is set — and it only ever touches worktrees created by this session, never another session's or a manually added `git worktree add` directory. Four hook events ship alongside: `WorktreeCreate` / `WorktreeRemove` / `PermissionDenied` / `CwdChanged`; the hook runtime is now shared across components so tools outside the middleware chain can fire hooks, and a `WorktreeCreate` hook may veto the creation or hand out a directory of its own (in which case git is not called at all). Hook `permissionDecision` finally takes effect — allow skips the approval dialog, deny rejects on the spot. The session directory became a shared handle: ACP reads the cwd from it every turn and emits `notifications/peri/cwd_changed`, the TUI status bar follows that directory for its git detection, and a directory change appends a cwd reminder to the conversation and re-reads CLAUDE.md / AGENTS.md for the new location (the session-frozen system prompt is untouched). Sub-agent isolation works too: `isolation: "worktree"` runs the sub-agent in its own checkout, keeps the directory and writes its path back into the result when there are changes, deletes it when there are none, and keeps it when the change count cannot be determined (#1034). |
| **Auto-Rerouted Tool Calls No Longer Show a Declared-Name / Rerouted-Output Hybrid on Replay** | v0.6.273 | When the model declares tool A but passes tool B's arguments and auto-rescue reroutes the call to B, the history-replay view (after `StateSnapshot`, on `resume` or session restore) rendered a hybrid block: the header showed the *declared* name — e.g. `● Bash` with **no** argument summary, because the declared tool's argument key was absent — while the body showed the rerouted tool's output; the same block also flipped names between streaming and replay, since events carry the rerouted identity but the replay lookup re-read the declared name from the AI message. The reroute deliberately rewrites only the runtime `ToolCall` (events, approvals and breaker attribution read it) and keeps the model's declared name in history, but `BaseMessage::Tool` stores no tool name, so display could only look the declared one up. Tool result messages now record the tool that **actually produced** them (`BaseMessage::Tool.tool_name`, filled from `ToolResult.tool_name` at construction), and the display layer prefers it, falling back to the old AI-message lookup for legacy records; the argument summary still comes from the declared input — in a mismatch that input is precisely the target tool's parameters, which is what made the rescue fire. LLM history is unchanged — request serialization never carries the new field. Streaming, replay and `resume` now render one consistent identity (#1027). |
| **`files_with_matches` Output Paths Are Normalized on Windows** | v0.6.273 | Grep's `files_with_matches` (the default) / `files_without_matches` output printed Windows-native paths — `.\dir\file.ext`, sometimes mixed separators like `cc-tui/src/command\core\help.rs` — while `content` output and Glob already showed normalized forward-slash relative paths: `-l` / `-L` lines are whole paths with no `line:` colon, so `find_path_end` found nothing and `convert_rg_line` returned each line verbatim, skipping every normalization step (measured in session history: 1,171 of 1,182 such outputs carried backslashes). File-list modes now normalize each line as a path (`normalize_display_path`: relative to cwd, `\`→`/`, strip `./` prefix) through a mode-aware conversion, while `content` / `count` lines keep the `path:line:content` treatment — backslash literals inside matched text are never rewritten (#1029). |
| **Auto-Rescue Rebinds the Tool Name from Raw Dialect Input** | v0.6.272 | When the model writes the wrong tool name but fills the arguments in the *correct* dialect for that kernel (dsh: searching file contents while declaring `glob`, passing grep's `include`), auto-rescue could not self-heal and cost an extra LLM round-trip. Inbound normalization (`normalize_reasoning`) looks up the `ParamSpec` by the *declared* tool name, so a wrong name applies the wrong rename rules — `include` stayed put (Glob has `RENAME_NONE`) and canonical `Grep` rejected it, so the "exactly one candidate" test matched nothing. Rescue now re-normalizes the model's **raw dialect input** (`raw_tool_calls`) through each *candidate's* own `ParamSpec` before the strict schema check, so whether a call can be rescued no longer depends on the model naming the tool correctly. Deliberately narrow: parameter aliases such as `path → file_path` are still applied by the **declared** tool name, not per candidate, so the rescue surface does not widen; the whitelist (`Grep` / `Glob` / `Read`, plus read-only `Bash`), the "exactly one" ambiguity rule and the read-only safety boundary are unchanged. Verified end-to-end with real tools on cc / dsh / pi (#1018). |
| **Failed Grep / Glob Calls in Collapsed Tool Groups Now Show the Tool Header** | v0.6.271 | v0.6.269 taught failed Bash calls in collapsed tool groups to show their command line, but only Bash was covered. A failed `Grep` / `Glob` still rendered just a truncated error summary, so you could not tell which `pattern` had failed — and since both tools display as `Search`, only the pattern identifies the call. The collapsed-mode header condition is now `matches!(tool_name, "Bash" \| "Grep" \| "Glob")` (the header helper already emits `● Search(<args>)` for non-Bash tools), and the collapsed-group height estimator counts the extra header + truncated-summary rows so scroll anchoring does not drift. Normal mode only; the error body stays a truncated summary and detail mode is unchanged (#1014). |
| **Default Log Filter Downgrades `cc_middlewares::hooks` to warn** | v0.6.271 | The hooks loader runs repeatedly on the Windows close-window SessionEnd snapshot path (`main.rs` re-reads App state every 500 ms), and each call re-reads and serde-parses the global `settings.json`, the project `settings.json` and `settings.local.json` while emitting an info line per successful load. Measured on a user's log: 2,874,833 of 2,880,892 lines (99.79%) were hook-loader output, inflating one file to 402 MB at a steady 12 lines/s and drowning out useful logs. The default `EnvFilter` in `cc-agent/src/telemetry/subscriber.rs` now sets `cc_middlewares::hooks=warn`. Warning/error hook logs (parse failures, untrusted-project notices) are unaffected, and an explicit `RUST_LOG` still overrides it (#1016). |
| **MCP Tool Header Shows Server / Tool and Lists Arguments** | v0.6.270 | MCP tools were rendered poorly in the message area: the tool name was assembled with `to_pascal` into `McpTavilyTavilySearch` (server name repeated twice, no separator), and the argument summary fell back to "take the first string field" — since serde_json's `Map` iterates alphabetically, that hit `country` instead of `query` and dropped the key names, so you could not see which arguments were actually passed; the status-bar statistic showed the same mangled name. A new `parse_mcp_tool_name` now splits `mcp__{server}__{tool}` — the first segment is the server, the rest is the tool (a tool name may itself contain `__`). `format_tool_name` renders `server - tool (MCP)`, `format_tool_name_for_stats` takes the short tool name, and `format_tool_args` uses the new `mcp_args_summary` to print one `key: value` per top-level key (values JSON-serialized, truncated at `TOOL_ARGS_MAX_CHARS` like every other branch). The message-area header changes from `McpTavilyTavilySearch(China)` to `tavily - tavily_search (MCP)(country: "China", max_results: 8, ...)`, and the status bar from `McpTavilyTavilySearch` to `tavily_search`. Display-only — no change to tool execution, permission decisions or message content (#1011). |
| **Failed Bash Calls in Collapsed Tool Groups Now Show the Command Header** | v0.6.269 | In normal (non-`Ctrl+O`) mode, a failed Bash call inside a collapsed tool group rendered only a truncated error summary with no command line, so you could not tell which command had failed without switching to detail mode. The group renderer now emits `● Bash(<command>)` before the truncated summary (first 3 lines + `… +N lines (ctrl+o to expand)`), and the collapsed-group height estimator counts those extra rows so the first frame is not too short and scroll anchoring does not drift. Successful calls stay folded as before, and detail mode is unchanged (#1005). |
| **RTK `rewrite` Exit 3 No Longer Dropped** | v0.6.268 | Before running a command the Bash tool calls `rtk rewrite "<cmd>"` to gain RTK's output compression, but it accepted only `exit code 0`. RTK documents four codes — `0` (rewrite printed, allow rules cover every command in the chain), `1` (no rewrite), `2` (deny matched), `3` (**rewrite printed**, but an ask rule matched or allow rules do not cover every command, including when no rules are configured at all). Both `0` and `3` print a rewrite and both must be used; they differ only in whether a hook may auto-allow, and this call site sits *after* approval, where HITL has already decided. Because `3` covers the "no rules configured" default, common git commands (`git status` / `log` / `diff`) were silently falling back. `rtk_rewrite_succeeded()` now accepts `0`/`3` and rejects `1`/`2` plus any non-documented code. Measured on RTK 1.2.0: `git status` / `git log` / `git diff` / `cargo test` all return `3`. Approval semantics are unchanged — the gate still uses the pure-string `predict_rtk_rewrite` (zero subprocesses, #288) (#999, #1000). |
| **`cc-compress` Crate Extracted for CLI Output Compression** | v0.6.268 | CLI output compression filters lived inside `cc-tools/src/tools/output_filter.rs` with only three filters, while the rule set keeps growing per command; `cc-middlewares` hooks also needed `strip_ansi` and had to reach through `cc-tools`. A new leaf crate (`cc-compress`, peer of `cc-widgets`: zero internal workspace deps, pure functions — no process spawn, no env reads, no filesystem) now hosts them, starting with three generic facilities: ANSI stripping with `\r` progress overwrite normalization, a `never_worse` guard (`bytes/4` token estimate; falls back to raw when filtering costs more, so small outputs cannot grow), and adjacent-duplicate folding (runs of ≥3 identical lines collapse to `[xN]`, blank lines exempt, never across gaps). `cc-tools` now re-exports `strip_ansi` from the new crate and routes `filter_command_output` through `never_worse`. **No per-command filters were added**; `git log` / `diff` / `show` need argv rewriting (the upstream injects `--pretty=format:` and over-fetches one commit to detect truncation), which conflicts with the HITL invariant "gate evaluation == approval display == actual execution" and needs separate design (#1001, #1002). |
> Older releases (v0.6.0 – v0.6.267) are listed in the [CHANGELOG](./CHANGELOG.md).

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
