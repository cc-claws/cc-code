<div align="center">

[English](README.md) | **中文**

# cc-code

**用开源模型跑 Agent Loop — Rust 写的终端编程助手，兼容 `.claude/` 生态全家桶**

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
| `.claude/` 生态 | 不兼容 | 直接用 `.claude/` 配置、agents、skills、hooks、MCP |
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
| **`.claude/` 生态兼容** | `.claude/` 配置、agents、skills、hooks、MCP、子 agent 直接复用 |
| **流式 Markdown** | 代码块、表格、diff 实时渲染 |
| **ACP 协议** | 接入 Zed 等 IDE，也支持自建 "Cloud Code" 平台 |
| **Auto Compact** | 长会话自动压缩，保持响应快且省 token |
| **Sub-Agent 并发** | 后台子 agent 并行执行，支持 fork 和 background 模式 |
| **HITL 审批** | 敏感操作由 JEV 语义门拦截 —— 独立 LLM 判定，只在判不准时才弹窗 |
| **双引擎文件搜索** | Grep/Glob 优先使用外部 ripgrep 二进制，回退到内置 Rust 引擎 |

### v0.6.x 新增功能

**图片附件链接（v0.6.192）：** 每张已发送图片在用户正文下显示一行 `  ⎿  [Image #N]`。有保存路径且 OSC 8 开启时，引用保持自身柔和配色、无人工下划线，可在支持的终端中打开本地图片；`DISABLE_HYPERLINKS=1` 或缺路径时显示不可点击的普通文本，写盘失败不阻断发送。图片写入系统临时目录，恢复会话不保证链接可用。Phase 1 不向模型发送本地路径。完整范围见 [CHANGELOG](./CHANGELOG.md)。

**Skill 工作流（v0.6.191）：** 匹配任务通过核心 `Skill` 工具加载，`/skill-name args` 直接展开指令。专属卡片折叠展示加载结果，`context: fork` 等待隔离子代理完成，激活指令在压缩后恢复。详见 [技能机制](./spec/global/domains/skill-trigger.md)。

**界面语言：** TUI 和 CLI 帮助支持英文与简体中文。使用 `/lang en` 或 `/lang zh-CN` 保存偏好；未保存语言时，启动检测 `LC_ALL`、`LANGUAGE`、`LC_MESSAGES` 和 `LANG`，不支持的 locale 回退英文。未发布改动见 [CHANGELOG](./CHANGELOG.md)。

**输入交互对齐参考实现（v0.6.166）：** Agent 运行中斜杠命令 / Skills / @ 文件候选浮层可交互——方向键/Tab 导航，Enter 仅补全已选中候选，Esc 先关浮层、再按才中断 Agent；「立即补充」遇前台命令时给 2 秒宽限期，超时自动转后台；状态栏权限模式标签新增 `⏵⏵` 前缀。详见 [CHANGELOG](./CHANGELOG.md)。

**子 Agent 取消（v0.6.139）：** 用自然语言让主 Agent 停掉某个子任务即可——模型走 `Agent(action="stop", task_id="bg-…")` 按 `task_id` 精确取消（**不做模糊匹配**，避免停错描述相似的兄弟任务），停止结果回给模型含任务摘要的确认，时间线打印 `• Agent "x" was stopped by main Agent`，抽屉行转为**红色实心 `●`** + `stopped by main Agent`（用户按键停止则显示 `stopped by user`）。等待期间主视口显示 `✻ Waiting for N background agents to finish`。

**子 Agent Prompt 卡片（v0.6.139）：** 主视口按 `Ctrl+O` 进入详细模式后，后台子任务的启动摘要由极简树形切换为经典工具卡片——`● Agent(任务预览)` + `  └ Backgrounded agent` + `  └ Prompt:` + 指令**全文**（6 列悬挂缩进折行），再次 `Ctrl+O` 收回，无需进入子视口即可核验下发给各子 Agent 的指令。未发布改动见 [CHANGELOG](./CHANGELOG.md)。

| 功能 | 版本 | 说明 |
|------|------|------|
| **工具头文件路径可点击** | v0.6.194 | `Write`/`Edit`/`Read` 的工具调用行路径现在包成 OSC 8 `file://` 链接，点击用系统默认程序打开文件，`Wrote N lines to <path>` 结果行同样可点。链接目标是绝对路径、显示文本保持相对 / `~` 记法，`DISABLE_HYPERLINKS=1` 退回纯文本，Windows 直接 exec `explorer.exe`（不经 `cmd` 再解析）。长会话超过 5000 行 / 300 条消息 / 16 MB 时自动切窗口化渲染，不再每次变化都重建整张行表。 |
| **流式渲染增量解析** | v0.6.193 | 流式气泡不再每帧重复解析 Markdown：气泡以 dirty + 空渲染构造，解析统一归渲染线程。最后一个换行之后超过 200 字符的未完成长行暂不进入显示快照，等换行到达再显示，长段落不再每帧重排；短回答仍逐字可见。block 档围栏边界改由有状态的 `BlockBoundaryScanner` 判定，不再用「最后一行是否以 ``` 开头」的启发式，正文裸围栏行不再提前分块（#739）。 |
| **图片附件链接** | v0.6.192 | 已发送图片仅在 `[Image #N]` 占位符仍在输入中时保存到系统临时目录（`cc-code-images/<session_uuid>/<image_id>.<ext>`），正文下每图一行 `  ⎿  [Image #N]`。有保存路径且 OSC 8 开启时引用可在支持的终端中打开本地图片；写盘失败不阻断发送，Phase 1 不向模型发送本地路径（#734）。 |
| **状态栏分隔符次要信息色** | v0.6.192 | 状态栏第一行的 ` | ` 分隔符改用次要信息色，不再走终端默认前景色，不再比同一行的模型名、cwd、token 数字抢眼。只改分隔符配色，文本内容、列宽与布局不变（#735）。 |
| **Skill 一级核心工具** | v0.6.191 | 技能成为一级核心工具：模型自己调用 `Skill`，不再靠 `Read` 打开 `SKILL.md` 触发；`/skill-name` 也不再向历史注入伪造的 `Ai[ToolUse: Read]` + `Tool[ToolResult]` 消息。核心工具白名单由 11 增至 12；`context: fork` 技能在隔离子代理中执行并等待结果，压缩恢复按工具名精确配对，只恢复最近激活的技能正文（#730）。 |
| **状态栏工具名 inactive** | v0.6.191 | 状态栏工具统计里的工具名改用调色板 `inactive`，不再走默认前景色，与其他次要信息同层级（#729）。 |
| **传统终端缺字形降级** | v0.6.191 | 传统终端（如经典 conhost）缺字形时，`❯` 与 `⏵` 定向降级为可渲染字符；同批补齐面板箭头、recap 前缀、todo、面板导航与滚动条的缺字降级（#727）。 |
| **最近工具最短可见时长** | v0.6.190 | 最近工具行最短可见时长由 300ms 经 800ms 调到 1200ms，不再一闪而过；最近工具图标统一为 `◐`，不再翻成 `✓`（#723）。 |
| **终端标题 960ms 帧间隔与多路复用器降级** | v0.6.190 | 标题帧间隔由 400ms 对齐到 960ms（此前接近频闪）；tmux/screen 下标题停在静态首帧，不再让外层 tab 跟着抖（#725）。 |
| **终端标题 ◐/◑ 两帧** | v0.6.189 | 终端标题不再与消息区共用六字呼吸字形：改用自有的 `◐`/`◑` 两帧交替，由单调时钟驱动，换回合、切模式、`reset()` 都不重置相位（#721）。 |
| **resize 后退出无残留边框** | v0.6.189 | 窗口尺寸变化后退出不再在屏幕顶部残留欢迎卡边框或分隔线：尺寸变化时打 reflow 标记，退出清理改为从屏幕顶部开始；尺寸未变时保持原清理范围（#718）。 |
| **`/gc` 按 size class 分解 allocated** | v0.6.188 | `/gc` 现在按分配尺寸分解 `allocated`，不再只留一坨「未识别」字节。它读 jemalloc 各 arena 的 bin / large class 计数器，打印 Top 12 个 size class（存活对象数、字节、占比）与覆盖率校验行。逐 class 求和精确复现 `stats.allocated`，所以这些是真实存活分配——包含估算看不到的 SyntaxSet/ThemeSet、编译正则、tokio/ACP 缓冲——不是估算值。mallctl 键在栈上拼装、查询自身的缓冲在 epoch 快照之前分配好，因为在 jemalloc 作全局分配器时被测量的分配器就是这个进程的分配器：用 `format!` 拼约 4000 个键会让覆盖率在 24.6 MB 上偏出 29,696 B。尺寸列用精确字节数（`1,280 B`），否则 `fmt_bytes` 会把 1024 B 和 1280 B 都渲染成 `1 KB`。Windows 上整段跳过（#716）。 |
| **HTTP hook 超时拆分与熔断器** | v0.6.187 | HTTP hook 不再被失联端点拖死主流程：连接阶段改用独立的 800ms 硬超时（`HTTP_HOOK_CONNECT_TIMEOUT_MS` 可覆盖），不再继承整段审批超时；端点级熔断器（300 秒冷却）在连接失败或 5xx 后让后续 hook 直接 0 延迟放行（#705）。 |
| **菊花余弦缓动与单调时钟驱动** | v0.6.187 | 等待菊花不再按渲染帧率匀速转圈：字形集对齐参考实现（`· ✢ * ✶ ✻ ✽`），帧号由 2000ms 往返的余弦缓动算出、单调时钟驱动，帧率高低不再影响旋转速度（#710）。 |
| **Bash 状态行不再断档** | v0.6.186 | 工具块从「排队/等待审批」进入「执行中」时，状态行曾整段消失约 2 秒（上一分支已退出、执行中分支未满 2 秒尚未进入），界面只剩工具头。现执行一开始即显示 `Running…` 占住状态行，耗时秒数与 `ctrl+b` 提示仍保持 2 秒后出现，短命令不因计时刷屏；同步修正渲染线程的跨阈值重建判定，避免裸 `Running…` 让「是否已有状态行」恒为真而使秒数与提示永不出现。排队与审批阶段的 `●` 恢复常亮（仅执行中闪烁）——排队不是「活跃」，等待语义由 `Waiting…` 文案承载（#706）。 |
| **工具指示器同频闪烁与状态栏优化** | v0.6.185 | 统一未完成工具指示器闪烁频率（`ToolCallStatus::Pending` 与 `Running` 共享 800ms 亮 / 800ms 灭的呼吸周期，只要呈灰色未完成状态即全局同频同相位闪烁，同时打通渲染线程心跳与增量刷新管线，消除 Waiting 状态下的错位与不闪，#698）；状态栏在无活跃 Agent 时不再显示 `· ← 0 agents`（#696）；独立纯动作行首字母大写（如 `Read 1 file`，#694）；中断结果行移除重复 ✗ 符号并统一大写 `Interrupted by user`（#692）。 |
| **Bash 实时输出、排队与审批状态** | v0.6.184 | Bash 工具块区分排队、等待审批、运行与完成。运行两秒后显示最后五个视觉行、耗时、显式 timeout 与输出字节数；Ctrl+O 展开最多 64 KiB 的保留尾部。实时输出默认启用，可通过 `config.tool_output_streaming` 关闭；主 Agent 与子 Agent 调用相互隔离（#675）。 |
| **工具输出 URL 成为真超链接** | v0.6.182 | 工具结果与 `!` 命令输出里的 URL 现在是真终端超链接：OSC 8 序列改到输出层注入（`terminal_backend/hyperlink.rs`），写字节时按坐标查表包裹，不再塞进 ratatui 单元格——序列载荷是可见字符，进 buffer 会让 `Buffer::diff` 把链接后面的整段文字跳过不画。URL 用亮蓝（`38;5;12`）、不加人工下划线，悬停与点击由终端提供；默认开启，`DISABLE_HYPERLINKS=1` 可关闭；原 ANSI Cyan + 下划线着色与 `style_urls()` 一并删除（#671）。 |
| **事件洪泛下计时不再冻结** | v0.6.181 | 长时间运行且期间无输出的命令，`Running… (Xs)` 秒数与块首 `●` 动画不再冻结。渲染线程此前每轮循环都重新创建 200ms 的 `sleep`，任何快于 5 Hz 的事件（流式输出、每帧视口上报）都会在它到期前取消，tick 一次都执行不到；`rebuild` 也补不上——渲染键（`content_hash ⊕ detail/diff 标志`）不含流逝时间，内容不变时直接复用旧行。现改为跨事件单调推进的 `tick_deadline` + `biased` 的 `sleep_until`，事件再密也能按时触发；没有运行中的工具时重置 deadline（#683）。 |
| **Bash 执行状态对齐** | v0.6.180 | 工具块改用显式执行阶段：空输出完成显示 `Done`（详细模式为 `(No output)`），不再残留运行指示；显式 timeout 显示为 `Running… (Ns · timeout Nm)` 并在完成结果底部保留尾注；后台化提示改悬挂缩进，子 Agent 不显示该快捷键；历史恢复一律为完成态（#675）。 |
| **npm 二进制改由平台可选依赖分发** | v0.6.179 | `@cc-claw/code` 主包不再用安装脚本下载二进制：二进制拆成五个平台包（`@cc-claw/code-win32-x64`、`-darwin-x64`、`-darwin-arm64`、`-linux-x64`、`-linux-arm64`），经 `optionalDependencies` 按宿主 `os`/`cpu` 只装对应一个，安装期不再执行 postinstall 脚本；`bin/cc-code` 启动器从平台包内定位并拉起二进制（#678）。 |
| **消息区 Markdown 链接输出 OSC 8 超链接** | v0.6.178 | 消息区 Markdown 链接现输出 OSC 8 转义序列，支持的终端（Windows Terminal、iTerm2、WezTerm、kitty 等）可悬停提示、Ctrl/Cmd+点击打开，不再是点不动的纯文本 URL。序列不参与布局与折行计算，URL 内的控制字符会被剥离，避免恶意链接提前终止序列（#671）。 |
| **`.md` 预览不再着色** | v0.6.177 | Write 与 Edit 预览不再给 `.md` 内容上色。参考实现虽把 `md` 识别为 markdown，但其主题 scope 表没有 highlight.js 产出的 `section` / `strong` 映射，实际渲染输出与纯文本逐字节相同；cc-code 此前用 syntect 的 Markdown 语法，标题与粗体会明显染色。现从高亮白名单移除 markdown 扩展名，回退纯文本，与参考实现观感一致。 |
> 更早版本（v0.6.0 – v0.6.176）见 [CHANGELOG](./CHANGELOG.md)。

---

## 安装

支持 macOS (x86_64 / Apple Silicon)、Linux (x86_64 / aarch64)、Windows (x86_64)。

### npm（推荐）

```bash
npm install -g @cc-claw/code
```

安装过程不执行任何脚本：二进制放在平台专属的可选依赖包（`@cc-claw/code-<平台>-<架构>`）里，npm 按操作系统与架构自动选中其中一个。因此普通安装、`--ignore-scripts`、pnpm、bun 都能用，安装期也不需要访问 GitHub。

> [!NOTE]
> 用 `--omit=optional` / `--no-optional` 安装会导致二进制缺失，此时运行 `cc-code` 会打印明确的补救说明，而不是 `ENOENT`。

Windows on ARM 暂无原生构建，可显式安装 x64 版本：`npm install -g --arch=x64 @cc-claw/code`。

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
├── npm/                       # npm 包：平台可选依赖 + bin 启动器
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
