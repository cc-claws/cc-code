<div align="center">

**English** | [中文](README_ZH.md)

# cc-code

**Terminal coding agent powered by open-source models — Rust-built, Claude Code compatible**

DeepSeek-V4-Pro + Mimo-2.5Pro + GLM-5.1 driven, zero migration from `.claude/` config.

[![npm](https://img.shields.io/npm/v/@cc-claw/code)](https://www.npmjs.com/package/@cc-claw/code)
[![GitHub stars](https://img.shields.io/github/stars/cc-claws/cc-code?style=social)](https://github.com/cc-claws/cc-code/stargazers)
[![License](https://img.shields.io/badge/license-Apache%202.0-blue?style=flat-square)](LICENSE)
[![Website](https://img.shields.io/badge/website-cc--claw.com-orange?style=flat-square)](https://www.cc-claw.com)
[![GitHub last commit](https://img.shields.io/github/last-commit/cc-claws/cc-code?style=flat-square)](https://github.com/cc-claws/cc-code/commits/main)

<p align="center"><code>npm install -g @cc-claw/code</code></p>

### 🌐 Official Website: **[cc-claw.com](https://www.cc-claw.com)**

[Why cc-code](#why-cc-code) · [Core Capabilities](#core-capabilities) · [Install](#install) · [Nobody Coding](#how-we-built-cc-code-with-nobody-coding) · [Acknowledgments](#acknowledgments)

</div>

## ❤️Sponsor

> [Want to appear here?](mailto:wismyzhizi2018@gmail.com)

<details open>
<summary>Click to collapse</summary>

[![Kimi K2.6](assets/partners/logos/kimi.png)](https://platform.moonshot.cn/console?aff=cc-code)

Kimi K2.6 is an open-source, native multimodal agentic model from Moonshot AI, built for long-horizon coding, coding-driven design, and swarm-based task orchestration. It handles complex end-to-end engineering work across front-end, DevOps, performance optimization, and full-stack workflows. [Register here](https://platform.moonshot.cn/console?aff=cc-code)

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
| Claude Code Ecosystem | Incompatible | Use `.claude/` config, agents, skills, hooks, MCP directly |

---

## Core Capabilities

| Capability | Description |
|------------|-------------|
| **Rust Native** | Fast startup, low memory, zero runtime overhead |
| **Context Optimized** | System prompt frozen + dynamic content isolated, no token waste |
| **Multi-LLM Support** | Anthropic / OpenAI-compatible APIs, DeepSeek, GLM — switch freely |
| **Claude Code Compatible** | `.claude/` config, agents, skills, hooks, MCP, sub-agents all reusable |
| **Streaming Markdown** | Code blocks, tables, diffs rendered in real-time |
| **ACP Protocol** | Connect to Zed and other IDEs, or build your own "Cloud Code" platform |
| **Auto Compact** | Long sessions auto-compressed, stays fast and cheap |
| **Sub-Agent Concurrency** | Background sub-agents run in parallel with fork and background modes |
| **HITL Approval** | Sensitive operations auto-intercepted with auto-classifier and shared-mode |
| **Dual-Engine File Search** | Grep/Glob prefer the external ripgrep binary, falling back to the built-in Rust engine |

### v0.6.x New Features

| Feature | Version | Description |
|---------|---------|-------------|
| **Jev Rule Cache Pretty-Printed JSON** | v0.6.118 | The Jev security-rule disk cache (`~/.cc-code/jev/peri-<project-hash>/<rules-hash>.json`) used to be written with `serde_json::to_vec` as **compact single-line** JSON — rule bodies run to thousands of characters crammed onto one line, making it nearly unreadable when a user opens the file to inspect the rules. `write()` now uses `serde_json::to_vec_pretty` (2-space indent), so both the rule-body file and the `<signature>.index.json` index are pretty-printed uniformly. The content hash is computed over the **on-disk bytes** (`digest(&bytes)`), so it stays self-consistent either way and cache hits still work; legacy caches miss once (their filename hash no longer matches the formatted bytes) and re-extract, with no functional impact. Only the on-disk format changes — the in-memory cache and Jev's decision logic are untouched (#412) |
| **Windows Global Entry Launches exe Directly (Ctrl+C Fix)** | v0.6.117 | On Windows Terminal (ConPTY), `cc-code` was started via the npm-generated global wrapper (`<prefix>/cc-code.cmd|.ps1`, derived from the `bin` field pointing at the node script `bin/cc-code`), which spawned the real `cc-code.exe` through node's `execFileSync`. Ctrl+C's `CTRL_C_EVENT` is broadcast to the whole console process group (node + `cc-code.exe`): while `cc-code.exe` intercepts the signal via `SetConsoleCtrlHandler` (returning 1 to block default termination and injecting a KeyEvent for the app-level interrupt / idle double-tap exit), the node parent had no handler and was killed by default, collapsing the synchronous `execFileSync` wait and dragging `cc-code.exe` down with it — so pressing Ctrl+C in any agent state exited immediately, with the log cut off mid-streaming and no cleanup record. `overwriteNpmGlobalWrapper()` is now added to `install.js` and, during the Windows postinstall, rewrites the npm global entry to **launch `bin/cc-code.exe` directly** (referenced relative to the prefix root, avoiding hard-coded absolute paths). With the node middle layer removed, `cc-code.exe` is the only member of the process group and `SetConsoleCtrlHandler` works as intended. The rewrite is defensive: it only overwrites when the exe exists and the npm global wrapper can be located, and any failure leaves the install unblocked (the exe-adjacent `bin/cc-code.cmd|.ps1` still works); the `bin/cc-code` node script and non-Windows paths are unchanged. Verified on a real machine: after the global entry launches the exe directly, Ctrl+C no longer exits, restoring the intended "Ctrl+C interrupts the agent / idle double-tap exits" behavior (#410, #411) |
| **Schema Circuit Breaker Windowed Counting** | v0.6.116 | Fixes the schema circuit breaker never firing under real failure patterns: the model often alternates mis-typed parameters across tools (WebSearch → WebFetch → WebSearch) with successful calls interleaved, while the old implementation counted "consecutive" failures and `reset()` cleared the cross-tool aggregate counter on **any** successful tool call — so the threshold could never be reached (measured: 3 schema failures and 4 total failures produced 0 injected hints). `TOOL_SIGNATURE_HINTS` also lacked reverse entries, so passing `query`/`num_results` to another tool produced no hint at all: the model was told "use WebFetch" on one side yet got silence on the other, oscillating between the two tools. Counting is now **windowed** (capacity 6; both per-tool and cross-tool counts are derived from the window instead of separate counters), `reset()` only clears that tool's backoff state without emptying the window, and successful calls occupy window slots to age out old failures. Reverse hint entries were added (`query`/`num_results` → `WebSearch`, `old_string`+`new_string` → `Edit`, `content`+`file_path` → `Write`). Verified against a real trace's raw call sequence: 0 triggers before, `PerTool { tool_name: "WebSearch", count: 2 }` after (#404, #408) |
| **Jev Rule Extraction Timeout Ladder & Low-Effort Extraction** | v0.6.115 | The rule-extraction disk cache shipped in v0.6.114 (#398, #399) **never actually took effect** in practice — `~/.cc-code/jev/` was never created, because three root causes compounded into `complete` always being `false`, so the "persist complete non-empty results only" branch was never reached: (1) extraction inherited the session's extended-thinking tier (`effort=xhigh`), pushing each chunk to a measured 18–73 s; (2) a timeout was treated as "the chunk is too big" and **halved recursively**, but a timeout means a slow gateway, not a large input — halving merely re-bets the same deadline and inflates one wait into 7 (14 calls in a single `ensure_loaded`; a mutation test measured 28 for the old logic); (3) the 15 s default deadline was below the measured per-chunk cost. Extraction now uses a dedicated low-effort tier (`effort=low`, `budget_tokens=1024`, no longer inheriting session config), measurably cutting each chunk to 10–15 s with output tokens down from ~6k to ~2.5k and far less variance. Timeouts now use a **deadline ladder** — retry once at 2× the first deadline (default 15 s → 30 s) — and a new failure-kind split means **timeouts no longer trigger halving**, which is retained only for failures where shrinking genuinely helps (unparseable or empty replies). The default extraction deadline is raised to 30 s. If the extraction thinking tier ever becomes configurable it **must** join the cache signature, or stale results from the previous tier will be served (noted in code) |
| **Jev Rule Disk Cache & Global Instruction Fallback** | v0.6.114 | Auto mode now persists Jev security rule extractions to disk with SHA-256 content-addressed caching, skipping repeated LLM rule extractions across restarts (#398, #399). Optimized queued message shortcuts to `Ctrl+Enter` send and `Ctrl+X` discard (#400, #401). System Prompt instruction loading now falls back across ordered candidate paths (`~/.cc-code/AGENTS.md` -> `~/.cc-code/CLAUDE.md` -> `~/.claude/CLAUDE.md` -> `~/.claude/AGENTS.md`), unifies data sources with Jev security rules, and allows empty candidates to transparently fall through (#402, #403) |
| **Interaction Interrupt Guard & Memory Optimization** | v0.6.113 | Fixed Questions and Approval popups where a single `Ctrl+C` abruptly exited the TUI — now safely interrupts the turn, unblocks waiting transports, and preserves the session; batch tool approval now responds immediately to cancellation; setup wizard reuses double-tap exit debouncing (#396, #397). Switched syntax highlighting engine to Oniguruma to drastically cut PHP resident memory, added mimalloc idle thread heap collection on Windows, and corrected `/gc` diagnostic metrics (#394, #395) |
| **Todo List Overflow Folding** | v0.6.112 | The Todo list beneath the loading spinner now caps at `MAX_VISIBLE_TODOS = 5`. When many tasks exist (e.g. 8–10 items), only the first 5 render expanded while remaining items collapse into a compact summary row (e.g. `... +3 pending`), distinguishing pending and completed counts; `spinner_extra_count` and `todo_render_line_count` are refactored so logical line reservations cap at 6 lines (5 items + 1 summary), keeping viewport clipping and scrollbar metrics perfectly aligned (#392, #393) |
| **Status Bar Recent-Tool Feed** | v0.6.111 | The status bar's second row used to show the running tool only while it ran — and often never at all: `poll_agent` drains every ACP notification within one frame, so millisecond tools (Read/Glob) had their ToolStart + ToolEnd handled in the same frame and `◐ Read : x.rs` was never drawn once. Summaries were a whitelist of 11 tools (Agent / TodoWrite / AskUserQuestion / MCP showed a bare name), and ordering kept the oldest entry on the left. The row is now a feed driven by `AgentComm.recent_tools`: newest on the left, at most 2 (a third pushes the oldest out), each entry visible for `max(actual duration, 300ms)` — fast tools linger, slow ones vanish the moment they finish — and finishing a tool now only bumps the aggregate `✓ Name ×N` counter instead of adding a summary entry. Summaries cover every tool (AskUserQuestion → first question, Agent → description, TodoWrite → task count, otherwise the first non-empty string field), the character cap moved 20 → 30, truncation stays path-semantic, and `◐`/`✓`/name/summary colors are uniform. The row never wraps: overflow is cut by display width (unicode-width) with a trailing `…` instead of being silently clipped |
| **Schema Circuit Breaker & Typed Failure Tracking** | v0.6.110 | Schema validation circuit breaker previously counted failures per tool name independently, allowing the agent to evade the breaker by alternating between different tools; consecutive failure tracking also used full error text as hash keys, fragmenting counts across dynamic parameter names. The breaker now adds an aggregate cross-tool failure tracker (threshold 3) with exponential backoff to immediately arrest multi-tool guessing loops, while consecutive failure tracking groups by typed `(tool_name, ToolErrorKind)` to reliably capture recurring errors regardless of error string variations. Injected warnings are strictly flushed after all tool results to preserve `tool_use` and `tool_result` contiguity (#379) |
| **Verb Shimmer & Thermal Ladder** | v0.6.109 | Following Codex CLI's cosine wave attenuation physics, verbs (`Executing…` / `Thinking…`) now feature independent gentle shimmer (5.0s period, toned 45% brighter within the same hue, never jarring); long Bash tasks maintain an 8.0s ultra-low frequency shimmer as a visual heartbeat. The thinking status word decouples from verb shimmer, staying 100% static while warming by elapsed time (0~2.5s muted gray ➔ 2.5~5s soft white ➔ 5~15s warm orange ➔ 15~60s still thinking light gold ➔ >=60s deep in thought amber gold). Also in v0.6.109: added `⎿` (U+23BF) to the ASCII fallback map to fix `?` tool prefix on odd column-width terminals |

> Older releases (v0.6.0 – v0.6.106) are listed in the [CHANGELOG](./CHANGELOG.md).

---

## Install

Binaries available for macOS (x86_64 / Apple Silicon), Linux (x86_64 / aarch64), and Windows (x86_64).

### npm (Recommended)

```bash
npm install -g @cc-claw/code
```

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
├── cc-middlewares/           # Middleware: filesystem, terminal, MCP, Hooks, etc.
│   ├── README.md              # Middleware overview
│   └── CLAUDE.md              # Development guide and traps
├── cc-tui/                  # TUI application (Ratatui)
│   ├── README.md              # TUI usage guide
│   └── CLAUDE.md              # Development guide and traps
├── cc-acp/                  # ACP service layer: bridges TUI/IDE with Agent
│   └── README.md              # ACP architecture and data flow
├── cc-widgets/              # Widget component library
│   └── README.md              # Component list and examples
├── cc-lsp/                  # LSP client library
│   └── README.md              # LSP operations and config
├── langfuse-client/           # Langfuse telemetry client
│   └── README.md              # Telemetry config and usage
├── npm/                       # npm package: postinstall script + shell wrapper
│   └── README.md              # npm package docs
├── scripts/
│   ├── install.sh             # macOS / Linux installer
│   └── install.ps1            # Windows installer
├── side-projects/             # Experimental projects (peri-sync)
├── spec/                      # Design docs and specs
│   ├── global/                # Global architecture docs
│   ├── issues/                # Issue analysis docs
│   └── archive/               # Archived feature specs
├── docs/                      # Long-form docs (prd, adr, designs, acp)
├── human/                     # Manual walkthrough checklists
├── CLAUDE.md                  # Project development guide
├── CHANGELOG.md               # Version changelog
├── CONTRIBUTING.md            # Contribution guide
├── README.md
├── README_ZH.md
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
