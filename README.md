<div align="center">

**中文** | [English](README_EN.md)

# Peri

**用开源模型跑 Agent Loop — Rust 写的终端编程助手，兼容 Claude Code 全家桶**

DeepSeek-V4-Pro + Mimo-2.5Pro + GLM-5.1 驱动，`.claude/` 配置零迁移，RISC-V 也能跑。

[![npm](https://img.shields.io/npm/v/@cc-claw/peri)](https://www.npmjs.com/package/@cc-claw/peri)
[![GitHub stars](https://img.shields.io/github/stars/wismyzhizi2018/peri?style=social)](https://github.com/wismyzhizi2018/peri/stargazers)
[![License](https://img.shields.io/badge/license-Apache%202.0-blue?style=flat-square)](LICENSE)

```bash
npm install -g @cc-claw/peri
```

[为什么选 Peri](#为什么选-peri) · [核心能力](#核心能力) · [安装](#安装) · [Nobody Coding](#我们怎么用-nobody-coding-造-peri) · [致谢](#致谢)

</div>

---

## 为什么选 Peri？

| 对比项 | 其他终端 Agent | Peri |
|--------|---------------|------|
| 运行时 | Node.js / Bun，动辄吃 1GB 内存 | Rust 原生，启动快，~50MB 内存 |
| 模型绑定 | 锁死一家 LLM | 随便换：Anthropic、OpenAI 兼容、DeepSeek、GLM |
| Prompt 缓存 | 每轮重算，token 白烧 | 冻结 system prompt，95-99% 缓存命中率 |
| 工具加载 | 全量塞进每轮请求 | 核心工具常驻，其余 Tool Search 按需懒加载 |
| IDE 集成 | 只有终端 | ACP 协议，Zed 等 IDE 直连 |
| Claude Code 生态 | 不兼容 | 直接用 `.claude/` 配置、agents、skills、hooks、MCP |

---

## 核心能力

| 能力 | 说明 |
|------|------|
| **Rust 原生** | 快启动、低内存、零运行时开销 |
| **Context 优化** | system prompt 冻结 + 动态内容隔离，token 不浪费 |
| **多 LLM 支持** | Anthropic / OpenAI 兼容 API，DeepSeek、GLM 随便切 |
| **Claude Code 兼容** | `.claude/` 配置、agents、skills、hooks、MCP、子 agent 直接复用 |
| **流式 Markdown** | 代码块、表格、diff 实时渲染 |
| **ACP 协议** | 接入 Zed 等 IDE，也支持自建 "Cloud Code" 平台 |
| **Auto Compact** | 长会话自动压缩，保持响应快且省 token |
| **实验功能** | 内置 LSP、分屏、后台子 agent 并行 |

---

## 安装

支持 macOS (x86_64 / Apple Silicon)、Linux (x86_64 / aarch64 / riscv64)、Windows (x86_64)。

### npm（推荐）

```bash
npm install -g @cc-claw/peri
```

### 升级

```bash
npm update -g @cc-claw/peri
```

### macOS / Linux（脚本安装）

```bash
curl -fsSL https://raw.githubusercontent.com/wismyzhizi2018/peri/main/scripts/install.sh | bash
```

### Windows (PowerShell)

```powershell
irm https://raw.githubusercontent.com/wismyzhizi2018/peri/main/scripts/install.ps1 | iex
```

---

## 我们怎么用 Nobody Coding 造 Peri

**Nobody Coding** 字面意思：没有人类写过一行 Peri 代码 — 架构、TUI、harness tuning 全是 AI 干的。人决定 *做什么*，AI 想 *怎么做*。你不是在结对编程，你是在管一个不睡觉的工程师。Peri 99% 的代码都是这么来的。

> 最近的 commit 几乎全是 DeepSeek、Mimo 和 GLM 的产出。Claude 只在最初参与过。

### 典型工作流

| 你要做的事 | 流水线 |
|-----------|--------|
| 发现 bug 或技术债 | `issue-create` → `systematic-debugging` → `writing-plans` → `subagent-driven-development` → `issue-archive` → 改进 CLAUDE.md |
| 开新功能 | `grill-me` → `writing-plans` → `subagent-driven-development` |
| 代码库变乱了 | `slop-cleaner` → `improve-codebase-architecture` → `writing-plans` → `subagent-driven-development` |
| 需要理解架构 | `teacher` → 分配任务 → `teacher` |

---

## 仓库结构

```text
peri/
├── @cc-claw/peri/                # 核心：Agent loop、工具系统、持久化、遥测
├── peri-middlewares/           # 中间件：文件系统、终端、MCP、Hooks 等
├── peri-tui/                  # TUI 应用 (Ratatui)
├── peri-acp/                  # ACP 服务层：桥接 TUI/IDE 与 Agent
├── peri-widgets/              # Widget 组件库
├── peri-lsp/                  # LSP 客户端库
├── langfuse-client/           # Langfuse 遥测客户端
├── scripts/
│   ├── install.sh             # macOS / Linux 安装器
│   └── install.ps1            # Windows 安装器
├── side-projects/             # 实验性项目
├── README.md
└── LICENSE                    # Apache 2.0
```

---

## 致谢

| 项目 | 说明 |
|------|------|
| [Claude Code Best](https://github.com/claude-code-best/claude-code) | 社区支持和反馈 |
| [Superpowers](https://github.com/obra/superpowers) & [Matt Pocock's Skills](https://github.com/mattpocock/skills) | 驱动 Peri AI 工程工作流的 skill 套件 |
| [ACP](https://agentclientprotocol.com/) | Agent-IDE 通信开放协议 |
| [rmcp](https://github.com/anthropics/rmcp) | Rust MCP 客户端库 |
| [Ratatui](https://ratatui.rs) & [Tokio](https://tokio.rs) | TUI 框架和异步运行时 |
| [Langfuse](https://langfuse.com) | LLM 可观测性 |
| [Zed](https://zed.dev) | 第一个 ACP 兼容 IDE，验证了协议可行性 |

---

## 许可证

[Apache License 2.0](LICENSE) — 可自由使用、修改、分发，包括商业用途。
