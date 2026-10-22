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

**输入交互对齐 Claude Code（v0.6.166）：** Agent 运行中斜杠命令 / Skills / @ 文件候选浮层可交互——方向键/Tab 导航，Enter 仅补全已选中候选，Esc 先关浮层、再按才中断 Agent；「立即补充」遇前台命令时给 2 秒宽限期，超时自动转后台；状态栏权限模式标签新增 `⏵⏵` 前缀。详见 [CHANGELOG](./CHANGELOG.md)。

**子 Agent 取消（v0.6.139）：** 用自然语言让主 Agent 停掉某个子任务即可——模型走 `Agent(action="stop", task_id="bg-…")` 按 `task_id` 精确取消（**不做模糊匹配**，避免停错描述相似的兄弟任务），停止结果回给模型含任务摘要的确认，时间线打印 `• Agent "x" was stopped by main Agent`，抽屉行转为**红色实心 `●`** + `stopped by main Agent`（用户按键停止则显示 `stopped by user`）。等待期间主视口显示 `✻ Waiting for N background agents to finish`。

**子 Agent Prompt 卡片（v0.6.139）：** 主视口按 `Ctrl+O` 进入详细模式后，后台子任务的启动摘要由极简树形切换为经典工具卡片——`● Agent(任务预览)` + `  └ Backgrounded agent` + `  └ Prompt:` + 指令**全文**（6 列悬挂缩进折行），再次 `Ctrl+O` 收回，无需进入子视口即可核验下发给各子 Agent 的指令。未发布改动见 [CHANGELOG](./CHANGELOG.md)。

| 功能 | 版本 | 说明 |
|------|------|------|
| **统一 Bash 及工具截断提示风格** | v0.6.175 | 统一 Bash、用户 `!` 本机命令与错误摘要行的折叠提示为 `… +N lines (ctrl+o to expand)`（单数：`… +1 line`，单行超额：`… (ctrl+o to expand)`），与 diff / 新建文件代码预览风格完全对齐（#661）。同版彻底将命令/文件候选浮层从视口高度中移除，消除 24 行终端下误切全屏与留白（#659）。 |
| **Write 文件内容预览恢复** | v0.6.174 | 修复界面通知晚于写盘时新建、覆盖预览消失，以及追加预览重复内容的问题。主、子 Agent 共用执行前快照，预览不再依赖界面读盘时序（#654）。 |
| **URL 背景与状态栏对齐** | v0.6.173 | 工具结果与 `!` 输出的 URL 显式使用终端默认背景，使 ANSI Cyan 的观感与状态栏模型名一致（PR #656）。 |
| **欢迎卡显示模型档位与版本更新提示** | v0.6.172 | 空会话首屏此前只有版本号、当前目录和开始提示，看不出当前跑的是哪个模型、哪一档推理强度。现补齐三处：① 标题拆成品牌名 ACCENT + BOLD、版本号 MUTED 两段，版本号按剩余宽度收口而品牌名永不截断；② 新增 `model · (effort effort)` 副标题，模型名取 `services.model_name`（`/model` 切换后同步更新）、档位取配置 `thinking.effort`，任一项缺失或未配 Provider 时整行省略——不编造默认档位；③ 探测到新版本时用 WARNING `Update available! <当前> → <最新>` + MUTED `To update, run: cc-code update` 两行**替换**原来的开始提示。新增 `cc-tui/src/app/update_check.rs`：启动后在后台查一次 GitHub release 列表（只认 `npm-v*` tag、跳过 prerelease、按语义版本取最大值而非取列表第一条），整个请求压在 3s 超时内，结果经 oneshot 回传、事件循环 `poll_update_check()` 收取，渲染路径只读缓存、永不等待网络；任何失败（无网、限流、JSON 异常）都只记 `debug` 日志并保持正常欢迎页。`CC_CODE_DISABLE_UPDATE_CHECK=1` 可关闭，`cargo test` 下自动禁用。同版：空会话的状态栏不再显示毫无意义的 `Cache - · MEM 19MB`，欢迎卡边框在中英文下严格闭合且高度恒为 7 行（#653）。 |
| **紧凑启动与随内容增长的终端视口** | v0.6.171 | 终端从 shell 输出下方紧凑启动，随输入、回复与面板按实际视觉行数增长，长满宿主高度后才锁定全屏。欢迎卡作为纯 UI 前缀参与滚动，自然滚出后不会把布局缩回去。复制、链接与高亮坐标跟随非零视口起点，退出时只清理应用自己拥有的行（#652）。 |
| **工具结果中的 URL 链接色定稿 ANSI Cyan** | v0.6.170 | 工具结果与 `!` shell 命令输出里的 URL 此前用 `theme::cur().ide`（深色 `#4782C8`），链接色随 `/theme` 漂移，且与正文 `text_soft` 明度接近，扫读时难以快速定位。现 `style_urls()` 固定 `Color::Cyan` + 下划线，不再跟随主题，与状态栏的模型名 / 分支名同一口径。覆盖两个调用点：`format_prefixed_text()`（Bash / Read / Grep / Glob 等所有走 `⎿` 前缀的工具结果行）与 `format_shell_output_lines()`（`!命令` 输出）。识别口径沿用 CC `OutputLine.tsx::URL_IN_JSON` 并切掉句末 `)` `]` `}` `,` `;` `.` `:`；仍为视觉区分（非 OSC 8——ratatui cell 塞不了转义序列，`sanitize_display_text()` 会剥 `\x1b`），点击由终端自身 URL 识别提供。Markdown 链接（`[text](url)` → `theme.link()` = SAGE `#4EBA65`）是另一条路径，不受影响（#649）。 |
| **长会话详情按视口渲染** | v0.6.169 | `/config` →「详情渲染」可选 `auto`（默认）、`cache`、`virtual`，即时生效并保存为 `config.detail_render_strategy`。虚拟路径仅物化当前窗口，不同变体共用 16 MiB 成品预算；原始历史、元数据和视口内巨型消息仍有独立内存开销。完整行为及边界见 [CHANGELOG](./CHANGELOG.md)（#629，PR #648）。 |
| **Bash 输出 JSON 逐行美化（OutputLine 装饰器）** | v0.6.168 | Bash / MCP 工具结果统一经过对齐 CC 的 `OutputLine` 装饰器：单行 JSON 展开为 2 空格缩进的美化输出，并带精度守卫（紧凑重序列化与原文不一致即保留原文，大整数、`1.10` 这类字面量永不被改写），非 JSON 行走快速路径，整段超过 10,000 字符跳过美化；折行与截断共用同一把尺——按显示宽度预折行、折叠态只显示 3 个视觉行 + `… +N lines (ctrl+o to expand)` 提示。同批：diff 删除行改三档底色、工具路径按 CC 口径显示、状态栏 Edit 命名修正（#642）。 |
| **工具结果与 `!` 输出的 URL 链接色** | v0.6.167 | 工具结果与 `!` shell 命令输出中的 URL 由手写前缀扫描识别（不引正则依赖，句末 `)` / `]` / `}` 等标点自动切除），以主题链接色 + 下划线呈现；`!` 输出先按既有 ANSI 着色切段再逐段着色，命令自带颜色不丢。此为视觉区分（非 OSC 8），点击仍由终端自身的 URL 识别提供（#640）。 |
| **Ctrl+O 切换瞬时完成（渲染键缓存）** | v0.6.166 | 此前切换详细模式会清空全部消息的渲染哈希、对整段历史重渲染 + 重折行——270 条消息约 6.9 秒、10,000 条消息约 239 秒（debug 实测）。现渲染线程按「内容哈希 ⊕ 该消息类型真正敏感的 detail/diff 维度」生成渲染键，每条消息同时保留展开 / 折叠双变体，折行结果按消息分段缓存（段键带逐行 epoch，`Running… (Xs)` 这类指示器文本变化只失效自己那一段）：热切换分别降到约 33ms 与 1.38 秒，并有回归测试锁定执行期（流式尾巴 + 运行中工具）反复切换与从零渲染逐行一致（#626，PR #631）。同批：diff 与 markdown 渲染对齐 CC（逐行 syntect 高亮 + Monokai Extended 主题、markdown 块间空行只来自源文本）、Glob 遍历禁用符号链接跟随并加条目/时长上限（#636）、候选浮层在 Agent 运行中可完整导航、steer 遇前台命令有 2 秒宽限期后自动转后台（#638）。 |
> 更早版本（v0.6.0 – v0.6.163）见 [CHANGELOG](./CHANGELOG.md)。

---

## 安装

支持 macOS (x86_64 / Apple Silicon)、Linux (x86_64 / aarch64)、Windows (x86_64)。

### npm（推荐）

```bash
npm install -g @cc-claw/code
```

> [!WARNING]
> **在 npm 12+（2026 年 7 月发布）上，这一条命令不够。** npm 现在默认阻止依赖包的 `postinstall` 脚本，而二进制正是靠这个脚本下载的：安装会显示成功，但运行 `cc-code` 时报 `spawnSync ...cc-code.exe ENOENT`。请显式放行：
>
> ```bash
> npm install -g --allow-scripts=@cc-claw/code @cc-claw/code
> ```
>
> 想对所有全局安装长期放行：`npm config set allow-scripts=@cc-claw/code --location=user`
>
> 用 **pnpm / bun**？它们同样阻止 build script —— 需先 `pnpm approve-builds`（或把包加入允许列表），否则 `cc-code` 装不到二进制。

### 升级

```bash
npm update -g @cc-claw/code
```

npm 12+ 需带同样的参数：`npm update -g --allow-scripts=@cc-claw/code @cc-claw/code`。

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
