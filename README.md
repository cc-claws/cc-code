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
| `.claude/` Ecosystem | Incompatible | Use `.claude/` config, agents, skills, hooks, MCP directly |
| Safety Gate | Prompts you for every risky command | JEV semantic gate — a dedicated LLM judge that only prompts when it isn't sure |

---

## Auto Mode: The JEV Semantic Gate

`Auto` is the default permission mode (fail-closed). Instead of asking you about every call, it decides in two layers:

- **Deterministic layer** — your own rules, the built-in hard-deny list, and a read-only fast lane. Zero cost, no credentials required.
- **JEV semantic gate** — a dedicated LLM judge (`JEV_MODEL`, default `openrouter/typesafe/jev-1.13`) reached over its own endpoint (`JEV_ENDPOINT`) with its own key (`JEV_API_KEY`). It scores each call against a set of conditions and pops a single confirmation prompt only when the call lands in the uncertain band.

No credentials configured? The deterministic layer still runs and an LLM classifier takes over as fallback; when there is no confirmation channel (sub-agents, `-p`) the call is denied. The session model is never involved — judging is offloaded to JEV, so safety checks don't spend your context tokens. `Shift+Tab` toggles between `Auto` and `Bypass`.

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

**Skill workflows (unreleased):** Matching tasks use the core `Skill` tool, while `/skill-name args` loads instructions directly. Skill cards show a compact loading result; `context: fork` waits for an isolated agent, and active skill instructions survive compaction. See [skill behavior](./spec/global/domains/skill-trigger.md).

**Interface language:** TUI and CLI help support English and Simplified Chinese. Use `/lang en` or `/lang zh-CN` to save your preference. Without a saved language, startup detects `LC_ALL`, `LANGUAGE`, `LC_MESSAGES` and `LANG`; unsupported locales fall back to English. See [CHANGELOG](./CHANGELOG.md) for unreleased changes.

**Typing UX aligned with the reference implementation (v0.6.166):** Suggestion overlays (slash commands / skills / @ files) are now interactive while the agent is running — arrows/Tab navigate, Enter completes only a selected candidate, Esc dismisses the overlay first and interrupts the agent on a second press. "Send now" (steer) with a foreground command running waits a 2s grace period, then moves it to the background automatically. The status-bar permission label now shows a `⏵⏵` prefix. See [CHANGELOG](./CHANGELOG.md).

**Sub-agent cancellation (v0.6.139):** Just tell the main agent to stop a sub-task — it calls `Agent(action="stop", task_id="bg-…")` to cancel by exact `task_id` (**no fuzzy matching**, so look-alike sibling tasks are never stopped by mistake). The receipt carries the task summary so the agent can confirm what it stopped; the timeline prints `• Agent "x" was stopped by main Agent`, and the drawer row turns into a **red filled `●`** with `stopped by main Agent` (keyboard stops show `stopped by user`). While waiting, the main viewport shows `✻ Waiting for N background agents to finish`.

**Sub-agent prompt cards (v0.6.139):** Press `Ctrl+O` in the main viewport to switch detailed mode; the launch summary of background sub-tasks then turns from the compact tree into classic tool cards — `● Agent(task preview)` + `  └ Backgrounded agent` + `  └ Prompt:` followed by the **full** instruction text (6-column hanging indent, wrapped). Press `Ctrl+O` again to collapse. See [CHANGELOG](./CHANGELOG.md) for unreleased changes.

| Feature | Version | Description |
|---------|---------|-------------|
| **`/gc` Size-Class Breakdown** | v0.6.188 | `/gc` now decomposes `allocated` by allocation size instead of leaving a lump of "unidentified" bytes. It reads jemalloc's per-arena bin and large-class counters and prints the top 12 size classes with live object counts, byte totals and share, plus a coverage line. Summing every size class reproduces `stats.allocated` exactly, so those are real live allocations — including what the estimates cannot see (SyntaxSet/ThemeSet, compiled regular expressions, tokio/ACP buffers) — not estimates. mallctl keys are assembled on the stack and the query's own buffers are allocated before the epoch snapshot, because with jemalloc as the global allocator the allocator being measured *is* the process allocator: formatting ~4,000 keys with `format!` perturbed the coverage check by 29,696 B on 24.6 MB. Size classes print in exact bytes (`1,280 B`), since `fmt_bytes` would render both 1024 B and 1280 B as `1 KB`. Skipped on Windows, where mimalloc has no equivalent (#716). |
| **No Gap in Bash Status Line** | v0.6.186 | When a Bash block moved from queued/awaiting-approval into running, the status line used to vanish for up to two seconds — the waiting branch had already exited while the running branch had not yet met its 2-second threshold, leaving only the tool header. The status line is now rendered the moment execution starts (`Running…`), with the elapsed counter and the `ctrl+b` hint still appearing only after 2 seconds so short commands never flash a timer. The render thread's threshold-crossing rebuild check was corrected too: the previous "is a Running line present" test became permanently true once the bare `Running…` line existed, so the counter and hint would never appear. Queued/awaiting-approval indicators return to a solid `●` (only running blinks) — queueing is not activity, and the waiting semantics are carried by the `Waiting…` copy (#706). |
| **Synchronized Indicator Pulsing and Status Bar Cleanups** | v0.6.185 | Unifies pulsating animation cadence across active tool states (`ToolCallStatus::Pending` and `Running` share an 800 ms ON / 800 ms OFF breathing cycle, pulsing uniformly whenever grey and uncompleted, while wiring render thread heartbeats to eliminate visual freezes in waiting states, #698); removes idle placeholder `· ← 0 agents` from the status bar (#696); capitalizes standalone read-only tool action lines (e.g. `Read 1 file`, #694); strips redundant ✗ glyphs from interrupted tool output and capitalizes `Interrupted by user` (#692). |
| **Live Bash Output, Queue and Approval States** | v0.6.184 | Bash blocks distinguish queued, awaiting approval, running and finished calls. After two seconds, running commands show the last five visual lines with elapsed time, explicit timeout and output size; Ctrl+O expands the retained 64 KiB tail. Live output is enabled by default and can be disabled with `config.tool_output_streaming`; main and sub-agent calls stay isolated (#675). |
| **Real OSC 8 Hyperlinks for Tool-Output URLs** | v0.6.182 | URLs in tool results and `!` command output are now genuine terminal hyperlinks. The OSC 8 sequence moved to the output layer (`terminal_backend/hyperlink.rs`), which looks links up by coordinate as bytes are written — stuffing the sequence into a ratatui cell made `Buffer::diff` skip every cell after the link, so trailing text vanished. URLs render bright blue (`38;5;12`) with no artificial underline, hover and click behaviour comes from the terminal, hyperlinking is on by default with `DISABLE_HYPERLINKS=1` as the opt-out, and the old ANSI Cyan + underline styling (`style_urls()`) is deleted (#671). |
| **Running Timer Survives Event Floods** | v0.6.181 | The `Running… (Xs)` counter and the pulsating `●` indicator no longer freeze while a long Bash command runs. The render worker recreated its 200 ms `sleep` on every loop iteration, so any event arriving faster than 5 Hz (streaming output, per-frame viewport reports) cancelled it before it expired — the tick never ran, and `rebuild` could not compensate because the render key (`content_hash ⊕ detail/diff flags`) carries no elapsed time, so unchanged content reuses the cached lines verbatim. The worker now keeps a monotonic `tick_deadline` and awaits `sleep_until` inside a `biased` select, so the tick fires on schedule however dense the event stream is; the deadline resets when no tool is running (#683). |
| **Bash Execution Status Alignment** | v0.6.180 | Tool blocks carry an explicit execution phase: a finished command with no output reads `Done` (`(No output)` in detail mode) instead of keeping a stale running indicator, explicit timeouts show as `Running… (Ns · timeout Nm)` and keep a footer note on completion, the backgrounding hint uses a hanging indent and is hidden for sub-agents, and restored history is always shown in the completed state (#675). |
| **npm Binaries via Platform Optional Dependencies** | v0.6.179 | The `@cc-claw/code` package no longer downloads a binary from an install script. The binary ships as five platform packages (`@cc-claw/code-win32-x64`, `-darwin-x64`, `-darwin-arm64`, `-linux-x64`, `-linux-arm64`) referenced through `optionalDependencies`, so npm installs only the one matching your platform and no postinstall script runs; the `bin/cc-code` launcher resolves and spawns the binary from the platform package (#678). |
| **OSC 8 Hyperlinks in Markdown Links** | v0.6.178 | Markdown links in the message area now emit OSC 8, so supported terminals (Windows Terminal, iTerm2, WezTerm, kitty…) show a hover affordance and open links with Ctrl/Cmd+click instead of leaving an inert plain-text URL. The sequence never participates in layout or wrap math, and control characters are stripped from URLs so a crafted link cannot terminate the sequence early (#671). |
| **No Syntax Highlighting in `.md` Previews** | v0.6.177 | Write and Edit previews no longer colour `.md` content. The reference implementation recognizes `md` as markdown, but its theme scope tables map neither `section` nor `strong` (the only scopes highlight.js emits for Markdown), so its rendered output is byte-identical to plain text. cc-code was using syntect's Markdown grammar and painting headings and bold text; the markdown extensions are now removed from the language whitelist and fall back to plain text. |
> Older releases (v0.6.0 – v0.6.176) are listed in the [CHANGELOG](./CHANGELOG.md).

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
