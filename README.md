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

**Long-history detail rendering (unreleased):** `/config` → Detail Render selects `auto` (default), `cache` (full rendering), or `virtual` (viewport rendering), applied immediately and saved as `config.detail_render_strategy`. Virtual rendering limits offscreen rendered-cache growth; source history and a large visible message still consume memory.

**Typing UX aligned with Claude Code (unreleased):** Suggestion overlays (slash commands / skills / @ files) are now interactive while the agent is running — arrows/Tab navigate, Enter completes only a selected candidate, Esc dismisses the overlay first and interrupts the agent on a second press. "Send now" (steer) with a foreground command running waits a 2s grace period, then moves it to the background automatically. The status-bar permission label now shows a `⏵⏵` prefix. See [CHANGELOG](./CHANGELOG.md).

**Sub-agent cancellation (v0.6.139):** Just tell the main agent to stop a sub-task — it calls `Agent(action="stop", task_id="bg-…")` to cancel by exact `task_id` (**no fuzzy matching**, so look-alike sibling tasks are never stopped by mistake). The receipt carries the task summary so the agent can confirm what it stopped; the timeline prints `• Agent "x" was stopped by main Agent`, and the drawer row turns into a **red filled `●`** with `stopped by main Agent` (keyboard stops show `stopped by user`). While waiting, the main viewport shows `✻ Waiting for N background agents to finish`.

**Sub-agent prompt cards (v0.6.139):** Press `Ctrl+O` in the main viewport to switch detailed mode; the launch summary of background sub-tasks then turns from the compact tree into classic tool cards — `● Agent(task preview)` + `  └ Backgrounded agent` + `  └ Prompt:` followed by the **full** instruction text (6-column hanging indent, wrapped). Press `Ctrl+O` again to collapse. See [CHANGELOG](./CHANGELOG.md) for unreleased changes.

| Feature | Version | Description |
|---------|---------|-------------|
| **Visual-line Capped Long JSON & Full WebSearch Expansion** | v0.6.162 | Fixed single-line huge JSON outputs bypassing the 3-line collapse limit in normal mode by switching truncation to visual wrapped lines via `push_visual_capped_output` (strictly ≤3 lines); relaxed WebSearch/WebFetch parameter pre-truncation from 60 chars to 4000, allowing long queries and URLs to wrap fully in detailed mode (`Ctrl+O`) (#618). |
| **Background Result Injected Only Once** | v0.6.160 | When a background sub-agent finished while the main agent was still running, its full output reached the model twice: once as the `Human` notification written by `drain_notifications`, and again as the synthetic `AgentResult` `tool_result` built for the auto-continuation turn. Both texts come from `to_notification()`, so the continuation round echoed the conclusion it had just given (measured 109 → 113 messages; the reply was "already stated above"). `inject_bg_result_messages` now checks each pending result against the conversation history by its `[Background task {short_id} ` prefix (the same 8-char id, shared by `completed` / `failed`) and skips the synthetic pair when the result is already there; when every result is already present, no empty tool-call message is appended. Results that complete after `Done` are untouched and still injected exactly once (#613). |
| **Unified English Spinner Verbs** | v0.6.159 | Removed forced Chinese localization (`ZH_VERBS`) for the loading indicator; spinner verbs are now unified to the original English quirky verb collection (`EN_VERBS`, e.g. `Thinking…`, `Drifting…`) across all interface locales (#611). |
| **`/theme auto` Follows Terminal Colour Capability and Background** | v0.6.157 | `/theme` used to offer explicit palettes only, so a 16/256-colour terminal such as Apple Terminal degraded as soon as `light` / `dark` (24-bit RGB palettes) were picked, and switching system light/dark or moving the same config to another terminal meant editing `settings.json` by hand. `theme = "auto"` now resolves through the new `cc-tui/src/terminal_theme/` module in two steps: **colour capability** (`TERM_PROGRAM=Apple_Terminal` → `Ansi`, everything else → `TrueColor`) and **background lightness** — an OSC 11 query (`\x1b]11;?\x07`) judged by relative sRGB luminance (threshold 0.5, accepting 8/16-bit components and both BEL / ST terminators), falling back to the system appearance on macOS (`defaults read -g AppleInterfaceStyle`, a missing key meaning light) and to dark elsewhere when the terminal does not answer. The two dimensions map onto the existing `light-ansi` / `dark-ansi` / `light` / `dark` themes; `default` and the daltonized preference never take part. The result is cached in a `OnceLock` (it is read on the render hot path, so the subprocess probe runs at most once), and the OSC round-trip happens in `terminal_theme::preload()` after `enable_raw_mode()` but before `EventReader::start()` — the reply arrives as raw stdin bytes and the event reader would consume it — only when the persisted theme is `auto` and only when stdin and stdout are both ttys, so piped input is never swallowed. The `/theme` row reads `Auto (follow terminal: <resolved>)`. Background is deliberately **not** inferred from Terminal.app's `Default Window Settings` profile: that is only the profile for the next new window and can be overridden per window (this machine's default profile is the black Pro while the actual window is the white Basic), so that path was removed (#604) |
| **Status Bar Follows `/theme` + Markdown Cache Byte Budget** | v0.6.155 | Switching `/theme` to a Light variant turned the message area into dark-on-light while the status bar kept its dark-theme literals (primary `Rgb(255,255,255)`, secondary `Rgb(153,153,153)`) — white on a pale background, barely readable; row 1 also used ANSI named colours while row 3 hard-coded RGB, so one status bar could split into two styles when the terminal palette changed. `status_bar.rs` now resolves every colour needed for a frame from the active theme through `SbColors`: `default` keeps each literal value (zero visual change, preserving the conclusion of #522), the other six themes read `cyan / yellow / magenta / success / warning / error / inactive / text / selection_bg / cursor_bg` from the `/theme` palette, and the context gauge keeps its own `context_ok / context_warn / context_high` because its ANSI `Green/Yellow/Red` are not the same values as the palette's `SAGE` (#597). Also: the markdown render cache had only a 1024-entry cap and no byte cap — about 52.8 MB resident at full capacity (measured 139,258 lines / 952,218 spans, largest entry 85 KB) — and since cache keys include `max_width`, every resize minted a fresh batch of width variants until the cache filled up. An 8 MB byte budget now drives LRU eviction with the entry count as a backstop; the byte cache moved up to `cc-widgets/src/byte_cache.rs` to be shared with diff rendering; an existing bug where silent `LruCache::put` eviction never subtracted the evicted bytes (leaving `bytes` monotonically inflated) is fixed; and `/gc` reports the budget (#594) |
| **`/gc` Gaps No Longer Clamped + Keyboard Enhancement Wired Up + macOS Startup Clear** | v0.6.154 | `/gc` computed both `OS RSS - jemalloc resident` and `known estimate - allocated` with saturating subtraction, but the subtracted side is not guaranteed to be smaller: with resident at 93.6 MB and kernel RSS at 84.9 MB the real difference of `-8.7 MB` printed as `= 0 B`, an equation that does not add up, and "negative difference" vs "both sides equal" mean opposite things for diagnosis. Both gaps are now signed, with an explanatory note whenever the result is negative (resident still counts pages the kernel stopped reporting for this process, so the two figures are not comparable; estimate items overlap and are computed from container capacity). Same batch: the duplicate `OS RSS:` line is gone (it was the same sysinfo `process.memory()` reading as `RSS:`, merely truncated to integer MB before subtracting, losing 0.8 MB); a failed jemalloc stats read no longer masquerades as `allocated == RSS` (which made `allocated-RSS` a constant `0 B`, hiding a measurement failure as a perfect match); and on macOS `retained` — always zero because `opt.retain` defaults to false — now reads "n/a" instead of a permanent zero (#593). Also: `keyboard_modes.rs` was never declared in `lib.rs` and `enable_keyboard_enhancement()` had zero call sites, so the kitty keyboard protocol was never pushed and `Ctrl+Enter` could not be told apart from `Enter`—it now works on iTerm2 ≥3.5 / WezTerm / Ghostty / kitty / Alacritty (#592), while the queued-message hint shows the key each platform can actually deliver: `Ctrl+Enter` on Windows, `⌥S` on macOS, `Alt+S` on Linux (#591). macOS additionally clears the screen once at startup, because Terminal.app keeps the alternate screen buffer across processes and ratatui's first frame is a diff against an empty buffer, so a killed session's last frame used to bleed through (#582). TUI user-visible strings were wired to i18n (70 new keys, 72 dead keys removed — the FTL now has zero unreferenced keys, #578), status bar rows gained a 2-space indent (#584), and the terminal title keeps animating while waiting for background sub-agents (#575) |
| **Background Notification Insertion + Immediate Notice Redraw** | v0.6.151 | While the agent is working, a backgrounded Bash command (`Ctrl+B`, or a foreground command auto-backgrounded on timeout) used to have its completion notification stuck in a queue until the turn ended (`Done`) — the delay was bounded only by how much of the task remained (measured in tens of minutes), and the `Done` exit was unreliable anyway: at turn end pending user messages took priority and the notification drain was skipped entirely, while the interrupt/error paths never consumed the queue at all. Notifications are now inserted through the existing steering channel as soon as they are produced — fire-and-forget `tokio::spawn` with the receipt collected every frame while running, merged into a single message when idle, still routed to the owning session across sessions. The consumption receipt is confirmed only right before the message is actually written into state (the agent-side consumption point moved from the top of the loop to after `before_model`, i.e. after compaction), so both `-32001` (never enqueued) and `-32002` (enqueued but not consumed) mean "not delivered" and are safe to retry; errors that cannot be classified (connection loss etc.) keep the text and log a warning without automatic retry — better undelivered than duplicated. Interrupt/error still does not auto-wake a new turn, and retained notifications ride along with the user's next submission (#571). Also: system notices are now redrawn immediately after being written — typing an unknown command such as `/dream` used to leave the UI completely unresponsive, because `App::push_system_note` only wrote into `view_messages` without emitting a render event, so the notice showed up only on the next keystroke. The redraw is now built into that method, fixing the same missing-redraw path for the `/rewind` no-history, queued-message steer failure and thread-switch failure notices (#573) |
| **Input History Panel (`/history`) + Notice Min-Visible Time + Tool Resource Release** | v0.6.150 | In a CLI, `history` means the lines *you* typed — that is what a shell's `history` lists — but `/history` opened the saved-conversation list, while your own input history (already kept in `ui.input_history` and persisted across sessions) was only reachable by stepping `Ctrl+↑`/`Ctrl+↓` one entry at a time. A new `InputHistoryPanel` lists them all from that same source: `↑`/`↓`/`j`/`k`/`PageUp`/`PageDown` to move, `Enter` fills the selected entry into the input box (no auto-send, matching recall semantics), `Esc` to close, empty-state hint included; multi-line entries collapse to a single width-truncated line and the cursor row is marked, with scrolling following it. **`/history` semantics changed (breaking)**: it now opens the input history, and conversation browsing moved to `/resume` (previously just an alias of `/history`) (#566). Also: a status-bar system notice now stays visible for at least 5 seconds, so the Jev-degradation broadcast (fallback classifier after 429/timeout) can no longer be wiped by the `ToolStart` that immediately follows — clearing moved from "blank it now" to "blank it once expired", with real removal driven by the idle event-loop tick (#564); and middleware tool resources no longer accumulate across turns — the tool wrapper now destructs its inner `Box` at the last `Arc` release and the delayed-tool proxy / sub-agent model factory hold the registry and session pool by weak reference, breaking the reference cycle so the last owner actually frees them (#568) |
| **Agent Center Detail Panel + Status Bar Session Count + Inline-Diff Switch Scope** | v0.6.149 | The status bar `← {n} agents` pill no longer counts every session ever created in the current directory (194 on a long-lived checkout) but only those updated within 24h and not in Needs-you/Working — the same rule as the Agent Center `Ready` filter (#554). The Agent Center detail panel was re-laid out: `Model:` / `Reasoning:` stay inline while `Project` / `Branch` / `Last message` / `Prompt` put the label on its own line with blank-line separators, values keep the status bar colors (model cyan, project yellow, branch cyan), `Last message` / `Prompt` previews are now markdown-rendered and wrapped instead of truncated, and `Last message` takes all remaining height (only 3 lines reserved for `Prompt`); the delete-confirm overlay now uses the theme's `popup_bg` instead of the brand teal `background`. The "Inline Diff" switch (`config.diff_enabled`) also stopped doubling as `Ctrl+O` detail mode — enabling it used to expand shell output, force tool blocks open and unwrap arguments; it now only adds the Write/Edit diff to the normal page (#558) |
| **Message Render Decorators Split & Bash Exit Code Promotion** | v0.6.148 | Refactored `message_render` (90KB+) into a modular directory, decoupling text layout algorithms from tool decorators (Bash / Glob / Read / AskUser). Promoted non-zero Bash exit codes to a sticky `  ⎿ Error: Exit code {code}` header, stripped trailing tags, and properly indented output body (#551). |
> Older releases (v0.6.0 – v0.6.145) are listed in the [CHANGELOG](./CHANGELOG.md).

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
