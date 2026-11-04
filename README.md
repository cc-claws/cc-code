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

## ❤️Sponsor

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
| **Unbounded Long Bash Timeout & BASH_MAX_TIMEOUT_MS Support** | v0.6.235 | Removes the rigid 10-minute (600,000ms) clamp on Bash commands that previously aborted large project builds and long tasks prematurely. Fully honors caller-specified longer timeouts and automatically expands `execution_timeout` to prevent watchdogs from killing intended long jobs. Also introduces `BASH_DEFAULT_TIMEOUT_MS` and `BASH_MAX_TIMEOUT_MS` environment variable overrides (#886, #887). |
| **Silent Auto-recap Failures & Bounded Retry Limit** | v0.6.234 | Fixes upstream proxy errors (such as 503 Service Unavailable) causing the background auto-recap to flood the terminal message stream with repeated red error notices. Auto-recap failures now fail silently to debug logs without polluting the chat view; a retry threshold of `MAX_AUTO_RETRY_COUNT = 1` stops infinite retry loops during proxy outages (#881, #882). |
| **Strip Non-standard ID from Tool Results & Dual-adaptation** | v0.6.233 | Strips the non-standard `id` field from `tool_result` content blocks to strictly comply with the Anthropic protocol, resolving `Extra inputs are not permitted` 400 errors from strict upstream proxies. Adds `detect_tool_result_id_requirement` to automatically adapt and re-inject `id` if non-standard gateways explicitly require it (#878, #879). |
| **Lightweight Model Thinking 400 Fix & Multi-tier Fallback** | v0.6.232 | Addresses HTTP 400 errors when extended thinking is globally enabled for lightweight models (like Haiku 3.x/4.x) that do not support thinking. Statically identifies these models to skip thinking fields, and extends the adaptive protocol negotiation with a `DisableThinking` fallback (retrying up to two levels: `Budget → Adaptive → Disabled`) to ensure uninterrupted execution (#875, #876). |
| **Smart Unified Diff Highlighting for Bash & PHP Syntax Fix** | v0.6.231 | Output from `git diff` in Bash or `!git diff` is now automatically rendered through the built-in diff pipeline with syntax highlighting, clean single-side line gutters, and word-level diff background contrast. Also fixes pure PHP code dropping syntax colors when evaluated without an opening `<?php` tag by prioritizing the `"PHP Source"` grammar over HTML embedding (#872, #873). |
| **Table Rows with VS16 Emoji Stay Aligned** | v0.6.228 | A markdown table row containing `⚠️` (`U+26A0 U+FE0F`) used to shift its column separators and right border one cell to the right, pushing the border past the table edge and leaving stray verticals after scroll redraws. The cause is in the output layer, not the table renderer: the trailing cell that `Buffer::diff` repaints for wide glyphs was written as a contiguous neighbour by the crossterm backend, which decides contiguity by cell index alone and ignores the 2-column cursor advance. Covered trailing cells are now dropped before cells are sent, judged on the actually emitted glyph width (a wide glyph downgraded to 1-column ASCII keeps its trailing cell) (#860). |
| **Compact Threshold Input & Range Optimization** | v0.6.224 | The "Compact threshold" row in the `/config` panel is redesigned as a percentage option item (range 10-99%, allowing earlier auto-compaction down to 10%). It removes unrestricted free-form text input in favor of arrow-key stepping and strict numeric entry, with automatic normalization on blur to prevent arbitrary long strings (#844, #845). |
| **Terminal Color Depth Detection** | v0.6.223 | Themes and the status bar used to emit `Color::Rgb` (24-bit) unconditionally, so terminals limited to 256/16 colors had to approximate it themselves and `NO_COLOR` was ignored. Color depth is now detected (`NO_COLOR`, `TERM=dumb`, `COLORTERM`/`TERM` truecolor signals, Apple Terminal and `*256*` → 256) and downgraded at the output layer: the 256-color step takes the nearer of the 6×6×6 cube and the 24-step gray ramp, the 16-color step quantizes to base colors, and `NO_COLOR` drops color while keeping bold/dim. Inside `TMUX` it clamps to 256 (default config drops `38;2` backgrounds); `PERI_TMUX_TRUECOLOR=1` opts out. Truecolor output is byte-identical to before (#601). |
| **Reliable Long-Session Resume** | v0.6.223 | Restoring a long session avoids redundant history reads and full cache rewrites. Child sessions retain newly appended messages, and closing idle sessions releases their serialization locks (#813). |
| **Live Thinking Char Count** | v0.6.220 | While the model is thinking, the header shows the live character count of the streamed reasoning (`∴ Thinking for 1247 chars…`) instead of a stopwatch, so you can tell it is still producing; the line freezes into `Thought for 16s, read 2 files` once the reply starts. Active and finished rows share one dim color (#836). |
> Older releases (v0.6.0 – v0.6.220) are listed in the [CHANGELOG](./CHANGELOG.md).

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
