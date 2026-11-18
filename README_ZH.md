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
| **长日志滚动不再构造整条消息的渲染成品** | v0.6.294 | 虚拟渲染此前把「测量」和「发布」耦在一起——滚进长消息中部时先按整条消息构造一份渲染成品（渲染行、换行映射、链接）再丢掉，真正发布视口时又整条复制一次；跨多条消息快速翻页还会把已装载的分段一路累积，直到发布阶段才开始淘汰，进程工作集随之抬升。现在两者解耦：`render_layout` 新增 `measure_viewport`，只做相对滚动定位与高度测量，不复制渲染行；`render` 才按视口范围发布成品，每个分段只发布可见行加两侧各一屏的已测量缓冲——40 行视口滚进 10,001 行日志中部时只发布 120 行，与整条渲染逐行等价（布局测试比对参考渲染的同一区段），滚动锚点与链接命中保持精确。分段驻留改用 `lru` crate 的 `LruCache`（工作区依赖），每次装载分段即按字节淘汰，普通分段共享 16 MiB 估算预算，超预算的单个分段可独占缓存；原先的 `evict` 只在发布末尾跑，且用 `used_at` 时间戳自选受害者。`/gc` 的渲染内存合计把虚拟驻留分段计入，已发布窗口仍单独估算，避免重复计数。`RenderWorker` 缓存 `has_live_sources`，不再每轮扫描全部源消息判断是否有活跃工具（#1098, #1099）。 |
| **会话启动 Hook 提示紧贴首条用户消息** | v0.6.293 | `SessionStart` 的汇总和失败行连续显示在发起它的用户消息下方，包括后续轮次才收到的异步失败（#1096）。 |
| **详细模式思考改为 `∴` 单行渲染，新增思考展示开关** | v0.6.291 | 详细模式（`Ctrl+O`）下思考原本渲染成两行——`Thought for Ns` 摘要行加 `⎿` 正文行，而同一段思考在工具组内活跃时读作 `∴ Thinking for N chars`，一旦冻结进详细模式就换了形状；摘要行与正文并列，耗时也成了重复信息。现在详细模式把思考折成一段连续正文：首行 `∴ `（`THINKING_PREFIX`），续行悬挂缩进 2 格（`THINKING_HANG`），宽度由 `thinking_content_width(width)` 计算，颜色统一 `theme::thinking_body()`，与摘要行同色；`AssistantBubble` 的 `Reasoning` 分支与 `ToolCallGroup` 的 `thinking` 分支渲染正文后直接 `continue`，摘要行整行去掉。新增 `AppConfig.thinking_ui: Option<bool>`（`thinking_ui_enabled()` 缺省 `true`，工作区可用 `merge_overrides` 覆盖），命名对齐既有 `hooks_ui`；**不复用** `ThinkingConfig`，那是传给 LLM 的推理参数。`/config` 新增「思考展示」行（`ROW_THINKING_DISPLAY`，紧跟「Hook 显示」之后，其后行号整体后移），经 `RenderTask.show_thinking` + `RenderEvent::ToggleThinking(bool)` + 混入 `render_key` 的 `thinking_sensitive()` 贯通，保存后即时生效，与 `ToggleHookRows` 同款。开（默认）：详细模式只显示 `∴` 正文、无摘要行，普通模式仍是 `Thought for Ns (ctrl+o to expand)`。关：详细模式整段不渲染思考，普通模式保留摘要行但去掉 ` (ctrl+o to expand)` 尾巴——展开也看不到东西，尾巴会自相矛盾。仅影响渲染，思考数据始终留在 VM 里，开关来回切都能还原历史（#1091, #1092）。 |
| **移除 Monitor / Sleep 工具，保留阻塞 `sleep` 拦截** | v0.6.290 | v0.6.285 为「让 agent 别在 shell 里干等」，新增了 `MonitorTool`、`SleepTool` 两个工具与前台 `sleep` 拦截。实跑下来两个工具的成本高于收益——等条件本就是 `run_in_background` 的职责，多出来的工具只是拉长工具列表、诱导选错。现整体删除四个文件（`monitor`、`monitor_test`、`sleep`、`sleep_test`），`TerminalMiddleware::build_tools` 恢复只返回 `Bash`，`tool_names()` 改回 `vec!["Bash"]`，`cc-tools/src/lib.rs` 去掉对应导出。拦截与其替代路径全部保留：`BLOCKED_SLEEP_MIN_SECONDS = 2`、`split_shell_subcommands`、`parse_sleep_seconds`、`detect_blocked_sleep_pattern` 与 invoke 处的判定一字未动；只有 `blocked_sleep_message()` 去掉「改用 Sleep 工具」那句，条件等待仍指向 `run_in_background: true` 与 until-loop 两条路径。顺带修正 Write **新建文件**的 `… +N lines (ctrl+o to expand)` 提示色——该路径走的是 `cc-widgets/src/diff/renderer.rs` 的 `render_new_file_code` 而非 `push_visual_capped_output`，提示行硬编码 `theme.dim()`（即 `subtle`，#505050），与 `⎿` 折角色不统一；函数新增 `hint_color: Option<Color>` 参数（cc-tui 传 `inactive`，不可能出现提示行的调用点传 `None`）（#1089, #1090）。 |
| **工具名降至 #F2F2F2、思考正文降至 #666666** | v0.6.290 | v0.6.287 曾把工具名推到纯白 `#FFFFFF`、思考正文交给 `inactive`（#999999）；实跑下来前者刺眼、后者在深色底上仍偏亮，两者都在与正文争注意力。改走「派生而非新增调色板字段」的老路（绕开字段锁定的 `palettes_parity_test`，八套主题各自跟随自身调色板）新增两个派生函数：`theme::tool_name()` 对 `cur().text` 逐通道 `saturating_sub(TOOL_NAME_DIM = 13)`，DEFAULT 主题下精确落在 `#F2F2F2`，命名色（ANSI 主题）原样返回；`theme::thinking_body()` 取 `inactive` 向 `subtle` 插值 0.7，得 `153×0.3 + 80×0.7 ≈ 102`，三通道同值即 `#666666`（选 0.7 而非 0.5，是因为思考成段出现，只降半档仍偏亮；0.7 才拉开与 hook 行、排队预览等 `inactive` 元素的距离）。着色点：`push_group_entry_header` 的工具名 span、`ToolCallState::new(display_name, …)` 与 `ToolCallGroup` 的 `name_style` 走 `tool_name()`；`dim_markdown_lines` 的无前景色 span、`push_thinking_body` 的 `prefix_style`、`AssistantBubble` 思考摘要行、`ToolCallGroup` 组内摘要行从 `theme::cur().inactive` 改为 `thinking_body()`。`inactive` 本身一字未动——hook 汇总行、排队预览、Bash `⎿` 前缀等 60 余处仍共用它（#1088）。 |
| **思考正文配色回退到纯 INACTIVE，块边界空行补齐** | v0.6.289 | v0.6.287 配色改动的两处后续修正。思考正文回退到 `theme::cur().inactive`：派生的 `theme::thinking_body()` 把 `inactive` 向 `thinking` 插值 25%，得到的 `#9B9DAC` 灰里透蓝紫，与紧邻的 hook 汇总行（纯 `inactive`）色相不一致；该派生函数整体删除，三处着色点（思考正文、`Thought for Ns` 摘要行、`ToolCallGroup` 组内思考行）统一改用 `inactive`，八套主题各自跟随自身调色板，同屏不再有色相跳变。用户气泡改为 agent 真正接活后才翻白：原先回车交出即翻白，但那只是「进了队列」，中途失败的轮次会出现「白字却什么都没发生」；改由 `App::mark_turn_user_dispatched()` 在 agent 首次产出正式内容（`AssistantChunk` 正文或 `ToolStart`）时从尾部翻白并重建一次，纯 `AiReasoning` 不算，排队消息不在 `view_messages` 内，历史快照仍恒为 `dispatched: true`。同一 assistant 消息内 text → reasoning 的边界补上空行：压缩通知（`📦 [ACP] Compressed …`）作为 text block 与 `Thought for Ns` 摘要行同属一条气泡，`ContentBlockView::Text` 渲染完不补分隔、`Reasoning` 只在自身尾部补，导致消息内部的边界无间隔；现仅在 `lines` 非空且末行含非空白内容时补一行，思考作为首个 block 的常规路径（后台 continuation、AskUser 气泡）不变（#1082, #1084, #1083）。 |
| **PermissionRequest 裁决能被解析，熔断跳过不再静默** | v0.6.288 | 外部审批方（如 clawd 桌面宠物）通过 HTTP hook 参与权限裁决时，peri 收到响应却当没收到过：界面上只留下一行 `1 PermissionRequest hook ran · 0ms`，既看不到裁决结果，也看不到失败原因。两个原因。其一，响应字段根本没接上——`SyncHookResponse.hook_specific_output` 少了 camelCase rename，serde 找的是 `hook_specific_output`，真实 JSON 里是 `hookSpecificOutput`，整个字段被静默丢弃；受影响的还有 `PreToolUse` 的 `updatedInput` / `permissionDecision`，既有测试没发现是因为它们直接构造 Rust 结构体、没走真实 JSON 解析。现给该字段加 `#[serde(default, rename = "hookSpecificOutput", alias = "hook_specific_output")]`，并新增 `HookSpecificOutput::PermissionRequest` 变体与 `PermissionRequestDecision` 类型（`behavior` 取 `allow` / `deny`，`allow` 可带 `updatedInput`）来解析回复：allow 转成 `PermissionOverride{decision: Allow}` 并跳过审批弹框，deny 转成 `PermissionOverride{decision: Deny}` 并带原因当场拒绝，与参考实现的 `PermissionRequestHookSpecificOutputSchema` 对齐。其二，熔断窗口期被记成了一次干净运行——HTTP hook 连接失败会让该端点熔断 300 秒，熔断期内 peri 不发任何请求直接短路返回 `Allow`，且这条日志是 debug 级，默认 `RUST_LOG=info` 下完全看不见，但运行结果仍被记成「ran=1、耗时 0ms」，于是凭空造出一条假的 `PermissionRequest hook ran · 0ms` 记录。实测日志里有同一端点连续 37 次 `HTTP hook request failed, tripping circuit breaker for 300s`，之后每次触发都显示 0ms。现在短路改为返回 `HookAction::Error` 并写明原因（"endpoint circuit breaker open (cooling down 300s)"），与参考实现「hook 失败产出可见的 `non_blocking_error` 记录、但不阻断调用」的处理一致；`HookAction::Error` 仍不阻断工具调用，审批会回落到 peri 自带的 HITL 弹框（#1078, #1079）。 |
| **工具名亮白、思考正文提亮、用户消息按投递状态翻白** | v0.6.287 | 三处消息流配色调整，都是「让固定字段更易扫读」方向，不改布局与交互。**工具名**此前是 `TEXT_SOFT`（#D0D0D0），与同行参数摘要同色，一行内两个字段争抢注意力，现改为 `TEXT`（#FFFFFF），让工具名成为行内唯一被强调的固定字段，状态语义仍由 `●` 指示器承载（PR #914 曾反向改回 `TEXT_SOFT`，本次按最新偏好调回），单条 `ToolBlock` 与 `ToolCallGroup` 组内条目同步。**思考正文**原用 `subtle`（#505050），深色底上几乎不可读，新增派生函数 `theme::thinking_body()`，取 `inactive`（#999999）向 `thinking`（#A2A9E4）插值 25%，保留紫调且明显暗于正文；派生而非新增调色板字段，八套主题自动跟随各自的 `inactive`/`thinking`，不触发 `palettes_parity_test` 的字段锁定；思考摘要行同款提亮，并保留「活跃与冻结同色」不变性，冻结瞬间无亮度跳变。**用户消息**正文原本无前景色（终端默认），无法表达「是否已被 agent 接收」，`UserBubble` 新增 `dispatched` 字段：未送出渲染 `MUTED`（与排队区预览一致）、已送出渲染 `TEXT`，提交路径在 `apply_pipeline_action` 前调用 `mark_dispatched()`，历史消息快照一律 `true`，子会话「投递确认前的用户消息」保持 `false`；`dispatched` 参与 `Hash`，否则渲染线程会命中缓存行停在灰字。提交瞬间「灰 → 白」形成转正反馈（#1074, #1075）。 |
| **`<system-reminder>` 原文不再漏给 hook 与界面** | v0.6.285 | 后台任务通知此前以 `<system-reminder>` XML 形式拼接在用户输入之后，并被原样交给 clawd 等外部组件——`cc-tui` 存的是拼接后的整段文本，`cc-middlewares` 又把整条 Human 消息当作 `UserPromptSubmit` 的 prompt。界面侧的清洗只在首尾各切一次标签，位于中间的标签因此剥不掉。现在两侧都按区间切除：`cc-middlewares` 一旦发现开标签就切掉整段（含未闭合标签之后的全部内容）；`cc-tui-core` 区分两种形态——拼接在用户正文之后的提醒连同正文一并切除（通知本就由系统提示行单独呈现），整条被标签包住的摘要只剥标记、保留正文。后台通知的判定挪到剥离之后，拼接形态不会在途中被截成纯系统提示行（#1071）。 |
| **首轮会话标题不再被思考残片污染** | v0.6.282 | 新开会话发出首轮消息后，标题栏显示的不是会话主题而是模型的思考过程（形如「我们需要回答用户？…」），并写入会话库 `threads.title` 持久化。原因是标题提炼请求硬编码了 `max_tokens: 30` 且继承会话的 thinking 配置——推理模型的思考与正文共享同一份输出预算，30 个 token 被思考全部吃光后 API 以 `stop_reason = max_tokens` 返回截断结果，响应里的 `content` 是思考残片而非标题，被原样渲染并落库。现移除硬编码的 `max_tokens`（它是上限而非预留，放宽不额外计费），并新增截断守卫：`stop_reason == MaxTokens` 一律丢弃、回退本地确定性标题；同一教训与规则提炼路径的既有结论对齐（#1064, #1065）。 |
> 更早版本见 [CHANGELOG](./CHANGELOG.md)。

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
