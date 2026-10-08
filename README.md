<div align="center">

**English** | [中文](README_ZH.md)

# cc-code

**Terminal coding agent powered by open-source models — Rust-built, Claude Code compatible**

DeepSeek-V4-Pro + Mimo-2.5Pro + GLM-5.1 driven, zero migration from `.claude/` config, runs on RISC-V.

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
| **Nobody Coding** | 99% of code produced by DeepSeek, Mimo, and GLM — humans decide what, AI figures out how |

### v0.6.x New Features

| Feature | Version | Description |
|---------|---------|-------------|
| **Detail-Mode Long-Command Status Fix** | v0.6.101 | In detailed mode (Ctrl+O) an over-long Bash command wraps the tool header across multiple lines (#264), but the tick refresher still assumed a single-line header: it overwrote the command continuation with `Running…` and froze the real status line (resulting in two `Running…` lines with mismatched times). The status line is now located by content instead of a fixed index |
| **`/gc` Diagnostic Honesty & VM Estimation** | v0.6.100 | `/gc` memory labels are now platform-aware (`active` / `mapped` / `retained` mean different things under jemalloc vs mimalloc, so Windows numbers no longer imply phantom fragmentation); the estimator now also covers `view_messages`, previously the bulk of the "unidentified" allocations |
| **Background Shell Notification i18n** | v0.6.99 | The display text for background-shell completion / timeout / cancelled / terminated / waiting-for-input notices is now localized (previously hardcoded Chinese, shown even in English); adds a process-global language registry so static `MessageViewModel` constructors can resolve the current language, sync'd at startup and on `/lang` |
| **ACP Permission Forwarding** | v0.6.94 | `cc-code acp` now forwards tool permission requests to the IDE client via `session/request_permission` instead of auto-approving (fail-closed: client unsupported / call failure / unknown option = denied); stdio default permission mode changed from Bypass to AutoMode (use `session/set_mode` for unattended bypass) |
| **Fail-Closed Permissions** | v0.6.90 | Approval is now on by default: unset `YOLO_MODE` no longer bypasses HITL (explicit `-y/--yolo` or `YOLO_MODE=true` to skip); HITL gate evaluates the post-rewrite command; `git clone` hardened against option injection |
| **Three-Choice Tool Approval** | v0.6.84 | HITL approval dialog now offers allow-once / allow-for-session / reject; choose "session" to stop repeated prompts for the same `(tool, path)` within the session (path-level, session-scoped approval memory) |
| **Spinner Color & Stuck Detection Fixes** | v0.6.83 | `thought for Ns` state word now always muted (only in-progress states warm up with time); stuck-detection no longer misfires on blank `thinking` fingerprints; stuck-detection switch-strategy hint switched to English for consistency |
| **Thinking Status Line & Tool Summary** | v0.6.82 | Spinner third field 4-state machine (`thinking` / `thought for Ns` / `still thinking` / `thinking more`); message-area thought line → `Thought for Ns, <action counts>`; consecutive thinking+read-only-tool rounds merged into one line; Bash non-verbose output summary with `... (N more lines) (ctrl+o to expand)` |
| **Semantic Gate Hardening & Two Permission Modes** | v0.6.81 | HITL gate rebuilt on upstream Jev semantics: `curl\|bash` and interpreter-family bypasses are now hard-denied, write path traversal can no longer skip the gate, and judge unavailability returns to human confirmation instead of blocking the whole session; permission modes collapsed to `auto`/`bypass` (default auto), with the deterministic layer no longer sharing a switch with the semantic judge |
| **Session Recap Persistence** | v0.6.80 | Recap and task-summary lines persist to `ThreadMeta` (`latest_recap`/`last_task_summary`), no longer lost on restart |

> Older releases (v0.6.0 – v0.6.76) are listed in the [CHANGELOG](./CHANGELOG.md).

---

## Install

Binaries available for macOS (x86_64 / Apple Silicon), Linux (x86_64 / aarch64 / riscv64), and Windows (x86_64).

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
| **Find a bug or piece of tech debt** | `issue-create` → `systematic-debugging` → `writing-plans` → `subagent-driven-development` → `issue-archive` → improve CLAUDE.md |
| **Want to build a new feature** | `grill-me` → `writing-plans` → `subagent-driven-development` |
| **Notice the codebase getting messy** | `slop-cleaner` → `improve-codebase-architecture` → `writing-plans` → `subagent-driven-development` |
| **Need someone to grok the architecture** | `teacher` → assign a task → `teacher` |

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
├── side-projects/             # Experimental projects (llm-gateway, etc.)
├── spec/                      # Design docs and specs
│   ├── global/                # Global architecture docs
│   ├── issues/                # Issue analysis docs
│   └── prd/                   # Product requirement docs
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
