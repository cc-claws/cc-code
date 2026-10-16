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

**Sub-agent cancellation (v0.6.139):** Just tell the main agent to stop a sub-task — it calls `Agent(action="stop", task_id="bg-…")` to cancel by exact `task_id` (**no fuzzy matching**, so look-alike sibling tasks are never stopped by mistake). The receipt carries the task summary so the agent can confirm what it stopped; the timeline prints `• Agent "x" was stopped by main Agent`, and the drawer row turns into a **red filled `●`** with `stopped by main Agent` (keyboard stops show `stopped by user`). While waiting, the main viewport shows `✻ Waiting for N background agents to finish`.

**Sub-agent prompt cards (v0.6.139):** Press `Ctrl+O` in the main viewport to switch detailed mode; the launch summary of background sub-tasks then turns from the compact tree into classic tool cards — `● Agent(task preview)` + `  └ Backgrounded agent` + `  └ Prompt:` followed by the **full** instruction text (6-column hanging indent, wrapped). Press `Ctrl+O` again to collapse. See [CHANGELOG](./CHANGELOG.md) for unreleased changes.

| Feature | Version | Description |
|---------|---------|-------------|
| **Input History Panel (`/history`) + Notice Min-Visible Time + Tool Resource Release** | v0.6.150 | In a CLI, `history` means the lines *you* typed — that is what a shell's `history` lists — but `/history` opened the saved-conversation list, while your own input history (already kept in `ui.input_history` and persisted across sessions) was only reachable by stepping `Ctrl+↑`/`Ctrl+↓` one entry at a time. A new `InputHistoryPanel` lists them all from that same source: `↑`/`↓`/`j`/`k`/`PageUp`/`PageDown` to move, `Enter` fills the selected entry into the input box (no auto-send, matching recall semantics), `Esc` to close, empty-state hint included; multi-line entries collapse to a single width-truncated line and the cursor row is marked, with scrolling following it. **`/history` semantics changed (breaking)**: it now opens the input history, and conversation browsing moved to `/resume` (previously just an alias of `/history`) (#566). Also: a status-bar system notice now stays visible for at least 5 seconds, so the Jev-degradation broadcast (fallback classifier after 429/timeout) can no longer be wiped by the `ToolStart` that immediately follows — clearing moved from "blank it now" to "blank it once expired", with real removal driven by the idle event-loop tick (#564); and middleware tool resources no longer accumulate across turns — the tool wrapper now destructs its inner `Box` at the last `Arc` release and the delayed-tool proxy / sub-agent model factory hold the registry and session pool by weak reference, breaking the reference cycle so the last owner actually frees them (#568) |
| **Agent Center Detail Panel + Status Bar Session Count + Inline-Diff Switch Scope** | v0.6.149 | The status bar `← {n} agents` pill no longer counts every session ever created in the current directory (194 on a long-lived checkout) but only those updated within 24h and not in Needs-you/Working — the same rule as the Agent Center `Ready` filter (#554). The Agent Center detail panel was re-laid out: `Model:` / `Reasoning:` stay inline while `Project` / `Branch` / `Last message` / `Prompt` put the label on its own line with blank-line separators, values keep the status bar colors (model cyan, project yellow, branch cyan), `Last message` / `Prompt` previews are now markdown-rendered and wrapped instead of truncated, and `Last message` takes all remaining height (only 3 lines reserved for `Prompt`); the delete-confirm overlay now uses the theme's `popup_bg` instead of the brand teal `background`. The "Inline Diff" switch (`config.diff_enabled`) also stopped doubling as `Ctrl+O` detail mode — enabling it used to expand shell output, force tool blocks open and unwrap arguments; it now only adds the Write/Edit diff to the normal page (#558) |
| **Message Render Decorators Split & Bash Exit Code Promotion** | v0.6.148 | Refactored `message_render` (90KB+) into a modular directory, decoupling text layout algorithms from tool decorators (Bash / Glob / Read / AskUser). Promoted non-zero Bash exit codes to a sticky `  ⎿ Error: Exit code {code}` header, stripped trailing tags, and properly indented output body (#551). |
| **Cross-project Agent Command Center** | v0.6.147 | `/agents` or empty-draft `←` opens a full-page session center: project/status/model groups, five status filters, pagination, details, search, rename and confirmed permanent deletion. Enter resumes in the target project; `n` starts an empty session there. Agent definitions move to `/agent-design`; `/tasks` is replaced (#542) |
| **Input `/command` Highlighting + `/rewind` Command Entry + Busy-Input Command Routing** | v0.6.145 | Three TUI input/render fixes: (1) `/command` tokens in the input box are highlighted with the suggestion color using the same rules as Claude Code's `findSlashCommandPositions` — skipped in `!` shell mode so paths like `/etc` aren't colored (#529); (2) Rewind moved from a double-`Esc` shortcut to a real `/rewind` command (alias `/undo`) — `Esc` is the cancel/interrupt key, so tapping it twice popped the rollback picker by accident while typing `/rewind` did nothing (#531); (3) pressing `Enter` while the agent is running no longer turns commands into model prompts — `/exit`, `/gc`, `/theme` and `!` shell lines used to be queued and then fed to the model as plain text. Routing now happens in one place (`CommandSystem::resolve`): messages, skills, ACP commands and forwarding commands queue until the turn ends, while idle-only commands, `!` input and unknown commands are rejected with the input kept. `Ctrl+Enter` can no longer steer a command into a running turn (#533) |
| **Sub-Agent Drawer Activity Column Fixed & Throttled** | v0.6.144 | The drawer's "what it's doing" column used to flicker between the tool name (`Bash` / `Grep`) and the assistant's text, because the server wrote the tool name into the task snapshot's `current_activity` while the TUI's `progress` branch overwrote the locally-collected assistant text with it. The server no longer writes it and the TUI no longer accepts it, so the column is driven purely by assistant text and status labels. It also now refreshes at most every 200ms via a render snapshot (`activity_shown`), synced immediately when first filled or when the sub-agent finishes — dropping render frames, not state (#526) |
| **Status Bar Colors Pinned — No Longer Follows `/theme`** | v0.6.143 | The status bar's hand-picked colors had been swept into the palette when `/theme` landed (#509), so its look drifted with the theme: model name / cwd / git branch turned from ANSI named colors into fixed RGB, and the context meter followed `error`/`warning`/`success`. The status bar is a finished, fixed design, so it now opts out of theming — a local `sb` constants module restores the pre-#509 values at 52 call sites, and rendering is byte-for-byte identical to before the theme system (#523) |
| **Theme Panel Keyboard Routing Fix** | v0.6.142 | The `/theme` picker has `Session` scope, but the keyboard and paste allow-lists omitted `PanelKind::Theme`, so keys fell through to later stages instead of reaching the panel — `↑`/`↓` didn't move the cursor and `Esc` couldn't close it. Both routing paths now include the panel, with a regression test added (#521) |
| **Spinner Tips Scheduling Aligned with Claude Code** | v0.6.141 | Spinner tips moved from a fixed ~3s rotation on every render tick to CC's scheduling model: each tip has a stable id and a session cooldown, one is picked and **withheld** at the start of a turn and only published when the turn ends (at most one per turn, never swapped mid-turn), picking the tip seen longest ago among those past cooldown, with history persisted to `~/.cc-code/spinner-tips.json`. New config `spinner_tips_enabled` and `spinner_tips_override` (custom entries + external file). Also: the MCP protocol library `rmcp` was upgraded to 3.5.1, migrating the client to the 3.x data model and API (#511, #519) |
| **Theme Command `/theme` — 7 Switchable Palettes (6 Matching Claude Code Exactly)** | v0.6.140 | Switch UI color schemes with `/theme`: the existing scheme stays as `default`, plus `dark` / `light` / `light-daltonized` / `dark-daltonized` / `light-ansi` / `dark-ansi` ported value-for-value from Claude Code's theme definitions; live preview in the picker, choice persists across restarts. |
> Older releases (v0.6.0 – v0.6.139) are listed in the [CHANGELOG](./CHANGELOG.md).

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
