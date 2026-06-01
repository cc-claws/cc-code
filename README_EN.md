<div align="center">

[中文](README.md) | **English**

# Peri

**Open-source models in an Agent Loop — a Rust terminal coding agent, fully compatible with the Claude Code ecosystem**

Powered by DeepSeek-V4-Pro + GLM-5.1. Zero migration — your `.claude/` config just works. Even runs on RISC-V.

[![GitHub stars](https://img.shields.io/github/stars/konghayao/peri?style=social)](https://github.com/konghayao/peri/stargazers)
[![License](https://img.shields.io/badge/license-Apache%202.0-blue?style=flat-square)](LICENSE)

[Why Peri](#why-peri) · [Features](#features) · [Install](#install) · [Nobody Coding](#how-we-built-peri-with-nobody-coding) · [Acknowledgments](#acknowledgments)

</div>

---

## Why Peri?

| | Other terminal agents | Peri |
|---|---|---|
| **Runtime** | Node.js / Bun, easily 1GB+ memory | Native Rust, fast startup, ~50MB memory |
| **Model lock-in** | Tied to one LLM | Any: Anthropic, OpenAI-compatible, DeepSeek, GLM |
| **Prompt caching** | Recomputes every round, wasting tokens | Frozen system prompt, 95-99% cache hit rate |
| **Tool loading** | All tools in every request | Core tools resident, rest lazy-loaded via Tool Search |
| **IDE integration** | Terminal only | ACP protocol — connects to Zed and other IDEs |
| **Claude Code ecosystem** | Incompatible | Drop-in `.claude/` config, agents, skills, hooks, MCP |

---

## Features

| Feature | Description |
|---------|-------------|
| **Native Rust** | Fast startup, low memory, zero runtime overhead |
| **Context optimized** | Frozen system prompt + dynamic content isolation — no token waste |
| **Multi-LLM** | Anthropic / OpenAI-compatible APIs — DeepSeek, GLM, whatever works for you |
| **Claude Code compatible** | `.claude/` config, agents, skills, hooks, MCP, sub-agents — all reusable |
| **Streaming Markdown** | Code blocks, tables, diffs — all rendered live |
| **ACP protocol** | IDE-ready via [ACP](https://agentclientprotocol.com/) — Zed and more |
| **Auto Compact** | Long sessions stay fast and cheap |
| **Experimental** | Built-in LSP, split screen, background sub-agents |

---

## Install

Binaries available for macOS (x86_64 / Apple Silicon), Linux (x86_64 / aarch64 / riscv64), and Windows (x86_64).

### macOS / Linux

```bash
curl -fsSL https://raw.githubusercontent.com/konghayao/peri/main/scripts/install.sh | bash
```

### Windows (PowerShell)

```powershell
irm https://raw.githubusercontent.com/konghayao/peri/main/scripts/install.ps1 | iex
```

---

## How We Built Peri with Nobody Coding

**Nobody Coding** means exactly what it sounds like. No human wrote a single line of Peri — not the architecture, not the TUI, not the harness tuning that makes open-source models reliable in an Agent loop. Humans decide *what*. AI figures out *how*. You're not pair programming — you're product managing an engineer that never sleeps. 99% of Peri was built this way.

> Recent commits are almost entirely DeepSeek and GLM. Claude was just there in the beginning.

### Typical Workflows

| When you... | Pipeline kicks off |
|---|---|
| **Find a bug or tech debt** | `issue-create` → `systematic-debugging` → `writing-plans` → `subagent-driven-development` → `issue-archive` → improve CLAUDE.md |
| **Want to build a new feature** | `grill-me` → `writing-plans` → `subagent-driven-development` |
| **Notice the codebase getting messy** | `slop-cleaner` → `improve-codebase-architecture` → `writing-plans` → `subagent-driven-development` |
| **Need someone to grok the architecture** | `teacher` → assign a task → `teacher` |

---

## Repository Structure

```text
peri/
├── src/                       # Rust source code
│   ├── agent/                 # Agent loop core
│   ├── tui/                   # Terminal UI (Ratatui)
│   ├── tools/                 # Built-in tool implementations
│   └── ...
├── scripts/
│   ├── install.sh             # macOS / Linux installer
│   └── install.ps1            # Windows installer
├── tests/
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
