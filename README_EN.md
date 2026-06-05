<div align="center">

[中文](README.md) | **English**

# Peri

**Terminal coding agent powered by open-source models — Rust-built, Claude Code compatible**

DeepSeek-V4-Pro + Mimo-2.5Pro + GLM-5.1 driven, zero migration from `.claude/` config, runs on RISC-V.

[![npm](https://img.shields.io/npm/v/@cc-claw/peri)](https://www.npmjs.com/package/@cc-claw/peri)
[![GitHub stars](https://img.shields.io/github/stars/wismyzhizi2018/peri?style=social)](https://github.com/wismyzhizi2018/peri/stargazers)
[![License](https://img.shields.io/badge/license-Apache%202.0-blue?style=flat-square)](LICENSE)

```bash
npm install -g @cc-claw/peri
```

[Why Peri](#why-peri) · [Core Capabilities](#core-capabilities) · [Install](#install) · [Nobody Coding](#how-we-built-peri-with-nobody-coding) · [Acknowledgments](#acknowledgments)

</div>

---

## Why Peri?

| Comparison | Other Terminal Agents | Peri |
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
| **Experimental** | Built-in LSP, split-screen, background sub-agent parallelism |

---

## Install

Binaries available for macOS (x86_64 / Apple Silicon), Linux (x86_64 / aarch64 / riscv64), and Windows (x86_64).

### npm (Recommended)

```bash
npm install -g @cc-claw/peri
```

### Upgrade

```bash
npm update -g @cc-claw/peri
```

### macOS / Linux (Script)

```bash
curl -fsSL https://raw.githubusercontent.com/wismyzhizi2018/peri/main/scripts/install.sh | bash
```

### Windows (PowerShell)

```powershell
irm https://raw.githubusercontent.com/wismyzhizi2018/peri/main/scripts/install.ps1 | iex
```

---

## How We Built Peri with Nobody Coding

**Nobody Coding** means exactly what it sounds like. No human wrote a single line of Peri — not the architecture, not the TUI, not the harness tuning that makes open-source models reliable in an Agent loop. Humans decide *what*. AI figures out *how*. You're not pair programming — you're product managing an engineer that never sleeps. 99% of Peri was built this way.

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
peri/
├── peri-agent/                # Core: Agent loop, tool system, persistence, telemetry
├── peri-middlewares/           # Middleware: filesystem, terminal, MCP, Hooks, etc.
├── peri-tui/                  # TUI application (Ratatui)
├── peri-acp/                  # ACP service layer: bridges TUI/IDE with Agent
├── peri-widgets/              # Widget component library
├── peri-lsp/                  # LSP client library
├── langfuse-client/           # Langfuse telemetry client
├── scripts/
│   ├── install.sh             # macOS / Linux installer
│   └── install.ps1            # Windows installer
├── side-projects/             # Experimental projects
├── README.md
└── LICENSE                    # Apache 2.0
```

---

## Acknowledgments

| Project | Description |
|---------|-------------|
| [Claude Code Best](https://github.com/claude-code-best/claude-code) | Community support and feedback |
| [Superpowers](https://github.com/obra/superpowers) & [Matt Pocock's Skills](https://github.com/mattpocock/skills) | Skill suites driving Peri's AI engineering workflow |
| [ACP](https://agentclientprotocol.com/) | Open protocol for agent-IDE communication |
| [rmcp](https://github.com/anthropics/rmcp) | Rust MCP client library |
| [Ratatui](https://ratatui.rs) & [Tokio](https://tokio.rs) | TUI framework and async runtime |
| [Langfuse](https://langfuse.com) | LLM observability |
| [Zed](https://zed.dev) | First ACP-compatible IDE, proved the protocol works |

---

## License

[Apache License 2.0](LICENSE) — free to use, modify, and distribute, including commercial use.
