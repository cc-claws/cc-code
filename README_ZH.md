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
| **Jev 规则缓存改为多行 JSON 输出** | v0.6.118 | Jev 安全规则的磁盘持久化缓存（`~/.cc-code/jev/peri-<项目哈希>/<规则哈希>.json`）此前用 `serde_json::to_vec` 落盘为**紧凑单行** JSON，规则正文动辄数千字符全挤在一行，用户直接打开查看规则时几乎无法阅读。现将 `write()` 序列化改为 `serde_json::to_vec_pretty`（2 空格缩进），规则正文文件与 `<签名>.index.json` 索引同步美化为多行。内容哈希对**落盘字节**计算（`digest(&bytes)`），格式化前后各自自洽，缓存仍能正常命中；存量旧缓存因文件名（哈希）与格式化后不一致会 miss 一次并重新提炼，无功能影响。仅影响磁盘落盘格式，内存缓存与 Jev 判定逻辑不变（#412） |
| **Windows 全局入口直起 exe（修复 Ctrl+C 直接退出）** | v0.6.117 | 修复 Windows Terminal（ConPTY）下经 npm 全局入口启动时按 Ctrl+C 直接退出 TUI 的缺陷：npm 全局 wrapper（`<prefix>/cc-code.cmd|.ps1`）经 node（`execFileSync`）拉起 `cc-code.exe`，Ctrl+C 的 `CTRL_C_EVENT` 广播给整个控制台进程组（node + `cc-code.exe`），`cc-code.exe` 虽用 `SetConsoleCtrlHandler` 拦截了信号，但无 handler 的 node 父进程按默认行为被终止，`execFileSync` 同步等待随之崩断并拖死 exe——表现为 agent 任意状态按 Ctrl+C 都直接退出、日志在 streaming 中戛然而止。现于 Windows postinstall 阶段新增 `overwriteNpmGlobalWrapper()`，把全局入口改写为**直起 `bin/cc-code.exe`**（相对 prefix 根引用，不硬编码绝对路径），去掉 node 中间层后进程组内只剩 `cc-code.exe`，`SetConsoleCtrlHandler` 正常生效，恢复「Ctrl+C 中断 agent / 空闲双击退出」的设计行为。改写为防御性实现：仅在 exe 存在且能定位到 npm 全局 wrapper 时才覆盖，失败不阻塞安装；`bin/cc-code` node 脚本与非 Windows 路径行为不变（#410、#411） |
| **Schema 熔断改为滑动窗口计数** | v0.6.116 | 修复 Schema 熔断器在真实失败模式下从不触发的缺陷：模型常在多个工具间交替配错参数（WebSearch → WebFetch → WebSearch）且失败之间夹着成功调用，而旧实现的计数口径是「连续失败」、`reset()` 又在**任意工具**成功时清空跨工具聚合计数，导致阈值永远凑不满（实测 3 次 schema 失败、4 次失败注入 0 次提示）；同时 `TOOL_SIGNATURE_HINTS` 缺反向条目，把 `query`/`num_results` 打给别的工具时静默无提示，模型在一侧被提示「改用 WebFetch」、另一侧无反馈，被来回推入震荡。现计数改为**滑动窗口**口径（容量 6，单工具与跨工具计数均自窗口派生），`reset()` 只复位该工具的退避状态、不再清空窗口，成功调用同占窗口位推动旧失败淘汰；并补全反向条目（`query`/`num_results` → `WebSearch`、`old_string`+`new_string` → `Edit`、`content`+`file_path` → `Write`）。以真实 trace 原始序列验证：修复前触发 0 次，修复后触发 `PerTool { tool_name: "WebSearch", count: 2 }`（#404、#408） |
| **Jev 规则提炼超时阶梯与低思考档提炼** | v0.6.115 | v0.6.114 引入的规则提炼磁盘缓存（#398、#399）在真实环境里**从未生效**——`~/.cc-code/jev/` 目录始终不存在。三个根因叠加成 `complete` 恒为 `false`，永远进不了「仅持久化完整非空结果」的分支：其一，提炼调用继承了会话的扩展思考档（`effort=xhigh`），实测把单块拖到 18–73 秒；其二，超时被当作「块太大」去做**对半递归拆分**，但超时的成因是网关慢而非输入过大，切小只是拿同一时限再赌一次，还把一次等待放大成 7 次（单个 `ensure_loaded` 内触发 14 次调用，突变测试测得旧逻辑实际 28 次）；其三，15 秒的默认时限低于实测单块耗时。现提炼改用独立低思考档（`effort=low`、`budget_tokens=1024`，不再继承会话配置），实测单块降至 10–15 秒、输出 token 由 6k 级压到 2.5k 级且方差显著收敛。超时改为**时限阶梯**——首试时限后放宽 2 倍再试一次（默认 15s → 30s），并新增失败原因区分，**超时不再触发对半递归**，该能力仅保留给「回复不可解析 / 为空」这类切小确实有效的失败。提炼时限默认值同步上调至 30 秒。若将来把提炼思考档改为可配置，**必须**将其纳入缓存签名，否则会命中旧档位算出的结果（代码内已留注记） |
| **Jev 安全规则磁盘持久化与全局指引候选回退** | v0.6.114 | Auto 模式下 Jev 安全规则提炼结果按完整来源签名引入 SHA-256 内容寻址磁盘持久化缓存，重启后跨会话命中即可跳过重复 LLM 规则提炼（#398、#399）。排队消息快捷键优化为 `Ctrl+Enter` 发送与 `Ctrl+X` 删除（#400、#401）。System Prompt 指引加载支持有序全局候选列表（`~/.cc-code/AGENTS.md` -> `~/.cc-code/CLAUDE.md` -> `~/.claude/CLAUDE.md` -> `~/.claude/AGENTS.md`），空文件自动穿透，并与 Jev 安全门规则提炼彻底对齐同一加载链路（#402、#403） |
| **交互中断保护与内存调优** | v0.6.113 | 修复提问与审批弹窗单次 Ctrl+C 直接退出 TUI 的缺陷，改为安全中断当前轮次并解开等待；批量工具审批增加取消优先响应；配置向导统一复用双击退出防抖（#396、#397）。高亮引擎改用 Oniguruma 降低 PHP 语法常驻内存，接入 Windows 工作线程空闲 mimalloc 堆回收，修正 `/gc` 内存统计口径（#394、#395） |
| **Todo 列表折叠保护** | v0.6.112 | 消息区底部 Spinner 关联的 Todo 任务列表引入 `MAX_VISIBLE_TODOS = 5` 上限控制。当存在较多任务（如 8~10 个）时，仅前 5 项展开渲染，超出部分折叠为一行紧凑统计（如 `... +3 pending`），并自适应区分 pending / completed 状态；重构 `spinner_extra_count` 与 `todo_render_line_count`，行数计算精准收敛为最多 6 行（5 任务 + 1 统计行），保障视口裁剪与滚动条位置绝对精准一致（#392、#393） |
| **状态栏第二行「最近工具」实时摘要** | v0.6.111 | 第二行的运行中工具段原来只在执行期间存在，而且经常根本看不到：`poll_agent` 每帧把 ACP 通知一次 drain 干净，Read/Glob 这类毫秒级工具的 ToolStart + ToolEnd 落在同一帧，`◐ Read : x.rs` 一帧都没渲染过；摘要来源还只是 11 个工具的白名单（Agent / TodoWrite / AskUserQuestion / MCP 只有裸名字），顺序也是「老在左」。现改为由 `AgentComm.recent_tools` 驱动的一行流水：最新在最左、最多留 2 条（第 3 条挤掉最老），条目显示时长取 `max(实际执行时长, 300ms)`——快工具垫到 300ms 后消失，慢工具执行结束即刻消失，工具结束只让右侧聚合计数 `✓ Name ×N` +1、不再生成带摘要的完成条目。摘要覆盖全部工具（AskUserQuestion → 首个问题，Agent → description，TodoWrite → 任务数，其余 → 第一个非空字符串字段），字符上限 20 → 30，截断仍是路径语义，`◐`/`✓`/工具名/摘要配色统一。第二行强制单行不折行，超宽按显示列宽（unicode-width）在行尾 `…` 收口，不再被 `Paragraph` 静默裁掉 |
| **跨工具 Schema 聚合熔断与类型化连续失败追踪** | v0.6.110 | Schema 熔断器此前按工具名独立计数，模型在不同工具间轮换猜错参数即可逃逸单工具熔断阈值；连续失败检测按完整错误文本做 key，参数名差异导致计数被稀释。现引入跨工具聚合连续失败追踪（阈值 3 次）并支持指数退避，精准阻断跨工具乱猜参数死循环；连续失败检测改为按 `(tool_name, ToolErrorKind)` 聚合，错误文本动态变化亦能可靠累计；警告提示严格在所有工具结果写入后统一追加，杜绝孤立 `tool_result` 风险（#379） |
| **动词流光动效与状态词阶梯升温** | v0.6.109 | 参考 Codex CLI 物理余弦衰减模型，动词（`Executing…` / `Thinking…`）引入独立舒缓流光（5.0s 周期，同色系提亮 45% 绝不刺眼），长 Bash 工具执行期间以 8.0s 极低频独立流光充当生命体征心跳；思考状态词与动词动静解耦，状态词完全不闪烁，纯按时间阶梯从浅灰逐步加温至琥珀金（0~2.5s 浅灰 ➔ 2.5~5s 柔白 ➔ 5~15s 动词暖橙 ➔ 15~60s still thinking 浅金 ➔ >=60s deep in thought 琥珀金）。同版本另修：ASCII 降级表补 `⎿`（U+23BF）消除异常列宽终端前缀问号 |

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
