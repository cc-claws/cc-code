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

## ❤️ Sponsor

> [想出现在这里？](mailto:wismyzhizi2018@gmail.com)

<details open>
<summary>Click to collapse</summary>

[![Kimi K2.6](assets/partners/logos/kimi.png)](https://platform.moonshot.cn/console?aff=4D2GLR)

Kimi K2.6 是 Moonshot AI 开源的原生多模态 Agent 模型，专为长程编程、编程驱动设计和群组任务编排而生。支持前端、DevOps、性能优化、全栈工程等复杂端到端工作流。邀请码：4D2GLR。[点击注册](https://platform.moonshot.cn/console?aff=4D2GLR)

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

> 更多底层架构、兜底分类器工作原理与故障排查详见 [Auto 模式与权限把关指南](docs/auto-mode-guide.md)。

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
| **Git Worktree** | `--worktree` 在隔离 checkout 中启动会话，`EnterWorktree` / `ExitWorktree` 让 Agent 进出独立工作目录，后续 shell 与文件工具调用立即使用新目录；子 Agent 可用 `isolation: "worktree"` 获得独立 checkout |
| **HITL 审批** | 敏感操作由 JEV 语义门拦截 —— 独立 LLM 判定，只在判不准时才弹窗 |
| **双引擎文件搜索** | Grep/Glob 优先使用外部 ripgrep 二进制，回退到内置 Rust 引擎 |
| **Monitor / Sleep 工具** | 等待条件用 `Monitor`（后台起任务 + 输出文件）、单纯等待用 `Sleep`（不占 shell 进程），前台裸 `sleep N` 会被拦截并给出指引 |

### v0.6.x 新增功能

**不再把 `<system-reminder>` 原文漏给 hook 与界面（v0.6.285）：** hook 收到的用户提示词里，此前会带上 `cc-tui` 拼接后台任务通知用的 `<system-reminder>` 标签原文，桌面宠物 clawd 等外部组件会直接显示 XML。现在 `cc-middlewares` 在调用 `UserPromptSubmit` 前按区间切掉整段提醒；界面侧按「开标签前有无用户正文」区分形态——拼接在用户输入之后的提醒连同正文一并切除（通知本就由系统提示行单独呈现），整条被标签包住的 compact 摘要只剥标记、保留正文。详见 [CHANGELOG](./CHANGELOG.md)。

**前台裸 `sleep` 改为引导使用 Monitor / Sleep（v0.6.285）：** 模型写 `sleep 30` 干等时会占住整个 shell，期间无法中断也看不到输出。现在前台 `Bash` 以 `sleep N`（N ≥ 2 秒）开头会直接拒绝，并提示改用 `Monitor` 等待条件、或用 `Sleep` 单纯等待（后者不占 shell 进程、可 Ctrl+C 打断）；不足 2 秒的限流写法与后台执行照常放行。详见 [CHANGELOG](./CHANGELOG.md)。

**审批弹窗补描述行与文件级 diff 预览（v0.6.206）：** 工具入参带 `description` 时，弹窗在标题下方单独显示说明行；`Write`/`Edit` 审批改为文件级全文 diff 预览（新建文件走带行号与语法高亮的代码预览），文件读不到、超限或片段定位不到时回退片段 diff。弹窗布局与配色对齐参考实现：顶部满宽实线、内容区上下虚线、编号选项、底部固定快捷键提示，diff 主题随 `/theme` 切换生效。详见 [CHANGELOG](./CHANGELOG.md)。

**Grep / Glob 对齐参考实现最新规范（v0.6.202）：** `Grep` 描述强调与权限审查 UI、可点击文件链接（`file_path:line_number`）集成，补 ripgrep 语法转义与跨行模式说明，参数新增 `context` 别名（等价 `-C`）并支持解析；`Glob` 明确按修改时间倒序、返回 cwd 相对路径，禁止传 `"undefined"` / `"null"` 字面量，无匹配统一返回 `No files found`、截断改用标准提示文案。ripgrep 引擎与纯 Rust 引擎两条路径都做了路径相对化，系统提示词同步强化 Grep/Glob 优先，不再在 Shell 里裸调 `grep`/`rg`/`find`。详见 [CHANGELOG](./CHANGELOG.md)。

**项目 hooks 与用户/插件 agent 定义（v0.6.199）：** 项目 `.claude/settings.json` 的 hooks 现在真正加载（此前只读全局 hooks、项目 hooks 静默失效），用 `cc-code trust-project`（或 `CC_CODE_TRUST_PROJECT_HOOKS=1`）显式信任后执行。Agent 定义除项目级外，也在用户级（`~/.claude/agents/`）和插件中查找，顺序为项目 > 用户 > 插件 > 内置。详见 [CHANGELOG](./CHANGELOG.md)。

**滚动与输入响应优化（v0.6.201）：** 后台准备布局时继续处理输入；稳定滚动复用已确认宽度，用户交互、流式输出与后台更新采用统一绘制节奏。全屏切换、窗口缩放和选区复制保持必要的帧顺序。详见 [CHANGELOG](./CHANGELOG.md)。

**只读 Bash 并入工具折叠组（v0.6.197）：** 文件查询类 Bash（`ls`、`cat`、`grep`、`rg`、`find`、`head`、`awk` 等）现在与 `Read` / `Grep` / `Glob` 并入同一个只读组，折叠摘要显示 `ran N shell commands`，与 `read`、`searched` 并列。判定改为**前端命令词法**——模型不再传 `file_inspect`（已从工具 schema 删除），`2>/dev/null` / `2>&1` 与引号内的分隔符、`>` 不再被误判成写盘。思考行收敛为一行摘要：运行中 `∴ Thinking for Ns…`、冻结后 `  Thought for Ns`，正文走 `Ctrl+O`。自动 compact 不再清空消息区，卡住检测也不再误伤连续编辑。详见 [CHANGELOG](./CHANGELOG.md)。

**文件查询类 Bash 折叠（v0.6.195）：** 只读查看文件的 Bash 调用现在可以并入只读工具组。模型通过可选参数 `file_inspect` 标记（单条命令同样适用），折叠摘要显示 `ran N shell commands`，与 `read`、`searched` 并列；失败的命令在折叠态仍保留退出码与 stderr，`Ctrl+O` 展开完整命令与输出。运行中、已转后台、模型显式设了 `timeout` 的命令不会折叠，实时输出与 `Ctrl+B` 提示不受影响。详见 [CHANGELOG](./CHANGELOG.md)。

**图片附件链接（v0.6.192）：** 每张已发送图片在用户正文下显示一行 `  ⎿  [Image #N]`。有保存路径且 OSC 8 开启时，引用保持自身柔和配色、无人工下划线，可在支持的终端中打开本地图片；`DISABLE_HYPERLINKS=1` 或缺路径时显示不可点击的普通文本，写盘失败不阻断发送。图片写入系统临时目录，恢复会话不保证链接可用。Phase 1 不向模型发送本地路径。完整范围见 [CHANGELOG](./CHANGELOG.md)。

**Skill 工作流（v0.6.191）：** 匹配任务通过核心 `Skill` 工具加载，`/skill-name args` 直接展开指令。专属卡片折叠展示加载结果，`context: fork` 等待隔离子代理完成，激活指令在压缩后恢复。详见 [技能机制](./spec/global/domains/skill-trigger.md)。

**界面语言：** TUI 和 CLI 帮助支持英文与简体中文。使用 `/lang en` 或 `/lang zh-CN` 保存偏好；未保存语言时，启动检测 `LC_ALL`、`LANGUAGE`、`LC_MESSAGES` 和 `LANG`，不支持的 locale 回退英文。未发布改动见 [CHANGELOG](./CHANGELOG.md)。

**输入交互对齐参考实现（v0.6.166）：** Agent 运行中斜杠命令 / Skills / @ 文件候选浮层可交互——方向键/Tab 导航，Enter 仅补全已选中候选，Esc 先关浮层、再按才中断 Agent；「立即补充」遇前台命令时给 2 秒宽限期，超时自动转后台；状态栏权限模式标签新增 `⏵⏵` 前缀。详见 [CHANGELOG](./CHANGELOG.md)。

**子 Agent 取消（v0.6.139）：** 用自然语言让主 Agent 停掉某个子任务即可——模型走 `Agent(action="stop", task_id="bg-…")` 按 `task_id` 精确取消（**不做模糊匹配**，避免停错描述相似的兄弟任务），停止结果回给模型含任务摘要的确认，时间线打印 `• Agent "x" was stopped by main Agent`，抽屉行转为**红色实心 `●`** + `stopped by main Agent`（用户按键停止则显示 `stopped by user`）。等待期间主视口显示 `✻ Waiting for N background agents to finish`。

**子 Agent Prompt 卡片（v0.6.139）：** 主视口按 `Ctrl+O` 进入详细模式后，后台子任务的启动摘要由极简树形切换为经典工具卡片——`● Agent(任务预览)` + `  └ Backgrounded agent` + `  └ Prompt:` + 指令**全文**（6 列悬挂缩进折行），再次 `Ctrl+O` 收回，无需进入子视口即可核验下发给各子 Agent 的指令。未发布改动见 [CHANGELOG](./CHANGELOG.md)。

| 功能 | 版本 | 说明 |
|------|------|------|
| **工具名亮白、思考正文提亮、用户消息按投递状态翻白** | v0.6.287 | 三处消息流配色调整，都是「让固定字段更易扫读」方向，不改布局与交互。**工具名**此前是 `TEXT_SOFT`（#D0D0D0），与同行参数摘要同色，一行内两个字段争抢注意力，现改为 `TEXT`（#FFFFFF），让工具名成为行内唯一被强调的固定字段，状态语义仍由 `●` 指示器承载（PR #914 曾反向改回 `TEXT_SOFT`，本次按最新偏好调回），单条 `ToolBlock` 与 `ToolCallGroup` 组内条目同步。**思考正文**原用 `subtle`（#505050），深色底上几乎不可读，新增派生函数 `theme::thinking_body()`，取 `inactive`（#999999）向 `thinking`（#A2A9E4）插值 25%，保留紫调且明显暗于正文；派生而非新增调色板字段，八套主题自动跟随各自的 `inactive`/`thinking`，不触发 `palettes_parity_test` 的字段锁定；思考摘要行同款提亮，并保留「活跃与冻结同色」不变性，冻结瞬间无亮度跳变。**用户消息**正文原本无前景色（终端默认），无法表达「是否已被 agent 接收」，`UserBubble` 新增 `dispatched` 字段：未送出渲染 `MUTED`（与排队区预览一致）、已送出渲染 `TEXT`，提交路径在 `apply_pipeline_action` 前调用 `mark_dispatched()`，历史消息快照一律 `true`，子会话「投递确认前的用户消息」保持 `false`；`dispatched` 参与 `Hash`，否则渲染线程会命中缓存行停在灰字。提交瞬间「灰 → 白」形成转正反馈（#1074, #1075）。 |
| **`<system-reminder>` 原文不再漏给 hook 与界面** | v0.6.285 | 后台任务通知此前以 `<system-reminder>` XML 形式拼接在用户输入之后，并被原样交给 clawd 等外部组件——`cc-tui` 存的是拼接后的整段文本，`cc-middlewares` 又把整条 Human 消息当作 `UserPromptSubmit` 的 prompt。界面侧的清洗只在首尾各切一次标签，位于中间的标签因此剥不掉。现在两侧都按区间切除：`cc-middlewares` 一旦发现开标签就切掉整段（含未闭合标签之后的全部内容）；`cc-tui-core` 区分两种形态——拼接在用户正文之后的提醒连同正文一并切除（通知本就由系统提示行单独呈现），整条被标签包住的摘要只剥标记、保留正文。后台通知的判定挪到剥离之后，拼接形态不会在途中被截成纯系统提示行（#1071）。 |
| **前台裸 `sleep` 改为引导使用 Monitor / Sleep** | v0.6.285 | 模型写 `sleep 30` 或 `sleep 5 && check` 会占住整个 shell：进程无法中断、也看不到任何输出。现在前台 `Bash` 以 `sleep N`（N ≥ 2 秒且后面不接管道）开头会直接拒绝执行，并提示改用 `Monitor`（等待条件，返回 `taskId` 与输出文件）或 `Sleep`（`duration_seconds`，不占 shell 进程、可 Ctrl+C 打断）；不足 2 秒的限流写法、`run_in_background` 与 `Monitor` 本身照常放行（#1070）。 |
| **首轮会话标题不再被思考残片污染** | v0.6.282 | 新开会话发出首轮消息后，标题栏显示的不是会话主题而是模型的思考过程（形如「我们需要回答用户？…」），并写入会话库 `threads.title` 持久化。原因是标题提炼请求硬编码了 `max_tokens: 30` 且继承会话的 thinking 配置——推理模型的思考与正文共享同一份输出预算，30 个 token 被思考全部吃光后 API 以 `stop_reason = max_tokens` 返回截断结果，响应里的 `content` 是思考残片而非标题，被原样渲染并落库。现移除硬编码的 `max_tokens`（它是上限而非预留，放宽不额外计费），并新增截断守卫：`stop_reason == MaxTokens` 一律丢弃、回退本地确定性标题；同一教训与规则提炼路径的既有结论对齐（#1064, #1065）。 |
| **连按 Enter 不再被误判为多行粘贴** | v0.6.281 | Windows Terminal 上，Agent 运行（loading）期间连续按 Enter，输入框会偶发凭空冒出 `[Pasted text #N +X lines]` 占位符，而用户并未粘贴任何东西。原因是模拟粘贴检测（在不支持 bracketed paste 的终端上把逐字符 Key 突发合并成 `Event::Paste`）的起始窗口用 `pump.next(START_WINDOW = 1 ms)` 实现，但输入泵对已进入内部队列的事件会立即返回、根本不看这个 deadline，「1ms 内是否还有后续事件」实际退化成「输入泵此刻是否已有排队事件」：主线程渲染与流式处理让队列积压，连按时多个 Enter 同批入队，被合并成 `Event::Paste("\n\n")`，再经 `paste_line_count > 1` 折叠成占位符落进输入框。现给每个事件带上被读到瞬间的到达时刻（`StampedEvent` 在输入线程 `event::read()` 返回后立刻打点），起始探测与空闲续帧都改按真实到达间隔判定，已缓存但在窗口之外的事件退回队列按独立按键逐轮处理——既不并入粘贴也不丢弃，事件顺序保持单调；顺带修掉「加载/流式卡顿期间快速打字被误判成粘贴」的同类隐患（#1060, #1061）。同版修复 `cc-code update` 在 Windows 上解压阶段必然失败：v0.6.279 为绕开 Windows PowerShell 5.1 缺少三参数 `ExtractToDirectory` 重载的问题，把覆盖语义改由「解压前删除并重建版本目录」保证，而待解压的 zip 恰好就在该目录内、随目录一起被删掉，解压随即报「未能找到路径……的一部分」，用户完全无法升级。现把 zip 改到系统临时目录，收尾仍按变量清理、不在安装目录留下归档残留；覆盖语义（重复安装同版本仍靠解压前删目录）与 checksum 失败分支的清理逻辑均不变。0.6.279 / 0.6.280 内嵌的脚本引用钉在含缺陷的 commit 上、无法自更新，是最后需要手工 `irm … \| iex` 的两个版本；0.6.281 起 `cc-code update` 恢复正常（#1058, #1059）。 |
| **报错轮次的失败总结行不再被补发的 Done 清空** | v0.6.280 | 一轮以 LLM 报错结束（如 provider 端点不可达，界面出现 `Agent Error` 卡片）时，错误卡片下方变成空白——正常完成时可见的 `✻ Charged for 36s · done 20:29` 消失，本轮耗时与完成时刻无从获知。原因是 executor 恒定先发 `Error` 再补发 `Done`：`handle_error` 写入失败总结行后，紧随的 `handle_done` 因 `reconcile_already_done` 仍把本轮判为异常终止，落进清空支把刚写入的行连同耗时、完成时刻一起擦掉，`persist_last_task_summary()` 也被覆盖成 `None`，`-c`/`-r` 恢复会话时同样丢失。现于清空支增加 `last_summary_is_error()` 守卫，已结算的失败总结行得以保留，错误卡片下方重新显示 `✻ Failed after Xs · HH:MM`；中断与取消行为不变。同版解除 clippy 1.100.0 漂移造成的 Clippy 门禁失败（命中 `cc-widgets` 测试代码的 `needless_range_loop`）（#1055, #1056）。 |
| **worktree 状态即时刷新与任务收尾卡住修复** | v0.6.275 | 进入/退出 worktree 后，状态栏在当前轮即可跟随 Git 分支，覆盖 ExecuteExtraTool 包装调用并丢弃旧目录查询；正常、错误与取消通过显式信号结束主事件泵，排空已入队消息后发送唯一完成通知，修复缓存 worktree 工具保留 Sender 导致界面一直停留在 Retrieving 的问题。新增 13 项回归覆盖目录通知、事件泵收尾与 TUI 状态（#1039, #1040）。 |
| **Git worktree 隔离落地：启动参数、两个 Agent 工具、四个 hook 事件与子 Agent 隔离** | v0.6.274 | worktree 此前只存在于提示词的一句「不承诺隔离」，代码层面没有可执行的能力。现把隔离补齐——**启动参数**：`cc-code --worktree [NAME]` 直接在隔离 checkout 里启动 TUI（目录落 `~/.cc-code/worktrees`，分支 `worktree/<name>`，不传名称用随机名；只对 TUI 生效，`acp` / `plugin` / `update` 子命令不受影响），退出时若仍停在会话内创建的 checkout 里，逐个询问 keep / remove，删除只在用户明确同意时发生。**Agent 工具**：新增 `EnterWorktree` / `ExitWorktree`——前者按 HEAD 建 `worktree/<name>` 分支的独立 checkout 并把会话工作目录切进去（下一轮生效），托管目录优先仓库内 `.claude/worktrees`（创建时自动写入 `.git/info/exclude`，不污染 `git status`），不在 Git 仓库内时回退 `~/.cc-code/worktrees`；名称按「分段白名单 + 64 字符」校验，同名目录直接复用而不是失败；后者按 keep / remove 收尾，存在未提交文件或新增提交时默认拒绝删除（需显式 `discard_changes`），且只作用于本会话创建的 worktree——跨会话的、手工 `git worktree add` 的目录一律不碰，进程重启后回退读磁盘登记快照。**Hook 事件**：新增 `WorktreeCreate` / `WorktreeRemove` / `PermissionDenied` / `CwdChanged` 四个事件，`HookRuntime` 抽为可跨组件共享的执行器，工具这类链外组件也能触发 hook；`WorktreeCreate` hook 可以否决创建或直接交出目录（交出目录时完全不调用 git）。**权限裁决**：hook 的 `permissionDecision` 至此真正生效——allow 跳过审批弹框，deny 当场拒绝。**会话目录**：新增 `SessionCwd` 共享句柄，ACP 每轮从它取 cwd 并发出 `notifications/peri/cwd_changed`，TUI 状态栏的 git 探测跟随该目录，目录变化后向对话追加 cwd reminder 并按新目录重读 CLAUDE.md / AGENTS.md（会话内冻结的系统提示不受影响）。**子 Agent 隔离**：`isolation: "worktree"` 真正生效——独立 checkout 运行，留下改动则保留目录并把路径写回结果，无改动则删除，变更统计失败时按「未知即不安全」保留。改动文件：`cc-tools/src/worktree/`、`cc-middlewares/src/tools/worktree_tool.rs`、`cc-middlewares/src/hooks/`、`cc-middlewares/src/subagent/tool/execute_bg.rs`、`cc-agent/src/session_cwd.rs`、`cc-acp/src/session/acp_server/prompt.rs`、`cc-tui/src/main.rs` 与 `cc-tui/src/app/agent_ops/acp_bridge.rs`。验证：`cargo check` / `cargo test` / `cargo clippy --all-targets -D warnings` 全部通过。特性/影响：worktree 从「模型口头承诺」变成有工具、有 hook、有退出清理的可执行隔离；既有会话目录与工具调用路径不受影响（#1034）。 |
| **工具错配改道后历史回放不再显示「声明名 + 改道后输出」杂交体** | v0.6.273 | 模型声明工具 A 却传工具 B 的参数、经自动改道实际执行 B 后，历史回放（`StateSnapshot` 到达 / `resume` / 会话恢复）会渲染出杂交体：头部是**声明名**（如 `● Bash` 且**没有**参数摘要括号——声明工具的入参键不存在），正文却是改道后工具的输出；且同一块在流式与回放两个时刻显示两个名字——流式事件携带改道后身份，回放按 `tool_call_id` 回查 AI 消息的声明名。改道机制有意只替换运行时 `ToolCall`（事件、审批、熔断归因读它）、历史保留模型声明名，但 `BaseMessage::Tool` 不存工具名，展示层只能回查声明名。现结果消息记录**实际产生它的工具名**（`BaseMessage::Tool.tool_name`，构造时取 `ToolResult.tool_name`），展示层优先取它、旧记录缺省回退原回查；参数摘要仍取声明入参——错配场景里它恰是目标工具的参数（正是改道判定的成立条件）。LLM 历史口径不变，请求序列化不携带该字段。流式、回放、resume 的工具块展示从此同源一致（#1027）。 |
| **Grep files 模式输出的 Windows 路径完成归一化** | v0.6.273 | Grep 的 `files_with_matches`（默认模式）/ `files_without_matches` 输出是 Windows 原生路径——`.\dir\file.ext`，甚至混合分隔符（`cc-tui/src/command\core\help.rs`）——而 `content` 输出与 Glob 一直是正斜杠相对路径：`-l` / `-L` 的行是整条路径、没有 `行号:` 冒号，`find_path_end` 落空后 `convert_rg_line` 整行原样返回，所有归一化步骤被跳过（会话库实测：该形态 1182 条输出中 1171 条带反斜杠）。files 模式现在按整行路径归一化（`normalize_display_path`：转相对 cwd、反斜杠转正斜杠、剥 `./` 前缀），`content` / `count` 行仍按 `path:line:content` 结构处理——正文里的反斜杠字面量永不改写（#1029）。 |
> 更早版本（v0.6.0 – v0.6.271）见 [CHANGELOG](./CHANGELOG.md)。

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
├── cc-tools/                # 叶子工具 crate：无状态核心工具与跨平台进程包装
│   └── README.md              # 工具清单与契约
├── cc-terminal/             # 跨平台终端底层：终端后端、视口、剪贴板、ConPTY
│   └── README.md              # 终端后端与能力说明
├── cc-middlewares/           # 中间件：HITL、SubAgent、Skills、MCP、Hooks（文件/终端工具已下沉至 cc-tools / cc-terminal）
│   ├── README.md              # 中间件概览
│   └── CLAUDE.md              # 开发指南和陷阱记录
├── cc-tui-core/             # 会话核心与展示模型：i18n、主题、markdown 解析、MessageViewModel
│   └── README.md              # 展示模型与核心状态
├── cc-tui/                  # TUI 应用 (Ratatui)
│   ├── README.md              # TUI 使用指南
│   └── CLAUDE.md              # 开发指南和陷阱记录
├── cc-acp/                  # ACP 服务层：桥接 TUI/IDE 与 Agent
│   └── README.md              # ACP 架构和数据流
├── cc-widgets/              # Widget 组件库
│   └── README.md              # 组件列表和使用示例
├── cc-compress/            # 叶子 crate：CLI 输出压缩过滤器（ANSI 剥离、token 守卫、相邻行去重）
│   └── README.md              # 压缩过滤器与契约
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
