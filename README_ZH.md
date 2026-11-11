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
| **HITL 审批** | 敏感操作由 JEV 语义门拦截 —— 独立 LLM 判定，只在判不准时才弹窗 |
| **双引擎文件搜索** | Grep/Glob 优先使用外部 ripgrep 二进制，回退到内置 Rust 引擎 |

### v0.6.x 新增功能

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
| **MCP 工具头按 server/tool 展示并列出参数** | v0.6.270 | MCP 工具在消息区展示形态不佳：工具名被 `to_pascal` 拼成 `McpTavilyTavilySearch`（server 名重复两遍、无分隔符）；参数摘要走兜底逻辑只取「第一个字符串字段」，而 serde_json 的 Map 按字母序迭代，命中的是 `country` 之类字段而非 `query`，且丢掉 key 名，用户看不出实际传了哪些参数；状态栏统计同样显示 `McpTavilyTavilySearch`。现新增 `parse_mcp_tool_name` 拆 `mcp__{server}__{tool}`——第一段为 server、其余整体为 tool（tool 名本身可能含 `__`）；`format_tool_name` 输出 `server - tool (MCP)`、`format_tool_name_for_stats` 取 tool 段短名、`format_tool_args` 走新增的 `mcp_args_summary` 逐顶层 key 输出 `key: value`（值按 JSON 序列化，按 `TOOL_ARGS_MAX_CHARS` 兜底截断，与其余分支口径一致）。消息区工具头由 `McpTavilyTavilySearch(China)` 变为 `tavily - tavily_search (MCP)(country: "China", max_results: 8, ...)`，状态栏由 `McpTavilyTavilySearch` 变为 `tavily_search`。属展示层改动，不涉及工具执行、权限判定与消息内容（#1011）。 |
| **聚合组折叠态补上失败 Bash 的命令头** | v0.6.269 | 普通模式下，同一轮聚合的工具组里失败的 Bash 调用只渲染一段截断错误摘要，没有命令本身，用户无从判断是哪条命令失败，只能切到详细模式才看得到。现于折叠态先渲染 `● Bash(<command>)` 命令头，再接原有的截断错误摘要（前 3 行 + `… +N lines (ctrl+o to expand)`）；折叠态高度估算同步计入这两行，避免首帧高度偏小导致滚动定位漂移。成功的调用仍按原样折叠隐藏，详细模式行为不变（#1005）。 |
| **RTK `rewrite` 的 `exit 3` 不再被丢弃** | v0.6.268 | Bash 工具执行前会调 `rtk rewrite "<cmd>"` 以获取 RTK 的输出压缩，但此前只接受 `exit code 0`。RTK 文档化了四个退出码：`0`（打印重写且 allow 规则覆盖链上每个命令）、`1`（无重写）、`2`（deny 命中）、`3`（**打印了重写命令**，但命中 ask 规则或 allow 未全覆盖，含「根本没有配置任何规则」）。`0` 与 `3` 都打印了重写、都须采纳，区别只在「是否可由 hook 自动放行」，而该调用点处于**审批之后**，审批已由 HITL 完成。由于 `3` 覆盖「未配置任何规则」的默认情形，常见的 git 类命令（`git status` / `log` / `diff`）此前被静默回退。现 `rtk_rewrite_succeeded()` 认可 `0`/`3`，拒绝 `1`/`2` 及其余非文档化码。实测（RTK 1.2.0）：`git status` / `git log` / `git diff` / `cargo test` 均返回 `3`。审批语义不变——门控仍用纯字符串预测 `predict_rtk_rewrite`（零子进程，#288）（#999, #1000）。 |
| **抽出 `cc-compress` crate 承载 CLI 输出压缩** | v0.6.268 | CLI 输出压缩过滤器此前内嵌在 `cc-tools/src/tools/output_filter.rs`，仅 3 个过滤器，而规则量将按命令持续扩充；`cc-middlewares` 的 hooks 侧也需要 `strip_ansi` 且得绕道 `cc-tools`。现抽出独立叶子 crate（`cc-compress`，与 `cc-widgets` 同级：零 workspace 内部依赖，纯函数——不 spawn 进程、不读环境、不碰文件系统），首批迁入三个通用设施：ANSI 剥离与 `\r` 进度覆盖规范化、`never_worse` 守卫（按 `bytes/4` 估算 token，过滤更贵时回退原文，小输出不会反向膨胀）、相邻重复行折叠（连续 ≥3 行相同折叠为 `[xN]`，空行豁免、不跨距离）。`cc-tools` 改为从新 crate 转出 `strip_ansi`，并让 `filter_command_output` 过 `never_worse`。**本次未新增按命令的过滤器**；`git log` / `diff` / `show` 需改写 argv（上游会注入 `--pretty=format:` 并多取一条判截断），与 HITL 的「门控评估 == 审批展示 == 实际执行」不变量冲突，需单独设计（#1001, #1002）。 |
| **后台运行状态行加旋转图标** | v0.6.267 | Bash 工具移入后台后，工具行下方的状态文案 `Running in the background (↓ to manage)` 是纯文字，与上方带图标的工具行相比不够醒目，现于文案前加旋转语义图标 🔄（U+1F504）。该常量由两处渲染点共用（后台任务卡片、Bash 工具行），改常量即两处一致生效。🔄 为 East Asian Width 双宽字符，与既有 `unicode-width` 截断口径一致，静态渲染下宽度稳定，不会引发后续文本抖动（#996, #997）。 |
| **工具描述补反锚定并按 schema 精简** | v0.6.266 | 工具描述提示词统一遵循一条规则：schema 是唯一真相。`Glob` 此前缺少 `Grep` 与 `Write` 都有的 NEVER 反锚定，导致本应发给 Bash 的 `{command, description}` 落到只读工具上时没有任何阻拦；现补上 `NEVER use Glob to execute shell commands or scripts — that is Bash. \`command\` is not a valid parameter here.`。`Glob` 与 `Grep` 同时删掉仅复述自身 schema 的文字（`pattern` / `path` 默认值、`output_mode` 全枚举、`-C` / `-A` / `-B` 语义、`head_limit` / `offset`），由 151 / 248 词降至 78 / 138 词，并保留 schema 从未覆盖的本项目特有行为（Grep 超长行 500 字节不输出；Glob 最多 1000 条且遵守 `.gitignore` / `.ignore`）。内核工具策略段删掉三条与工具 schema 重复的签名条款（373 → 198 词），并修正一句过时注释——`hidden(true)` 的语义是**忽略**隐藏路径（#993, #994）。 |
| **工具错配自动改道支持只读 Bash** | v0.6.265 | 模型把 Bash 参数（`{command, description}`）发给只读工具时，此前只报 schema 错误加一句 `💡 Did you mean to use 'Bash'?`，需额外一轮 LLM 往返纠正，现纳入自动改道。改道同时前移到新增的阶段零，在**任何事件与审批之前**替换调用的 name/input——此前改道执行过晚，界面会显示「`Grep` 名 + Bash 参数」（取只读绿、`file_inspect` 恒 false），且 `requires_approval` 看到的是原名 `Grep` 而静默跳过审批，改道后的 Bash 无审批直接执行。Bash 是唯一例外：仅当参数严格匹配其 schema **且**命令被判定为只读查看时才构成候选，仍遵循「恰好一个候选」语义；判定属启发式而非沙箱，故改道后照常走 HITL 审批。只读判定从 `cc-tui-core` 下沉到 `cc-agent/src/shell/readonly.rs`（原路径转出）——改道逻辑在 `cc-agent`，依赖方向要求如此。同一机制的第一阶段已在 v0.6.262 上线：参数严格匹配唯一只读工具（`Grep` / `Glob` / `Read`）时静默改道，不再触发 schema 校验失败（#988, #989）。 |
| **Grep / Glob 的 `pattern` 误用诊断与零匹配提示** | v0.6.265 | 向 `Grep` 的 `pattern` 传路径或 glob（如 `src/*.rs`）是 schema 合法的，于是被当正则执行——`.` 通配、`*` 作量词——合法地匹配不到任何内容、静默返回 `No files found`。错配诊断、自动改道、熔断器全部挂在参数校验失败路径上，无一触发，空结果极易被误读为「该路径下没有这些文件」。现两侧 `pattern` 描述都写明各自接收何种输入（并给出就地反例 `"src/*.rs"`），且 glob 形态的 pattern 零匹配时会反向按 glob 复查，命中则追加 `💡 Did you mean to use 'Glob'?` 提示。三重收窄保证真阴性不出噪音——仅零匹配时介入、仅含 `*`/`?`/`[` 才试探、仅 glob 确有命中才提示——纯字面路径与正常搜索输出逐字节不变（#986, #987）。 |
| **状态栏提示冒号重复修复，`Alt+P` 文案修正** | v0.6.264 | 状态栏第二行快捷键提示此前渲染出两个冒号（`Ctrl+O ::详情`）：locale 的 `key-*` 文案自带前导 `:`，`format_hints` 又补了一个；键名段还带尾随空格，使键名与冒号之间多出一个空格。现将冒号收口到 `format_hints` 一处——剥掉 locale 前缀再补一个冒号，并去掉键名段尾随空格让冒号紧贴键名（空描述如 `● Verbose` 保留尾随空格以维持段间距），locale 值不动以免波及共用的面板提示。默认模式现为 `Ctrl+O:详情  Ctrl+P:内核  Alt+P:模型`，详细模式为 `● Verbose  Ctrl+O:退出详细`，OAuth / Questions / Rewind / 退出确认及各面板 `status_bar_hints()` 同步生效。顺带修正 `Alt+P` 文案：该键实际打开命令面板（Provider / Model / Effort 选择），原文案「设置」比实际范围窄，`key-status-settings` 中英文分别改为 `:模型` / `:model`（#984）。 |
| **Ctrl+P 切换内核面板，Alt+P 打开命令面板** | v0.6.263 | 此前 `Ctrl+P` 与 `Alt+P` 都绑定到命令面板，内核切换只能手动输入 `/kernel`。现拆分重叠绑定：`Ctrl+P` 弹出内核选择面板（cc / dsh / pi），复用 `/kernel` 无参入口逻辑（统一走 `App::open_kernel_panel()`）；`Alt+P` 保持打开命令面板（Provider / Model / Effort），行为不变。两面板同属 `MutexGroup::Settings` 互斥组，切换时自动关闭另一面板。状态栏提示改为 `Ctrl+O ::详情 │ Ctrl+P ::内核 │ Alt+P ::设置`，tips 与 `/help` 同步更新，`TUI-STYLE.md` 一并修订（#981, #982）。 |
> 更早版本（v0.6.0 – v0.6.262）见 [CHANGELOG](./CHANGELOG.md)。

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
