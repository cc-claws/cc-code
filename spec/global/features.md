# 已有功能清单

## 核心引擎（cc-agent）

- **子 Agent 独立事件信封：** 以实例 ID 包装完整子事件，覆盖思考、消息快照、工具、指标与诊断；ACP 标准事件仅经 `session/update`，生命周期仅经 `peri/subagent_lifecycle`。

- **ReAct 循环执行器:** `ReActAgent` 核心默认最多 10 次迭代（`cc-acp` 构建时 `max_iterations(500)` 放宽），思考 → 工具调用 → 反馈自动推进，parallel 工具调用（同轮多工具同时执行）
- **MockLLM 测试工具:** `MockLLM::tool_then_answer()` 按脚本回放推理序列，无需真实 API，覆盖单元测试场景
- **OpenAI 适配器:** 支持 `message.reasoning_content`（DeepSeek-R1/o 系列），streaming SSE，`type:"function"` 工具格式
- **Anthropic 适配器:** Prompt Cache（默认开启，最后消息末尾 `cache_control:ephemeral`），Extended Thinking（`budget_tokens`），`system` 字段 blocks 格式
- **MessageAdapter 双向转换:** `OpenAiAdapter` / `AnthropicAdapter` 实现 `MessageAdapter` trait，`BaseMessage` ↔ Provider 原生 JSON
- **ContentBlock 完整支持:** Text / Image（Base64 & URL）/ Document / ToolUse / ToolResult / Reasoning / Unknown 透传
- **Middleware Chain:** `Middleware<S>` trait，七个生命周期钩子（`before_agent` / `before_tools_batch` / `before_tool` / `after_tool` / `before_model` / `after_model` / `after_agent`）+ `collect_tools`
- **系统提示词段落化:** 14 个 .md 段落文件（6 静态 + 8 动态，其中若干 feature-gated），`__SYSTEM_PROMPT_DYNAMIC_BOUNDARY__` 边界标记分隔静态/动态（标记前可缓存），PromptFeatures 条件注入，include_str! 编译时嵌入
- **消息管线统一:** MessagePipeline 唯一入口，PipelineAction 枚举，ToolStart+ToolEnd 事件拆分
- **尾部重建:** reconcile_tail() 方法，Done/Interrupted 时触发，RebuildAll 只替换尾部
- **工具参数 Schema 预校验:** 对齐参考实现的校验错误格式风格，结构化分项输出缺失参数 / 意外参数 / 类型不符；`suggest_tool_mismatch` 启发式诊断（基于参数特征指纹对 WebFetch/Bash/Grep/Read/Agent 给出纠偏建议）；`SchemaFailureTracker` 熔断器（滑动窗口口径，窗口容量 6；单工具阈值 2、跨工具聚合阈值 3，由窗口派生且成功调用同样占位以淘汰旧失败，防盲目重试循环）
- **工具动作签名循环检测:** 连续 3 轮相同工具动作时注入纠正消息，打断重复循环
- **Anthropic 适配器（自适应兼容）:** 非流式响应自适应解析反向代理返回的 OpenAI 格式（`choices` / `message` / `tool_calls`），缺 `content` 时回退解析；SSE 流式自适应解析
- **/recap 会话回顾:** 独立 `aux_model`（与 compact_model 解耦），单轮禁用工具、不写 history、支持 Ctrl+C 取消
- **项目记忆（v0.6.139，#507；精简读取 #732）:** 纯文件架构、零专有工具，与既有 `~/.claude/projects/`、`~/.cc-code/projects/` 资产兼容。宿主在会话创建时只读一次 `MEMORY.md` 并冻结（空内容也冻结），主提示词携带一份精简读取规则与最多 20 KB（20,000 UTF-8 字节）索引，保留入口 200 行限制；相关任务遇到空预览、截断或未命中时按关键词搜索完整索引，索引缺失或无匹配再搜索记忆目录，按需读取详情或分页。仅在用户明确要求时用 `Write`/`Edit` 保存、更新或删除，不自动总结对话，不增加后台记忆模型调用。原入口 200 行/25KB 截断及独立 `MemoryMiddleware` API 保持兼容；宿主不再注册该中间件，避免重复注入。安全门保留记忆路径 carve-out，TUI 路径继续脱敏渲染

- **核心工具描述对齐参考实现（v0.6.138）:** Bash / WebFetch 改为 **NOT 反向枚举**（读用 `Read` NOT `cat`、内容检索用 `Grep` NOT `grep`/`rg` 等），补齐 `Grep`/`Read`/`TodoWrite`/`AskUserQuestion` 边界句；**`Write` 改为 NEVER 语义**——默认不再主动创建 `.md`/README，仅用户明确要求时创建（行为语义变更）。详见 [domains/tools.md](domains/tools.md)
- **系统提示词静态段落对齐参考实现（v0.6.139，#504）:** `01_intro` 补「信息足够即行动」与 Corrections；`03_doing_tasks` 补 Scope discipline（不超范围加功能/抽象、优先编辑现有文件、不做投机抽象）；`04_actions` 补授权与影响判定框架（破坏性/难逆/影响共享状态/上传第三方需确认）；`06_tone_style` 明确「用户只看文本输出」与结果诚实性。详见 [domains/system-prompt.md](domains/system-prompt.md)

- **Agent 内核 cc / dsh / pi 体系（v0.6.241 / v0.6.257 / v0.6.258 / v0.6.263）:** 三个内核共用一套内核边界——`cc-agent/src/kernel/` 下按内核分 `spec/`（参数映射与来源标注）与 `prompts/`（方言提示），cc 内核保持逐字节等价，dsh/pi 走方言翻译保证工具与审批契约一致。v0.6.257 起 cc 的工具定义与工具规则不再绕过转换器，`kernel/prompts/cc.md` 由 `spec/cc.rs` 声明引用，`kernel/mod.rs` 新增 `StaticToolPolicy` 静态/追加消费接口；同版补齐 cc 内核身份声明与 `tools` / `rules` 分节的核心工具清单。TUI 侧 `/kernel` 命令提供 cc / dsh / pi 选择列表面板（`show` 子命令查看当前内核），`Ctrl+P` 弹出内核选择面板（`Alt+P` 为 Provider / Model / Effort 命令面板，两面板同属 `MutexGroup::Settings` 互斥组），欢迎页按当前会话显示 `Agent Kernel: <id>`
- **dsh / pi 工具参数对齐与执行语义（v0.6.270，#1008）:** `kernel/spec/{dsh,pi}.rs` 对齐 skill、bash、questions 与 grep 参数并修正 dsh 来源标注；`kernel/prompts/{dsh,pi}.md` 明确毫秒超时、0-based offset、搜索上限及 workdir/id 能力限制；新增 `kernel/find.rs` 接入仅 pi 生效的 `find.limit` 适配，保留底层警告并覆盖两种工具注册路径
- **工具错配自动改道（v0.6.262 / v0.6.265）:** 两阶段。阶段一（#980）：schema 校验失败时若参数 100% 匹配**恰好一个**只读工具（`Grep` / `Glob` / `Read`）则静默改道执行，不再把结构化错误回灌给模型；0 个或 ≥2 个命中（空参数 / 多余键 / `{pattern, path}` 歧义）均回退原错误路径，方向单向安全（`command` 类参数永不被改道），工具调用身份对外不变仅日志留痕。阶段二（#988, #989）：把改道扩展到只读 Bash，并前移到 `collect_tool_results` 入口的**阶段零**，在任何事件与审批之前替换调用的 name/input——此前改道过晚导致界面显示「`Grep` 名 + Bash 参数」、`requires_approval` 看到原名而静默跳过审批；只读判定随之从 `cc-tui-core` 下沉到 `cc-agent/src/shell/readonly.rs`（`cc-tui-core` 保留 re-export），判定属启发式而非沙箱，改道后的 Bash 照常走 HITL 审批
- **工具描述补反锚定并按 schema 精简（v0.6.260 / v0.6.266）:** 工具提示词统一遵循「schema 是唯一真相」——参数细节只写在 schema，描述里不重复，防错锚定就近写在工具描述内部而非系统提示词通用条款。`Glob` 补上此前缺失的 NEVER 反锚定句（`Grep` / `Write` 已有）；`Glob` / `Grep` 删除与自身 schema 逐字重复的内容，由 151 / 248 词压至 78 / 138 词，保留 schema 未覆盖的本项目特有行为；`kernel/prompts/cc.md` 工具策略段删掉三条与 schema 重复的签名条款（373 → 198 词）
- **跨工具 Schema 失败第 2 次即注入提醒（v0.6.256）:** `SchemaFailureTracker` 维护历史累计失败工具列表与一次性跨工具首现提示状态，在 Schema 失败分支消费首现提示并随 ToolResult 正文下发；提示不新增 system 消息、不影响 prompt cache，全局仅发一次，聚合熔断阈值与强制止动行为不变

## 工具库（cc-tools）

- **FilesystemMiddleware（2026-11 下沉自 cc-middlewares）：** 提供 `Read`、`Write`、`Edit`、`Glob`、`Grep` 五个文件工具；只读工具无需 HITL
- **TerminalMiddleware / BashTool（2026-11 下沉自 cc-middlewares）：** `Bash` 工具，120 秒超时，跨平台（Windows `cmd /C`，其他 `bash -c`）；Windows 下 `cmd /C` 失败且 stderr 匹配"命令未识别"时自动 fallback 到 Git Bash，多语言 stderr 匹配（English/中文/法语/德语）+ 兜底模式，`MSYS_NO_PATHCONV=1` 防路径转换，剩余超时继承
- **跨平台进程包装：** shell 命令构造、输出捕获、Windows 作业对象、RTK 改写预测
- **输出处理：** 输出过滤、截断落盘、Todo 解析
- **Grep / Glob 的 `pattern` 语义歧义与零匹配兜底（v0.6.265，#986, #987）:** 向 `Grep` 传文件路径或 glob 形态的 `pattern`（如 `src/*.rs`）对 schema 完全合法，会被当作正则执行（`.` 通配、`*` 作量词）而合法地匹配不到任何内容，静默返回 `No files found`；既有的错配诊断、自动改道、失败熔断全挂在参数校验失败路径上，无一触发，易把零匹配误判为「该路径下确实没有这些文件」。现两侧 `pattern` 参数级描述写明各自接收何种输入并给出就地反例，零匹配且 `pattern` 含 glob 元字符时复用 Glob 的 `collect_files` 反查命中数、命中则追加 `💡 Did you mean to use 'Glob'?` 提示。三重收窄保证真阴性不出噪音：仅零匹配介入、仅含 `*` / `?` / `[` 才试探、仅 glob 确有命中才提示
- **RTK 重写 `exit 3` 被采纳（v0.6.268，#999, #1000）:** Bash 工具执行前调 `rtk rewrite "<cmd>"` 获取 RTK 输出压缩，此前只接受 `exit code 0`，而 RTK 文档化的 `3`（打印了重写命令、但 allow 未全覆盖，含「未配置任何规则」的默认情形）被当作失败丢弃，导致 `git status` / `log` / `diff` 等常见命令的压缩从未生效。现新增 `rtk_rewrite_succeeded()` 认可 `0` / `3`、拒绝 `1` / `2` 及其余非文档化码；审批语义不变（调用点位于审批之后，HITL 门控仍用零子进程的纯字符串预测 `predict_rtk_rewrite`）
- **边界：** 仅依赖 `cc-agent`；复合工具（Skill/AskUser/SubAgent/ToolSearch）依赖上层状态，仍保留在 cc-middlewares

## 输出压缩（cc-compress）

- **CLI 输出压缩叶子 crate（v0.6.268，#1001, #1002）:** 零 workspace 内部依赖的纯函数库（不 spawn 进程、不读环境、不碰文件系统），与 `cc-widgets` 同级。三个模块：`ansi`（`strip_ansi` 剥离 ANSI 转义并把 `\r` 覆盖的进度条输出规范化为最终一帧）、`guard`（`estimate_tokens` 按 `bytes/4` 估算 + `never_worse` 守卫：过滤结果更费 token 时回退原文，避免小输出场景追加标记行后反向膨胀）、`dedup`（`dedup_adjacent_lines` 折叠相邻 ≥3 次的相同行为 `行 [xN]`，空行豁免、不跨距离折叠）。`cc-tools/src/tools/output_filter.rs` 改为从本 crate 转出 `strip_ansi`，`filter_command_output` 出口接 `never_worse`。
- **边界：** 本 crate 不提供按命令定制的过滤器（`git log` / `diff` / `show` 需改写 argv 并额外拉取数据判断截断，与 HITL 的「门控评估 == 审批展示 == 实际执行」不变量冲突，需单独设计）；上游规则提炼见 [docs/2026-11-10-cli-output-compression-rules.md](../../../docs/2026-11-10-cli-output-compression-rules.md)

## 终端底层（cc-terminal）

- **终端后端与视口：** 双缓冲、字形列宽探测、视口兼容、OSC 8 超链接注册
- **剪贴板：** arboard / OSC 52 / 图片粘贴 / stderr 抑制
- **Windows 伪控制台：** `conpty.rs` / `conpty_host.rs`
- **终端能力：** 色深探测、OSC 0 标题、键盘增强模式
- **shell 执行：** shell 命令执行与流式输出
- **边界：** 依赖 `cc-agent`、`cc-tools`，不得依赖 `cc-tui` / `cc-tui-core`

## 会话核心与展示模型（cc-tui-core）

- **配置与国际化：** 配置类型、i18n、会话导出、输入/shell 历史
- **展示模型：** 主题/调色板、markdown 解析、`MessageViewModel`、终端主题跟随
- **工具展示与通知：** 工具参数格式化、后台通知展示、显示列宽截断
- **MCP 工具头按 server / tool 展示并列出参数（v0.6.270，#1011）:** 工具名此前由 `to_pascal` 拼成 `McpTavilyTavilySearch`（server 名重复两遍、无分隔符）；参数摘要走兜底逻辑只取「第一个字符串字段」——serde_json 的 Map 按字母序迭代，命中的是 `country` 之类字段而非 `query`，且丢掉 key 名，用户看不出实际传了哪些参数；状态栏统计同样显示拼接名。现新增 `parse_mcp_tool_name` 拆 `mcp__{server}__{tool}`（第一段为 server、其余整体为 tool，tool 名本身可能含 `__`）；`format_tool_name` 输出 `server - tool (MCP)`、`format_tool_name_for_stats` 取 tool 段短名、`format_tool_args` 走新增的 `mcp_args_summary` 逐顶层 key 输出 `key: value`（值按 JSON 序列化，按 `TOOL_ARGS_MAX_CHARS` 兜底截断，与其余分支口径一致）。属展示层改动，不涉及工具执行、权限判定与消息内容
- **UI 状态核心：** `DisplayFlags`、`UiStateCore`（约 40 个纯状态字段）
- **边界：** 不得依赖 `cc-tui`，否则循环依赖

## 中间件（cc-middlewares）

- **后台任务管理：** 子 Agent 恒后台执行（无前台同步模式），会话级任务注册表（并发上限 3）、下一次模型调用前消费的定向 mailbox、显式超时预算 `timeout_secs`（省略/零无限制）、独立持久化子线程；结果经完成通知回流并自动续接。

- **HitlMiddleware:** `before_tool` 拦截敏感操作（bash/write/edit/delete/rm），四种决策：Approve / Edit / Reject / Respond；oneshot channel 异步等待用户决策
- **项目 hooks 与信任（v0.6.199，#765；v0.6.205，#783）:** 项目 `.claude/settings.json` 的 hooks 与全局 hooks 一并加载，但须显式信任后才执行（`cc-code trust-project` 或 `CC_CODE_TRUST_PROJECT_HOOKS=1`）；信任清单为 `~/.cc-code/trusted_projects.jsonl`（一行一条，坏行只跳过该行，旧纯文本清单自动迁移并留 `.legacy.bak`）；home 解析统一走系统 home 目录，HOME 未设置时不再静默跳过项目/插件 hooks，未信任时 TUI 启动显式提示；v0.6.210 起 hook 执行结果上报到 UI（详细模式展示汇总，异步 hook 先占位 running、完成时就地替换，子代理继承会话 hooks）；v0.6.212 起 `hooks_ui` 配置（缺省 `true`）可在 `/config` 面板关闭 hook 行展示，切换即时生效、数据保留
- **command hook 支持 exec form args（v0.6.255，#954, #955）:** Windows 上 `~/.claude/settings.json` 中的 hook 常被第三方桌面工具写成 `command` + `args` 的 exec form，此前 `HookType::Command` 没有 `args` 字段，serde 静默丢弃参数并把 `command` 整串交给 `cmd /C`——可执行路径含空格时被切碎，此类 hook 全部以 exit 1 失败。现 `args` 非空时直接以 `command` 为可执行文件、`args` 为 argv 启动（不经过 shell），逐项做 `${CLAUDE_PLUGIN_ROOT}` / `${CLAUDE_PLUGIN_DATA}` / `$ARGUMENTS` 变量替换；`args` 为空的旧配置行为不变，exec form 下 `shell` 字段被忽略
- **SessionEnd 关窗可靠触发与 hook payload session_id 全链路对齐（v0.6.251，#941, #942）:** Windows 点 X 关窗时 `CTRL_CLOSE_EVENT` 无人处理，SessionEnd 从不触发，clawd 等按 session_id 聚合的外部 hook 消费方里会话永远停在 working；现关窗在 handler 线程从事件循环每 500ms 刷新的全局快照同步触发 SessionEnd（3.5s 超时 + 原子互斥，与正常退出不重复触发）。三条 SessionEnd 路径（关窗 `other`、`/clear` `clear`、输入框可见时退出 `prompt_input_exit`）均携带真实 session_id，退出路径等待异步 hook 完成；另补齐 SubagentStart / SubagentStop 的真实 session_id 透传（原硬编码空串会把子 Agent 生命周期事件归到 "default" 会话）
- **SubAgentMiddleware:** 提供 `Agent` 工具，读取 `.claude/agents/{id}.md`，工具集过滤（tools 白名单 + disallowedTools 黑名单），防递归（始终排除 `Agent` 自身），返回格式含工具调用摘要；定义查找顺序为项目级 > 用户级（`~/.claude/agents/`）> 插件 > 内置（v0.6.199，#759）
- **SkillsMiddleware / SkillTool:** 冻结名称与限额描述列表；一级 `Skill` 工具返回完整指令、参数展开和 camelCase 元数据。Slash 直接追加 Human 元指令，`context: fork` 同步等待隔离子代理；会话独立激活记录支持 compact 恢复，详见 [技能机制](./domains/skill-trigger.md)
- **AgentsMdMiddleware:** `before_agent` 自动读取项目指引文件，prepend System prompt（单条 System 消息，会话内冻结）。加载模型对齐 dsh：**同目录候选全部加载并合并**（`AGENTS.md` / `CLAUDE.md` / `.claude/AGENTS.md`，基础层 → `.local` 覆盖层；trim 后内容相同去重），**跨目录从项目根（含 `.git`）逐级向下到 cwd 拼接**；每段带 provenance 头 `## <相对路径>`；单文件超 1 MiB 头 70%+尾 20% 截断（CJK 安全），渲染总量超 256 KiB 停止追加并标注；`@import` 对所有候选生效（深度 3 + 环检测；默认限**所属项目根内**，越界保留占位符；用户自有全局文件不受限）。**用户全局层为有序候选回退**：`~/.cc-code/AGENTS.md` → `~/.cc-code/CLAUDE.md` → `~/.claude/CLAUDE.md` → `~/.claude/AGENTS.md`，取首个存在且非空者；空文件不遮蔽后续候选。全局加载统一复用 `load_user_global_instruction`，与 Jev 安全门的 `read_global_content()` 为同一数据源（#402、#403）。
- **TodoMiddleware:** `after_tool` 解析 `TodoWrite` 结果，推送 Todo 状态到渲染 channel
- **Jev 规则提炼缓存:** 完整规则 JSON 按内容 SHA-256 存入 `~/.cc-code/jev/`，来源/项目/模型/提炼配置签名一致时跨进程复用；实际加载的规则变化后在新会话重新提炼，不增加界面提示。提炼单块超时按阶梯重试一次（首试后放宽 2 倍）且不触发对半递归拆分；仅完整成功的结果落盘。具体工具调用仍执行权限判定，详见 [HITL 设计](./domains/hitl-permissions.md#jev-规则提炼缓存)。磁盘缓存以 `to_vec_pretty` 多行 JSON 落盘（规则正文与 `<签名>.index.json` 索引同步美化），内容哈希对落盘字节计算，缓存仍能正常命中。
- **AskUserTool:** `AskUserQuestion` 工具（对齐参考实现的 AskUserQuestion），入参为 `questions` 数组（1–4 个），每题含 `question` 问题文字、`header` 短标签（≤12字）、`multi_select` 字段、`options`（每项含 `label` + `description`），始终允许自定义输入；oneshot channel 挂起等待用户输入
- **Token 追踪:** TokenTracker 累积追踪 input/output/cache tokens，ContextBudget 上下文窗口预算管理
- **Micro-compact:** 零 API 调用轻量压缩，可压缩工具白名单 + 时间衰减清除，图片/文档替换
- **Full Compact:** 自由 Markdown 摘要，工具对完整性保护，PTL 降级重试
- **LLM 重试:** RetryableLLM<L> 装饰器，指数退避+25%随机抖动，LlmRetrying 事件通知
- **rg CLI 双引擎文件搜索:** 优先外部 ripgrep 二进制，按 `PERI_RG_PATH` → exe 同级 `bin/rg(.exe)` → 系统 `PATH`（直接执行 `rg --version` 探测，不依赖 which/where）顺序解析，缓存到进程级 `OnceLock`；探测失败回退纯 Rust 引擎（grep + grep-regex crate，WalkParallel 多线程并行，15 秒超时）。Grep/Glob 双引擎输出格式对齐
- **MCP 中间件:** McpMiddleware 作为 MCP Client 连接外部服务器（stdio/HTTP），`mcp__{server}__{tool}` 动态工具注册，`mcp_read_resource` 资源读取工具，三层配置合并（全局 settings.json < 插件 plugin.json < 项目 .mcp.json，优先级递增），${VAR} 环境变量展开
- **MCP 运行时管理:** /mcp 面板（Browse/Tools/Resources 三视图），后台连接池初始化不阻塞 TUI，重连/删除服务器
- **MCP OAuth 2.0:** rmcp auth feature + AuthClient，Authorization Code + PKCE 流程，401 自动触发，Token 持久化 ~/.cc-code/oauth_tokens.json（0600），混合回调（本地 HTTP → TUI 手动粘贴），回调服务器注入并严格校验 rmcp 生成的 state 参数（CSRF 纵深防御）
- **工具名称对齐参考实现：** 12 个核心工具名称完全对齐（Read/Write/Edit/Glob/Grep/Bash/WebFetch/WebSearch/Agent/AskUserQuestion/TodoWrite/Skill），Grep 使用结构化接口，HITL 默认审批清单同步更新
- **RTK 代理双轨制:** 探测外部 `rtk` 二进制（where/which），对 git/cargo/npm/pnpm/npx/yarn/bun/docker/kubectl/pytest/python/php/go/dotnet/tsc/eslint/gh/find/grep/rg/ls/tree/cat/diff/curl/wget 等 27 类命令执行 `rtk rewrite` 重写以降低输出 Token；失败回退原始命令；过滤 RTK 宿主 stderr 噪音
- **output_filter 噪音清洗:** RTK git status 噪音清洗（`clean — nothing to commit`）；移除毒性通用折叠（避免吞并代码上下文），仅对 git status 做针对性剔除
- **Read 多模态读取:** 图片读取 + 魔数校验（Magic Bytes）防伪造图片；工具包装器透传 invoke_content 保证多模态内容不丢失
- **Bash 跨平台健壮性:** 多行命令（含字面换行）在 Windows 改走 Git Bash 避免 `cmd /C` 截断；管道超时保留已累积输出；后台 shell 任务在 fork 常驻子进程后不再永久挂起；移除 Git Bash fallback 重试标记（`[Retried with Git Bash]`）混入上下文

## TUI 界面（cc-tui）

- **图片附件落盘与可点击引用（Phase 1，v0.6.192）：** 发送时仅将仍有占位符的图片保存至 `temp_dir()/cc-code-images/<session_uuid>/<image_id>.<ext>`；写盘失败只记 `warn`，不阻断模型发送。正文下每图一行 `  ⎿  [Image #N]`，有保存路径且超链接开启时通过既有 OSC 8 通道显示引用行（取 text_soft、无人工下划线、不改前景色）；`DISABLE_HYPERLINKS=1` 或缺路径时显示不可点击的普通文本。新消息移除图片数量尾巴；清会话只清内存路径索引、不删文件，恢复会话不保证链接可用，不向模型注入路径。完整边界见 [CHANGELOG](../../CHANGELOG.md)。
- **Bash 执行阶段与实时输出（#675，v0.6.184）：** 排队、等待审批、运行与完成按调用精确推进；运行中的 stdout/stderr 以有界尾部显示，普通模式保留最后五个视觉行，Ctrl+O 展开保留尾部。`config.tool_output_streaming` 默认启用，项目配置可覆盖；完整行为与验证边界见 [PRD](../../docs/prd/bash-execution-status-alignment.md)。
- **紧凑启动与增长视口（#650，v0.6.171）：** 从 shell 输出下方启动，每次增长仅原生滚动必要行数并上移起点，内容或已占用高度长满整个宿主才锁定全屏；折叠、新建会话及放大终端均不缩回。#772 修正：未发送草稿只按 3 行输入基线参与增长，实际多行输入借用已有消息区，删空恢复布局，不因草稿假性全屏。欢迎卡作为纯 UI 前缀跨全屏保留并随消息滚动，不写入消息历史、系统提示词或消息缓存；包含真实版本、cwd、/help 与简短 /login，窄屏省略吉祥物。切屏前完成重排，切屏、resize 与首帧同步输出。详细交互见 [TUI-STYLE](../../TUI-STYLE.md#终端视口v06171-650)；原 #650 三平台 CI 全绿，#772 待 CI 验证。
- **详情渲染策略（#629，v0.6.169）：** `/config` 提供 `auto`（默认）/ `cache` / `virtual`，通过 `config.detail_render_strategy` 保存并即时通知渲染线程；项目配置可覆盖全局配置，缺省或未知值回退 auto。虚拟路径按视口渲染并限制离屏成品缓存，不承诺整个会话固定内存。

- **后台子任务抽屉（v0.6.130）：** 独立视口与指标、导航游标和当前任务分离、定向补充消息、全局审批、单任务/全部停止、五行与极矮终端布局；详细交互见 [TUI-STYLE](../../TUI-STYLE.md)。

- **文件提及候选（#463）:** 单独输入 `@` 立即弹面板，默认仅枚举当前目录并限制 15 项；关键词递归模糊搜索 200ms 防抖，保留最后一次输入，截断缓存或进入新目录时重新搜索。视口最多 10 行并适配可用高度，选中项始终可见；搜索中、空目录、无匹配提供双语提示，搜索期间阻止旧候选误补全及 Enter 误提交。

- **多会话历史:** `SqliteThreadStore` 持久化会话，`/resume` 面板浏览（j/k 导航，d 删除，Enter 打开，Esc 新建）
- **模型别名映射:** 四档别名 opus/sonnet/haiku/fable（`ALL_ALIASES: [&str; 4]`），`/model` 四 Tab 面板（`AliasTab::{Opus,Sonnet,Haiku,Fable}`），`/model <alias>` 快捷切换；模型切换快捷键已废弃 Alt+M/Ctrl+T，统一走 Ctrl+P/Alt+P 命令面板
- **TUI 命令:** 以 TUI 命令注册表（`command::default_registry()` 及应用层命令）为准，ACP 内置注册表另有 28 个命令；`/agents` 打开跨项目会话中心，`/agent-design` 选择 agent 定义，`/tasks` 入口移除；`/history`、`/resume`、`/clear`、`/help`、`/compact`、`/config`、`/cost`、`/context`、`/memory`、`/mcp`、`/recap`(away/catchup)、`/commit`(ci)、`/review`(pr)、`/export`(save)、`/gc`、`/lang`、`/init`、`/setup`、`/channel`(ch)、`/rename`、`/effort`、`/loop`、`/cron`、`/ps`（打开后台任务面板，Agent 运行中也可立即弹出）、`/doctor`、`/hooks`、`/plugin`、`/exit`(quit)、`/model`、`/theme`、`/login`、`/rewind`、`/streaming` 保持既有语义；Command trait 支持 alias 机制
- **Agent Command Center（#542）:** `/agents` 或满足门控的空草稿 `←` 打开全页根会话中心；跨 cwd、项目/状态/模型分组、五档状态过滤、10 条真分页、详情延迟读取、标题搜索、重命名及二次确认永久删除。Enter 恢复目标项目，`n` 在选中项目新建，`o` 当前项目历史；运行任务/审批未结束不切换。持久化模型/推理/分支快照与根会话终态，跨进程按保存状态降级。
- **Skills 补全:** 输入 `/` 触发 Skills 浮层，Tab 导航，Enter 补全为 `/skill-name`；发送后通过 `SkillPreloadMiddleware` 安全追加 Human 元指令并投影专属 Skill 卡片。折叠视图为工具头与单行加载结果，详情模式展示 Markdown，工具统计统一使用 Skill
- **候选浮层运行中交互（对齐参考实现的候选浮层行为）:** Agent 运行中候选存在时 `↑↓`/`Tab` 可导航浮层；`Enter` 仅已选中候选时补全（不提交），未选中走排队；`Esc` 分层关闭（先浮层后中断，`hint_dismissed_for` 抑制位文本不变不再弹、编辑后恢复）；`↓` 优先级浮层导航 > 后台 shell 聚焦 > @ 提及 > 滚动
- **Steer 前台命令宽限期:** 点「立即补充」时若有单条前台命令在跑，2 秒宽限（`STEER_FOREGROUND_GRACE`）后自动转后台并提示；多条并行不自动选择，重复 steer 不重置计时器
- **状态栏权限模式符号:** 权限模式标签前固定显示 `⏵⏵` 前缀（对齐参考实现的权限模式符号）
- **HITL 弹窗:** `ApprovalNeeded` 事件触发审批弹窗，展示工具名称和参数，支持 Approve / Edit / Reject / Respond；v0.6.206 起工具入参带 description 时标题下单独显示说明行，Write/Edit 改为文件级全文 diff 预览（新建文件走带行号与语法高亮的代码预览），超限、读不到文件或片段定位不到时回退片段 diff（#785）
- **AskUser 弹窗:** `AskUserBatch` 事件触发问答弹窗，支持批量问题，单选/多选
- **YOLO 模式:** `-y` 参数启动，自动 Approve 所有 HITL 请求（不影响 ask_user）
- **剪贴板图片粘贴:** `Ctrl+V` 读取 PNG 图片，Base64 编码为 Image ContentBlock，支持多张图片
- **渲染线程分离:** 独立渲染线程（`parking_lot::RwLock<RenderCache>` + `Notify` 驱动），零 sleep，与 Agent 执行线程解耦，按需重绘
- **Headless 测试模式:** `App::new_headless(w, h)` + `ratatui TestBackend`，与生产渲染管道完全一致，用于 CI 集成测试
- **弹窗滚动支持:** 所有面板（AskUser/Model/Agents/Thread）高度限制在屏幕 80%，内容超长可 ↑↓ 滚动
- **Bracketed Paste Mode:** `Ctrl+V` 粘贴多行文本，保留换行不触发 Enter 提交
- **Loading 输入缓冲:** Agent 运行中可继续输入，消息自动缓存，完成后合并发送
- **TODO 状态面板:** 输入框上方固定面板，颜色分类（InProgress 黄/Completed 暗灰/Pending 白）
- **Welcome Card:** 左上角紧凑欢迎卡，宽度上限 64 列、高度 5～7 行；显示实际版本、cwd、/help，无 Provider 时引导 /login，宽度与高度足够时显示原有半块吉祥物；长目录按显示列宽省略
- **Sticky Human Message Header:** 聊天区顶部固定显示最后一条 Human 消息（1-3 行截断），滚动时不随之移动，/clear 后消失，打开历史 Thread 自动恢复
- **配色系统（v1.1）:** 橙色仅保留最高优先级交互（命令输入框）；工具名三级分级（bash=ACCENT / 写操作=WARNING / 只读=MUTED）；配置面板边框 MUTED 降噪；HITL/AskUser 弹窗 WARNING
- **Setup Wizard:** 首次启动自动检测配置完整性，三步引导（Provider → API Key → Model Alias），支持 Anthropic/OpenAI Compatible，save_setup() 原子写回 settings.json
- **历史面板工作区过滤:** /resume 面板按 cwd 过滤 ThreadMeta，只显示当前工作区的对话，标题包含工作区路径
- **定时任务（cron）:** /loop 注册定时任务（cron 表达式 + prompt），/cron 面板管理（导航/删除/切换启用）；AI 通过 CronRegister/CronList/CronRemove 工具创建管理；内存任务表上限 20，TUI 重启后清空
- **子 Agent 模型切换:** agent.md 的 model 字段生效，LLM Factory 签名升级为 Fn(Option<&str>)，alias 解析在 TUI 层；SkillFrontmatter 增加 model 文档字段
- **工具颜色分层:** 工具名（颜色+BOLD）+ 参数（DarkGray），文件路径自动缩短
- **/compact 历史替换:** /compact 在原会话内以摘要替换历史消息（`build_compacted_messages`），不创建新 Thread
- **App 结构体拆分:** App 为单结构体（10 字段：core/session/services/ui/panels/input/runtime/langfuse/theme/input_pump），状态按 ChatSession / ServiceRegistry / UiState / PanelManager / InputState 分工承载
- **Spinner 动画:** 动词从 TODO activeForm 获取，Token 计数平滑递增动画，已用时间显示；完成态对齐参考实现风格（`✻ {verb} for {elapsed} · done {HH:MM}`）
- **智能折叠策略:** 只读工具默认折叠、写操作默认展开，SubAgent 步数超过 4 自动折叠
- **syntect 代码高亮:** markdown-highlight feature flag 控制，base16-ocean.dark 主题，单行代码块不高亮
- **鼠标文字选区:** TextSelection 模块管理拖拽状态，WrappedLineInfo 换行映射，Ctrl+C 优先级链（选区复制>中断>退出），REVERSED 反色高亮
- **全局屏幕选区:** ScreenSelection 基于渲染 Buffer 覆盖面板/状态栏/sticky header/bg agent bar/空白区域，与消息区 TextSelection 跨区域衔接；ScreenSnapshot 在 `terminal.draw()` 后克隆 Buffer 作为文本源；双击选整行（消息区用 TextSelection 纯文本，其他区域用 ScreenSelection 整屏行）；松开鼠标自动复制 + "已复制 N 个字符" toast
- **Skills / 触发:** Skills 触发键从 # 统一到 / 前缀，提示浮层合并命令组+Skills 组，命令优先
- **两档权限模式:** `auto`（默认）/ `bypass`，Shift+Tab 在 Auto ↔ Bypass 间循环，未知取值回退 auto，状态栏实时显示
- **Background Agent:** 子 Agent 恒后台执行（`run_in_background` 参数已退场），最多 3 并发，完成经 `peri/subagent_lifecycle` 通知回流、主时间线只留树形启动摘要与完成行并自动 continuation；底栏抽屉管理（游标/视口/单停与全停），不再用 ToolBlock 展示与状态栏 `[BG: N]` 指示器
- **输入泵（InputPump）:** 独立后台输入泵隔离 Windows 控制台重入/阻塞风险，安全启用鼠标悬停（`MouseEventKind::Moved`）；有界事件批处理（EventReader）合并高频滚轮事件防掉帧；滚动条滑块相对拖拽消除点击漂移
- **执行中消息队列 + 按轮次 steering:** agent 执行期间输入的消息进入待发队列，按轮次以增量 StateSnapshot 注入执行循环，不打断当前执行
- **粘贴本机图片路径:** 含 `file://` URL 或引号包裹绝对路径自动转附件；Alt+V 快捷键粘贴图片；Windows 8.3 短文件名波浪号不再误拦截
- **Markdown 超链接点击:** 点击超链接打开默认浏览器（保留 dest_url + 字符级命中区，跨平台）
- **悬挂缩进（hanging indent）:** 列表项/引用块/长消息气泡续行对齐；工具行超长折行续行缩进；消息区长行折行悬挂缩进
- **动态终端标题（Status Surface）:** 对齐 OpenAI Codex 规范，按生命周期状态（Working/Thinking/ActionRequired/Idle）更新终端标题，获取会话主题后追加项目名
- **状态栏 Prompt Cache 命中率:** 替代瞬时 CPU 指标（分级配色），保留 MEM
- **终端宽度变化 Markdown 不重复渲染:** 传统控制台歧义字符双列残影修复
- **Windows 子进程控制台隔离:** `CREATE_NO_WINDOW` 消除 PHP 等子进程代码页切换触发的全屏闪屏
- **i18n 补全:** 附件栏标题与 Del 提示接入 i18n
- **启动语言检测与 CLI/TUI 双语补全:** 保存语言优先，未配置时检测 locale；CLI 帮助、命令反馈、状态面板及 `/gc` 诊断使用 Fluent，`/help` 描述随语言切换刷新（#314、#315、#316）
- **后台 shell 通知 i18n + 进程级语言注册表:** 完成/超时/取消/终止/等待输入通知的展示文案改走 `LcRegistry::tr()`；新增 `i18n::init_global`/`global` 进程级注册表（`LcRegistry: Sync`，`FluentBundle` 用 concurrent 变体），供无 App 上下文的静态 `MessageViewModel` 构造路径读取当前语言，启动与 `/lang` 切换时同步
- **`/gc` 诊断分平台语义 + view_messages 估算:** `active`/`mapped`/`retained` 按 `alloc_name` 条件化标注（jemalloc 真实活跃页 vs mimalloc `page_committed` 历史高水位/`reserved` 虚拟地址），Windows 不再报虚假碎片；新增 `estimate_view_messages_heap()` 遍历 `MessageViewModel` 全部变体（此前 `origin_messages`+`completed` 漏掉 `view_messages`，是「未识别」大头）
- **详细模式长命令运行状态刷新修复:** 详细模式超长命令 header 折成多行后，tick 增量刷新改为**按内容定位** `Running…` 状态行（新增 `is_shell_running_status_line`），不再写死 `lines[1]`、不再用固定行数判「状态行未渲染」，消除「命令续行被覆盖 + 秒数冻结 + 两处 Running… 时间不一致」
- **详细模式工具头前缀稳定（#343）:** 运行中指示器在 `●`/空格间闪烁时，详细模式 header 折行不再 trim 首段行首空白（`push_wrapped_line_keep_first_lead`），`● Bash(` 前缀不再随闪烁左移
- **`/gc` RSS 变化符号修正（#346）:** 方向语义统一为 `after - before`（增加为正），`fmt_signed_delta()` 输出 `+N`/`-N`/`±0`；`allocated - RSS` 带符号并按方向分支文案
- **`/gc` Markdown 缓存内存统计（#351）:** `MarkdownCache::stats()` 单次加锁快照，按 `capacity()` 估算堆占用（总量 / 平均 / 最大条目、行 / Span 数），纳入「已知合计」并标注「非 RSS」
- **Markdown 缓存字节预算（#594）:** `MarkdownCache` 在 1024 条上限之外追加 8 MB 字节预算，复用 `cc-widgets/src/byte_cache.rs` 的 `ByteBudgetCache`（与 diff / diff 渲染缓存同一实现），写入后按 LRU 弹出到预算内，单条自身超预算立即淘汰，条数上限降级为兜底；`/gc` 增加「字节预算」读数。同时修复 `ByteBudgetCache` 在**条数上限触发淘汰**时不扣减字节、导致字节统计持续虚高的缺陷
- **排队消息快捷键 Alt+S / Alt+X（#347）:** 「立即发送」由 `Ctrl+S` 迁移为 `Alt+S`、删除由 `Ctrl+X` 迁移为 `Alt+X`（Windows conhost 会截走 `Ctrl+S`）
- **状态栏第二行「最近工具」实时摘要（#390）:** `AgentComm.recent_tools`（`VecDeque`，容量 2）驱动，最新在最左、第 3 条挤掉最老；条目显示时长取 `max(实际执行时长, 300ms)`（快工具垫到 300ms，慢工具结束即刻消失），工具结束只让右侧聚合计数 `✓ Name ×N` +1；摘要覆盖全部工具（AskUserQuestion → 首个问题 / Agent → description / TodoWrite → `N 项任务` / 其余 → 第一个非空字符串字段），字符上限 30、沿用路径语义截断；`◐`（运行）/`✓`（停留期）配色统一；第二行强制单行不折行，超宽按 `unicode-width` 在行尾 `…` 收口
- **主 Agent 可按 `task_id` 取消后台子 Agent（`Agent(action="stop")`）:** `Agent` 工具新增 `action`（`spawn` 默认 / `stop`）与 `task_id`；`action=stop` 按 `task_id` 精确取消（**拒绝任务名/描述模糊匹配**，兄弟任务描述相似易停错），底层复用 `BackgroundTaskRegistry::cancel()`（触发 `CancellationToken`，执行器正常终态收尾，不 abort）。`prompt` 从 schema 的 `required` 移除（`action=stop` 无需 prompt，硬编码必填会让模型编造），必填性下沉到 `invoke` 按 `action` 分支校验。停止结果回给模型含任务摘要的可读确认。停止终态独立成行并区分来源（`was stopped by main Agent` / `stopped by user`），抽屉行改用**红色实心 `●`**（`#FF6B80`）+ 红色 `stopped by …` 取代橙色 `✗ stopped`。新增等待态计数行 `✻ Waiting for N background agents to finish`（单复数随数量，归零移除）
- **SubAgent 启动摘要 Prompt 卡片展开（Ctrl+O）:** 后台子任务启动摘要默认折叠为极简树形（`• N background agents launched` + `├──`/`└──` 任务）；按 `Ctrl+O` 进入详细模式后切换为方案 B 经典卡片（`● Agent(任务预览)` + `  └ Backgrounded agent` + `  └ Prompt:` + 指令全文，6 列悬挂缩进折行），再次 `Ctrl+O` 收回。`AgentEvent::SubagentStarted` 新增 `prompt: Option<String>` 携带指令**全文**（此前仅 `task_preview` 40 字摘要），`BackgroundAgentSession.prompt` 持久该会话指令，`MessageViewModel::SystemNote` 新增 `detail_content`（参与 `PartialEq`/`Hash`）由 `detail_mode` 单选渲染；卡片 `●` 统一为系统大实心圆点 `U+25CF`（对齐 `● Bash(...)`），不得退化为小圆点 `•`
- **Sticky Header 已禁用:** v0.6.71 起高度固定 0（保留实现，不再展示）
- **权限模式循环:** Auto ↔ Bypass（未知值回退 auto）
- **PageUp/PageDown 半页滚动:** 20 行（输入框为空时生效）
- **「回到最新」浮层按钮（#448）:** 消息区滚动离开底部且内容溢出时，在消息区最后一行居中显示「↓ 回到最新」浮层按钮；点击按钮或按 `End`（输入框为空）回到底部即消失，离开底部期间有新内容时文案切换为「↓ 有新活动」；覆盖式浮层，复用 `scroll_follow`/`scrollbar_max_offset` 滚动状态，不改变布局与滚动位置，面板/弹窗激活时隐藏且不拦截指针

- **主题系统（`/theme`，v0.6.140）:** 全部 UI 颜色抽成统一调色板（69 个语义字段 + cc-code 扩展字段），7 套调色板 + `auto` 跟随模式（按终端颜色能力与背景明暗解析成下述某套）：`default`（cc-code 现行配色）+ `dark`/`light`/`light-daltonized`/`dark-daltonized`/`light-ansi`/`dark-ansi`（后六套名称与全部字段与参考源码的 `src/utils/theme.ts` **逐值一致**，`palettes_parity_test.rs` 逐字段断言锁定防漂移）。`/theme` 面板光标移动实时预览、Enter 确认、Esc 还原，`/theme <name>` 直接切换，选择持久化到配置 `theme` 字段、重启恢复；cc-widgets 的 markdown/diff 渲染同步注入当前调色板（语法高亮随明暗族切换、渲染缓存 key 含主题指纹）。渲染层统一 `theme::cur().<字段>`，禁止裸 `Color::Rgb`；**状态栏为例外**（见下）
- **Spinner 变色与 tips 调度对齐参考实现（v0.6.140 / v0.6.141）:** 动词改走成对流光（底色 + 流光色，±1 列窄带、50ms 步进、自左向右扫过），无新 token 超 10s 后流光熄火、动词与字形向 error 色平滑插值（停滞转红），删除自研「时间热度四档」；tips 由按渲染 tick 无差别轮换改为带会话冷却的调度模型——每轮挑选一条并**扣留**、轮结束才发布（每轮最多一条、轮内不换），取「距上次展示最久」者，历史持久化 `~/.cc-code/spinner-tips.json`，新增 `spinner_tips_enabled` / `spinner_tips_override` 配置（#510、#511）
- **状态栏配色固定、不参与主题化（v0.6.143，#523）:** 状态栏配色是打磨定稿的固定值，不随 `/theme` 切换——`status_bar.rs` 内经模块内固定配色常量 `sb` 输出，渲染结果与主题系统引入之前逐字节一致；`/theme` 面板补齐 session 面板键盘路由（`↑`/`↓`/`Esc` 生效，v0.6.142，#521）
- **文件查询类 Bash 并入只读工具组（v0.6.195）:** Bash 新增可选参数 `file_inspect`（模型声明"只读查看文件"，单条也置位；纯展示，不参与执行/权限/审批/遥测）。带标记且已落定（`Finished`、未转后台、无后台启动标记、未显式设 `timeout`）的 Bash 与 `Read`/`Grep`/`Glob` 合组：普通模式折叠摘要新增 `ran N shell command(s)`（顺序 read → listed → searched → ran shell → recalled → updated），单条同样折叠；`Ctrl+O` 逐条展开并复用单条 Bash 的输出处理（退出码错误头、stderr 分段、非零退出错误色、空输出 `(No output)`），折叠态保留错误摘要。运行中/已转后台的 Bash 不进组，实时输出与 `Ctrl+B` 不受影响。同批修复聚合把带耗时的思考气泡当作空思考跳过、导致思考正文与耗时丢失的问题（`is_reasoning_only` 只跳过真正空的）

- **只读 Bash 词法判定与折叠组一等公民（v0.6.197）:** 判定收敛到命令词法 `is_readonly_shell_command`（v0.6.265 起实现下沉到 `cc-agent/src/shell/readonly.rs`，`cc-tui-core` 仅保留 re-export 供展示侧调用）（读取器白名单 `ls`/`cat`/`head`/`tail`/`find`/`wc`/`rg`/`grep`/`tree`/`du`/`stat`/`file`/`less`/`more`/`strings`/`jq`/`awk`/`cut`/`sort`/`uniq`/`tr` + 中性命令 + 拒绝写盘，`sed` 仅 `-n`），`Bash` 的 `file_inspect` 参数已从工具 schema 删除，历史残留字段一律忽略。词法补齐三处：`2>/dev/null` / `2>&1` / `>&2` 不算写盘；引号内的 `;` `|` `&&` `>` 不再当分隔符或重定向；命令替换与反引号仍保守判为非只读。折叠组成为一等公民（组自带思考行、跨轮重建不丢）。思考行收敛为一行摘要：活跃 `∴ Thinking for Ns, <现在时>…`（符号常显、每秒刷新）、冻结 `  Thought for Ns, <过去时> (ctrl+o to expand)`（无符号），完整正文走 `Ctrl+O`，`THINKING_RETAIN_MS` 与留存渲染态下线。自动 compact 不再清空消息区（保留历史 + 重建起点前移），视图历史上限 1 万条（超出裁剪并插省略提示）。卡住检测指纹由「前 200 字符」改为全文 64 位摘要，修复连续 `Edit` 被误判后跳过工具执行的问题；`interrupt()` 冻结当前思考段，修掉取消后思考行继续走秒

- **聚合组内失败 Bash 显示命令头（v0.6.269，#1005）:** 普通模式下同一轮聚合的工具组里，失败的 Bash 调用只渲染一段截断错误摘要、没有命令本身，用户无从判断是哪条命令失败，只能切到详细模式。现抽出 `push_group_entry_header` 与 `render_group_bash_body` 让详细模式与折叠态共用同一份命令头与正文口径，折叠态下失败的 Bash 先渲染 `● Bash(<command>)` 再接原有截断摘要（前 3 行 + `… +N lines (ctrl+o to expand)`）；折叠态高度估算同步计入命令头与摘要行，避免首帧高度偏小导致滚动定位漂移。成功的调用仍按原样折叠，详细模式行为不变
- **思考耗时以 LLM 计时优先（v0.6.252，#945, #946）:** `Thought for 1s` 与实际思考时长严重不符（真机近 1 分钟仅显示 1s）——流式适配器已回填含 TTFT 的 `duration_ms`，但 TUI 每次装配都用组计时覆盖该值，而组计时锚点为首条 reasoning chunk 到达（不含 TTFT）。现 `fill_group_duration` 统一填充：LLM 值优先、组计时仅兜底（下限 1s）；组语义重构为「一段思考的墙钟」（工具运行时长不计入、每轮清零跨轮残留），多行冻结思考行按 `finished_groups` 逐行消费修复 off-by-one 错位
- **面板关闭后视口回缩（v0.6.254，#951, #952）:** 打开命令面板再取消后页面留白——欢迎卡片在顶部、输入框/状态栏浮在屏幕中部、下方大片空白。根因是 growing 视口的高度记忆 `allocated_rows` 只增不减，面板撑出的高度被永久记住，关闭后 `required_rows` 回落被 `max` 吞掉。现面板等开关型瞬时块只影响当帧高度、不进持久记忆，关闭立即回收；收缩释放行显式清理，消息内容等持久增长仍保持只增不减；输入草稿自 #772 起不参与增长

## ACP 服务层（cc-acp）

- **ACP 传输抽象:** `AcpTransport` trait 统一 MpscTransport（TUI 内存通道）和 StdioTransport（IDE stdio），JSON-RPC 2.0 协议
- **Session 管理:** SessionManager 管理会话生命周期（new/prompt/compact/set_model/set_mode/cancel）
- **Agent 构建:** `build_agent()` 统一组装 Middleware Chain + LLM + 工具，TUI 和 stdio 共用
- **事件映射:** `ExecutorEvent` → `SessionNotification` 标准 ACP 通知转换
- **HITL/AskUser 桥接:** `AcpTransportBroker` 通过 ACP RPC（`RequestPermission` + `elicitation/create`）替代 oneshot channel
- **上下文压缩:** auto-compact 触发 + micro/full compact 执行 + resubmit 全部在 executor 循环内完成；手动 `/compact` 与自动 compact 共用 `build_compacted_messages` 生成摘要消息（`<system-reminder>` 包裹，TUI 折叠为一行提示）

## 独立基础库

- **Widget 组件库（cc-widgets）:** 16 个公开模块（bordered_panel、checkbox_group、diff、file_tree、form、input_field、list、markdown、message_block、radio_group、scrollable_area、selectable_list、spinner、tab_bar、text_selection、tool_call），仅依赖 ratatui + pulldown-cmark（markdown / markdown-highlight 为可选 feature）；`ByteBudgetCache` 供 diff 与 markdown 渲染共用字节预算。无 workspace 内部依赖
- **diff 渲染排版三次收敛（v0.6.256 / v0.6.259 / v0.6.260）:** ① 加减号 marker 移到行号右侧（`+ 25 内容` → `  25 + 内容`），gutter 宽度 n+3 → n+4、续行占位改为行号位填空格；② 修正 marker 移动时多出的一个空格致行号/marker/正文整体右移一列、长行提前折行的问题，gutter 恢复 n+3、续行 marker 前占位 n+2，并补固定第 17 行列位置与折行边界测试（40 列宽下 35 字符完整显示、36 字符才折行）；③ 上下文行号由最暗灰阶 `theme.dim()`（#505050）提亮为 `theme.muted()`（#999999，与 tip 提示同色阶），仅影响 diff 上下文行，`+/-` 行 decoration 色不动。影响所有 diff 渲染路径（工具结果详情、HITL 审批弹窗）
- **LSP 客户端库（cc-lsp）:** `LspClient` / `LspServerPool`（多服务器连接池）、`DiagnosticsRegistry`（诊断收集与汇总）、JSON-RPC 编解码与协议层、全局 + 插件 LSP 配置加载；被 `cc-middlewares` 的 `LspMiddleware` / `LspTool` 消费。无 workspace 内部依赖
- **遥测客户端（langfuse-client）:** `LangfuseClient` + `Batcher`（批量上报与背压）、`IngestionEvent` / `ObservationBody` / `SpanBody` 与 OTLP 类型转换、错误类型；由 `LANGFUSE_PUBLIC_KEY` / `LANGFUSE_SECRET_KEY` / `LANGFUSE_BASE_URL` 驱动，公钥或私钥缺一即禁用遥测。无 workspace 内部依赖

## 基础设施

- **SQLite 线程持久化:** sqlx SqlitePool(max=5) 原生异步连接池，WAL 模式，`append_messages` 事务保证 crash-safe，`StateSnapshot` 事件驱动增量写入，数据库文件 + WAL/SHM 应用 0o600 权限（Unix），grandparent 目录权限校验
- **会话回顾/总结持久化:** ThreadMeta 新增 `latest_recap` 与 `last_task_summary`（TaskSummary { verb, elapsed_ms, done_at }）字段并落库，`-c`/`-r` 恢复会话时 recap 与任务完成总结行不再丢失；SQLite 幂等 ALTER TABLE 迁移；ThreadStore 提供 update_latest_recap / update_last_task_summary 单列 UPDATE（避免重写 ~1MB cached_context）
- **结构化日志:** `RUST_LOG` 级别控制，`RUST_LOG_FORMAT=json` 切换 JSON 格式
- **配置持久化:** `~/.cc-code/settings.json` 存储 Provider/Model 配置，`AppConfig` 统一读写，`env` 字段替代 .env 文件注入环境变量
- **日志路径迁移:** 日志默认写入 `~/.cc-code/logs`
- **应用主目录统一 `~/.cc-code`（#349，破坏性）:** 移除对改名前 `~/.peri` 的逐文件回退，`app_home::app_data_path`/`app_data_dir` 一律返回 `~/.cc-code/...`；仅在 `~/.peri` 存在的数据文件不再被读取（需手动迁移）。`hitl` 敏感目录名单中的 `.peri` 保留（安全用途）
- **npm 安装分发:** 发布时由构建脚本组装各平台二进制包，`npm/bin/cc-code` 启动器按当前平台分发到对应平台包；Windows 下全局入口直起 `bin/cc-code.exe`（去掉 node 中间层，修复 Ctrl+C 直接退出）

- **LLM baseUrl 自动容错清洗与友好提示（v0.6.227，#856, #857）:** 自动补齐协议头（本地与内网补 `http://`，公网域名补 `https://`），剥除 Anthropic 适配器冗余的 `/v1` 与 `/messages` 路径防止 404；为 OpenAI 根域名补齐 `/v1` 并剥除 `/chat/completions`；转译底层 reqwest 错误为明确排查提示，多层入口防呆
- **Setup 向导外部配置自动探测与一键迁移（v0.6.226，#852）:** 废弃强制语言选择弹窗，自动继承系统语言并检测已有配置
- **Compact 压缩后快照锚点重置修复（v0.6.226，#850）:** 修复 compact 后向模型重复发送已压缩历史问题
- **终端色彩能力探测与输出降级（v0.6.223，#601）:** 自动根据 `NO_COLOR` 与终端环境变量四档降级色深输出

---
*最后更新: 2026-11-11 — 补 cc-compress 分组与独立基础库分组（cc-widgets / cc-lsp / langfuse-client），补 v0.6.251~v0.6.270 功能条目，修正只读 Bash 词法判定的归属描述；此前：2026-11-04 — 补充 cc-tools / cc-terminal / cc-tui-core 拆分后的 crate 分组；2026-11-03 — LLM baseUrl 自动容错清洗与友好提示（v0.6.227，#856, #857）*
