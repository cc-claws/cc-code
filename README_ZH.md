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
| **长会话详情按视口渲染** | v0.6.169 | `/config` →「详情渲染」可选 `auto`（默认）、`cache`、`virtual`，即时生效并保存为 `config.detail_render_strategy`。虚拟路径仅物化当前窗口，不同变体共用 16 MiB 成品预算；原始历史、元数据和视口内巨型消息仍有独立内存开销。完整行为及边界见 [CHANGELOG](./CHANGELOG.md)（#629，PR #648）。 |
| **Bash 输出 JSON 逐行美化（OutputLine 装饰器）** | v0.6.168 | Bash / MCP 工具结果统一经过对齐 CC 的 `OutputLine` 装饰器：单行 JSON 展开为 2 空格缩进的美化输出，并带精度守卫（紧凑重序列化与原文不一致即保留原文，大整数、`1.10` 这类字面量永不被改写），非 JSON 行走快速路径，整段超过 10,000 字符跳过美化；折行与截断共用同一把尺——按显示宽度预折行、折叠态只显示 3 个视觉行 + `… +N lines (ctrl+o to expand)` 提示。同批：diff 删除行改三档底色、工具路径按 CC 口径显示、状态栏 Edit 命名修正（#642）。 |
| **工具结果与 `!` 输出的 URL 链接色** | v0.6.167 | 工具结果与 `!` shell 命令输出中的 URL 由手写前缀扫描识别（不引正则依赖，句末 `)` / `]` / `}` 等标点自动切除），以主题链接色 + 下划线呈现；`!` 输出先按既有 ANSI 着色切段再逐段着色，命令自带颜色不丢。此为视觉区分（非 OSC 8），点击仍由终端自身的 URL 识别提供（#640）。 |
| **Ctrl+O 切换瞬时完成（渲染键缓存）** | v0.6.166 | 此前切换详细模式会清空全部消息的渲染哈希、对整段历史重渲染 + 重折行——270 条消息约 6.9 秒、10,000 条消息约 239 秒（debug 实测）。现渲染线程按「内容哈希 ⊕ 该消息类型真正敏感的 detail/diff 维度」生成渲染键，每条消息同时保留展开 / 折叠双变体，折行结果按消息分段缓存（段键带逐行 epoch，`Running… (Xs)` 这类指示器文本变化只失效自己那一段）：热切换分别降到约 33ms 与 1.38 秒，并有回归测试锁定执行期（流式尾巴 + 运行中工具）反复切换与从零渲染逐行一致（#626，PR #631）。同批：diff 与 markdown 渲染对齐 CC（逐行 syntect 高亮 + Monokai Extended 主题、markdown 块间空行只来自源文本）、Glob 遍历禁用符号链接跟随并加条目/时长上限（#636）、候选浮层在 Agent 运行中可完整导航、steer 遇前台命令有 2 秒宽限期后自动转后台（#638）。 |
| **jemalloc 调优真正生效** | v0.6.163 | 分配器配置此前以 `MALLOC_CONF` 环境变量导出，而 tikv-jemalloc 根本不读这个名字（只认 `_RJEM_MALLOC_CONF` 或静态符号 `_rjem_malloc_conf`），所有调优参数一直在空转；现改为导出静态配置符号，并已在发布版二进制上实测验证生效（#622）。同批修复安装脚本解压内置 ripgrep tar.gz 失败、导致 `bin/rg` 永久缺失的问题（#620）。 |
| **单行长 JSON 视觉行截断 + WebSearch 详细模式展开** | v0.6.162 | 修复普通模式下单行巨型 JSON 因换行逻辑行仅有 1 行而绕过 3 行折叠上限、折行刷爆终端的缺陷，改由 `push_visual_capped_output` 按实际折行视觉行严格限制显示行数（≤3 行）；同时解除 WebSearch/WebFetch 参数在入队时的 60 字符硬截断，详细模式（Ctrl+O）下超长搜索词与 URL 现可完整折行展开（#618） |
| **后台结果只注入一次** | v0.6.160 | 后台子 Agent 在主 Agent 本轮仍在运行时完成，完整 output 会经两条通道各进一次上下文：`drain_notifications` 写入的 `Human` 通知，以及自动续接轮合成注入的 `AgentResult` `tool_result`。两份文本同源，续接轮因此复述刚给出的结论（实测 messages 109 → 113，回复「已在上一条消息中完整给出」）。`inject_bg_result_messages` 现按 `[Background task {short_id} ` 前缀（与通知同源的 8 位 id，`completed` / `failed` 共用）在 history 中判重，命中即跳过合成注入；全部命中时不再追加空的工具调用消息。`Done` 之后完成的任务不受影响，仍恰好注入一次（#613） |
| **Spinner 动词统一英文** | v0.6.159 | 取消加载指示器（loading spinner）对中文语言的强制机翻分流（`ZH_VERBS`），动词在所有界面语言下统一使用原版英文趣味动词（`EN_VERBS`，如 `Thinking…`、`Drifting…` 等），消除生硬突兀的直译词（#611） |
| **`/theme` 新增 `auto`，跟随终端颜色能力与背景明暗** | v0.6.157 | `/theme` 此前只能显式选一套配色——Apple Terminal 这类只有 16/256 色的终端一旦选中 `light` / `dark`（24-bit 真彩色调色板）就会退化，而切系统深浅色或换终端跑同一份配置时还得手改 `settings.json`。现 `theme = "auto"` 由新模块 `cc-tui/src/terminal_theme/` 两级判定：① **颜色能力**——`TERM_PROGRAM=Apple_Terminal` 记为 `Ansi`，其余记为 `TrueColor`；② **背景明暗**——先发 OSC 11 查询（`\x1b]11;?\x07`），按回复的 sRGB 相对亮度判明暗（阈值 0.5，接受 8/16 bit 分量与 BEL / ST 两种终止符），终端不应答时 macOS 退回系统外观（`defaults read -g AppleInterfaceStyle`，键不存在即浅色），其余平台按深色兜底。两维组合映射到既有的 `light-ansi` / `dark-ansi` / `light` / `dark` 四套主题，`default` 与 daltonized 偏好不参与自动选择。判定结果缓存进 `OnceLock`（`theme::cur()` 在渲染热路径上，子进程探测最多发生一次）；OSC 往返在 `enable_raw_mode()` 之后、`EventReader::start()` 之前由 `terminal_theme::preload()` 完成（回复从 stdin 原始字节读入，事件读取器一旦启动就会抢先消费），且仅在持久化主题为 `auto` 时预探测，显式主题的用户不付这笔启动开销，stdin / stdout 任一侧不是 tty 时直接跳过以免吃掉管道输入。`/theme` 面板该行显示 `Auto (follow terminal: <解析结果>)`，面板高度随 `ThemeName::ALL` 伸缩。**刻意不从 Terminal.app 的 `Default Window Settings` profile 推断背景色**：那只是下一个新窗口用哪个 profile，窗口可在 `Shell > Show Inspector` 里单独指定（本机默认 profile 是黑底的 Pro，实际窗口却是白底的 Basic），据此判定必错，该链路已删除（#604） |
| **状态栏跟随 `/theme` + markdown 缓存字节预算** | v0.6.155 | **状态栏按主题取色**：`/theme` 切到 Light 系列后消息区是浅底深字，状态栏却仍沿用深色主题的字面量（主文字 `Rgb(255,255,255)`、次要信息 `Rgb(153,153,153)`），白字压浅底几乎不可读；且第一行走 ANSI 名色、第三行写死 RGB，换终端配色方案时同一根状态栏会劈成两种风格。`status_bar.rs` 新增 `SbColors`，按当前主题解析一帧渲染所需的全部颜色：`default` 主题逐值保留原有字面量（观感零变化，守住 #522 的结论），其余 6 套主题取 `/theme` palette 的 `cyan / yellow / magenta / success / warning / error / inactive / text / selection_bg / cursor_bg`，上下文仪表因 ANSI `Green/Yellow/Red` 与 `SAGE` 的 `Rgb(78,186,101)` 并非同值而单列 `context_ok / context_warn / context_high`（#597）。**markdown 缓存补字节预算**：此前只有 `CACHE_CAPACITY = 1024` 的**条数**上限、没有**字节**上限，满容量常驻约 52.8 MB（实测 139258 行 / 952218 Span，最大单条 85 KB），而缓存键含 `max_width`，每次 resize 都产生一批新宽度变体直到灌满；现加 8 MB 字节预算、条数上限降为兜底并按 LRU 淘汰，`diff/byte_cache.rs` 上提为 `cc-widgets/src/byte_cache.rs` 供 diff 与 markdown 共用，同时修掉条数上限触发 `LruCache::put` 静默淘汰不扣字节、`bytes` 单调虚高的缺陷，`/gc` 补显示预算读数（#594） |
> 更早版本（v0.6.0 – v0.6.154）见 [CHANGELOG](./CHANGELOG.md)。

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
