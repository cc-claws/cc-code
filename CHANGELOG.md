# Changelog

Perihelion Agent 版本变更记录。

---

## v0.6.120 — 2026-10-10

### Fixes

- **Windows 传统 CMD 控制码刷屏与星形缺字修复（#415）**：传统 CMD 控制台（conhost）启动后 stdout 的 VT（虚拟终端）模式可能丢失，导致 crossterm 缓存的 ANSI 能力与实际模式不一致——输出退化为控制码刷屏、星形 spinner 缺字（缺字仍占一列却无法被现有列宽探针识别）。现于输出前补齐 VT 必要位并记录恢复、清屏重绘（保留其他控制台模式），`terminal_title_ops`/`terminal_title`/`panel_memory` 保护标题输出与编辑器返回路径，`terminal_backend` 区分原生控制台与 ConPTY 使用不同 ASCII 动画帧并保持逻辑帧原文。同批修复无控制台/重定向环境（如 CI）下 VT 恢复失败提前返回、导致 `last_terminal_title` 缓存未记录与单元测试断言失败的问题（改为先记录缓存状态再执行平台相关 VT 恢复）。（#415）

### Docs

- **活文档与代码漂移校准（#417）**：对照最新代码校准 TUI 命令数（29）、系统提示词段落文件数（14）、核心工具数（11）、cc-widgets 通用组件数（15）与 ReAct 迭代上限（核心默认 10 / 构建 500）；修复 `docs/README.md` 中 7 处指向不存在文件的断链，并在 README 仓库结构树补齐 `TUI-STYLE.md`、`DEVLOG.md`（中英对齐）。（#417）

## v0.6.119 — 2026-10-10

### Fixes

- **手动 `/compact` 摘要补齐 `<system-reminder>` 包裹，界面恢复折叠（#413）**：手动 `/compact` 与自动 compact 各自复制了一份摘要格式化逻辑，手动路径漏掉了 `<system-reminder>` 包裹。而 TUI 依赖该标签决定渲染方式——有标签时摘要折叠成一行「📋 上下文已压缩」，无标签时则被当作普通用户消息把整段摘要铺在界面上；该消息持久化后，load/resume 同样以未折叠形态重现。现将摘要构造抽取为公共函数 `build_compacted_messages`（统一负责 `<system-reminder>` 包裹 + Human 消息构造 + 拼接 re_inject 消息），自动路径（`cc-middlewares/src/compact_middleware.rs`）与手动路径（`cc-acp/src/session/command/compact.rs`）共用同一实现，消除两条路径的格式漂移，并修正「与 auto-compact 路径对齐」的误导性注释。补测试断言两条路径产出的摘要消息结构一致。

## v0.6.118 — 2026-10-10

### Fixes

- **Jev 规则磁盘缓存改为多行 JSON 输出，提升可读性（#412）**：Jev 安全规则的磁盘持久化缓存（`~/.cc-code/jev/peri-<项目哈希>/<规则哈希>.json`）此前用 `serde_json::to_vec` 落盘为**紧凑单行** JSON——规则正文动辄数千字符全挤在一行，用户直接打开查看规则时几乎无法阅读。现将 `write()` 的序列化改为 `serde_json::to_vec_pretty`（2 空格缩进），规则正文文件与 `<签名>.index.json` 索引同步美化为多行。内容哈希对**落盘字节**计算（`digest(&bytes)`），格式化前后各自自洽，缓存仍能正常命中；存量旧缓存因文件名（哈希）与格式化后不一致会 miss 一次并重新提炼，无功能影响。仅影响磁盘落盘格式，内存缓存与 Jev 判定逻辑不变。

## v0.6.117 — 2026-10-10

### Fixes

- **Windows 全局入口直起 exe，修复 npm 安装下 Ctrl+C 直接退出 TUI（#410、#411）**：Windows Terminal（ConPTY）下敲 `cc-code`，npm 全局 wrapper（`<prefix>/cc-code.cmd|.ps1`）由 npm 依据 `package.json` 的 bin（指向 node 脚本 `bin/cc-code`）自动生成，再经 node 以 `execFileSync` 拉起真正的 `cc-code.exe`。Ctrl+C 的 `CTRL_C_EVENT` 会广播给整个控制台进程组（node + `cc-code.exe`），`cc-code.exe` 内部虽用 `SetConsoleCtrlHandler` 拦截了信号（返回 1 阻止默认终止，注入 KeyEvent 走应用层中断 / 空闲双击退出），但 node 父进程没有 handler、按默认行为被直接终止，`execFileSync` 的同步等待随之崩断，把 `cc-code.exe` 一并拖死——表现为 agent 任意状态下按 Ctrl+C 都直接退出，日志在 streaming 中戛然而止、无任何清理记录。现于 `install.js` 的 Windows postinstall 阶段新增 `overwriteNpmGlobalWrapper()`，把 npm 全局入口改写为**直起 `bin/cc-code.exe`**（相对 prefix 根引用，避免硬编码绝对路径），去掉 node 中间层后进程组内只剩 `cc-code.exe`，`SetConsoleCtrlHandler` 正常生效。改写为防御性实现：仅在 exe 存在且能定位到 npm 全局 wrapper 时才覆盖，任何失败都不阻塞安装（此时 exe 同目录 `bin/cc-code.cmd|.ps1` 仍可直起）；`bin/cc-code` node 脚本与非 Windows 路径行为不变。已实机验证：全局入口改为直起 exe 后 Ctrl+C 不再直接退出，TUI 恢复「Ctrl+C 中断 agent / 空闲双击退出」的设计行为。

## v0.6.116 — 2026-10-10

### Fixes

- **Schema 熔断改为滑动窗口计数，修复交替错配逃逸（#404、#408）**：v0.6.110 引入的 Schema 熔断器在**真实失败模式**下从不触发——模型常在多个工具之间交替把参数配错（配错一次 `WebSearch`、配错一次 `WebFetch`、再配错一次 `WebSearch`），且失败之间夹杂成功调用。原因有二：其一，计数口径是「连续失败」，而 `reset()` 在**任意工具**成功时都会清空跨工具聚合计数，聚合阈值 3 因此永远凑不满，单工具阈值 2 也因工具交替而停在 1；其二，`TOOL_SIGNATURE_HINTS` 只有单向条目（表内没有任何一项指向 `WebSearch`），导致把 `query`/`num_results` 打给别的工具时静默无提示，模型在一侧收到「改用 WebFetch」、在另一侧却收不到任何反馈，被提示表**来回推入震荡**。现熔断计数改为**滑动窗口**口径（窗口容量 6，单工具与跨工具计数均自窗口派生，不再单独维护计数器）：`reset()` 只复位该工具的退避状态、**不再清空窗口**，成功调用同样占用窗口位以推动旧失败自然淘汰；同时补全反向条目（`query`/`num_results` → `WebSearch`、`old_string`+`new_string` → `Edit`、`content`+`file_path` → `Write`）。实测以真实 trace 的原始调用序列逐条喂入熔断器，此前 3 次 schema 失败注入 0 次提示，修复后触发 `PerTool { tool_name: "WebSearch", count: 2 }`。

## v0.6.115 — 2026-10-10

### Fixes

- **Jev 规则提炼超时按阶梯重试并改用低思考档，修复磁盘缓存从未落盘（#405、#406）**：v0.6.114 引入的规则提炼持久化缓存（#398、#399）在真实环境里**从未生效**——`~/.cc-code/jev/` 目录始终不存在。三个根因叠加，使提炼结果恒为「不完整」而永远进不了「仅持久化完整非空结果」的分支：其一，提炼调用继承了会话的扩展思考档（`effort=xhigh`），实测单块耗时被拖到 18–73 秒；其二，超时被当作「块太大」去做**对半递归拆分**，而超时的成因是网关慢、并非输入过大，切小只是拿同一时限再赌一次，还把一次等待放大成 7 次（实测单个加载器的一次 `ensure_loaded` 内触发 14 次调用，突变测试测得旧逻辑实际 28 次）；其三，提炼时限默认 15 秒低于实测单块耗时。现提炼改用独立低思考档（`effort=low`、`budget_tokens=1024`，不继承会话配置），实测单块耗时降至 10–15 秒、输出 token 由 6k 级压到 2.5k 级且方差收敛；超时改为**时限阶梯**——首试时限后放宽 2 倍再试一次（默认 15s → 30s），并新增失败原因区分，**超时不再进入对半递归**，该能力仅保留给「回复不可解析 / 为空」这类切小确实有效的失败。提炼时限默认值同步上调至 30 秒。若将来把提炼思考档改为可配置，**必须**将其纳入缓存签名，否则会命中旧档位算出的结果（代码内已留注记）。

## v0.6.114 — 2026-10-10

### Features

- **Jev 安全规则提炼磁盘持久化缓存（#398、#399）**：Auto 模式首次门控将项目、个人、全局规则与 hook 的提炼结果按完整来源签名进行 SHA-256 内容寻址持久化缓存，存储至 `~/.cc-code/jev/peri-<项目哈希>/<规则哈希>.json`，应用重启后跨会话命中缓存即可直接跳过耗时的规则提炼 LLM 请求；来源签名严格覆盖规范化项目路径、展开引用后的完整规则正文、模型 provider/id 与提示词参数；采用文件系统原子写入与所有者权限保护，读取校验版本与内容哈希，损坏自动优雅降级重新提炼。
- **排队消息快捷键优化为 Ctrl+Enter 发送与 Ctrl+X 删除（#400、#401）**：排队消息的操作快捷键优化为 `Ctrl+Enter` 立即发送、`Ctrl+X` 移除排队消息，解除与其他输入态快捷键的潜在冲突，提供更顺畅的输入流水线交互。

### Fixes

- **用户全局指引多候选回退与 Jev 数据源归一（#402、#403）**：修复此前 System Prompt 指引注入中用户全局层硬编码单一文件 `~/.cc-code/AGENTS.md`，导致配置在 `~/.claude/CLAUDE.md` 的个人全局规则无法注入上下文的缺陷；引入有序候选回退列表（`~/.cc-code/AGENTS.md` -> `~/.cc-code/CLAUDE.md` -> `~/.claude/CLAUDE.md` -> `~/.claude/AGENTS.md`），空文件不遮蔽后续候选，自动穿透至有效规则；抽象统一方法 `load_user_global_instruction`，使 Prompt 指引注入与 Jev 安全门规则提炼（`read_global_content()`）彻底对齐同一加载管线。

## v0.6.113 — 2026-10-10

### Fixes

- **交互弹窗 Ctrl+C 中断保护与审批取消及时响应（#396、#397）**：修复提问（Questions）和工具审批（Approval）弹窗此前单次按下 `Ctrl+C` 直接退出 TUI 的问题，改为向 Agent 发送取消中断（`session/cancel`）并解除 transport 等待，保留 TUI 会话且清空退出确认状态；修复配置向导退出缺少防抖的问题，复用普通模式 100ms~2s 双击退出逻辑；在批量 `before_tool` 工具等待中增加取消优先分支，使长时间工具审批在用户中断时可立即响应，保持工具延迟写入约束；后续或迟到的交互请求自动标记取消，避免二次弹出孤立弹窗。
- **PHP 语法高亮内存缩减与空闲工作线程堆内存自动回收（#394、#395）**：将 syntect 代码高亮引擎从 `default-fancy` 切换为静态编译的 `default-onig`（Oniguruma），解决 PHP 等语法正则在 fancy 引擎下引发的大量常驻内存开销（独立测试中单段 PHP RSS 从 89.6 MiB 降至 15.6 MiB，整段会话文本从 118.4 MiB 降至 17.5 MiB）；在 Windows 平台通过 `mi_option_set` 注入有效的 mimalloc purge 参数，并在 Tokio runtime 的 `on_thread_park` 回调中接入工作线程空闲堆回收机制（每线程至多 1s 回收一次）；修正 `/gc` 命令与内存诊断在 resident、metadata、RSS 差额及消息数统计上的口径。

## v0.6.112 — 2026-10-10

### Features

- **消息区底部 Todo 列表默认最多展示 5 项并折叠溢出统计（#392、#393）**：在消息区底部 Spinner 关联的 Todo 列表区域引入 `MAX_VISIBLE_TODOS = 5` 上限。当存在大量待办任务（例如 8~10 个）时，不再全量平铺占满垂直视口，仅渲染前 5 项，超出部分折叠为一行优雅统计（例如 `    ... +3 pending`），并自适应统计 `pending` / `completed` 数量；同步重构 `spinner_extra_count` 及 `todo_render_line_count`，使多任务时的行数计算从无界增长收敛为最多 6 行（5 任务 + 1 统计行），保障视口裁剪、滚动条与实际渲染行数精确对齐。

## v0.6.111 — 2026-10-09

### Features

- **状态栏第二行改为「最近工具」实时摘要（#390）**：原来第二行的运行中工具段只在执行期间存在，且受三个限制——`poll_agent` 每帧把 ACP 通知一次 drain 干净，Read/Glob 这类毫秒级工具的 ToolStart + ToolEnd 常落在同一帧，`◐ Read : x.rs` **一帧都渲染不到**；摘要来源是 11 个工具的白名单，Agent / TodoWrite / AskUserQuestion / MCP 只有裸名字；顺序是「老在左」，最新开始的排在最右。现改为由 `AgentComm.recent_tools` 驱动的一行流水：最新在最左、最多留 2 条（第 3 条挤掉最老的），条目显示时长取 `max(实际执行时长, 300ms)`——快工具垫到 300ms 后消失，慢工具执行结束即刻消失，工具结束只让右侧聚合计数 `✓ Name ×N` +1、不再生成带摘要的完成条目。摘要覆盖扩展到全部工具（AskUserQuestion 取首个问题、Agent 取 description、TodoWrite 显示任务数、其余取第一个非空字符串字段），字符上限由 20 提到 30（首版值），截断仍保持路径语义；`◐`/`✓`/工具名/摘要配色统一、不跟工具身份色。第二行强制单行不折行，整行超宽时按显示列宽（unicode-width）在行尾 `…` 收口，不再被 `Paragraph` 静默裁掉。

## v0.6.110 — 2026-10-09

### Fixes

- **Schema 熔断器跨工具聚合与连续失败按错误类型聚合（#379）**：修复模型在不同工具间轮换猜错参数时单工具计数永远达不到阈值（2 次）导致的熔断逃逸缺陷，新增跨工具聚合连续失败追踪（阈值 3 次）与指数退避机制；修复连续失败检测按完整错误文本做 key 导致参数名差异稀释计数的缺陷，引入类型化 `ToolErrorKind` 枚举并将 key 优化为 `(tool_name, error_kind)`；弃用脆弱的字符串嗅探，在参数校验失败点精准抛出 `AgentError::ToolSchemaValidationFailed` 并结构化传递；调整告警写入时序，确保所有 `ToolResult` 紧随 `AiMessage` 写入后再追加 `System` 提示，杜绝孤立工具结果风险。

## v0.6.109 — 2026-10-09

### Features

- **动词流光动效与纯时间驱动思考状态词色温阶梯（#387、#388）**：参考 Codex CLI 物理余弦衰减模型，在 `cc-widgets` 实现动词独立流光（Shimmer），以 5.0s 为周期舒缓掠过 1.6s 抛光流光（同色系提亮 45%，绝不产生刺眼纯白反差），作为长任务与长 Bash 执行期间的生命体征心跳信号，防止进程假死焦虑；思考状态词与动词动静解耦，状态词完全不闪烁，纯按时间流逝从浅灰逐步加温至琥珀金（0~2.5s 浅灰 `#999999` ➔ 2.5~5s 柔白中间色 `#D0D0D0` ➔ 5~15s 动词暖橙 `#EB9F7F` ➔ 15~60s still thinking 浅金 `#FFD966` ➔ >=60s deep in thought 琥珀金 `RGB 255,152,0`），长 Bash 场景维持 8.0s 极低频独立流光守候，状态词定格显示已结束的思考耗时（置灰静止不抢戏）。

### Fixes

- **ASCII 降级表补 `⎿`（U+23BF）（#386）**：解决在异常列宽终端上工具行前缀因缺少映射降级为问号的问题。

## v0.6.108 — 2026-10-09

### Performance

- **`LlmCallStart` 载荷按需构造，无订阅者不再全量克隆消息历史（#369）**：`llm_step.rs` 在每轮 LLM 调用前无条件 `state.messages().to_vec()`，把整个消息历史（含工具结果正文、图片 base64）深拷贝一份，仅用于构造 `LlmCallStart` 事件的 `messages` 字段。该克隆发生在 `emit()` 的订阅者判断**之前**——Langfuse 未启用时照常执行，长会话下每轮 MB 级、再乘以 ReAct 迭代次数，为无人读取的数据付 O(轮数 × 历史大小) 的分配成本；#306 只修掉了 tracer 侧的第二次深拷贝。由于 `LlmCallStart.messages/tools` 的唯一真实消费者是 Langfuse tracer（TUI 与 ACP mapper 均丢弃该事件），现按「按需快照」实施：`AgentEventHandler` 新增 `wants_llm_call_payload()`（默认 `false`），executor 据此决定是否构造载荷——`false` 时发空载荷、跳过全量拷贝。`cc-acp` 的 `FnEventHandler` 闭包替换为 `PumpHandler`，以「Langfuse 是否启用」声明需求；`cc-middlewares` 的 `SourceAgentIdHandler`（子 Agent 事件包装器）委托 inner 的声明，避免子 Agent 的 Langfuse 输入快照丢失。

### Fixes

- **用户 `!` 命令块渲染对齐工具结果行（#384）**：TUI 中用户直接执行的 `!` shell 命令块与工具结果行的缩进/前缀风格不一致，现统一对齐，消除同一输出区域的视觉割裂。

## v0.6.107 — 2026-10-09

### Features

- **紧凑审批面板与完整参数展示（#368）**：审批面板此前将 Bash 参数单行截断、批量工具重复展示选项与快捷键，长脚本难以核对。现按内容收紧布局、完整换行展示参数（Bash 保留换行/缩进与中文显示列宽，Edit 展示上下文与增删对比），批量显示当前位置与三类计数，没有内部滚动区、超高内容优先当前工具并明确提示剩余未显示行数，快捷键固定在底部；仅改 UI/文案/测试，权限判定、父子 Agent 限制、会话审批记忆、ACP 返回与键盘处理不变。

## v0.6.106 — 2026-10-09

### Features

- **指引文件分层合并，加载模型对齐 dsh（#357）**：项目同时存在 `AGENTS.md`、`CLAUDE.md` 或多级目录指引时，原实现会**漏载规则**。现按项目根 → 工作目录逐层收集同层候选依次加载，记录 provenance（来源路径），同目录按内容去重，并在 `session/new` 时冻结结果（`frozen_instructions`）。用户全局层由旧的 `~/.claude/AGENTS.md` 迁移至 `~/.cc-code/AGENTS.md`。空文件跳过、换行归一；大文件有界读取（单文件默认 1 MiB、最终注入默认 256 KiB），中文切边安全；递归 `@import` 共享输出预算，缺失/不可读/循环引用保留占位符。审计修复：空指引也保存冻结快照；正确解析含 `..` 的 cwd 并保留发现路径供 excludes 使用；候选先内容去重再登记路径，避免别名挡住后续目录的规则（另见 `spec/global/domains/agent-instructions.md`）。
- **子 Agent 继承父权限，消除委派绕过审批（#357）**：Auto 模式下，普通、后台与 fork 子 Agent 现在**共享父级权限模式、Jev 门、规则加载器、分类器及会话审批记忆**，修复了「委派给子 Agent 即可绕过工具审批」的问题。父保留允许 / 拒绝 / 询问三态；子只有允许 / 拒绝，不确定或服务失败默认拒绝，规则提炼失败、不完整或取消也可被识别。检查覆盖真实工具名、间接工具目标（`ExecuteExtraTool` 解包）、工具限制及实际执行目录，并禁止递归委派。
- **HITL 审批界面同时提供三档选择（#357）**：审批弹窗同时显示「同意本次 / 本次会话同意 / 拒绝」，上下键选择、Enter 提交、Tab / Shift+Tab 切换工具、Esc 全部拒绝；滚动保留当前工具的三项选择，参数按终端列宽截断，中英文同步。会话记忆按工具类型细化：本次批准不记忆；文件按工具与路径，Bash 按完整命令 + 执行目录 + 分支，其他工具按完整参数；明确禁止规则优先于记忆。

### Fixes

- **`/export` 导出不再截断工具调用信息（#363）**：导出的 Markdown/PlainText 中，工具调用参数被 `chars().take(200)` 一刀切——真实日志中 210 次 Bash 调用有 **156 次（74%）被切在正好 200 字符**，JSON 未闭合、命令从中间断掉；`Write.content`、`Edit` 的 `old_string`/`new_string` 同样受影响。现去掉截断、完整输出参数 JSON；同时加固 `ContentBlock::ToolResult` 分支（原只写行数统计，改为输出完整正文并标注 `is_error`），并按正文最长连续反引号自适应围栏长度，防止 heredoc 脚本内容破坏 Markdown 结构。（详见 [#362](https://github.com/cc-claws/cc-code/issues/362)）

## v0.6.105 — 2026-10-08

### Fixes

- **日志文件打不开不再 panic，降级到 stderr（#354）**：启动 `cc-code` 时若 `~/.cc-code/logs/{service}.log` 因 ACL 被写坏（空 DACL）而无法打开，`subscriber.rs` 的 `.expect("cannot open log file")` 会**直接 panic 掉整个进程**——日志只是诊断设施，不该是启动硬依赖。现抽 `resolve_log_writer(log_path) -> BoxMakeWriter`：打开失败时 `eprintln!` 警告并退回 `std::io::stderr`（json / 非 json 两条分支共用同一 writer 类型）；`ensure_utf8_bom()` 失败静默忽略（BOM 仅为显示优化）；`set_global_default` 失败降级为提示。修复后日志文件 ACL 坏了也能正常启动。

### Refactoring

- **统一英文内置指令并优化任务执行约束（#356）**：统一 14 个主模板、4 个内置 Agent、核心工具与参数说明、Skills、ACP 命令、压缩/回顾及审批模型的英文指令。清理「四行/一词回复」「编辑后停止」等固定限制，明确授权范围、持续执行、完成结果与验证证据，规定审批与拒绝不得被工具切换绕过。工具说明与实现对齐：Read 默认分页、32 MiB 文件上限、未实现的 PDF 页码；Write/Edit 不再宣称强制先读校验；Bash 使用实际 shell/timeout 契约；Agent 区分独立上下文与继承快照、不承诺 worktree 隔离。Deferred 工具目录按名称排序、每项只保留首个非空描述行（≤160 Unicode 字符），完整描述与 schema 按需经 `SearchExtraTools` 获取。同时修复 HITL 提示段落的开关复用运行时 `is_yolo_mode`（此前默认审批开启却未注入审批说明）。模型可见文本量：核心工具描述 −40.9%、内置 Agent 定义 −31.9%。



### Features

- **Markdown 缓存内存统计（#351）**：`/gc` 此前只显示 Markdown 缓存**条数**（`markdown_cache: 1024/1024 条`），无法判断其字节占用，不足以评估内存节省空间。现新增 `MarkdownCache::stats()`（单次加锁快照，**不克隆解析产物、不提升 LRU 次序、不改缓存策略**），按 `Text.lines` / `Line.spans` / 链接数组及**自有字符串 `capacity()`** 估算堆占用，输出总量、平均/最大条目、渲染行数与 Span 数，并把估算纳入 `/gc` 的「已知合计」；口径明确标注「不含 LRU / 分配器开销，非 RSS」。同时把「未识别」注解由「非泄漏」改为更严谨的「**余量来源待定位，不能据此判断是否泄漏**」，并把原始字节字段经 tracing 输出。**注**：本版仅补统计，**未**实施字节预算 / 容量下调（另见 `spec/issues/2026-10-08-markdown-cache-memory-accounting.md`）。

### Refactoring

- **移除 `~/.peri` 兼容，应用数据统一 `~/.cc-code`（#349，破坏性）**：项目已是 cc-code，不再兼容改名前的旧主目录。此前 #289 为「老用户数据不丢失」保留了「新优先、旧回退」的**逐文件**回退，且具粘性——只要某文件只在 `~/.peri` 就一直使用旧路径（实测 `input-history.json` / `oauth_tokens.json` 长期落 `~/.peri`）。现删除 `cc-agent/src/app_home.rs` 的 `legacy_app_home_dir(_in)`，`app_data_path_in` / `app_data_dir_in` 一律返回 `~/.cc-code/...`；`cc-tui/src/main.rs::inject_env_from_settings` 只读 `~/.cc-code/settings.json`；并清理各处陈旧注释与用户可见报错文案（含 `acp_stdio.rs` 的 provider 缺失提示）。**破坏性变更**：只在 `~/.peri` 存在的数据文件不再被读取（用户需自行迁移）；新写入一律走 `~/.cc-code`。`hitl/jev/policy.rs` 敏感目录名单中的 `.peri` 予以保留（若用户机仍存在该目录，继续阻止工具读取，属安全而非兼容）。

## v0.6.103 — 2026-10-08

### Fixes

- **详细模式运行中工具头前缀不再随指示器闪烁左移（#343）**：详细模式（Ctrl+O）下查看运行中的超长命令时，工具头前缀 `● Bash(` 会随运行指示器**闪烁而左右抖动**——亮帧显示 `● Bash(...`，熄灭帧开头 `● ` 消失、整行左移（观感为「看不到工具名前缀」）。根因三处叠加：`format_indicator()` 让运行中指示器在 `●` 与 `" "` 间闪烁；详细模式 header 走 `wrap_full` 折行；`wrap_line_spans_rich()` 会 **trim 每段行首空白**——熄灭帧首段是空格被 trim，`指示器 + 分隔空格` 前缀整体丢失。现为折行核心增加「保留首段行首空白」路径（`trim_first_lead` 仅控制首段，续行行为不变），工具头 `wrap_full` 分支改用 `push_wrapped_line_keep_first_lead`，其它折行路径行为完全不变。
- **`/gc` RSS 变化符号颠倒 + allocated 差值文案方向修正（#346）**：`/gc` 输出的 `RSS: 197.9 MB → 196.6 MB (+1.3 MB)` 方向是反的——**下降**被显示成 `+`。根因是 `delta = before - after` 却只在 `>= 0` 时加 `+`，使「减少」带上了 `+`（`OS RSS` 同病）；`/gc` 的设计目标正是「消除误导指标」，符号反了等于制造新误导。现统一方向语义为 `after - before`（增加为正），新增 `fmt_signed_delta()` 输出 `+N` / `-N` / `±0` 供两处共用；`allocated - RSS` 改为带符号输出并按方向分支文案。
- **排队消息快捷键 `Ctrl+S` / `Ctrl+X` 迁移为 `Alt+S` / `Alt+X`（#347）**：Windows conhost 会把 `Ctrl+S` 当作终端**流控键**截走，按键根本到不了应用，导致「立即发送」失效。迁移为 `Alt+S`（立即发送）/ `Alt+X`（删除排队消息）。

## v0.6.102 — 2026-10-08

### Chores

- **补全版权署名与 Cargo 包元数据**：LICENSE 在上游 `Copyright 2026 KonghaYao` 之下追加本衍生作品署名 `Copyright 2026 cc-claws (modifications to the Derivative Work)`（上游版权行为 Apache-2.0 §4c 强制保留义务，未删改）；`[workspace.package]` 新增 `authors = ["cc-claws"]` 与 `license = "Apache-2.0"` 作为全仓单一数据源；7 个 workspace crate（cc-agent / cc-middlewares / cc-tui / cc-acp / cc-widgets / cc-lsp / langfuse-client）统一改为 `license.workspace` / `authors.workspace` 继承——此前仅 cc-agent / cc-middlewares 硬编码 license，其余 5 个缺 `license`，全部缺 `authors`。
- **合并 `CORE_VERIFY.md` 到 `human/TESTING.md`**：根目录 8 行的手写冒烟清单（hello 对话 / `/clear` / `/history` / 工具调用完整性 / 多轮失忆 / Ctrl+C 中断 / ask_user）零引用、未收录文档地图，易被忽略。现整理为 `human/TESTING.md` 的「核心功能冒烟清单（发布前人工走查）」段并删除原文件，内容一字未丢。
- **删除 `prompts.md`**：根目录 4 条 `/loop` 命令自用模板，零引用、未收录文档地图，且模板内容引用了并不存在的 `progress.md`。
- **`lefthook.yml` 对齐 CI**：pre-commit 的 clippy 由 `cargo clippy --all-targets -- -W clippy::all` 改为与 CI 一致的 `cargo clippy --workspace --all-targets -- -D warnings`（`-W` 只告警不拦截，是「本地 lefthook 通过、CI 变红」的根因，如 PR #339）；`check` 补 `--workspace`；移除永远空转的 `typos` 命令（`typos --ignore-hidden 2>/dev/null || true`，无配置文件且 `|| true` 保证永不失败）。

### Docs

- **修正 README 与代码不符的陈旧描述**：删除 RISC-V 支持声明（`README.md` 头部、安装平台段及 `npm/README.md` 平台表）——发布的 5 个平台均不含 riscv64（唯一构建 riscv64 的 `release-agent.yml` 因 `agent-v*` tag 从未触发）；`side-projects/` 描述由「llm-gateway 等」更正为实际的 `peri-sync`；仓库结构树把不存在的 `spec/prd/` 更正为 `spec/archive/` 并补上实际存在的 `docs/`、`human/`；Typical Workflow 中不存在的 skill `grill-me`/`improve-codebase-architecture` 更正为真实存在的 `brainstorming`；特性表「更早版本」边界由 v0.6.76 修正为 v0.6.79。
- **维护 v0.6.100 / v0.6.101 文档 + 清理当前状态文档中的 `peri-*` 残留**：CHANGELOG 补 v0.6.100 / v0.6.101 条目；README 中英特性表各 +2 条（一一对应、保持最新 10 条）；`spec/global/domains/tui.md`、`features.md`、`index.md` 同步；新增 `spec/issues/2026-10-08-detail-mode-long-cmd-running-status-overwrites-header.md` 根因分析。另修正 v0.6.95（#333）crate 改名 `peri-*` → `cc-*` 时遗漏的当前状态文档（`spec/global/`、`docs/`、crate 侧 README/CLAUDE.md 等 60 个文件）；历史快照目录（`spec/archive*`、`spec/reviews`、`docs/superpowers/*`、CHANGELOG/DEVLOG 历史条目）按规则保留不改。

## v0.6.101 — 2026-10-08

### Fixes

- **详细模式超长命令运行时，状态刷新不再覆盖 header 续行（#341）**：详细模式（Ctrl+O）下 Bash 命令超长时，ToolBlock header 会折成多行（#264 引入）。渲染线程的 tick 增量刷新仍按「header 恒 1 行」的旧假设处理状态行，导致两处故障：场景 B 写死 `lines.get_mut(1)` 更新 `Running… (Xs)` 秒数，多行 header 下命中的是命令续行——命令文本被就地改写为 `Running…` 并残留右括号，真正的状态行（下标 ≥2）因不在下标 1 而被冻结、秒数不再前进，屏幕上呈现为「两处 `Running…` 且时间不一致」；场景 A 用固定行数 `cached_line_count < 3` 判断「状态行尚未渲染」，多行 header 下行数早已 ≥ 3，判定恒为假，首次跨越 2 秒阈值时状态行永远不会出现。现改为**按内容定位状态行**（新增 `message_render::is_shell_running_status_line()`，与子 Agent 路径已有的内容定位做法一致），两个场景都不再依赖固定下标 / 固定行数。注：本问题由 #264 引入，与 #287 无关（#287 仅调整折行时的词边界回退，反而让长 token 场景更易折成多行、更易触发）。仅影响「详细模式 + 超长命令折行 + 运行中（>2s）」，非详细/短命令路径行为不变。

## v0.6.100 — 2026-10-08

### Fixes

- **`/gc` 内存诊断修正：消除误导指标 + 纳入 `view_messages` 估算（#339）**：`/gc` 输出的内存诊断存在误导性数字。P0 —— `active`/`mapped`/`retained` 语义**分平台**（jemalloc：真实活跃页 / 映射量 / 保留未归还 OS；mimalloc：`page_committed` 历史触及高水位 / `reserved` 虚拟地址 / 虚拟地址空间），旧代码用统一标签，Windows 上 `active=1392 MB` 与派生的「碎片=1332 MB」完全无用（真实 `resident` 仅 100 MB、无碎片问题）。现按 `alloc_name` 条件化标注，并给「碎片」行加忽略提示。P1 —— `estimate_messages_heap` 只统计 `origin_messages` + `completed`，漏掉 `view_messages`（含每块内嵌 `Text<'static>`），正是「未识别 59 MB」的主因。现新增 `estimate_view_messages_heap()` 遍历 `MessageViewModel` 全部变体，「已知合计」改为「消息 X + VM Y」三元展示，`origin vs completed` 完全相同标注为「设计冗余，非泄漏」，并追加注脚说明「未识别」构成（markdown 缓存 / ACP 缓冲 / tokio / tracing），明确非泄漏。附带修正 `estimate_links_heap` 的 clippy `manual_slice_size_calculation` 告警（改用 `std::mem::size_of_val`），该告警曾致 CI 三平台全红。

### Chores

- **移除 ARM32 部署包与构建链路（#340）**：删除 `deploy/peri-arm32-v0.2.0/` 与 `scripts/build-arm32.sh`——ARM32 未纳入 CI / npm 发布链路，属 peri 时代遗留。

---

## v0.6.99 — 2026-10-07

### Fixes

- **后台 shell 通知展示文案接入 i18n（#338）**：`shell_notification_display_text` 把「后台 shell 已完成/超时终止/已取消/已终止/等待输入」硬编码为中文，即使用户语言为英文仍显示中文。根因是该函数处于静态构造路径（`MessageViewModel::user/system/from_base_message*` 内部调用），拿不到 `App`/`ServiceRegistry` 上下文。现新增 6 个 `shell-notify-*` FTL key（en + zh-CN），展示文案改走 `LcRegistry::tr()`；新增进程级语言注册表（`i18n::init_global`/`global`，启动与 `/lang` 切换时同步），供静态构造路径读取当前语言；`FluentBundle` 换用 concurrent 变体使 `LcRegistry` 满足 `Sync`，可跨线程（渲染线程）安全读取。

---

## v0.6.98 — 2026-10-07

### Fixes

- **排队快捷键提示对比度提升（#335, #336）**：排队消息行尾的 `Ctrl+S send now · Ctrl+X delete` 提示用了 MUTED + DIM，在 USER_BG 背景上几乎看不清。改为 TEXT_SOFT 高亮。

---

## v0.6.95 — 2026-10-01

### Breaking

- **crate 改名 peri-* → cc-***：与上游彻底切割，6 个 workspace crate 重命名（peri-agent→cc-agent、peri-middlewares→cc-middlewares、peri-tui→cc-tui、peri-acp→cc-acp、peri-widgets→cc-widgets、peri-lsp→cc-lsp）。二进制名 `cc-code` 不变，npm 用户无感；Rust 侧依赖 peri-* 的需改名。
- **清理无用 side-projects**：删除 git-graph、agent-defect-analyzer、daytona、git-stats、llm-gateway、pty-server（保留 peri-sync，`cc-code sync` 在用）。

### Fixes

- **面板打开时 Ctrl+C 先关面板**：此前 `handle_panels` 无条件把 Ctrl+C 穿透到双击退出逻辑，面板开着时双击/长按 Ctrl+C 会直接退出整个 TUI（2026-06-24 修"面板开着退不出"时摆过来的钟摆）。现改为空闲时先关闭顶层面板（session/global），这次按键不计入双击退出；agent 运行中仍穿透中断。另过滤按住 Ctrl+C 的键盘重复事件，一次物理按下只算一次，避免长按被误判为双击退出。

## v0.6.94 — 2026-10-01

### Fixes

- **后台任务输出文件创建加重试**：`DiskOutput::spawn_writer` 建目录/建文件失败时直接静默退出，导致 `exit_signal` 触发后输出文件不存在（Windows CI 偶发 NotFound flake，如 `test_executor_hard_deadline_survives_every_background_mode`；与 #323 同一家族的 runner 文件系统问题）。现改为最多重试 5 次（100ms 递增退避）再放弃；对应测试的读盘断言改为缺失视为无输出（该测试验证的是硬期限行为）。
- **Windows CI 加 Defender 扫描排除（#323）**：ci.yml 给 `.cargo` / `target` / `TEMP` 加 Defender 扫描排除，治文件锁 flake；测试失败时上传日志 artifact 方便诊断。

### Security Fixes

- **ACP stdio 权限审批转发给客户端**：`cc-code acp` 的 `StdioBroker` 之前直接放行所有工具审批请求（且默认 `PermissionMode::Bypass`），ACP 客户端永远收不到 `session/request_permission`，HITL 名存实亡。现改为 `AcpPermissionBroker`：敏感操作以 `session/request_permission` 交给 IDE 客户端审批（allow-once / allow-always / reject-once / reject-always；allow-always 复用会话级审批记忆），客户端不支持/调用失败/未知选项一律按拒绝处理（fail-closed）；stdio 默认权限模式改为 `AutoMode`，无人值守场景仍可用 `session/set_mode` 显式切到 bypass。

---

## v0.6.93 — 2026-10-01

### Security Fixes

- **HITL 预审批不再执行 rtk 二进制（#288）**：`gate_effective_call()` 为判断改写效果会在审批前执行外部 `rtk`（`verify_rtk_executable` 只校验 `--version` 退出码，PATH/RTK_PATH 劫持可致审批前代码执行）。现以纯字符串预测 `process::predict_rtk_rewrite()` 替代，零子进程；存在性检查 `rtk_rewrite_likely()` 替代 `--version` 探测；审批后的真实执行路径不变。另 `YOLO_MODE=""` 视同未设置。
- **配置目录统一到 `~/.cc-code`（#289）**：新增 `peri-agent::app_home`，新目录优先、仅旧目录存在时回退 `~/.peri`、全新安装用 `~/.cc-code`；迁移 MCP 全局配置、OAuth、threads、skills、历史、sync 共 10 处调用点；Jev `PROTECTED_DIR_SEGMENTS` 补上 `.cc-code`，新配置目录保持写保护。
- **npm 安装包校验 + 迁移确认（#290）**：`verifyChecksum()` 对下载的 tarball 做 sha256 校验（对照 release `checksums.txt`），缺失/不匹配直接失败；Claude Code 迁移改为确认制：`CC_CODE_NO_MIGRATE=1` / `--no-migrate` 跳过，非 TTY 跳过并提示手动迁移，TTY 下 `[y/N]` 默认否。
- **项目 hooks 需显式信任（#18）**：`.claude/settings.local.json` 的 hooks 不再自动加载执行（防 clone 即 RCE，PoC 已验证可利用）。需 `CC_CODE_TRUST_PROJECT_HOOKS=1` 或项目列入 `~/.cc-code/trusted_projects`。
- **插件 hooks 需信任（#17）**：未信任插件的 hooks 被过滤，不再自动执行。
- **Sync KDF 改用随机 salt（#21）**：`derive_key(pair_code, salt)`，salt 经协议传输，防预计算攻击。
- **Sync relay 警告（#22）**：sender 显示警告，提示 relay 可解密同步内容（含 API keys）。
- **自更新脚本 SHA256 校验（#19）**：下载后验 hash 才执行，防篡改。

### Fixes

- **内存 Arc 循环引用修复（#306）**：tracer.rs 内存占用从 2.00x 降至 1.00x（100% 确认压测）。
- **消息硬上限 10 万（#307）**：`MAX_MESSAGES = 100_000`，超限 degraded_prune，稳定 500MB 左右。
- **跨平台修复（#308-#311）**：Windows 密钥文件 DACL、多行命令 hard-error、Unix 杀进程组、SIGTERM 恢复终端。
- **Robustness 修复（#317-#319）**：settings.json 非对象 JSON 不再 panic、双 SQLite 失败降级内存模式、配置面板保存失败 UI 报错。

---

## v0.6.92 — 2026-09-30

### Fixes

- **详细模式超长无空格命令的头行折行不再孤立 `●`（#287）**

---

## v0.6.91 — 2026-09-30

### Features

- **Windows 启用 mimalloc 全局分配器（#285）**：`peri-tui` 在 Windows 下启用 mimalloc，顺带修复 `/gc` 诊断。

---

## v0.6.90 — 2026-09-30

### Security Fixes

- **默认启用审批、门控评估实际执行命令（#284）**：`YOLO_MODE` 未设置不再默认免审批（fail-closed），`-y/--yolo` 正确接线；HITL 门控评估 rtk 改写后的实际执行命令；`git clone` 加 `--` 防参数注入；`scripts/install.sh/ps1` 修正改名后的仓库地址与二进制名。

### Features

- **排队消息支持 Ctrl+S / Ctrl+X（#282）**：不用鼠标也能补充/删除排队消息。

### Fixes

- **Thought 归并吸收前置只读计数（#280）**：消除重复或断层的动作摘要。

---

## v0.6.89 — 2026-09-30

### Fixes

- **git 分支探测异步化（#278）**：Agent 工作期间状态栏分支名不再陈旧。

---

## v0.6.88 — 2026-09-30

### Fixes

- **markdown 水平线按可用宽度渲染（#273）**：替代硬编码 60 字符。
- **Edit 工具报错文案改英文（#272）**：消除中英混排。

---

## v0.6.87 — 2026-09-30

### Features

- **前台 `!` 命令结果回流 Agent 上下文（#270）**

### Fixes

- **后台任务面板长命令截断 + 补齐 i18n（#271）**

---

## v0.6.86 — 2026-09-30

### Fixes

- **工具名配色统一（#269）**：状态语义只由 `●` 表达。
- **折行末段可整体容纳时不再多拆一刀（#267）**
- **非详细模式 Bash 命令宽度比例调整为 16/19（#265）**
- **Bash 命令超长改为限制宽度截断（#264）**：详细模式完整折行对齐。

---

## v0.6.85 — 2026-09-29

### Features

- **ACP 协议一致性批次 1（#259）**：补齐错误响应、历史回放、ResourceLink、图片能力。

### Fixes

- **跨平台 Hook 命令路由统一（#263）**：命令路由与后台任务生命周期统一，稳定跨平台 Hook 命令测试，修复跨平台 Clippy 导入。

---

## v0.6.84 — 2026-09-29

### Features

- **HITL 审批弹窗支持三选：一次性 / 本次会话 / 拒绝（#261）**：此前只能二选（批准/拒绝）且逐次生效，而语义门（jev）无状态、每次独立判定——模型对绝对路径的 `local_scope` 条件打分在阈值附近抖动（实测 0.07~0.16），同一文件反复编辑会被反复弹窗。现弹窗提供三档：`allow_once`（仅本次）、`allow_always`（写入**会话级审批记忆**，同 `(工具, 路径)` 本次会话内免问）、`reject_once`；`Space` 循环切换、`Enter` 提交、`Esc` 全部拒绝。

### 说明

- 审批记忆为**路径级 + 会话作用域**：键为 `(工具名, 词法规范化路径)`，随会话销毁丢弃；Bash 等无路径的命令类工具不参与记忆。

---

## v0.6.83 — 2026-09-29

### Fixes

- **卡住检测误判空白 thinking（#256）**：`check_stuck` 的指纹判空仅挡字节空串，模型在工具调用轮常返回 `"\n"` / `" "`，被当作有效指纹入窗后逐轮完全相同，第 3 轮即误报「重复的思考循环」。改为 **trim 后判空**，空白指纹直接跳过检测。
- **卡住检测换策略提示改用英文（#256）**：agent 层无 i18n（`LcRegistry` 在 peri-tui，不可反向依赖），且同文件其余注入提示（连续失败 / schema 熔断 / 动作循环）均为英文，本条硬编码中文是唯一异类。文案提为 `STUCK_HINT` 常量，用户侧语言由 system prompt 保证。
- **spinner 状态词 `· thought for Ns` 配色修正（#252, #254）**：该字段属思考完成态，原随耗时升温变色；按设计应始终 MUTED 灰——仅 `thinking` / `still thinking` / `thinking more` 三个进行中状态词随热度变色。

---

## v0.6.82 — 2026-09-29

### Features

- **Thinking 状态行 + 工具动作汇总，对齐 Claude Code 非详细模式（#248, #249）**：运行中 spinner 行新增第三字段四态状态机——`· thinking` / `· thought for Ns` / `· still thinking` / `· thinking more`（英文固定输出，`still thinking` 优先于 `thinking more`，10s 阈值可调）；verb 整轮固定不再随工具逐段换词；配色随时间四档升温（5s/15s/30s，仅 verb + 状态词变色，其余 MUTED）。
- **消息区思考行改为秒数 + 动作计数（#248, #249）**：`∴ Thought for N chars` → `Thought for Ns[, read N files] (ctrl+o to expand)`。秒数在 LLM 流式层计时并随消息持久化（`ContentBlock::Reasoning.duration_ms`），历史恢复后仍可用；连续多轮「思考 + 只读工具」合并为一行（秒数/计数累加，跨 Bash 不断开仅不计数）；只读工具折叠为计数文案（含单复数）；纯动作行（无 reasoning 的只读工具组）折叠显示。
- **Bash 非详细模式展示输出摘要（#248, #249）**：显示输出前 3 行 + 截断提示 `... (N more lines) (ctrl+o to expand)`，失败必显（判定依据 `is_error` 标记而非内容前缀）。

### Fixes

- **失败 Bash 输出行颜色口径统一（#250, #251）**：非零退出（`bash_failed`）时圆点/状态已标红 Failed，但输出行与 `⎿` 前缀仍灰白——`result_color`/`border_color` 用 `*is_error` 判定漏了 bash_failed 场景。改为 `state.is_error`，与 header 指示器一致，附 2 个回归测试。

### 说明

- **测试命名规范定死并清理 98 处风格债（#250, #251）**：`CLAUDE.md` 明确测试函数/helper 命名必须全英文 `test_<被测对象>_<场景>` snake_case（禁止中英混排），14 个文件 98 处存量中文命名一次性改为英文，注释与断言消息保持中文不变。
- 设计文档、四轮代码审计记录与渲染效果稿见 `docs/designs/2026-09-29-*`。

---

## v0.6.81 — 2026-09-28

### Features

- **权限模式收敛为 `auto` / `bypass` 两档，默认 Auto（#246, #247）**：实测用户不使用其余模式——`Default`/`DontAsk`/`AcceptEdit` 要么每次弹窗、要么半自动，体验差且不智能。`Shift+Tab` 循环变为 `Auto ↔ Bypass`；TUI 启动默认 Auto，`-p` 默认仍为 Bypass。`PermissionMode::from(u8)` 的**未知取值一律回退 Auto**，绝不因陈旧/异常值意外滑进 Bypass。`--permission-mode` 只认 `bypass`（其余回退 auto），`-a/--approve` 等同 `--permission-mode auto`。
- **规则来源扩展至 `.claude/hooks/*.sh`，并带来源标注**：很多项目的"铁律"并不写在 CLAUDE.md 里，而是直接做成可执行 hook；那些脚本里的 guard 条件与 `BLOCKED` 消息本身就是自然语言规则。采集顺序为个人 `CLAUDE.local.md` → 项目 `CLAUDE.md`/`AGENTS.md` → 项目 hooks → 全局 `~/.claude/CLAUDE.md`，拦截时按来源文件分组展示，用户可直接定位到该改哪个文件。

### Fixes

- **补齐硬黑名单：下载执行（`curl|bash`）及其全部常见变体（#246, #247）**：此前 `curl|bash` 与 `sudo`/`env`/`xargs` 包装、`/bin/bash` 路径前缀、`bash <(curl …)`、`sh -c "$(curl …)"` 等 **8/8 变体全部漏拦**，只挂在"升级语义判定"上——等于用概率墙守最经典的 RCE。现补齐包装命令、进程替换、命令替换三类形态，并覆盖解释器家族 `eval` / `python -c` / `node -e` / `xargs`（实测 5/7 漏拦）。
- **修复写路径穿越可跳过整道门（#246, #247）**：`path.starts_with(cwd)` 为纯词法比较，`cwd/../../etc/passwd` 会被判成"项目内非受保护路径"直接放行，**写保护与策略检查全都没发生**。改为先 `normalize_lexical` 再比较，并配反向对照（真正的项目内路径仍走快车道）。
- **硬黑名单不再误伤日常命令（#246, #247）**：普通 glob（`* ? [] {}`）此前被当作"目标静态不可解析"，导致 `rm -rf build/*`、`rm -rf node_modules/*` 落入**不可申诉**的硬拦。"不可解析"收窄为 `$(…)`/`${…}`/`$VAR`/反引号/`~user`；同时**补上**根目录 glob（`rm -rf /*`）与 Windows 盘符根，防止削弱。
- **判定不可用时不再堵死整个会话（#246, #247）**：原 fail-closed 在端点 503/超时时会把**所有工具调用**拦下（实测）。按上游语义改为**回到人工确认**：网络失败、超时、错误响应、不可读响应体、越界分数一律视为 unknown，多问一次而不是拒绝执行，更不是把 agent 一棍子打死。
- **缺失/越界分数不再被当作"明确违规"（#246, #247）**：此前缺失分数按 `0` 处理 → 判为"安全条件被明确违反" → 硬拦，把"网关响应不完整"伪装成"用户违规"，错误且无从排查。现标记为 `unknown` 并回到人工确认；仅**明确违规**才拦。
- **规则提炼不再静默丢规则或静默失效（#246, #247）**：规则较多的大 CLAUDE.md 会撞输出上限导致**整块产出为空**（本仓库自身实测 0 条）。新增分块提炼、截断抢救（尽力捞回已生成的完整规则对象）、失败自适应对半切分；提炼失败不再与"来源本来就为空"混为一谈，改为可观测并明确告警"规则当前没有被执行"。
- **不再设置 `max_tokens` 进行规则提炼（#246, #247）**：推理模型的**思考同样消耗输出预算**，此前的 4096/8192 会被思考吃光 → `content` 为空 → 一块规则全丢；改为继承 provider 配置（默认 32000）。
- **提炼口径改为「操作级禁令」（#246, #247）**：原口径"面向 AI 编码助手的禁止/约束"把工程规范一并捞入（本仓库提出 33 条，几乎都不是安全门能拦的），稀释判定注意力。现只提取"约束即将执行的操作"且"仅凭本次工具调用参数即可判断"的规则。
- **确定性防线与语义判定解耦（#246, #247）**：此前门只在配置了 API key 时构造，未配 key 时 Auto 模式连硬黑名单一并丢失（零确定性防护）。现门始终构造，Auto 模式改为**先跑确定性层 → 有凭据才走语义 → 否则落回分类器**。关闭语义判定不再等于关闭确定性防线。

### 说明

- 拦截文案按读者拆分：**用户段**为白话、不含内部术语，**agent 段**为结构化说明（`decision` / `rule` / `source` / `retryable`）并明确给出可行路径与"禁止绕过"的指令。
- 审计日志补充判定请求 `id`、`cost`、三态结论与每个条件的 `(概率, 是否未知)`；上下文补充当前 git 分支等事实，使"禁止在 main 上提交"这类**带条件**的规则可被判定。
- 实测留档：**规则数稀释实验**（1/10/40 条规则均正确拦下同一条清晰违规）→ 据此**不做**逐规则判定/预筛的架构改动。

---

## v0.6.80 — 2026-09-28

### Fixes

- **`-c`/`-r` 恢复会话时丢失 recap 与任务完成总结行（#244, #245）**：`✻ Cooked for 25s · done 14:26` 与 `※ recap: ...` 两行原为纯内存展示态、不进 message history 也未持久化，重启恢复即消失。`ThreadMeta` 新增 `latest_recap` 与 `last_task_summary`（`TaskSummary { verb, elapsed_ms, done_at }`）字段并落库（SQLite `threads` 表幂等 `ALTER TABLE` 迁移，旧 `meta.json` 靠 `#[serde(default)]` 兼容）；`ThreadStore` 新增 `update_latest_recap` / `update_last_task_summary` 窄更新接口，SQLite override 为单列 `UPDATE`，避免每轮重写 ~1MB 的 `cached_context`。`handle_recap_completed` 与 `cleanup_agent_state` 分别写回两字段，`open_thread()` 读 meta 回填并无条件覆盖，顺带修复切换 thread 时 recap 残留串台。

---

## v0.6.76 — 2026-09-24

### Features

- **新增 `/recap` 会话回顾命令与终端失焦自动回顾（#237, #238）**：`/recap`（别名 `/away`、`/catchup`）生成一句话回顾——「高层目标 + 当前任务 → 下一步」，单轮禁用工具、不写 history、支持 Ctrl+C 取消；TUI 新增 `AutoRecapState` 调度状态机，终端失焦 + 已完成轮次 ≥3 + 60s 静默后自动触发（两次回顾间至少新增 2 轮，聚焦取消、失败 30s 重试、in-flight 结果按 revision 失效）；`/config` 新增「会话回顾」开关（`config.auto_recap`，默认开），环境变量 `PERI_AUTO_RECAP_DELAY`、`PERI_AUTO_RECAP_MIN_TURNS` 可覆盖。
- **非流式 Anthropic 响应自适应兼容反向代理 OpenAI 格式（#238）**：部分代理网关在非流式请求下返回 OpenAI 格式 JSON（`choices`/`message`/`tool_calls`/`usage.prompt_tokens`），新增 `parse_anthropic_json_response` 统一解析入口，缺少 `content` 字段时回退解析 OpenAI 结构并合成 Anthropic block。

### Fixes

- **recap 模型来源与 compact 解耦（#237, #238）**：原实现复用 `compact_model`，用户关闭「自动压缩」（或设 `DISABLE_COMPACT` / `DISABLE_AUTO_COMPACT`）后 `/recap` 与自动回顾会永久失效；改为独立 `aux_model`，不受 compact 开关影响。
- **recap 渲染接入 i18n 并移除死代码（#238）**：后缀提示原本从 LLM 摘要文本 `rfind`（摘要本身不含该后缀，生产环境永不显示）且硬编码英文，改为 i18n key `app-recap-hint`；删除无任何代码路径触发的 `※ recap:` 特殊渲染分支。

## v0.6.75 — 2026-09-24

### Features

- **工具参数 Schema 校验可读性优化与启发式诊断（#234, #235）**：重构 `validate_against_schema`，对齐 Claude Code `formatZodValidationError` 风格，结构化分项输出缺失参数、意外未定义参数与类型不符；基于参数特征指纹对 `WebFetch`（误传 `url`+`prompt`）、`Bash`（误传 `command`）、`Agent`（缺失子 agent 类型且未设 `fork`）提供启发式纠偏建议；新增同一工具连续 2 次参数校验失败熔断拦截（Circuit Breaker），防止模型陷入盲目重试死循环。

### Fixes

- **引入独立输入泵以安全启用鼠标悬停并修复消息区滚动条交互（#232, #236）**：通过独立后台输入泵（`InputPump`）隔离 Windows 控制台下的重入与阻塞风险，安全消费 `MouseEventKind::Moved` 鼠标悬停事件；精确计算消息区滚动条滑块交互范围与相对拖拽位移，消除滚动条点击漂移与量化跳跃。

---

## v0.6.74 — 2026-09-23

### Fixes

- **RTK git status 噪音过滤与移除毒性通用折叠（#207, #214, #233）**：针对 RTK 格式的 `clean — nothing to commit` 增加专门匹配清洗；移除通用多行重复块折叠（避免代码上下文被意外吞并），恢复兜底分支原始输出。

---

## v0.6.73 — 2026-09-23

### Fixes

- **Windows 下执行外部子进程触发全屏黑白闪屏（#229, #231）**：解决在 Windows 平台下执行 PHP 等子进程时未隔离控制台，导致外部运行时修改控制台代码页（936 与 65001 切换）触发宿主终端自愈物理清屏（`terminal.clear()`）的问题；在 Windows shell 创建时增加 `CREATE_NO_WINDOW (0x08000000)` 标志实现控制台完全隔离，子进程代码页变动不再干扰父 TUI 会话。

### Performance

- **长内容下高频鼠标滚轮防抖批处理与滑块平滑拖拽（#230, #231）**：引入有界事件批处理机制（`EventReader`），合并高频连续滚轮事件，大幅减少排队重绘导致的掉帧与迟滞；从真实帧缓冲区读取滚动条滑块区域并以初始按下位置为锚点做相对计算，消除滑块点击漂移与量化跳跃。

---

## v0.6.72 — 2026-09-23

### Features

- **消息区 Markdown 超链接点击打开默认浏览器（#225, #228）**：解决此前渲染层丢弃 `dest_url` 导致 Markdown 超链接 `[text](url)` 无法点击、终端无法探测的问题；在 `peri-widgets` 和 `peri-tui` 链路保留链接目标与字符级命中区，消息区单次点击链接即可跨平台唤起系统默认浏览器（Windows / macOS / Linux），并与文本选区、多行折行、引用块、列表项完整对齐。

---

## v0.6.71 — 2026-09-23

### Changes

- **禁用 sticky header 顶部固定消息条（#218, #219）**：消息区顶部最近用户消息固定条信息重复、占用布局高度且过长时截断，整体禁用渲染，布局高度归还消息区；关联 headless 测试统一标记 `#[ignore]`，恢复功能时删除标记即可。

### Fixes

- **附件栏标题与 Del 提示接入 i18n 多语言（#222）**：修复英文环境下「待发送附件」栏标题与 Del 删除提示仍显示硬编码中文的问题，文案接入 `lc.tr()` i18n 链路并补齐中英 locales，附件栏文案跟随语言设置切换。

---

## v0.6.70 — 2026-09-23

### Fixes

- **TUI 状态栏模型已切换但网关请求仍用旧模型（#169, #215）**：解除 ACP Client 在无会话时对配置同步的静默丢弃，`session/new` 与 `session/load` 消费 `model` 参数，修复发送首条消息前切换模型 100% 不生效的缺陷；`/history` 恢复历史会话传参规范化并加服务端全名反查兜底，防止恢复后回退默认模型。
- **快捷键调整与 Tips/文档收口（#169, #215）**：废弃 `Ctrl+T`/`Alt+M` 模型循环快捷键（模型切换统一走命令面板），新增 `Alt+P` 等价打开命令面板（`Ctrl+P`/`Alt+P`）；纠正 tips 中"长按 Ctrl+V"、"Ctrl+N/P 切换 Session"等与代码不符的描述，补齐 PageUp/Down 等快捷键的触发条件；同步清理 CLAUDE.md、README、注释中的 `Ctrl+T` 残留并修正"禁止 PageUp/PageDown"的过时规范。

---

## v0.6.69 — 2026-09-23

### Fixes

- **工具调用可靠性：边界提示词、字段级参数校验与动作循环检测（#200, #211）**：系统提示词新增工具边界互斥与参数错误处置规则；工具参数调用前按 JSON Schema 预校验，报错包含缺失字段名、期望类型与实际收到的 keys；新增动作签名循环检测，连续 3 轮相同工具动作时注入纠正消息，防止无效重复动作。
- **多行命令 stdout 截断修复（#212, #211）**：Windows 上含字面换行符的多行命令改走 Git Bash 执行，不再被 `cmd /C` 截断为首行输出，`node -e` 等多行脚本可完整返回全部 stdout。
- **命令输出通用重复行/块折叠（#214, #211）**：`filter_command_output` 兜底分支新增通用折叠——首行相同的多行块或连续相同行出现 ≥3 次时保留前 2 个样例并附折叠摘要，高重复度构建警告不再原样刷入上下文。

---

## v0.6.68 — 2026-09-21

### Fixes

- **移除 Git Bash fallback 重试标记混入 LLM 上下文（#209, #210）**：在 Windows 下 Bash 工具 cmd /C 执行失败后使用 Git Bash 重试时，不再将 `[Retried with Git Bash]` 机制性元信息追加到输出末尾，避免干扰 LLM 上下文。
- **Read 工具图片魔数校验与伪图片防御（#207, #208）**：在读取图片前先验证文件头部魔数字节（Magic Bytes），防止文本或损坏文件被误传为图片导致模型报错。
- **消息气泡长段落续行悬挂缩进对齐修复（#205, #206）**：修复 TUI 渲染长消息气泡时段落续行的悬挂缩进对齐问题。
- **修复 Windows 8.3 短文件名波浪号拦截与测试时序（#203, #204）**：修复剪贴板在 Windows 8.3 格式短文件名中包含 `~` 时的错误拦截，并增强 headless 测试时序稳定性。

---

## v0.6.66 — 2026-09-21

### Features & Improvements

- **执行中消息队列与按轮次 steering**：Agent 执行期间输入的消息进入待发队列，按轮次以增量 StateSnapshot 注入执行循环，不再打断当前执行或丢失历史。
- **粘贴本机图片路径自动转为附件**：粘贴 `file://` URL 或引号包裹的绝对路径时直接识别为图片附件，避免把路径文字当图片发送。

### Fixes

- **修正 headless 多轮消息快照回归测试（#201, #202）**：按增量 steering 协议补齐 `begin_round` 并发送第二轮增量消息，仅修正测试建模，不改变生产逻辑。

---

## v0.6.64 — 2026-09-21

### Features & Improvements

- **清洗 RTK 外部宿主 stderr 提示噪音（#184, #185）**：过滤 RTK 外部宿主写入 stderr 的提示噪音，避免误导 Agent 判断。

---

## v0.6.63 — 2026-09-20

### Features & Improvements

- **对齐本地命令块渲染样式（#182, #183）**：
  - 将用户 `!` 本机命令展示重构为与 Claude Code 一致的命令块布局；
  - 标题行使用整行背景与粉色 `!` 前缀，输出内容以 `  └ ` 引导的树状缩进对齐呈现；
  - 隐藏命令块标题中冗余的 exit code 与 cwd 信息，无输出时以 `(No output)` 占位提示；
  - 保留完整底层执行、ANSI 解析、超长折叠截断与后台化控制逻辑。

---

## v0.6.62 — 2026-09-20

### Features & Improvements

- **状态栏新增 Prompt Cache 命中率指标**：以常驻百分比替换瞬时 CPU 指标，并根据命中率分级显示颜色，继续保留 MEM 指标。
- **移除消息区冗余的低命中率警告气泡**：低于 80% 时仅保留 tracing 告警，缓存状态统一由状态栏展示，避免打断正常阅读。

---

## v0.6.60 — 2026-09-20

### Features & Improvements

- **对齐 OpenAI Codex 规范的动态终端标题（Status Surface）体系（#162, #177）**：
  - 参考 OpenAI Codex CLI (`codex-rs`) 官方规范，将终端标题提升为标准的状态表面抽象；
  - **双轨制会话主题提炼**：首轮 Prompt 发送后，轨 1 本地确定性提取器 0ms 即时渲染标题；轨 2 在后台异步派发超轻量 LLM 总结任务精准提炼主题并更新（支持 `PERI_DISABLE_TITLE_GENERATION` 环境变量关闭），具备会话属主校验与防覆盖手动 `/rename` 机制；
  - **动态生命周期状态感知**：任务执行期间点阵/菊花帧帧动态旋转；回复完成后展示标志性橙色菊花 `✴`；HITL 审批与提问交互等待时提升为 `[ ! ] Action Required` 呼吸闪烁；
  - **底层安全清洗与写出去重**：过滤控制字符与 Trojan Source / Bidi 隐形字符，字形簇截断保护，OSC 0 内容变动才写出，消除 Idle 态无谓 I/O 抖动；
  - **宿主终端保护**：进入/退出 AlternateScreen 时通过 XTerm title stack（`\x1b[22;0t` push / `\x1b[23;0t` pop）保护并 100% 还原宿主原有标题。

---

## v0.6.59 — 2026-09-20

### Features & Improvements

- **支持 Alt+V 快捷键粘贴图片附件并对齐分支规范（#165, #166）**：
  - 针对 Windows Terminal / PowerShell / VS Code 等现代终端在宿主层拦截 `Ctrl+V` 用于纯文本粘贴、导致系统剪贴板中的图片无法传入 TUI 的问题，新增 `Alt+V` 专用快捷键（兼容大写 `V` 及 macOS `Option+V` 字符 `√`），实现稳定穿透终端宿主触发剪贴板图片提取并挂载为待发送附件；
  - 文本粘贴回退统一走 `paste_text_into_textarea` 管道，保障文件路径归一化与多行文本折叠占位符行为一致；
  - `CLAUDE.md` 明确本仓库分支规范为最高优先级，禁用 `#` 命名以保护 GitHub Actions PR CI 链路，并补充现代终端粘贴快捷键实战避坑指南。

---

## v0.6.58 — 2026-09-20

### Bug Fixes

- **修复 Windows 传统控制台歧义字符残影与 Markdown 表格视口溢出缺陷（#158, #159, #163）**：
  - 新增 `WidthSafeBackend` 终端适配层与 `ConsoleWidthProbe` 物理列宽探针，在 Windows 绘制输出边界动态探测物理光标偏移，针对不匹配的东亚歧义字符（`∴`、`●`）以及特殊符号（制表框 `+`、对勾 `v`、叉号 `x`、进度条 `#`/`-` 等）提供等宽平替，彻底解决 Windows CMD/conhost（新宋体 CP936）下光标偏移与行尾幽灵残影问题；
  - 终端生命周期集成 `TuiBackend`，在 `draw_app` 中感知字体、字号与代码页指纹变化自适应重绘与清理缓存；
  - Markdown 表格渲染引入视口硬约束和多列溢出防御削减，彻底杜绝表格宽度超出视口触发操作系统级硬折行；优化长路径断词，优先在路径分隔符（`/`、`\`、`-`、`_`）处断行，避免扩展名孤立；
  - 消息气泡渲染时预先扣除前缀缩进（2 字符），避免内容满宽后加前缀触发折行；
  - 修复 CI 在 Ubuntu 下的类型复杂度警告与 macOS 平台异步钩子测试超时。

---

## v0.6.57 — 2026-09-20

### Bug Fixes

- **修复工具包装器丢失多模态图片内容导致 Read 识图失败的缺陷（#157）**：
  - `ToolWrapper`、`ArcToolWrapper` 及 `peri-middlewares` 中的 Arc 包装器三个 `BaseTool` 包装器补齐 `invoke_content` 透传委托，此前包装器未转发该方法，结构化多模态内容（图片 Base64）在包装层被丢弃，图片无法进入模型上下文；
  - 新增 `read_multimodal_test.rs` 回归测试（8 项），覆盖图片内容经包装器完整传递的链路。

---

## v0.6.56 — 2026-09-20

### Features & Improvements

- **Read 工具支持多模态图片读取（#153, #154）**：
  - 在 `BaseTool` trait 中新增 `invoke_content` 方法和 `ToolContent` 结构体，支持工具返回结构化多模态内容（如图片 Base64），同时保持 `invoke` 纯文本接口 100% 向后兼容，所有现有工具无需任何修改；
  - `ToolResult` 增加 `content: Option<MessageContent>` 字段，调度链路优先使用结构化内容写入 state，打通工具→LLM 的多模态通道；
  - `ReadFileTool` 从二进制扩展名列表中移除 PNG/JPG/JPEG/GIF/WebP/BMP，新增图片识别→字节读取→Base64 编码→`ContentBlock::Image` 返回的完整链路，含 20MB 大小保护；
  - 搭配多模态模型（如 Claude 3.5/3.7 Sonnet、GPT-4o 等）时，Agent 可通过 `Read("image.png")` 直接读取本地图片进行视觉分析。

---

## v0.6.55 — 2026-09-18

### Bug Fixes

- **修复常驻子进程导致后台 shell 任务无限停留 running 的缺陷（#149, #150）**：
  - 为 `run_streaming_child` 和 `execute_shell_command_with_stdin` 引入管道超时保护机制（`PIPE_DRAIN_TIMEOUT = 2s`），在主进程退出后若常驻子进程（如 `Start-Process` 或脚本脱离启动的常驻服务）继承了管道写端句柄，超时后自动中止读取等待，彻底解决 reader task 永远等不到 EOF 导致后台任务永久停留在 `running` 状态且无法派发完成通知的问题；
  - 累积输出移入线程安全的共享缓冲区，即使触发管道超时，主进程退出前产生的所有标准输出和错误日志仍 100% 完整保留，杜绝输出丢失；
  - 使用 `tokio::join!` 并发等待 stdout 与 stderr 读取任务，保证最差等待耗时严格控制在 2 秒以内。

---

## v0.6.54 — 2026-09-18

### Bug Fixes

- **消除 agent Bash 运行提示与生命周期切换时的终端清屏闪烁（#145, #146）**：
  - 移除 `register_agent_shell`、`background_agent_foreground`、`poll_agent_shells` 对 `request_terminal_clear_redraw()` 的调用，依赖 Ratatui 原生单元格 Diff 驱动平滑重绘，消除物理清屏 `terminal.clear()` 带来的瞬间黑屏/白屏闪烁；
  - 重构 `render_thread.rs` 中的秒数刷新逻辑，移除粗暴的 `message_hashes.clear()`；首次跨越 2s 阈值时仅增量失效当前消息 hash，超过 2s 持续运行时仅增量更新秒数行与动画帧，100% 复用历史消息缓存，避免高频全量重绘抖动。

---

## v0.6.53 — 2026-09-18

### Features & Improvements

- **工具行 Header 超长改为单行省略号截断（#141）**：
  - 移除此前工具调用 Header 的多行折行与 8 列缩进，恢复并保证严格单行展示，不再挤占视口垂直高度；
  - 新增 `truncate_to_display_width`，结合 `unicode-width` 按显示列宽动态计算与截断，并在末尾优雅追加 `…` 闭合括号；
  - 支持中英文长参数与长路径自适应截断，聚合工具组同步对齐，彻底杜绝换行。

### Bug Fixes

- **对齐 Spinner 完成态总结行与生命周期修复（#139）**：
  - 对齐 Claude Code 风格的过去式总结行动词与时刻展示（`✻ {verb} for {elapsed} · done {HH:MM}`）；
  - 移除 loading 态多余前导空格对齐 column 0，消除任务完成瞬间的水平抖动；
  - 重构中断（Ctrl+C）与报错生命周期清理，防止异常状态误展示成功动词。

---

## v0.6.52 — 2026-09-17

### Bug Fixes

- **状态栏运行耗时秒数补零与 Emoji 宽度对齐**：修复状态栏耗时未补零导致的字符残影和抖动，以及变体选择器导致的 Emoji 列宽错位
  - 秒数与分钟统一增加 `{:02}` 补零（如 `31m06s`），固定字符串渲染宽度，消除由 59 秒进入个位数秒时少 1 个字符造成的残影
  - 替换含 `U+FE0F` 的 `⏱️` 为标准单字符 `⏱`，消除不同终端下光标列宽计算偏移造成的字符错位叠加

---

## v0.6.51 — 2026-09-17

### Bug Fixes

- **Anthropic 适配器支持自适应解析 SSE 流式响应**：修复非流式请求（如后台 SubAgent 任务）收到反向代理网关强制返回的 SSE 流时，因直接调用 `serde_json::from_str` 导致反序列化崩溃的问题
  - 在非流式请求体构建中显式声明 `"stream": false`
  - 在 `handle_anthropic_response` 中自适应捕获 SSE 文本，复用 `SseParser` 还原为标准的 Anthropic 消息 JSON 结构，无损向下兼容各种代理/网关环境
  - 增加针对文本、工具调用（tool_use）、错误事件的 3 组完整回归测试用例

---

## v0.6.50 — 2026-09-17

### Bug Fixes

- **Markdown 终端宽度变化重复渲染**：修复终端宽度变化时，因未清空旧行缓存导致增量追加模式错误触发、内容翻倍重复渲染的问题
  - 宽度变化时显式清空 `rendered.lines`，彻底重置渲染流状态
  - 增加宽度变更防重复渲染的单元测试锁定正确行为

---

## v0.6.49 — 2026-09-17

### Refactor

- **移除 FolderOperation 工具**：
  - 彻底移除 `FolderOperationsTool` 及其全部单元测试，将文件系统工具集收敛为标准的 5 个工具（`Read`, `Write`, `Edit`, `Glob`, `Grep`）
  - 核心工具白名单（Core Tools）由 12 个收敛为 11 个
  - 清理 HITL 审批白名单、ACP 协议映射（`infer_tool_kind`）与 TUI 展示层中的冗余映射逻辑
  - 同步更新系统提示词和相关架构规范文档

---

## v0.6.48 — 2026-09-17

### Bug Fixes

- **Markdown 表格渲染挤压与空列**：宽终端下 AI 消息中的表格按固定 80 列渲染导致列被压成每行 2-3 个中文字、右侧留白，且表格尾部多出空白列
  - 表格渲染宽度改为跟随真实终端宽度，resize 时自动重解析
  - 修复 TableBuilder 行尾重复 push_cell 导致的多余空列
  - 列宽分配改为累积比例，空列不再吞掉剩余宽度
- **Clippy 兼容 Rust 1.98**：修复 `manual_slice_fill`、`drain_collect`、`chunks_exact_to_as_chunks` 三类新 lint
- **Windows CI 测试稳定性**：长前台命令测试改用 ping 模拟 sleep，消除 PowerShell 冷启动超时

---

## v0.6.47 — 2026-07-15

### Bug Fixes

- **provider_type 大小写不敏感**：settings.json 中 `type` 字段写成 `Anthropic`/`ANTHROPIC` 等大小写形式不再错误 fallback 到 OpenAI
- **Clippy 兼容 Rust 1.97**：修复 `input_field.rs` 的 `useless_borrows_in_formatting` 和 `markdown/mod.rs` 的 `manual_clear`

---

## v0.6.46 — 2026-07-09

### Bug Fixes

- **npm install 配置迁移字段格式修复**：`migrateFromClaudeCode` 输出改为 Rust 配置期望的 camelCase 字段（`type`、`apiKey`、`baseUrl`），修复新用户从 Claude Code 迁移后配置无法识别的问题
- **AgentShellSlot 计时器冻结**：任务结束后 `elapsed()` 不再持续增长，行为对齐 `BackgroundShell`

### Docs

- **README 默认英文**：`README.md` 切换为英文，`README_ZH.md` 保留中文版

---

## v0.6.45 — 2026-07-09

### Refactor

- **状态栏重构为 codebuddy-hud 风格**：动态 2-3 行布局，支持多工具并发跟踪、Git dirty 状态标记、上下文进度条优化

### Bug Fixes

- **clippy items-after-test-module**：将 `status_bar.rs` 测试模块移到文件末尾
- **tip-6 快捷键描述修正**：Ctrl+U/D → PageUp/Down（Ctrl+U 是删除到行首，Ctrl+D 是关闭 shell stdin）

---

## v0.6.44 — 2026-07-03

### Features

- **agent Bash 超时自动后台化**：BashTool 超时时不再无条件杀进程，支持通过 `auto_background_tx` 通知 TUI 将仍在运行的前台任务自动转后台继续运行
- **Windows cmd /C 引号修复**：移除 `has_cmd_special_chars` + `/S /C` 包裹，改用 `raw_arg` 传递命令文本，修复 `python "D:/x.py"` 引号泄漏到 argv 的问题
- **Ctrl+B 交互优化**：后台化后先聚焦底部 shell 入口，Enter 才打开面板，避免遮挡主视图

### Bug Fixes

- **clippy unused import + needless_borrow**：修复 `terminal.rs` 中 `timeout` 导入和 `&command` 引用的 clippy 错误

---

## v0.6.43 — 2026-07-02

### Refactor

- **状态栏工具名映射统一**：删除 `status_bar.rs` 中重复的 `format_tool_display_name` 函数，复用 `tool_display::format_tool_name`，修复 `FolderOperations` 在状态栏未简写为 "Folder" 的问题

---

## v0.6.42 — 2026-07-02

### Bug Fixes

- **后台面板输出自适应终端高度**（#110）：后台任务面板 output 区域根据终端高度动态调整，避免内容溢出
- **Ctrl+B 显示已运行时间**（#110）：后台 shell 任务在面板中展示已运行时长
- **Bash Ctrl+B 提示计时修正**：`Ctrl+B` 提示计时起始点修正，message pipeline 新增 transform/reconcile 阶段支持后台状态注入

---

## v0.6.41 — 2026-07-02

### Bug Fixes

- **MultiplexBroker 竞速跳过全 Reject**：ChannelBroker 无授权时不再抢先返回 Reject，MultiplexBroker 继续等待 TUI broker 的 Approve，消除「no authorized channels」误报
- **Ctrl+B 竞态**：`background_agent_foreground` 入口处 drain pending agent shell 注册，避免延迟注册未到达时 Ctrl+B 找不到前台 shell
- **/model 命令动态 alias**：改为从当前 provider 动态收集非空 alias 匹配，替代硬编码 opus/sonnet/haiku
- **Command 模式光标偏移**：draw_bar_cursor 改用 display_textarea 作为光标源，修正 ! 前缀移除后的位置偏移
- **tip 文案同步**：tip-2 改为「当前 Provider 的可用模型间切换」，tip-6 改为 Ctrl+U/D 滚动

---

## v0.6.40 — 2026-07-02

### Features

- **状态栏权限模式 cycle 提示**：权限模式标签后追加 `(Shift+Tab to cycle)` 灰色提示，方便用户发现快捷键

---

## v0.6.39 — 2026-07-02

### Features

- **跨 provider 模型切换**（#107）：model selection 值使用 `provider_id::alias` 格式，支持同名 alias 跨 provider 正确切换；模型列表展示所有 provider 的可用模型

### Bug Fixes

- **状态栏工具历史行对齐**（#105）：修复第二行在无内容时缺少前导空格导致与其他行左对齐不一致
- **Bash 输出预览窗口缩小**（#109）：Bash 输出截断阈值从 2000 行/100KB 降为 50 行/20KB，完整内容落盘供 Read 按需查看，避免大输出撑爆 context window

---

## v0.6.38 — 2026-07-01

### Bug Fixes

- **模型名上下文窗口标记过滤**：配置中 `mimo-v2.5-pro[1M]` 等含 `[...]` 后缀的模型名，传 API 时自动过滤为 `mimo-v2.5-pro`

---

## v0.6.37 — 2026-07-01

### Bug Fixes

- **状态栏第二行占位恢复**：修复工具执行前默认占位丢失的问题

---

## v0.6.36 — 2026-07-01

### Bug Fixes

- **状态栏第二行快捷键调整**：默认 hints 从 `Tab ::切换模式` 改为 `Ctrl+O ::详情`；详细模式新增 `● Verbose` 标识 + `Ctrl+O ::退出详细`；format_hints 支持空描述跳过

---

## v0.6.35 — 2026-07-01

### Features

- **PermissionMode 循环跳过 DontAsk**：Shift+Tab 循环切换权限模式时跳过 DontAsk，顺序变为 Default → AcceptEdit → AutoMode → Bypass
- **shell_command_with_shell 新增**：支持显式指定 shell（powershell/pwsh/bash），Hook executor 现在正确传递 shell 参数

### Bug Fixes

- **status_bar DontAsk 渲染修复**：补回 DontAsk match 分支防止 non-exhaustive 编译错误，删除残留 hint 引用

---

## v0.6.34 — 2026-07-01

### Features

- **滚动条和状态栏优化**（#104）：优化 TUI 滚动条和状态栏的渲染效果

### Bug Fixes

- **Windows CI 流式输出测试超时**：修复 Windows CI 环境下 Python 流式输出测试超时的问题
- **滚动条测试修复**：修复滚动条相关测试用例

### Documentation

- 完善项目所有 crate 的文档

---

## v0.6.33 — 2026-06-30

### Bug Fixes

- **控制字符渲染异常修复**（#102）：修复控制字符和 ANSI 转义序列导致 TUI 渲染异常的问题
- **Clippy 警告修复**：`map_or(true, ...)` 改为 `is_none_or(...)`，适配 Rust 1.95 新增的 `unnecessary_map_or` lint

---

## v0.6.32 — 2026-06-29

### Bug Fixes

- **后台 shell 通知显示优化**（#100）：后台 shell 完成/等待输入的 XML 通知在 TUI 聊天区显示为可读的中文提示（SystemNote），而非原始 XML 标签。前台小命令快速结束不再打断对话流，仅后台化（Ctrl+B）命令注入通知
- **`/review` skill fallback 测试修复**：mock skill 名称从 `review` 改为 `deploy`，避免与内置 `/review` 命令冲突
- **`Stdio` import 条件编译**：`peri-middlewares/terminal.rs` 的 `std::process::Stdio` 加 `#[cfg(windows)]` 修复非 Windows 平台 Clippy unused-import 错误

---

## v0.6.29 — 2026-06-28

### Features

- **Ctrl+B 后台 Shell**（#99）：Shell 命令支持 Ctrl+B 转为后台运行，输出写入磁盘，支持后台任务面板查看
- **`/commit` 命令**（#93）：一键 git commit，自动生成 commit message
- **`/review` 命令**（#95）：PR 代码审查
- **`/export` 命令**（#95）：对话导出
- **Read 工具行范围显示**（#91）：Read tool header 显示 offset/limit 行范围
- **全局屏幕选区（ScreenSelection）**（#94）：新增基于渲染 Buffer 的全局选区，覆盖面板、状态栏、sticky header、bg agent bar、空白区域。与消息区域现有 TextSelection 跨区域衔接，松开鼠标自动复制到剪贴板，蓝色高亮显示。详见 [spec/features/screen-selection-prd.md](spec/features/screen-selection-prd.md)
- **消息区域 TextSelection 内容锚定**：选区以消息内容为锚（而非屏幕坐标），滚动后选区跟随内容，复制纯文本不受 buffer 渲染影响
- **双击选整行**：消息区双击用 TextSelection 选整行（纯文本），其他非 textarea 区域双击用 ScreenSelection 选整屏行
- **spinner / 总结行可选可复制**：`✻ Brewed for...` + 进度条等位于 messages_area 底部但不在 wrap_map 内的行，现可通过 ScreenSelection 选中复制
- **选区 auto-scroll 改进**：auto-scroll 仅在鼠标移出消息区域外时触发，区域内首末行可正常选中（修复"最后 1 行难选中"）
- **复制 toast UX**：复制成功后状态栏显示 "已复制 N 个字符" toast

### Bug Fixes

- **拖选溢出 panic**：`visual_row + scroll_offset` 计算改用 `saturating_add`，修复 `scroll_offset=usize::MAX`（初始/提交/scroll_to_bottom 状态）时拖选导致 exit code 101 的崩溃
- **安装脚本 tag 前缀**：`install.sh` / `install.ps1` 匹配 `npm-v*` release tag 前缀
- **Clippy warnings**：`ShellCommandPool` Default derive + `map_or` → `is_none_or`

---

## v0.6.22 — 2026-06-27

### Security

- **MCP OAuth CSRF 防御**（#82）：回调服务器注入 rmcp 生成的 state 参数，纵深防御 CSRF 攻击
- **Session UUID 校验**（#74）：`session/load` + `session/resume` 增加 UUID 格式校验和存在性校验，防止路径穿越
- **文件权限加固**（#81）：history_persistence 文件权限 0600，grandparent 目录权限校验
- **At-mention 目录注入防护**（#77）：防止 `@path` 引用越权访问
- **工具输出截断加固**（#80）：防止超长输出绕过截断机制

### Bug Fixes

- **输入框鼠标乱码**（#88）：移除 `?1003h`（any-event tracking），防止 ConPTY 缓冲区溢出导致 SGR 鼠标转义序列泄漏为文本
- **Windows Ctrl+C 双击退出**（#86）：100ms debounce 防止 ConPTY 重复事件
- **AskUser 弹窗高度**（#76）：修复弹窗高度计算错误
- **Tab 缩进编辑**（#76）：修复 tab 缩进文件的 Edit 工具匹配问题
- **Grep offset 测试**（#86）：兼容 `persist_truncated_output` 附加行

### Chore

- npm 版本 bump 到 0.6.22

---

## v0.6.21 — 2026-06-27

### Features

- **Windows Git Bash fallback**：`cmd /C` 执行 Linux 命令（`grep`/`ls`/`find` 等）失败时自动 fallback 到 Git Bash，Agent 无需自行重试
  - 多语言 stderr 匹配（English/中文/法语/德语）+ 兜底模式
  - `GIT_BASH_PATH` 环境变量支持，`bash --version` 验证
  - `MSYS_NO_PATHCONV=1` 防止 MSYS 路径转换
  - 剩余超时继承（总超时 - cmd 耗时，至少 10s）
  - `git commit -m` 重写与 fallback 的 temp 文件清理时序修复

---

## v0.6.19 — 2026-06-26

### Bug Fixes

- **Windows GBK 编码修复**：`shell_exec.rs`（TUI `!command`）和 `executor.rs`（hook）的 subprocess stdout/stderr 在 Windows 中文环境下正确解码 GBK→UTF-8，不再显示乱码
- 共享 `decode_output_bytes()` 提取到 `peri-agent/encoding.rs`，消除重复代码
- `shell_exec.rs` 中文 anyhow context 消息改为英文

---

## v0.6.18 — 2026-06-26

### i18n

- **peri-lsp 硬编码中文改英文**：error.rs 12 处、transport.rs 8 处、client.rs 4 处、pool.rs 8 处，共 32 处 `#[error()]` / tracing / 错误字符串
- **peri-agent 硬编码中文改英文**：sqlite_store.rs 2 处、filesystem.rs 4 处，共 6 处 anyhow context 消息

---

## v0.6.17 — 2026-06-26

### i18n

- **spinner 和 thinking 块跟随语言设置**：peri-widgets 新增 `set_mode_with_label()` / `pick_verb_from()` 接口，TUI 调用方通过 `lc.tr()` 传入翻译后的 label。用户 `/lang en` 后 spinner 显示 "Thinking…"，`/lang zh-CN` 显示 "思考中…"
- 新增 `spinner-thinking` / `spinner-tool-use` / `spinner-responding` / `spinner-thinking-header` 翻译 key（en + zh-CN）

---

## v0.6.15 — 2026-06-25

### Bug Fixes

- **TUI 模型切换快捷键收敛**：删 Ctrl+Shift+T / Alt+Shift+M（与 Ctrl+P 命令面板重叠），统一 Ctrl+P 作为 Provider/Model/Effort 完整选择入口
- **Ctrl+T 硬编码 alias bug**：原硬编码 `[opus, sonnet, haiku]` 三选一，未按当前 Provider 实际配置过滤——切到只配 1 个 alias 的 Provider 时无法切换。改为从激活 Provider 的 `ProviderModels` 动态收集非空 alias

### Documentation

- CLAUDE.md 新增「PR / Issue 流程」+「分支命名规则」段落
- 分支名禁用 `#` 字符（会让 GitHub Actions `pull_request` trigger 静默失效）
- spec/issues/ 补模型切换快捷键收敛 issue 详细分析文档

### Chore

- 清理误传的 `.claude/CLAUDE.md`（adim 钉钉/PowerShell/PHP 那套无关内容），加入 `.gitignore`

---

## v0.6.13 — 2026-06-25

### npm 包

- npm 包二进制命名统一为 `cc-code-*`（原 `peri-*`），与 CI workflow 对齐
- `install.js` 下载文件名、解压目标、Windows wrapper 全部改为 `cc-code`
- `bin/cc-code` wrapper 查找 `cc-code-bin` / `cc-code.exe`
- `npm/README.md` 命令示例更新为 `cc-code`
- 删除 `mimo-code-vs-peri-analysis.md`

---

## v0.99.14 — 2026-06-02

### Performance

- 全局分配器从 mimalloc 切换到 jemalloc，碎片管理更优
- tokio worker_threads 限制为 4，18 核机器节省约 56 MB 栈空间
- list_threads 排除 cached_context 大字段，每线程内存从约 1 MB 降至约 1 KB
- LlmCallStart.messages 改为 Arc\<Vec\>，消除每次 LLM 调用的全量 clone
- history_for_cancel 用 Option\<MessageId\> 替代完整消息 clone

### Features

- **Rewind 对话回滚**：双击 ESC 弹窗选择回滚点，支持 /rewind 命令
- **/gc 命令**：手动内存回收 + RSS/jemalloc breakdown 诊断

### Bug Fixes

- PermissionRequest hook 在 Bypass/AutoMode 下不应触发
- 从 ~/.claude/settings.json 加载全局 hooks + TUI 退出时 fire SessionEnd
- /clear 时关闭旧 session 防内存泄漏
- 过滤 ACP 下发命令中与本地注册表重复的条目
- AgentResult invoke 消息优化，防止 LLM 轮询循环

### Refactoring

- CLAUDE.md 拆分为子模块文件
- 提取 ACP 共享逻辑，消除 TUI/Stdio 重复代码
- 移除 /split 命令
