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
| **会话恢复 Recap 持久化** | v0.6.80 | Recap 与任务完成总结行落库到 `ThreadMeta`（`latest_recap`/`last_task_summary`），`-c`/`-r` 恢复后不再丢失 |
| **/recap 命令与自动回顾** | v0.6.76 | `/recap`（别名 `/away` `/catchup`）用 aux_model 输出「目标→任务→下一步」；终端失焦+≥3 完成轮+60s 静默自动触发回顾（`/config` 开关，`PERI_AUTO_RECAP_*` 环境变量）；非流式 Anthropic 响应自适应兼容反向代理 OpenAI 格式 |
| **工具参数校验与输入泵** | v0.6.75 | Schema 校验错误对齐 Claude Code `formatZodValidationError` 可读化 + 工具错选启发式诊断（`suggest_tool_mismatch`）+ 连续失败熔断；引入独立 InputPump 安全启用鼠标悬停并修复滚动条拖拽 |
| **RTK 输出过滤** | v0.6.74 | 过滤 RTK git status 噪音（`clean — nothing to commit`），移除会吞并代码上下文的毒性通用折叠 |
| **Windows 控制台隔离与滚轮防抖** | v0.6.73 | `CREATE_NO_WINDOW` 隔离子进程控制台，消除 PHP 等触发的全屏闪屏；长内容下滚轮防抖批处理 + 滚动条滑块平滑拖拽 |
| **可点击 Markdown 超链接** | v0.6.72 | 消息区 Markdown 超链接跨平台点击打开默认浏览器 |
| **UI 细节打磨** | v0.6.71 | 禁用 sticky header 顶部固定消息条；附件栏标题与 Del 提示接入 i18n |
| **模型切换修复与命令面板统一** | v0.6.70 | 修复会话首条消息前切换模型不生效；废弃 `Ctrl+T`/`Alt+M` 循环快捷键，模型切换统一走命令面板（`Ctrl+P`/`Alt+P`） |
| **工具调用可靠性** | v0.6.69 | 边界提示词 + 字段级参数校验 + 动作循环检测；Windows 多行命令走 Git Bash 避免 cmd 截断；命令输出通用重复行/块折叠 |
| **图片魔数校验** | v0.6.68 | Read 工具校验图片魔数（Magic Bytes）防伪图片；长段落续行悬挂缩进；Windows 8.3 短文件名波浪号不再误拦截 |
| **消息队列与 Steering** | v0.6.66 | 执行期间输入的消息按轮排队增量注入（steering 不打断）；粘贴本机图片路径自动转附件 |
| **RTK stderr 清洗** | v0.6.64 | 清洗 RTK 外部宿主 stderr 噪音 |
| **本地命令块样式对齐** | v0.6.63 | 对齐本地命令块渲染样式 |
| **Prompt Cache 命中率** | v0.6.62 | 状态栏显示 Prompt Cache 命中率（替代瞬时 CPU）；移除消息区冗余低命中率警告气泡 |
| **动态终端标题** | v0.6.60 | 动态终端标题（Status Surface）对齐 OpenAI Codex 规范 |
| **Alt+V 粘贴图片** | v0.6.59 | 支持 `Alt+V` 快捷键粘贴图片附件 |
| **Windows 控制台残影修复** | v0.6.58 | 修复 Windows 传统控制台歧义字符双列残影 + Markdown 表格视口溢出 |
| **多模态工具包装器修复** | v0.6.57 | 修复工具包装器丢失多模态图片内容导致 Read 识图失败 |
| **多模态图片读取** | v0.6.56 | Read 工具支持多模态图片读取 |
| **后台 Shell 修复** | v0.6.55 | 修复常驻子进程导致后台 shell 任务无限 running |
| **生命周期无闪屏** | v0.6.54 | 消除 agent Bash 运行提示与生命周期切换时的终端清屏闪烁 |
| **工具行超长单行优雅截断** | v0.6.53 | 工具行 Header 超长根据终端列宽动态截断并以 `…` 闭合，杜绝折行挤占视口 |
| **Spinner 总结行动词与时刻** | v0.6.53 | 对齐 Claude Code 风格 `✻ {verb} for {elapsed} · done {HH:MM}`，规范生命周期 |
| **状态栏运行耗时秒数补零** | v0.6.52 | 耗时统一补零（如 `31m06s`），消除字符抖动；统一标准单字符 Emoji 避免错位 |
| **Anthropic 适配器 SSE 容错** | v0.6.51 | 非流式请求遭遇反向代理网关强制返回 SSE 时自适应解析还原，无损向下兼容 |
| **终端宽度变化防重复渲染** | v0.6.50 | 窗口 resize 重绘时重置旧行缓存，消除增量追加触发的 Markdown 内容翻倍渲染 |
| **文件系统工具精简收敛** | v0.6.49 | 核心工具精简收敛为 5 个（Read/Write/Edit/Glob/Grep），大幅减轻上下文负担 |
| **自适应终端宽度 Markdown 表格** | v0.6.48 | 表格列宽根据终端宽度动态分配，彻底解决宽屏下挤压截断与末尾多余空白列问题 |
| **Provider 类型大小写不敏感** | v0.6.47 | 配置中 `type` 字段大小写均可精准识别，避免误 fallback 到 OpenAI |
| **Bash 运行状态与计时优化** | v0.6.43 | 修正 Ctrl+B 后台计时冻结，缩小长输出预览窗口防止上下文被撑爆 |
| **跨 Provider 模型热切换** | v0.6.40 | 模型选择携带 Provider 上下文，无缝在 OpenAI / Anthropic / DeepSeek / GLM 间切换 |
| **Shift+Tab 权限模式轮切** | v0.6.40 | 状态栏提示与快捷轮切（bypass / default / dont-ask / accept-edit / auto-mode） |
| **Ctrl+B 后台 Shell** | v0.6.29 | Shell 命令支持 Ctrl+B 转为后台运行，并持续显示耗时与状态 |
| **/commit 命令** | v0.6.29 | 一键 git commit，自动生成 commit message |
| **/review 命令** | v0.6.29 | PR 代码审查 |
| **/export 命令** | v0.6.29 | 对话导出为 Markdown |
| **全局屏幕选区** | v0.6.29 | 基于渲染 Buffer 的全局选区，松开鼠标自动复制 |
| **Windows Git Bash** | v0.6.21 | cmd 失败自动 fallback 到 Git Bash |
| **i18n 支持** | v0.6.17 | 中英文切换 `/lang en` 或 `/lang zh-CN` |
| **Grep 对齐上游** | v0.6.15 | 对齐 Claude Code 的 files_with_matches 模式 |
| **Rewind 回滚** | v0.6.0 | 双击 ESC 弹窗选择回滚点 |
| **/gc 命令** | v0.6.0 | 手动内存回收 + RSS/jemalloc 诊断 |

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
├── peri-agent/                # 核心：Agent loop、工具系统、持久化、遥测
│   └── README.md              # Agent 框架使用指南
├── peri-middlewares/           # 中间件：文件系统、终端、MCP、Hooks 等
│   ├── README.md              # 中间件概览
│   └── CLAUDE.md              # 开发指南和陷阱记录
├── peri-tui/                  # TUI 应用 (Ratatui)
│   ├── README.md              # TUI 使用指南
│   └── CLAUDE.md              # 开发指南和陷阱记录
├── peri-acp/                  # ACP 服务层：桥接 TUI/IDE 与 Agent
│   └── README.md              # ACP 架构和数据流
├── peri-widgets/              # Widget 组件库
│   └── README.md              # 组件列表和使用示例
├── peri-lsp/                  # LSP 客户端库
│   └── README.md              # LSP 操作和配置
├── langfuse-client/           # Langfuse 遥测客户端
│   └── README.md              # 遥测配置和使用
├── npm/                       # npm 包：postinstall 脚本 + shell wrapper
│   └── README.md              # npm 包说明
├── scripts/
│   ├── install.sh             # macOS / Linux 安装器
│   └── install.ps1            # Windows 安装器
├── side-projects/             # 实验性项目（gig、llm-gateway 等）
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
