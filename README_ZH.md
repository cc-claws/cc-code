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

[为什么选 cc-code](#为什么选-cc-code) · [Auto 模式](#auto-模式jev-语义门) · [核心能力](#核心能力) · [安装](#安装) · [Nobody Coding](#我们怎么用-nobody-coding-造-cc-code) · [致谢](#致谢)

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
| 安全判定 | 每条危险命令都要你确认 | JEV 语义门 —— 独立 LLM 判定，只在判不准时才弹窗 |

---

## Auto 模式：JEV 语义门

`Auto` 是默认权限模式（fail-closed）。它不逐条问你，而是分两层决策：

- **确定性层** —— 你自己写的规则、内置硬黑名单、只读快车道。零成本，不依赖任何凭据。
- **JEV 语义门** —— 独立的 LLM 判定服务（`JEV_MODEL`，默认 `openrouter/typesafe/jev-1.13`），走自己的端点（`JEV_ENDPOINT`）和密钥（`JEV_API_KEY`）。它按条件集给这次调用打分，**只有落进不确定区间时才弹一次确认窗**。

没配判定凭据时，确定性层照常生效，再由 LLM 分类器兜底；没有确认通道时（子 agent、`-p`）一律拒绝。判定全程不占用会话模型 —— 安全校验不花你的上下文 token。`Shift+Tab` 在 `Auto` 与 `Bypass` 间切换。

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
| **HITL 审批** | 敏感操作由 JEV 语义门拦截 —— 独立 LLM 判定，只在判不准时才弹窗 |
| **双引擎文件搜索** | Grep/Glob 优先使用外部 ripgrep 二进制，回退到内置 Rust 引擎 |

### v0.6.x 新增功能

**界面语言：** TUI 和 CLI 帮助支持英文与简体中文。使用 `/lang en` 或 `/lang zh-CN` 保存偏好；未保存语言时，启动检测 `LC_ALL`、`LANGUAGE`、`LC_MESSAGES` 和 `LANG`，不支持的 locale 回退英文。未发布改动见 [CHANGELOG](./CHANGELOG.md)。

**子 Agent 取消（未发布）：** 用自然语言让主 Agent 停掉某个子任务即可——模型走 `Agent(action="stop", task_id="bg-…")` 按 `task_id` 精确取消（**不做模糊匹配**，避免停错描述相似的兄弟任务），停止结果回给模型含任务摘要的确认，时间线打印 `• Agent "x" was stopped by main Agent`，抽屉行转为**红色实心 `●`** + `stopped by main Agent`（用户按键停止则显示 `stopped by user`）。等待期间主视口显示 `✻ Waiting for N background agents to finish`。

**子 Agent Prompt 卡片（未发布）：** 主视口按 `Ctrl+O` 进入详细模式后，后台子任务的启动摘要由极简树形切换为经典工具卡片——`● Agent(任务预览)` + `  └ Backgrounded agent` + `  └ Prompt:` + 指令**全文**（6 列悬挂缩进折行），再次 `Ctrl+O` 收回，无需进入子视口即可核验下发给各子 Agent 的指令。未发布改动见 [CHANGELOG](./CHANGELOG.md)。

| 功能 | 版本 | 说明 |
|------|------|------|
| **后台 shell 完成通知按批次合并注入，消除多轮级联唤醒** | v0.6.137 | `Bash(run_in_background=true)` 任务完成后会强制唤醒 agent 开启新 turn；当一轮内积压多条通知时，旧逻辑在每次 `Done` 后**只从 pending 队列弹 1 条**（`pending.remove(index)`），n 条通知 → n 轮 turn 级联雪崩（每轮 `Done` 再触发下一条），现场表现为连续无意义的「收到」。现新增统一注入入口 `App::inject_bg_shell_notifications`：**空闲态**把「本批通知 + 队列中同会话待注入通知」**一次性 drain 拼接为单条**消息提交（n 条压成 1 轮 turn），**推理中**全部入队、`Done` 后一次性 drain 合并；stall 警告与 `!command` / agent shell 两条完成路径统一走该入口，删除三处重复的「首个 submit + 其余入队」分支。合并**仅在消息层拼接**（保留每条 `<background-task-completed>` XML，信息不丢失），其它会话通知仍留在队列。配套把展示解析重构为按文档顺序逐块扫描，支持合并消息含多个块（甚至完成 + 等待输入**混合类型**）不丢行、不串序。新增 6 项回归用例（#491） |
| **子 Agent 视口补齐完成总结行** | v0.6.136 | 切入刚跑完的后台子 Agent 独立视口时，底部此前空白、缺少主视口那条 `✻ Brewed for 32s · done 16:30` 总结行。根因：`SpinnerState` 仅在「活跃态 → `Idle`」时捕获摘要耗时，而子 Agent 的完成路径（`peri/subagent_lifecycle` 的 `"stopped"` 分支与 `AgentEvent::Done` 兜底）都未调用 `set_mode(Idle)`，摘要耗时恒为 0，渲染层退化为不画任何行。现补齐结算；被停止/失败的子 Agent 则清空总结行、不显示成功文案（#485） |
| **后台子 Agent 抽屉与独立视口（只有后台模式）** | v0.6.130 | 子 Agent 现在是**只有后台模式**，底栏抽屉是它们的唯一呈现，内部过程不再污染主对话。空输入框按 `↓` 浏览、`↑/↓` 移动游标、`Enter` 进入独立视口（独立滚动位置、输入草稿与 HUD 工具统计）、`x` 停止选中任务、`Ctrl+X Ctrl+K` 停止全部。正在阅读的子 Agent 结束时**留在屏幕上**（该行转绿显示 `✓ completed`），等你离开视口才回收会话。启动摘要渲染为对齐的 `├── / └──` 树形块，抽屉只用亮白/灰两档文字（游标行不设底色），右侧 token 指标为跟随流向的单箭头（`↓` 回复产出、`↑` 你输入）。`fork` 与普通子 Agent 合并为同一条构建路径，子任务标题优先取工具的 `description`，`run_in_background` 参数已退场（#466） |
| **`@` 立即显示有限文件候选** | v0.6.129 | 输入 `@` 即弹面板，默认只列当前目录，目录优先，最多 15 项、10 行可见；面板按终端高度缩小。关键词搜索子目录，200ms 防抖保留最后一次输入，搜索中和空结果有提示；截断候选和进入目录时重新搜索，避免漏匹配（#463） |
| **修复 Windows 下 cc-code 运行中 `cc-code update` 被误报为 UTF-8 错误** | v0.6.128 | Windows 下 `cc-code` 正在运行时执行 `cc-code update` 会失败并报 `stream did not contain valid UTF-8`，实为两层问题叠加：其一，`install.ps1` 用 `Copy-Item -Force` 覆盖便捷副本 `~/.cc-code/cc-code.exe`，而该 exe 正在运行时被 Windows 文件锁占用 → `IOException`，配合脚本 `$ErrorActionPreference="Stop"` 直接中止，导致 `current-version.txt` 与便捷副本都未更新；其二，该失败抛出的是中文本地化错误（GBK 字节），而 `update.rs::stream_output` 用严格 UTF-8 读取子进程输出 → 解码失败，把真实错误盖住。现 `update.rs` 改为容错解码（`String::from_utf8_lossy`）并前置强制 `[Console]::OutputEncoding=UTF8`；`install.ps1` 便捷副本更新改为锁安全——先复制，失败则把被占用的旧 exe 重命名到 `*.old.<时间戳>` 再复制（对齐 `npm/install.js` 的 `replaceBinary`），仍失败只告警不中止，保证 `current-version.txt` 正常写入（#453） |
| **「回到最新」按钮按输入框白条居中** | v0.6.128 | 「↓ 回到最新」浮层按钮原先按「消息区宽度减去最右 1 列（滚动条预留）」居中，相对输入框上方整宽白条整体左偏约 1 列，用户反馈「看着不居中」。现改为以输入框矩形（白条整宽）为居中基准，左右留白对称（80 列下 x=28，左右各 28）；仅视觉调整，显隐、命中区与滚动状态不变（#458） |
| **「回到最新」浮层按钮** | v0.6.127 | 消息区滚动离开底部且内容溢出时，在消息区最后一行居中显示「↓ 回到最新」浮层按钮；点击按钮或按 `End`（输入框为空）回到底部后按钮自动消失，离开底部期间有新内容（新消息或流式输出）时文案切换为「↓ 有新活动」。按钮为覆盖式浮层，复用既有 `scroll_follow` / `scrollbar_max_offset` 滚动状态，不改变消息区布局与滚动位置；面板/弹窗激活时隐藏且不拦截指针（#448） |
| **npm 安装支持 Windows 进程运行中热替换二进制** | v0.6.124 | 在 Windows 环境下，当 `cc-code` 处于运行状态时（如在会话内自升级或开启多个终端），内核文件锁导致 npm postinstall 的 `unlinkSync` 抛出 `EBUSY`/`EPERM` 引起安装回滚。现于 `install.js` 引入 `replaceBinary` 文件锁退避机制：常规删除受阻时自动将旧文件重命名为 `cc-code.exe.old.<timestamp>` 腾出路径并热替换新版本，并在安装时顺便尝试清理历史已退出的 `.old.*` 备份文件（#430、#431） |
| **Bash 运行状态行显示模型请求的 timeout** | v0.6.123 | Bash 运行状态行（`Running… (Xs)    (timeout Nm)`）此前显示的是 `execution_timeout`（硬执行上限，默认 10m），而非模型传入的 `timeout`（前台等待期限，默认 2m）——传 `timeout=300000`（5 分钟）仍显示 `10m`，看起来像「参数没生效」。现将 `timeout_ms` 贯通注册/槽位，视图字段 `execution_timeout_ms` 重命名为 `timeout_ms`，状态行改为取模型传入的 `timeout`（按分钟展示 `2m`/`5m`）。执行与超时语义不变（#428） |
| **diff 缓存加字节预算 + `/gc` 补统计渲染/diff 缓存** | v0.6.121 | `/gc` 的「已知合计」此前仅 ~5.9 MB，而 `mimalloc allocated` ~50 MB，存在约 44 MB「未识别」。排查发现两处盲区：其一，`DIFF_CACHE`（`cc-widgets/src/diff/mod.rs`）与 `RENDER_CACHE`（`cc-widgets/src/diff/renderer.rs`）只按条数限流（64 条），但单条可携带接近 `MAX_DIFF_SIZE_BYTES`（1 MB）的文本，上限不可控（理论 ~64 MB／每缓存），且完全不在 `/gc` 统计内；其二，渲染线程 `RenderCache`（整段对话渲染行 `lines` + `wrap_map` 逐行纯文本副本与字符宽度）同样未被统计。现新增 `ByteBudgetCache`（条数 + 字节双重上限的 LRU），两个 diff 缓存各设 **8 MB 字节预算**，超预算按 LRU 淘汰（未命中重算，行为不变）；`/gc` 新增 `render_cache` / `diff_cache` / `diff_render_cache` 统计并计入「已知合计」，剩余「未识别」收窄到语法高亮引擎、tokio、tracing 与分配器元数据；顺带修复 `renderer.rs` 一处既有注释乱码（#420、#421） |

> 更早版本（v0.6.0 – v0.6.120）见 [CHANGELOG](./CHANGELOG.md)。

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
├── spec/                      # 设计文档与规范
│   ├── global/                # 全局架构文档
│   ├── issues/                # Issue 分析文档
│   └── archive/               # 已归档的 feature 规范
├── docs/                      # 长文文档（prd、adr、designs、acp）
├── human/                     # 人工走查清单
├── CLAUDE.md                  # 项目开发指南
├── CHANGELOG.md               # 版本变更记录
├── CONTRIBUTING.md            # 贡献指南
├── DEVLOG.md                  # 开发日志（里程碑）
├── README.md
├── README_ZH.md
├── TUI-STYLE.md               # TUI 命令 / 快捷键 / 交互参考
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
