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
| Claude Code Ecosystem | Incompatible | Use `.claude/` config, agents, skills, hooks, MCP directly |
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
| **Claude Code Compatible** | `.claude/` config, agents, skills, hooks, MCP, sub-agents all reusable |
| **Streaming Markdown** | Code blocks, tables, diffs rendered in real-time |
| **ACP Protocol** | Connect to Zed and other IDEs, or build your own "Cloud Code" platform |
| **Auto Compact** | Long sessions auto-compressed, stays fast and cheap |
| **Sub-Agent Concurrency** | Background sub-agents run in parallel with fork and background modes |
| **HITL Approval** | Sensitive operations intercepted by the JEV semantic gate — a dedicated LLM judge that only prompts when it isn't sure |
| **Dual-Engine File Search** | Grep/Glob prefer the external ripgrep binary, falling back to the built-in Rust engine |

### v0.6.x New Features

**Interface language:** TUI and CLI help support English and Simplified Chinese. Use `/lang en` or `/lang zh-CN` to save your preference. Without a saved language, startup detects `LC_ALL`, `LANGUAGE`, `LC_MESSAGES` and `LANG`; unsupported locales fall back to English. See [CHANGELOG](./CHANGELOG.md) for unreleased changes.

**Sub-agent prompt cards (unreleased):** Press `Ctrl+O` in the main viewport to switch detailed mode; the launch summary of background sub-tasks then turns from the compact tree into classic tool cards — `● Agent(task preview)` + `  └ Backgrounded agent` + `  └ Prompt:` followed by the **full** instruction text (6-column hanging indent, wrapped). Press `Ctrl+O` again to collapse. See [CHANGELOG](./CHANGELOG.md) for unreleased changes.

| Feature | Version | Description |
|---------|---------|-------------|
| **Background Sub-Agent Drawer & Virtual Viewport (Background-Only Mode)** | v0.6.130 | Sub-agents now always run in the background, and the bottom drawer is the only place they appear — their internals never intrude on the main conversation. Press `↓` on an empty composer to browse them, `↑/↓` to move the cursor, `Enter` to open an isolated viewport (own scroll position, draft and HUD tool stats), `x` to stop the selected one and `Ctrl+X Ctrl+K` to stop all. A sub-agent that finishes while you are reading it stays on screen (row turns green with `✓ completed`) until you leave, and its session is reclaimed at that point. The launch summary renders as an aligned `├── / └──` tree, the drawer uses only bright-white/grey text (no background tint on the cursor row), and its token metric is a single arrow following the flow (`↓` reply output, `↑` your input). `fork` and regular sub-agents now share one build path, sub-agent titles come from the tool's `description`, and the `run_in_background` parameter was retired (#466) |
| **File Mentions Open Immediately with a Bounded List** | v0.6.129 | Typing `@` immediately opens file candidates: current directory only by default, directories first, at most 15 items and 10 visible rows. The popup adapts to terminal height; keywords search nested files with 200ms debounce, preserve the latest query, and show searching/empty states. Truncated results and directory transitions trigger a fresh search (#463) |
| **Windows Update Fails While cc-code Is Running (Misreported as UTF-8 Error)** | v0.6.128 | On Windows, running `cc-code update` while a `cc-code.exe` was running failed with `stream did not contain valid UTF-8`. Two layered bugs: (1) `install.ps1` used `Copy-Item -Force` to overwrite the convenience copy `~/.cc-code/cc-code.exe`, which Windows locks while it is running → `IOException`, and with `$ErrorActionPreference="Stop"` the whole update aborted, leaving `current-version.txt` stale; (2) that failure surfaced as a **localized Chinese** error (GBK bytes on a gb2312 console), but `update.rs::stream_output` read child output as strict UTF-8, so the real error was replaced by the misleading UTF-8 message. Now `update.rs` decodes child output lossily (never aborts on non-UTF-8) and forces `[Console]::OutputEncoding=UTF8` on Windows; `install.ps1` falls back to renaming the locked exe aside (`*.old.<ts>`) then copying (mirroring npm/install.js's `replaceBinary`), and no longer aborts if it still fails (#453) |
| **Scroll-to-Bottom Button Centered on the Composer Bar** | v0.6.128 | The `↓ Back to bottom` overlay was centered within the message area minus one column (the reserved scrollbar column), so it sat about one column left of the input box's full-width top border. It is now centered on the input box rect (the composer bar), giving symmetric left/right margins (x=28, 28/28 at 80 columns); purely visual — visibility, hit area and scroll state are unchanged (#458) |
| **Scroll-to-Bottom Button** | v0.6.127 | When you scroll the message area away from the bottom while content overflows, a centered `↓ Back to bottom` button appears on the last visible row; clicking it (or pressing `End` on an empty composer) returns to the latest and the button disappears. While new content arrives off-bottom, the label switches to `↓ New activity`. It is an overlay reusing the existing `scroll_follow` / `scrollbar_max_offset` state; message layout and scroll position are unchanged, and it hides without intercepting pointer input when a panel or popup is active (#448) |
| **Windows Running Binary Hot-Replacement on npm Install** | v0.6.124 | On Windows, running `npm install -g @cc-claw/code` while `cc-code` is active failed with `EBUSY`/`EPERM` because the Windows kernel locks running executables against deletion. `install.js` now introduces `replaceBinary`: when standard unlinking fails on busy/permission errors, it falls back to renaming the running executable to `cc-code.exe.old.<timestamp>` to clear the path, enabling hot-replacement in-place. `cleanOldBinaries` also sweeps legacy `.old.*` files once inactive (#430, #431) |
| **Bash Running Status Shows the Model's `timeout`** | v0.6.123 | The Bash running line (`Running… (Xs)    (timeout Nm)`) showed `execution_timeout` (hard runtime cap, default 10m) rather than the model's `timeout` (foreground-wait budget, default 2m), so a call with `timeout=300000` still read `10m` and looked like the parameter was ignored. `timeout_ms` is now carried through the registration/slot, the view field `execution_timeout_ms` was renamed to `timeout_ms`, and the line sources the model's timeout, formatted in minutes (`2m`/`5m`). Execution and timeout semantics are unchanged (#428) |
| **Diff Cache Byte Budget & `/gc` Memory Coverage** | v0.6.121 | `/gc`'s "known total" was only ~5.9 MB against ~50 MB of mimalloc-allocated memory, leaving ~44 MB unexplained. Two blind spots were found: (1) `DIFF_CACHE` (`cc-widgets/src/diff/mod.rs`) and `RENDER_CACHE` (`cc-widgets/src/diff/renderer.rs`) were capped only by entry count (64), while a single entry can carry close to `MAX_DIFF_SIZE_BYTES` (1 MB) — an uncontrollable ceiling (theoretical ~64 MB each) that `/gc` never measured; (2) the render thread's `RenderCache` (all rendered conversation lines `lines` plus `wrap_map`'s per-line **plain-text copies** and char widths) was likewise unmeasured. A new `ByteBudgetCache` (LRU bounded by both entry count and bytes) gives each diff cache an **8 MB byte budget**, evicting LRU on overflow (recompute on miss — behavior unchanged); `/gc` now also reports the render cache and both diff caches and folds them into the known total. The remaining gap narrows to the syntax engine (SyntaxSet/ThemeSet), tokio, tracing and allocator metadata (#420, #421) |
| **Windows CMD Control-Code Flooding & Spinner Glyph Fix** | v0.6.120 | On legacy Windows CMD (conhost) the stdout VT mode could be lost after startup, so crossterm's cached ANSI capability no longer matched the real mode — output degraded into raw control-code flooding and the star spinner glyph went missing (the missing glyph still occupied a column yet escaped the width probe). The fix re-asserts the required VT bits before output (recording the restore, clearing and repainting while preserving other console modes), guards title output and the editor-return path (`terminal_title_ops`/`terminal_title`/`panel_memory`), and splits `terminal_backend` into native-console vs ConPTY paths with different ASCII animation frames. The same batch fixes a headless/redirected (CI) failure where a failed VT restore returned early and left `last_terminal_title` uncached, breaking the title-cache unit tests (#415) |
| **Manual `/compact` Summary Wrapped in `<system-reminder>`** | v0.6.119 | Manual `/compact` and auto-compact each carried their own copy of the summary-formatting logic, and the manual path dropped the `<system-reminder>` wrapper. The TUI relies on that tag to choose its rendering: with it, the summary collapses to a single `📋 Context compacted` line; without it, the whole summary was rendered as an ordinary user message and, once persisted, reappeared un-collapsed on load/resume. Summary construction is now a shared function `build_compacted_messages` (unifying the `<system-reminder>` wrapper + Human message + re-injected messages), used by both the auto path (`cc-middlewares/src/compact_middleware.rs`) and the manual path (`cc-acp/src/session/command/compact.rs`), so the two paths can no longer drift; a misleading "aligned with auto-compact" comment was corrected and tests now assert both paths produce an identical summary structure (#413) |

> Older releases (v0.6.0 – v0.6.118) are listed in the [CHANGELOG](./CHANGELOG.md).

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
