<div align="center">

[English](README.md) | **中文**

# cc-code

**用开源模型跑 Agent Loop — Rust 写的终端编程助手，兼容 Claude Code 全家桶**

DeepSeek-V4-Pro + Mimo-2.5Pro + GLM-5.1 驱动，`.claude/` 配置零迁移。

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

### v0.6.x 新增功能

| 功能 | 版本 | 说明 |
|------|------|------|
| **状态栏第二行「最近工具」实时摘要** | v0.6.111 | 第二行的运行中工具段原来只在执行期间存在，而且经常根本看不到：`poll_agent` 每帧把 ACP 通知一次 drain 干净，Read/Glob 这类毫秒级工具的 ToolStart + ToolEnd 落在同一帧，`◐ Read : x.rs` 一帧都没渲染过；摘要来源还只是 11 个工具的白名单（Agent / TodoWrite / AskUserQuestion / MCP 只有裸名字），顺序也是「老在左」。现改为由 `AgentComm.recent_tools` 驱动的一行流水：最新在最左、最多留 2 条（第 3 条挤掉最老），条目显示时长取 `max(实际执行时长, 300ms)`——快工具垫到 300ms 后消失，慢工具执行结束即刻消失，工具结束只让右侧聚合计数 `✓ Name ×N` +1、不再生成带摘要的完成条目。摘要覆盖全部工具（AskUserQuestion → 首个问题，Agent → description，TodoWrite → 任务数，其余 → 第一个非空字符串字段），字符上限 20 → 30，截断仍是路径语义，`◐`/`✓`/工具名/摘要配色统一。第二行强制单行不折行，超宽按显示列宽（unicode-width）在行尾 `…` 收口，不再被 `Paragraph` 静默裁掉 |
| **跨工具 Schema 聚合熔断与类型化连续失败追踪** | v0.6.110 | Schema 熔断器此前按工具名独立计数，模型在不同工具间轮换猜错参数即可逃逸单工具熔断阈值；连续失败检测按完整错误文本做 key，参数名差异导致计数被稀释。现引入跨工具聚合连续失败追踪（阈值 3 次）并支持指数退避，精准阻断跨工具乱猜参数死循环；连续失败检测改为按 `(tool_name, ToolErrorKind)` 聚合，错误文本动态变化亦能可靠累计；警告提示严格在所有工具结果写入后统一追加，杜绝孤立 `tool_result` 风险（#379） |
| **动词流光动效与状态词阶梯升温** | v0.6.109 | 参考 Codex CLI 物理余弦衰减模型，动词（`Executing…` / `Thinking…`）引入独立舒缓流光（5.0s 周期，同色系提亮 45% 绝不刺眼），长 Bash 工具执行期间以 8.0s 极低频独立流光充当生命体征心跳；思考状态词与动词动静解耦，状态词完全不闪烁，纯按时间阶梯从浅灰逐步加温至琥珀金（0~2.5s 浅灰 ➔ 2.5~5s 柔白 ➔ 5~15s 动词暖橙 ➔ 15~60s still thinking 浅金 ➔ >=60s deep in thought 琥珀金）。同版本另修：ASCII 降级表补 `⎿`（U+23BF）消除异常列宽终端前缀问号 |
| **`LlmCallStart` 载荷按需构造** | v0.6.108 | executor 曾在每轮 LLM 调用前无条件 `state.messages().to_vec()`——仅为构造 `LlmCallStart` 事件就深拷贝整个消息历史（含工具结果正文、图片 base64），即使没有任何订阅者；#306 只修掉了 tracer 侧的第二次深拷贝。由于唯一真实消费者是 Langfuse tracer（TUI 与 ACP mapper 均丢弃该事件），`AgentEventHandler` 新增 `wants_llm_call_payload()`（默认 `false`），executor 在 handler 未声明时发空载荷——Langfuse 未启用时完全跳过 O(轮数 × 历史大小) 的分配。同版本另修：用户 `!` shell 命令块现与工具结果行对齐，不再使用独立缩进 / 前缀 |
| **紧凑审批面板** | v0.6.107 | 审批面板此前把 Bash 参数单行截断、批量工具重复展示选项与快捷键。现参数完整换行展示（Bash 保留换行 / 缩进与中文显示列宽，Edit 展示上下文与增删对比），布局按内容收紧，批量显示当前位置与三类计数，超高内容优先当前工具并明确提示剩余未显示行数；权限判定、父子 Agent 限制、会话审批记忆与按键处理不变 |
| **指引文件分层合并** | v0.6.106 | `AGENTS.md`、`CLAUDE.md` 与多层目录指引并存时，原实现会**漏载规则**。现按项目根 → 工作目录逐层收集同层候选、依次加载，同目录按内容去重，并在 `session/new` 时冻结；全局层由 `~/.claude/AGENTS.md` 迁移至 `~/.cc-code/AGENTS.md`。大文件有界读取（单文件默认 1 MiB / 注入默认 256 KiB）且中文切边安全，递归 `@import` 共享输出预算 |
| **子 Agent 权限继承** | v0.6.106 | Auto 模式下普通 / 后台 / fork 子 Agent 现共享父级权限模式、Jev 门、规则加载器、分类器与会话审批记忆，堵住「委派给子 Agent 即可绕过审批」的漏洞。父保留允许 / 拒绝 / 询问；子只有允许 / 拒绝，不确定、规则提炼失败或取消一律拒绝，并禁止递归委派 |
| **HITL 三档审批** | v0.6.106 | 审批弹窗同时提供「同意本次 / 本次会话同意 / 拒绝」：上下键选择、Enter 提交、Tab / Shift+Tab 切换工具、Esc 全部拒绝。会话记忆按工具细化（文件按工具+路径，Bash 按完整命令+执行目录+分支，其他按完整参数），显式禁止规则始终优先于记忆 |
| **`/export` 不再截断工具调用** | v0.6.106 | 导出的 Markdown/PlainText 此前把工具参数按 `chars().take(200)` 截断——真实日志中 210 次 Bash 调用有 156 次（74%）被切在正好 200 字符，JSON 未闭合、命令从中间断掉；`Write.content`、`Edit` 参数同样受影响。现完整输出参数，工具结果分支输出完整正文并标注错误，代码围栏按连续反引号自适应，heredoc 脚本不再破坏 Markdown 结构 |
| **日志打不开不再 panic** | v0.6.105 | `~/.cc-code/logs/{service}.log` 无法打开（如 ACL 被写坏成空 DACL）时，`.expect("cannot open log file")` 会**直接 panic 掉整个进程**——日志是诊断设施，不该是启动硬依赖。现改为警告并退回 `std::io::stderr`，`ensure_utf8_bom()` 静默忽略，`set_global_default` 降级为警告 |
> 更早版本（v0.6.0 – v0.6.106）见 [CHANGELOG](./CHANGELOG.md)。

---

## 安装

支持 macOS (x86_64 / Apple Silicon)、Linux (x86_64 / aarch64)、Windows (x86_64)。

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
| 开新功能 | `brainstorming` → `writing-plans` → `subagent-driven-development` |
| 代码库变乱了 | `slop-cleaner` → `writing-plans` → `subagent-driven-development` |
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
├── side-projects/             # 实验性项目（peri-sync）
├── spec/                      # 设计文档与规范
│   ├── global/                # 全局架构文档
│   ├── issues/                # Issue 分析文档
│   └── archive/               # 已归档的 feature 规范
├── docs/                      # 长文文档（prd、adr、designs、acp）
├── human/                     # 人工走查清单
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
