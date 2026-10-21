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

**Typing UX aligned with Claude Code (v0.6.166):** Suggestion overlays (slash commands / skills / @ files) are now interactive while the agent is running — arrows/Tab navigate, Enter completes only a selected candidate, Esc dismisses the overlay first and interrupts the agent on a second press. "Send now" (steer) with a foreground command running waits a 2s grace period, then moves it to the background automatically. The status-bar permission label now shows a `⏵⏵` prefix. See [CHANGELOG](./CHANGELOG.md).

**Sub-agent cancellation (v0.6.139):** Just tell the main agent to stop a sub-task — it calls `Agent(action="stop", task_id="bg-…")` to cancel by exact `task_id` (**no fuzzy matching**, so look-alike sibling tasks are never stopped by mistake). The receipt carries the task summary so the agent can confirm what it stopped; the timeline prints `• Agent "x" was stopped by main Agent`, and the drawer row turns into a **red filled `●`** with `stopped by main Agent` (keyboard stops show `stopped by user`). While waiting, the main viewport shows `✻ Waiting for N background agents to finish`.

**Sub-agent prompt cards (v0.6.139):** Press `Ctrl+O` in the main viewport to switch detailed mode; the launch summary of background sub-tasks then turns from the compact tree into classic tool cards — `● Agent(task preview)` + `  └ Backgrounded agent` + `  └ Prompt:` followed by the **full** instruction text (6-column hanging indent, wrapped). Press `Ctrl+O` again to collapse. See [CHANGELOG](./CHANGELOG.md) for unreleased changes.

| Feature | Version | Description |
|---------|---------|-------------|
| **ANSI Cyan URL Link Colour in Tool Results** | v0.6.170 | URLs inside tool results and `!` shell-command output were drawn in `theme::cur().ide` (dark `#4782C8`), so the link colour drifted with `/theme` and sat too close in lightness to the body `text_soft`, making URLs slow to spot while scanning. `style_urls()` now pins `Color::Cyan` with an underline — it no longer follows the theme, matching the status bar's model name and branch name. Covers both call sites: `format_prefixed_text()` (every `⎿`-prefixed tool result row — Bash / Read / Grep / Glob) and `format_shell_output_lines()` (`!command` output). URL detection keeps the CC `OutputLine.tsx::URL_IN_JSON` contract and trims trailing `)` `]` `}` `,` `;` `.` `:`; it is still visual only (no OSC 8 — ratatui cells cannot carry escape sequences and `sanitize_display_text()` strips `\x1b`), with clicking provided by the terminal's own URL detection. Markdown links (`[text](url)` → `theme.link()` = SAGE `#4EBA65`) are a separate path and unaffected (#649). |
| **Viewport-Lazy Detail Rendering** | v0.6.169 | `/config` → Detail Render selects `auto` (default), `cache`, or `virtual`, applied immediately and saved as `config.detail_render_strategy`. Virtual rendering materializes only the current window and shares a 16 MiB rendered-product budget across variants; source history, metadata and a huge visible message remain separate memory costs. See [CHANGELOG](./CHANGELOG.md) for behavior and boundaries (#629, PR #648). |
| **Bash Output JSON Pretty-Printing (OutputLine Decorator)** | v0.6.168 | Bash / MCP tool results now pass through an `OutputLine` decorator aligned with CC: single-line JSON is expanded to a 2-space indented pretty print behind a precision guard (if a compact re-serialization differs from the original, the line is kept verbatim, so big integers and `1.10`-style literals are never rewritten), non-JSON lines take a fast path, and content over 10,000 chars skips formatting entirely; wrapping and truncation share one ruler — pre-wrap by display width, 3 visible lines plus a `… +N lines (ctrl+o to expand)` hint when collapsed. Same batch: diff deletion rows gained three-tier background shading, tool paths display the CC way, and the status bar labels Edit operations correctly (#642). |
| **URL Link Colour in Tool Results & `!` Output** | v0.6.167 | URLs inside tool results and `!` shell-command output are found by a hand-written prefix scanner (no regex dependency; trailing `)` / `]` / `}` punctuation is trimmed back off) and rendered in the theme's link colour with an underline. `!` output is split by its existing ANSI colouring first and linkified per segment, so command colours survive; this is visual only (no OSC 8), with clicking still provided by the terminal's own URL detection (#640). |
| **Instant Ctrl+O Toggling (Render-Key Cache)** | v0.6.166 | Toggling detail mode used to clear every message's render hash and re-render + re-wrap the entire history — about 6.9s at 270 messages and 239s at 10,000 messages (debug measurements). The render thread now keys each message by content hash combined with only the detail/diff dimensions that message type is actually sensitive to, keeps both the detail and collapsed variants of every message, and caches wrapping per message segment (segment keys carry a per-line epoch, so a ticking `Running… (Xs)` indicator invalidates exactly its own segment): hot toggles drop to ~33ms and 1.38s respectively, and a regression test pins mid-execution toggles (streaming tail + running tools) to byte-identical fresh renders (#626, PR #631). Same release: diff and markdown rendering aligned with CC (per-line syntect highlighting with Monokai Extended themes, blank lines between markdown blocks now come only from the source text), Glob walks no longer follow symlinks and carry entry/time caps (#636), the typeahead popup is fully navigable while the agent is running, and steering past a foreground command gets a 2-second grace period before auto-backgrounding (#638). |
| **jemalloc Tuning Actually Takes Effect** | v0.6.163 | The allocator configuration was being exported as `MALLOC_CONF`, a variable tikv-jemalloc never reads (it honours `_RJEM_MALLOC_CONF` or the static `_rjem_malloc_conf` symbol), so every allocator tuning knob had silently done nothing. The config is now exported through the static symbol and was verified live on the shipped binary (#622). Same batch: the install script failed to extract the bundled ripgrep tarball, leaving `bin/rg` permanently missing (#620). |
| **Visual-line Capped Long JSON & Full WebSearch Expansion** | v0.6.162 | Fixed single-line huge JSON outputs bypassing the 3-line collapse limit in normal mode by switching truncation to visual wrapped lines via `push_visual_capped_output` (strictly ≤3 lines); relaxed WebSearch/WebFetch parameter pre-truncation from 60 chars to 4000, allowing long queries and URLs to wrap fully in detailed mode (`Ctrl+O`) (#618). |
| **Background Result Injected Only Once** | v0.6.160 | When a background sub-agent finished while the main agent was still running, its full output reached the model twice: once as the `Human` notification written by `drain_notifications`, and again as the synthetic `AgentResult` `tool_result` built for the auto-continuation turn. Both texts come from `to_notification()`, so the continuation round echoed the conclusion it had just given (measured 109 → 113 messages; the reply was "already stated above"). `inject_bg_result_messages` now checks each pending result against the conversation history by its `[Background task {short_id} ` prefix (the same 8-char id, shared by `completed` / `failed`) and skips the synthetic pair when the result is already there; when every result is already present, no empty tool-call message is appended. Results that complete after `Done` are untouched and still injected exactly once (#613). |
| **Unified English Spinner Verbs** | v0.6.159 | Removed forced Chinese localization (`ZH_VERBS`) for the loading indicator; spinner verbs are now unified to the original English quirky verb collection (`EN_VERBS`, e.g. `Thinking…`, `Drifting…`) across all interface locales (#611). |
| **`/theme auto` Follows Terminal Colour Capability and Background** | v0.6.157 | `/theme` used to offer explicit palettes only, so a 16/256-colour terminal such as Apple Terminal degraded as soon as `light` / `dark` (24-bit RGB palettes) were picked, and switching system light/dark or moving the same config to another terminal meant editing `settings.json` by hand. `theme = "auto"` now resolves through the new `cc-tui/src/terminal_theme/` module in two steps: **colour capability** (`TERM_PROGRAM=Apple_Terminal` → `Ansi`, everything else → `TrueColor`) and **background lightness** — an OSC 11 query (`\x1b]11;?\x07`) judged by relative sRGB luminance (threshold 0.5, accepting 8/16-bit components and both BEL / ST terminators), falling back to the system appearance on macOS (`defaults read -g AppleInterfaceStyle`, a missing key meaning light) and to dark elsewhere when the terminal does not answer. The two dimensions map onto the existing `light-ansi` / `dark-ansi` / `light` / `dark` themes; `default` and the daltonized preference never take part. The result is cached in a `OnceLock` (it is read on the render hot path, so the subprocess probe runs at most once), and the OSC round-trip happens in `terminal_theme::preload()` after `enable_raw_mode()` but before `EventReader::start()` — the reply arrives as raw stdin bytes and the event reader would consume it — only when the persisted theme is `auto` and only when stdin and stdout are both ttys, so piped input is never swallowed. The `/theme` row reads `Auto (follow terminal: <resolved>)`. Background is deliberately **not** inferred from Terminal.app's `Default Window Settings` profile: that is only the profile for the next new window and can be overridden per window (this machine's default profile is the black Pro while the actual window is the white Basic), so that path was removed (#604) |
> Older releases (v0.6.0 – v0.6.154) are listed in the [CHANGELOG](./CHANGELOG.md).

---

## Install

Binaries available for macOS (x86_64 / Apple Silicon), Linux (x86_64 / aarch64), and Windows (x86_64).

### npm (Recommended)

```bash
npm install -g @cc-claw/code
```

> [!WARNING]
> **On npm 12+ (released July 2026), this command alone is not enough.** npm now blocks dependency `postinstall` scripts by default — and that script is what downloads the platform binary. The install reports success, but running `cc-code` then fails with `spawnSync ...cc-code.exe ENOENT`. Allow the script explicitly:
>
> ```bash
> npm install -g --allow-scripts=@cc-claw/code @cc-claw/code
> ```
>
> To allow it for all global installs instead: `npm config set allow-scripts=@cc-claw/code --location=user`
>
> Using **pnpm or bun**? They block build scripts too — run `pnpm approve-builds` (or allowlist the package), otherwise `cc-code` ships without its binary.

### Upgrade

```bash
npm update -g @cc-claw/code
```

On npm 12+, pass the same flag: `npm update -g --allow-scripts=@cc-claw/code @cc-claw/code`.

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
