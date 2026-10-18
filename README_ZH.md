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

**子 Agent 取消（v0.6.139）：** 用自然语言让主 Agent 停掉某个子任务即可——模型走 `Agent(action="stop", task_id="bg-…")` 按 `task_id` 精确取消（**不做模糊匹配**，避免停错描述相似的兄弟任务），停止结果回给模型含任务摘要的确认，时间线打印 `• Agent "x" was stopped by main Agent`，抽屉行转为**红色实心 `●`** + `stopped by main Agent`（用户按键停止则显示 `stopped by user`）。等待期间主视口显示 `✻ Waiting for N background agents to finish`。

**子 Agent Prompt 卡片（v0.6.139）：** 主视口按 `Ctrl+O` 进入详细模式后，后台子任务的启动摘要由极简树形切换为经典工具卡片——`● Agent(任务预览)` + `  └ Backgrounded agent` + `  └ Prompt:` + 指令**全文**（6 列悬挂缩进折行），再次 `Ctrl+O` 收回，无需进入子视口即可核验下发给各子 Agent 的指令。未发布改动见 [CHANGELOG](./CHANGELOG.md)。

| 功能 | 版本 | 说明 |
|------|------|------|
| **`/gc` 差值修复 + 键盘增强协议接通 + macOS 启动清屏** | v0.6.154 | **`/gc` 差值不再被钳成 0**：`OS RSS - jemalloc resident` 与「已知估算 - allocated」此前用饱和减法，而被减的一方并不保证更大——实测 resident 93.6 MB 大于内核 RSS 84.9 MB 时，真差 `-8.7 MB` 被打印成 `= 0 B`，等式算术不成立；「差值为负」与「两边相等」在诊断上含义相反。现改为有符号差，负值时补提示（resident 仍计入内核已不再统计的 madvise 页，两口径不可比／估算项互相重叠且按 `capacity` 估算含冗余）。同批：删掉与 `RSS:` 重复的 `OS RSS:` 行（同一个 sysinfo 读数，只是整数 MB 截断后再相减，白丢 0.8 MB）；jemalloc stats 读取失败不再用 RSS 冒充 `allocated`（否则 `allocated-RSS` 恒为 `0 B`，测量失败伪装成完全一致）；macOS 上 `retained`（`opt.retain` 默认 false，恒为 0）改显示「不适用」（#593）。**`Ctrl+Enter` 真正生效**：`keyboard_modes.rs` 从未被 `lib.rs` 声明、`enable_keyboard_enhancement()` 全仓零调用点，应用从未推送 kitty 键盘协议，现接通后 iTerm2 ≥3.5 / WezTerm / Ghostty / kitty / Alacritty 上可用（#592），排队提示同时按平台显示真能按的键——Windows `Ctrl+Enter`、macOS `⌥S`、Linux `Alt+S`（#591）。**macOS 启动清屏一次**：Terminal.app 的 alternate screen buffer 跨程序保留，ratatui 首帧是 diff 模式，被杀会话的最后一帧会透出来（#582）。另有：TUI 用户可见文案接线到 i18n（新增 70 个 key、删除 72 个死 key，FTL 已无未引用 key，#578）、状态栏三行行首缩进统一 2 空格（#584）、等待后台子 Agent 期间终端标题保持动画（#575） |
| **后台完成通知运行中插入 + 系统提示立即重绘** | v0.6.151 | ① 后台 shell 完成通知改为运行中插入：agent 运行中后台化（`Ctrl+B` / 前台超时自动转后台）的命令跑完后，通知此前一律积压到轮次结束（`Done`）才注入——延迟上限等于任务剩余时长（实测十几分钟），而 `Done` 出口本身就不可靠：轮末优先 flush 用户排队消息会整段跳过通知的 drain，中断/报错两条分支根本不消费该队列。现复用既有 steering 通道，在通知产生时即插入当前轮次（运行中异步发起并每帧回收回执、空闲仍合并为一条、跨会话仍留给对应会话），并把消费回执的确认时机钉在「真正写入 state 之前」（`cc-agent` 侧消费点从轮首移到 `before_model` 之后、compact 之后），因此 `-32001`（未入队）/`-32002`（已入队未消费）都代表未送达、可安全回队重试；连接中断等无法判定的错误只保留原文并记 warn、不自动重投（宁可不送也不重复）；中断/报错后不自动唤醒新一轮，滞留通知随用户下一次主动提交一并送出（#571）；② 系统提示写入后立即重绘：空闲态输入 `/dream` 这类未知命令回车后界面毫无反应——提示其实已写进消息状态，但 `App::push_system_note` 只写 `view_messages` 不发渲染事件，要等下次按键才显示；现把重绘内置于该方法，同类路径（`/rewind` 无历史、排队消息 steer 失败、切换会话失败等提示）一并修复（#573） |
| **输入历史面板（`/history`）+ 系统提示最短可见 5 秒 + 工具资源释放** | v0.6.150 | ① **`/history` 改为「你输入过的内容」列表**：CLI 语境里 `history` 指用户输入历史（shell 的 `history` 就是列历史命令），此前 `/history` 打开的却是历史会话列表，想查自己输入过什么只能 `Ctrl+↑/↓` 一条条回溯——数据早在 `ui.input_history` 里（跨会话持久化）却没接出去。新增输入历史面板：`↑`/`↓`/`j`/`k`/`PageUp`/`PageDown` 导航、`Enter` 只把选中项回填输入框（不代发，与回溯语义一致）、`Esc` 关闭、空态有提示，多行条目折叠为单行并按显示宽度截断、光标行带强调色且滚动跟随；**`/history` 语义变更（破坏性）：原「历史会话浏览」迁到 `/resume`**（该命令原本是 `/history` 的别名）（#566）；② **状态栏系统提示至少停留 5 秒**：Jev 语义门降级播报（429/超时后走兜底分类器）此前被紧随其后的 `ToolStart` 等四条清除路径立刻抹掉、一闪而过读不到原因，现按「最短可见时长」范式给 `SystemNotice` 记录 `visible_until`，清除点改为「到期才清」、由事件循环空转时到期清除并重绘（切 thread 仍无条件清除）（#564）；③ **修复多轮对话后工具资源持续累积**：工具包装器未析构内部 `Box`，延迟工具代理与子 Agent 模型工厂又反向持有注册表/会话池形成引用环，现补唯一析构并改用弱引用，最后一个持有者释放后资源正常回收（#568） |
| **会话中心详情面板重排 + 状态栏会话计数 +「内联 Diff」开关收敛** | v0.6.149 | ① 状态栏 `← {n} agents` 不再统计当前目录累计历史会话（长期使用的目录实测 194 条），改为与会话中心 `Ready` 过滤同口径——只算 24h 内有更新、且未进入 Needs you / Working 的根会话（实测 194 → 10）（#554）；② 会话中心详情面板重排：`Model:` / `Reasoning:` 标签与值同行，`Project` / `Branch` / `Last message` / `Prompt` 标签独占一行、块间留空行，值沿用状态栏配色（模型青 / 项目黄 / 分支青），`Last message` / `Prompt` 预览改走 markdown 渲染并按列宽折行（长链接不再被 `…` 截断），高度上除 `Prompt` 预留 3 行外全部给 `Last message`，删除确认弹层底色改用弹层专用 `popup_bg`（原先误用品牌青绿 `background`）（#554）；③「内联 Diff」开关（`config.diff_enabled`）不再等同于 `Ctrl+O` 详细模式——此前开启会连带放大 shell 输出行数、强制展开工具、取消参数折行、展开 reasoning，现普通页面只多出 Write/Edit 的 diff（#558） |
| **message_render 修饰器拆分与 Bash 退出码置顶** | v0.6.148 | 重构 `message_render`（90KB+）为目录结构，解耦通用排版与独立工具修饰器（Bash / Glob / Read / AskUser）；Bash 失败时将退出码提升为首行置顶 `  ⎿ Error: Exit code {code}`、剔除末尾标记并将输出正文整齐对齐缩进（#551） |
| **跨项目 Agent Command Center** | v0.6.147 | `/agents` 或空草稿 `←` 打开全页会话中心：项目/状态/模型分组、五档过滤、分页、详情、搜索、重命名和二次确认永久删除。Enter 进入目标项目，`n` 在该项目新建空会话。Agent 定义入口改为 `/agent-design`，`/tasks` 由中心替代（#542） |
| **输入框 `/command` 高亮 + Rewind 改走 `/rewind` 命令 + 运行中命令分流** | v0.6.145 | 三项 TUI 输入与渲染修复：① 输入框内 `/command` token 改用 suggestion 蓝高亮，规则与 Claude Code `findSlashCommandPositions` 一致（`!` shell 态跳过，避免把 `/etc` 之类路径着色）（#529）；② Rewind 对话回滚从「双击 `Esc`」改为正式命令 `/rewind`（别名 `/undo`）——`Esc` 是取消/中断键，连按两次会误弹出回滚面板，而输入 `/rewind` 反而无效（#531）；③ Agent 运行中按 `Enter` 不再把命令静默变成模型提问——此前 `/exit`、`/gc`、`/theme` 与 `!` shell 输入都会被排队后当普通文本喂给模型。判定收敛到唯一入口 `CommandSystem::resolve`：普通消息 / Skill / ACP 命令 / 转发型命令入队等本轮结束后执行，需要空闲 UI 的命令、`!` 输入与未知命令一律拒绝并保留输入框内容；`Ctrl+Enter` 也不再能把命令插入正在执行的轮次（#533） |
| **子 Agent 抽屉「正在做什么」列修正与节流** | v0.6.144 | 该列此前在工具名（`Bash`/`Grep`）与助手文本之间来回跳：服务端把工具名写进任务快照 `current_activity`，TUI 的 `progress` 分支又用它覆盖本地采集的助手文本。现服务端不再写入、TUI 也不再接受，该列只由助手文本与状态文案驱动；并新增 200ms 时间节流（渲染快照 `activity_shown`，首次填充与子 Agent 结束时立即同步）——丢的是渲染帧而非状态（#526） |
| **状态栏配色固定，不再跟随 `/theme`** | v0.6.143 | `/theme` 上线（#509）时把状态栏写死的颜色一并扫进了调色板，导致状态栏观感随主题漂移：模型名 / cwd / git 分支由 ANSI 具名色变为固定 RGB，上下文仪表跟随 `error`/`warning`/`success`。状态栏配色是打磨定稿的固定值，现退出主题化——新增局部常量模块 `sb` 恢复 #509 之前的取值，52 处调用点改回原固定色，渲染结果与主题系统引入之前逐字节一致（#523） |
| **主题面板键盘路由修复** | v0.6.142 | `/theme` 选择面板 scope 为 `Session`，但键盘与粘贴白名单未包含 `PanelKind::Theme`，按键被穿透到后续 Stage——`↑`/`↓` 不移动光标、`Esc` 关不掉面板。现补齐两处路由并加回归测试（#521） |
> 更早版本（v0.6.0 – v0.6.141）见 [CHANGELOG](./CHANGELOG.md)。

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
