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
| **Reliable Long-Session Resume** | Unreleased | Restoring a long session avoids redundant history reads and full cache rewrites. Child sessions retain newly appended messages, and closing idle sessions releases their serialization locks (#813). |
| **Responsive Sub-agent Views** | v0.6.212 | Streaming updates are coalesced so typing and scrolling remain responsive; hidden views defer conversion, including on completion. Completed history uses a bounded projection cache released when leaving the sub-agent view, and a full render queue retries the latest snapshot (#796). |
| **Hooks Display Switch** | v0.6.212 | The `hooks_ui` setting (default on) adds a `Hooks display` row to `/config`: turning it off hides hook rows immediately while keeping the data, so toggling back restores them at once. Fold blocks no longer show hook rows, hook rows are forced to a single line, and duplicate failures are deduped per event (#808). |
| **Hook Runs Reported in the UI** | v0.6.210 | Hook execution rows attach to their tool block with a summary in detailed mode; async hooks show a `running` placeholder replaced in place on completion, rows merge per event, and subagents inherit session hooks with real SubagentStart/Stop execution. Non-blocking failures report through `HookAction::Error`, with failure classification for invalid JSON and prompt/http/agent hooks (#804). |
| **`/ps` Opens the Background Tasks Panel** | v0.6.210 | `/ps` opens the background tasks panel for past commands and live output at any time — even while the agent is running, it pops immediately instead of queueing or refusing (#806). |
| **Compaction Spinner Animations** | v0.6.208 | Compaction no longer appends a permanent "Compacting context…" line: history keeps only the result notice. Progress is shown on the spinner instead — a runner (12-cell track, `🏃` every 150ms for the first 20s then a slower `🚶`, leaving `•` footprints) or a UFO (MiMo-Code's scanner algorithm: 14 cells, 40ms/frame, a 5-cell `· ∙ ˙` trail), picked at random per compaction. Both sweep out and back with end holds, reuse the tip row so no layout height is added, and degrade to `R`/`W`/`o` on ASCII-only consoles (#792). |
| **`!` Commands Use Git Bash** | v0.6.207 | `!` commands now run through Git Bash like the Bash tool instead of `cmd /C`, so `sleep`, `ls` and `grep` work on Windows; the running state reads `Running… (Xs)` and the seconds keep ticking for silent commands (#790). |
| **Untrusted Hooks Notice Keeps the Welcome Card** | v0.6.207 | The startup notice for untrusted project hooks no longer counts as conversation history, so the welcome card and mascot stay visible; the notice itself is rendered with warning semantics (`⚠` in amber) instead of plain grey (#788). |
| **Approval Dialog Descriptions and Full-File Diffs** | v0.6.206 | Tool calls carrying a `description` show it on its own line under the dialog title; `Write`/`Edit` approvals preview the full before/after file diff (new files get a numbered, syntax-highlighted code preview), falling back to the fragment diff when the file can't be read or located (#785). |
| **Hooks Trust List in JSONL** | v0.6.205 | The project trust list moves to `~/.cc-code/trusted_projects.jsonl` (one entry per line, bad lines skipped individually, legacy text list auto-migrated with a `.legacy.bak` kept); home-directory resolution no longer silently fails when `HOME` is unset, so project and plugin hooks stop being skipped on Windows, and TUI startup now visibly warns about untrusted project hooks (#783). |

> Older releases (v0.6.0 – v0.6.203) are listed in the [CHANGELOG](./CHANGELOG.md).

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
