<div align="center">

[English](README.md) | **中文**

# cc-code

**用开源模型跑 Agent Loop — Rust 写的终端编程助手，兼容 Claude Code 全家桶**

DeepSeek-V4-Pro + Mimo-2.5Pro + GLM-5.1 驱动，`.claude/` 配置零迁移，RISC-V 也能跑。

[![npm](https://img.shields.io/npm/v/@cc-claw/code)](https://www.npmjs.com/package/@cc-claw/code)
[![GitHub stars](https://img.shields.io/github/stars/cc-claws/cc-code?style=social)](https://github.com/cc-claws/cc-code/stargazers)
[![License](https://img.shields.io/badge/license-Apache%202.0-blue?style=flat-square)](LICENSE)
[![Website](https://img.shields.io/badge/website-cc--claw.com-orange?style=flat-square)](https://www.cc-claw.com)
[![GitHub last commit](https://img.shields.io/github/last-commit/cc-claws/cc-code?style=flat-square)](https://github.com/cc-claws/cc-code/commits/main)

<p align="center"><code>npm install -g @cc-claw/code</code></p>

### 🌐 官网：**[cc-claw.com](https://www.cc-claw.com)**

[为什么选 cc-code](#为什么选-cc-code) · [核心能力](#核心能力) · [安装](#安装) · [Nobody Coding](#我们怎么用-nobody-coding-造-cc-code) · [致谢](#致谢)

</div>

## ❤️Sponsor

> [想出现在这里？](mailto:wismyzhizi2018@gmail.com)

<details open>
<summary>Click to collapse</summary>

[![Kimi K2.6](assets/partners/logos/kimi.png)](https://platform.moonshot.cn/console?aff=cc-code)

Kimi K2.6 是 Moonshot AI 开源的原生多模态 Agent 模型，专为长程编程、编程驱动设计和群组任务编排而生。支持前端、DevOps、性能优化、全栈工程等复杂端到端工作流。[点击注册](https://platform.moonshot.cn/console?aff=cc-code)

---

<table>
<tr>
<td width="180"><a href="https://platform.xiaomimimo.com?ref=JBEYTF"><img src="assets/partners/logos/mimo.png" alt="Xiaomi MiMo" width="150"></a></td>
<td>小米顶尖模型 MiMo V2.5，通过邀请码注册：双方各得 ¥10 API 体验金 + 首单 9 折。邀请码：JBEYTF。<a href="https://platform.xiaomimimo.com?ref=JBEYTF">点击注册</a>（注册后自动填入 · 体验金 40 天有效）</td>
</tr>

<tr>
<td width="180"><a href="https://www.bigmodel.cn/glm-coding?ic=MR7BVITFAY"><img src="assets/partners/logos/glm.png" alt="GLM" width="150"></a></td>
<td>智谱 GLM Coding Plan — 国内顶流编程大模型，20+ 主流工具全适配，性价比拉满。<a href="https://www.bigmodel.cn/glm-coding?ic=MR7BVITFAY">立即参与「拼好模」</a></td>
</tr>
</table>

</details>

---

## 为什么选 cc-code？

| 对比项 | 其他终端 Agent | cc-code |
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
| **Sub-Agent 并发** | 后台子 agent 并行执行，支持 fork 和 background 模式 |
| **HITL 审批** | 敏感操作自动拦截，支持 auto-classifier 和 shared-mode |
| **双引擎文件搜索** | Grep/Glob 优先使用外部 ripgrep 二进制，回退到内置 Rust 引擎 |
| **Nobody Coding** | 99% 代码由 DeepSeek、Mimo、GLM 产出 — 人决定做什么，AI 想怎么做 |

### v0.6.x 新增功能

| 功能 | 版本 | 说明 |
|------|------|------|
| **详细模式长命令状态刷新修复** | v0.6.101 | 详细模式（Ctrl+O）下 Bash 命令超长时 header 会折成多行（#264），但 tick 增量刷新仍按「header 恒 1 行」的旧假设处理：把命令续行覆盖成 `Running…`，同时冻结真正的状态行（屏幕上出现两处 `Running…` 且时间不一致）。现改为按内容定位状态行，不再依赖固定下标 / 固定行数 |
| **`/gc` 诊断去误导 + VM 估算** | v0.6.100 | `/gc` 内存标签改为**分平台**语义（`active` / `mapped` / `retained` 在 jemalloc 与 mimalloc 下含义不同，Windows 数字不再暗示虚假碎片）；估算器纳入 `view_messages`，此前「未识别」的大头正在此处 |
| **后台 shell 通知接入 i18n** | v0.6.99 | 后台 shell 完成 / 超时 / 取消 / 终止 / 等待输入通知的展示文案改为走 i18n（此前硬编码中文，英文语言下仍显示中文）；新增进程级语言注册表，供静态 `MessageViewModel` 构造路径解析当前语言，启动与 `/lang` 切换时同步 |
| **ACP 权限审批转发** | v0.6.94 | `cc-code acp` 改为通过 `session/request_permission` 把工具审批请求转发给 IDE 客户端，不再自动放行（fail-closed：客户端不支持 / 调用失败 / 未知选项一律拒绝）；stdio 默认权限模式由 Bypass 改为 AutoMode（无人值守可用 `session/set_mode` 切到 bypass） |
| **默认启用审批** | v0.6.90 | 审批默认开启：`YOLO_MODE` 未设置不再跳过 HITL（需显式 `-y/--yolo` 或 `YOLO_MODE=true` 才免审批）；HITL 门控评估改写后的实际执行命令；`git clone` 防参数注入 |
| **工具审批三选** | v0.6.84 | HITL 审批弹窗改为三选：一次性同意 / 本次会话同意 / 拒绝；选「本次会话同意」后同 `(工具, 路径)` 本次会话内免问（路径级、会话作用域审批记忆） |
| **spinner 配色与卡住检测修复** | v0.6.83 | `thought for Ns` 状态词改为始终灰色（仅进行中状态词随耗时升温）；卡住检测不再把空白 `thinking` 误判为循环；卡住检测换策略提示改用英文保持一致 |
| **Thinking 状态行与工具动作汇总** | v0.6.82 | spinner 行第三字段四态状态机（`thinking` / `thought for Ns` / `still thinking` / `thinking more`）；消息区思考行改为 `Thought for Ns, <动作计数>`；连续「思考+只读工具」轮合并为一行；Bash 非详细模式展示输出摘要 + `... (N more lines) (ctrl+o to expand)` |
| **语义门加固与权限模式收敛** | v0.6.81 | HITL 语义门按上游 Jev 语义重做：补上 `curl\|bash` 与解释器家族绕过、写路径穿越不再能跳过门、判定不可用改为回到人工确认而非堵死整个会话；权限模式收敛为 `auto`/`bypass`（默认 auto），确定性防线不再与语义判定共用开关 |
| **会话恢复 Recap 持久化** | v0.6.80 | Recap 与任务完成总结行落库到 `ThreadMeta`（`latest_recap`/`last_task_summary`），`-c`/`-r` 恢复后不再丢失 |

> 更早版本（v0.6.0 – v0.6.76）见 [CHANGELOG](./CHANGELOG.md)。

---

## 安装

支持 macOS (x86_64 / Apple Silicon)、Linux (x86_64 / aarch64 / riscv64)、Windows (x86_64)。

### npm（推荐）

```bash
npm install -g @cc-claw/code
```

### 升级

```bash
npm update -g @cc-claw/code
```

### macOS / Linux（脚本安装）

```bash
curl -fsSL https://raw.githubusercontent.com/cc-claws/cc-code/main/scripts/install.sh | bash
```

### Windows (PowerShell)

```powershell
irm https://raw.githubusercontent.com/cc-claws/cc-code/main/scripts/install.ps1 | iex
```

---

## 我们怎么用 Nobody Coding 造 cc-code

**Nobody Coding** 字面意思：没有人类写过一行 cc-code 代码 — 架构、TUI、harness tuning 全是 AI 干的。人决定 *做什么*，AI 想 *怎么做*。你不是在结对编程，你是在管一个不睡觉的工程师。cc-code 99% 的代码都是这么来的。

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
cc-code/
├── cc-agent/                # 核心：Agent loop、工具系统、持久化、遥测
│   └── README.md              # Agent 框架使用指南
├── cc-middlewares/           # 中间件：文件系统、终端、MCP、Hooks 等
│   ├── README.md              # 中间件概览
│   └── CLAUDE.md              # 开发指南和陷阱记录
├── cc-tui/                  # TUI 应用 (Ratatui)
│   ├── README.md              # TUI 使用指南
│   └── CLAUDE.md              # 开发指南和陷阱记录
├── cc-acp/                  # ACP 服务层：桥接 TUI/IDE 与 Agent
│   └── README.md              # ACP 架构和数据流
├── cc-widgets/              # Widget 组件库
│   └── README.md              # 组件列表和使用示例
├── cc-lsp/                  # LSP 客户端库
│   └── README.md              # LSP 操作和配置
├── langfuse-client/           # Langfuse 遥测客户端
│   └── README.md              # 遥测配置和使用
├── npm/                       # npm 包：postinstall 脚本 + shell wrapper
│   └── README.md              # npm 包说明
├── scripts/
│   ├── install.sh             # macOS / Linux 安装器
│   └── install.ps1            # Windows 安装器
├── side-projects/             # 实验性项目（llm-gateway 等）
├── spec/                      # 设计文档与规范
│   ├── global/                # 全局架构文档
│   ├── issues/                # Issue 分析文档
│   └── prd/                   # 产品需求文档
├── CLAUDE.md                  # 项目开发指南
├── CHANGELOG.md               # 版本变更记录
├── CONTRIBUTING.md            # 贡献指南
├── README.md
├── README_ZH.md
└── LICENSE                    # Apache 2.0
```

---

## 致谢

| 项目 | 说明 |
|------|------|
| [Peri (KonghaYao)](https://github.com/KonghaYao/peri) | 本项目 fork 自 Peri，基于 Apache 2.0 协议分发，感谢原作者的开创性工作 |
| [Superpowers](https://github.com/obra/superpowers) & [Matt Pocock's Skills](https://github.com/mattpocock/skills) | 驱动 cc-code AI 工程工作流的 skill 套件 |
| [ACP](https://agentclientprotocol.com/) | Agent-IDE 通信开放协议 |
| [rmcp](https://github.com/anthropics/rmcp) | Rust MCP 客户端库 |
| [Ratatui](https://ratatui.rs) & [Tokio](https://tokio.rs) | TUI 框架和异步运行时 |
| [Langfuse](https://langfuse.com) | LLM 可观测性 |
| [Zed](https://zed.dev) | 第一个 ACP 兼容 IDE，验证了协议可行性 |

---

## Star History

[![Star History Chart](https://api.star-history.com/svg?repos=cc-claws/cc-code&type=Date)](https://www.star-history.com/#cc-claws/cc-code&Date)

---

## 许可证

[Apache License 2.0](LICENSE) — 可自由使用、修改、分发，包括商业用途。
