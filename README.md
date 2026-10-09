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
| **Nobody Coding** | 99% of code produced by DeepSeek, Mimo, and GLM — humans decide what, AI figures out how |

### v0.6.x New Features

| Feature | Version | Description |
|---------|---------|-------------|
| **Lazy `LlmCallStart` Payload** | v0.6.108 | The executor used to run `state.messages().to_vec()` unconditionally before every LLM call — deep-copying the entire message history (tool-result bodies, image base64) just to populate the `LlmCallStart` event, even with no subscriber; #306 only removed the tracer-side second copy. Since Langfuse is the sole real consumer (TUI and the ACP mapper both drop the event), `AgentEventHandler` gained `wants_llm_call_payload()` (default `false`) and the executor now emits an empty payload unless the handler opts in — skipping the O(rounds × history) allocation entirely when Langfuse is off. Also in v0.6.108: user `!` shell-command blocks now align with tool-result rows instead of using a distinct indent/prefix |
| **Compact Approval Panel** | v0.6.107 | The approval panel truncated Bash arguments to one line and repeated options/hotkeys for batched tools. Parameters now wrap in full (Bash keeps newlines/indentation and CJK column width, Edit shows context plus a diff), the layout tightens to content, batches show position and per-choice counts, and oversized content prioritizes the active tool with an explicit "hidden lines" hint; permission logic, parent/child limits, session memory and key handling are unchanged |
| **Layered Instruction Merging** | v0.6.106 | When `AGENTS.md`, `CLAUDE.md` and directory-level guides coexist, rules used to be silently dropped. Candidates are now collected layer by layer from project root → cwd, loaded in order, deduplicated per directory by content, and frozen at `session/new`. The global layer moved from `~/.claude/AGENTS.md` to `~/.cc-code/AGENTS.md`; large files are bounded-read (1 MiB per file / 256 KiB injected by default) with CJK-safe truncation, and recursive `@import` shares the output budget |
| **Sub-Agent Permission Inheritance** | v0.6.106 | In Auto mode, normal / background / fork sub-agents now share the parent's permission mode, Jev gate, rule loader, classifier and session approval memory — closing the "delegate to a sub-agent to bypass approval" hole. The parent keeps allow / deny / ask; children only allow / deny, defaulting to deny on uncertainty, rule-extraction failure or cancellation; recursive delegation is forbidden |
| **HITL Three-Way Approval** | v0.6.106 | The approval popup now offers "approve once / approve for session / deny" together: arrow keys to select, Enter to submit, Tab / Shift+Tab to switch tools, Esc to deny all. Session memory is per-tool (files by tool+path, Bash by full command+dir+branch, others by full args), and explicit deny rules always win over memory |
| **`/export` No Longer Truncates Tool Calls** | v0.6.106 | Exported Markdown/PlainText used to slice tool-call arguments at `chars().take(200)` — in a real log 156 of 210 Bash calls (74%) were cut at exactly 200 chars, leaving unterminated JSON mid-command; `Write.content` and `Edit` strings suffered too. Arguments are now emitted in full, the tool-result branch prints the whole body and flags errors, and the code fence adapts to backtick runs so heredoc scripts can't break the Markdown |
| **Log File Failure No Longer Panics** | v0.6.105 | If `~/.cc-code/logs/{service}.log` couldn't be opened (e.g. an ACL corrupted to an empty DACL), `.expect("cannot open log file")` **panicked the whole process** — logging is diagnostics, not a startup dependency. It now warns and falls back to `std::io::stderr`, `ensure_utf8_bom()` fails silently, and `set_global_default` degrades to a warning |
| **Unified Built-in Instructions** | v0.6.105 | Unified English instructions across 14 main templates, 4 built-in agents, tool/param docs, Skills, ACP commands, compaction/recap and approval models: removed rigid "four-line / one-word reply" and "stop after edit" limits, made authorization scope, sustained execution and verification evidence explicit, and aligned tool docs with the implementation (Read paging & 32 MiB cap, Bash shell/timeout contract). Core tool descriptions −40.9%, built-in agent definitions −31.9% |
| **Markdown Cache Memory Accounting** | v0.6.104 | `/gc` now reports the Markdown cache's estimated heap footprint (total / average / largest entry, rendered lines & spans), computed in a single lock without cloning parsed results or disturbing LRU order; the estimate is folded into the "known" total, explicitly flagged as a `capacity()`-based estimate excluding LRU/allocator overhead and **not** RSS. The "unidentified" note is now honest: leftover bytes are not yet attributed and cannot be read as a leak |
| **Legacy `~/.peri` Compatibility Removed** | v0.6.104 | App data now lives exclusively under `~/.cc-code`; the per-file fallback to the pre-rename `~/.peri` directory is gone. **Breaking:** files that still exist only in `~/.peri` are no longer read (migrate them manually). The `.peri` entry stays in the HITL sensitive-dir denylist — a security measure, not compatibility |

> Older releases (v0.6.0 – v0.6.103) are listed in the [CHANGELOG](./CHANGELOG.md).

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
| **Find a bug or piece of tech debt** | `issue-create` → `systematic-debugging` → `writing-plans` → `subagent-driven-development` → `issue-archive` → improve CLAUDE.md |
| **Want to build a new feature** | `brainstorming` → `writing-plans` → `subagent-driven-development` |
| **Notice the codebase getting messy** | `slop-cleaner` → `writing-plans` → `subagent-driven-development` |
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
