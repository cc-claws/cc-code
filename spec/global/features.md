# 已有功能清单

![功能模块概览](./images/05-feature-modules.png)

## 核心引擎（cc-agent）

- **ReAct 循环执行器:** `ReActAgent` 支持最多 50 次迭代，思考 → 工具调用 → 反馈自动推进，parallel 工具调用（同轮多工具同时执行）
- **MockLLM 测试工具:** `MockLLM::tool_then_answer()` 按脚本回放推理序列，无需真实 API，覆盖单元测试场景
- **OpenAI 适配器:** 支持 `message.reasoning_content`（DeepSeek-R1/o 系列），streaming SSE，`type:"function"` 工具格式
- **Anthropic 适配器:** Prompt Cache（默认开启，最后消息末尾 `cache_control:ephemeral`），Extended Thinking（`budget_tokens`），`system` 字段 blocks 格式
- **MessageAdapter 双向转换:** `OpenAiAdapter` / `AnthropicAdapter` 实现 `MessageAdapter` trait，`BaseMessage` ↔ Provider 原生 JSON
- **ContentBlock 完整支持:** Text / Image（Base64 & URL）/ Document / ToolUse / ToolResult / Reasoning / Unknown 透传
- **Middleware Chain:** `Middleware<S>` trait，`before_agent` / `after_agent` / `before_tool` / `after_tool` / `collect_tools` 五个钩子
- **系统提示词段落化:** 12 个 .md 段落文件（8 静态+4 feature-gated），PromptFeatures 条件注入，include_str! 编译时嵌入
- **消息管线统一:** MessagePipeline 唯一入口，PipelineAction 枚举，ToolStart+ToolEnd 事件拆分
- **尾部重建:** reconcile_tail() 方法，Done/Interrupted 时触发，RebuildAll 只替换尾部
- **工具参数 Schema 预校验:** 对齐 Claude Code `formatZodValidationError` 风格，结构化分项输出缺失参数 / 意外参数 / 类型不符；`suggest_tool_mismatch` 启发式诊断（基于参数特征指纹对 WebFetch/Bash/Grep/Read/Agent 给出纠偏建议）；`SchemaFailureTracker` 熔断器（同一工具连续 2 次校验失败即拦截，防盲目重试循环）
- **工具动作签名循环检测:** 连续 3 轮相同工具动作时注入纠正消息，打断重复循环
- **Anthropic 适配器（自适应兼容）:** 非流式响应自适应解析反向代理返回的 OpenAI 格式（`choices` / `message` / `tool_calls`），缺 `content` 时回退解析；SSE 流式自适应解析
- **/recap 会话回顾:** 独立 `aux_model`（与 compact_model 解耦），单轮禁用工具、不写 history、支持 Ctrl+C 取消

## 中间件（cc-middlewares）

- **FilesystemMiddleware:** 提供 `Read`、`Write`、`Edit`、`Glob`、`Grep` 五个工具；只读工具无需 HITL
- **TerminalMiddleware:** 提供 `Bash` 工具，120 秒超时，跨平台（Windows: `cmd /C`，其他: `bash -c`）；Windows 下 `cmd /C` 失败且 stderr 匹配"命令未识别"时自动 fallback 到 Git Bash（`bash -c`），多语言 stderr 匹配（English/中文/法语/德语）+ 兜底模式，`MSYS_NO_PATHCONV=1` 防路径转换，剩余超时继承
- **HitlMiddleware:** `before_tool` 拦截敏感操作（bash/write/edit/delete/rm），四种决策：Approve / Edit / Reject / Respond；oneshot channel 异步等待用户决策
- **SubAgentMiddleware:** 提供 `Agent` 工具，读取 `.claude/agents/{id}.md`，工具集过滤（tools 白名单 + disallowedTools 黑名单），防递归（始终排除 `Agent` 自身），返回格式含工具调用摘要
- **SkillsMiddleware:** `before_agent` 扫描加载 Skills（`~/.claude/skills/` → `skillsDir` → `./.claude/skills/`），prepend System prompt
- **AgentsMdMiddleware:** `before_agent` 自动读取 `CLAUDE.md` / `AGENTS.md`，prepend System prompt
- **TodoMiddleware:** `after_tool` 解析 `TodoWrite` 结果，推送 Todo 状态到渲染 channel
- **AskUserTool:** `AskUserQuestion` 工具（对齐 Claude AskUserQuestion），入参为 `questions` 数组（1–4 个），每题含 `question` 问题文字、`header` 短标签（≤12字）、`multi_select` 字段、`options`（每项含 `label` + `description`），始终允许自定义输入；oneshot channel 挂起等待用户输入
- **Token 追踪:** TokenTracker 累积追踪 input/output/cache tokens，ContextBudget 上下文窗口预算管理
- **Micro-compact:** 零 API 调用轻量压缩，可压缩工具白名单 + 时间衰减清除，图片/文档替换
- **Full Compact:** 9 段结构化摘要模板，工具对完整性保护，PTL 降级重试
- **LLM 重试:** RetryableLLM<L> 装饰器，指数退避+25%随机抖动，LlmRetrying 事件通知
- **rg CLI 双引擎文件搜索:** 优先外部 ripgrep 二进制，按 `PERI_RG_PATH` → exe 同级 `bin/rg(.exe)` → 系统 `PATH`（直接执行 `rg --version` 探测，不依赖 which/where）顺序解析，缓存到进程级 `OnceLock`；探测失败回退纯 Rust 引擎（grep + grep-regex crate，WalkParallel 多线程并行，15 秒超时）。npm install.js 自动下载 rg 预编译二进制；Grep/Glob 双引擎输出格式对齐
- **MCP 中间件:** McpMiddleware 作为 MCP Client 连接外部服务器（stdio/HTTP），`mcp__{server}__{tool}` 动态工具注册，`mcp_read_resource` 资源读取工具，双层配置合并（全局 settings.json + 项目 .mcp.json），${VAR} 环境变量展开
- **MCP 运行时管理:** /mcp 面板（Browse/Tools/Resources 三视图），后台连接池初始化不阻塞 TUI，重连/删除服务器
- **MCP OAuth 2.0:** rmcp auth feature + AuthClient，Authorization Code + PKCE 流程，401 自动触发，Token 持久化 ~/.peri/oauth_tokens.json（0600），混合回调（本地 HTTP → TUI 手动粘贴），回调服务器注入并严格校验 rmcp 生成的 state 参数（CSRF 纵深防御）
- **工具名称对齐 Claude Code:** 10 个内置工具名称完全对齐（Read/Write/Edit/Glob/Grep/Agent 等），Grep 重构为结构化接口，HITL 默认审批清单同步更新
- **RTK 代理双轨制:** 探测外部 `rtk` 二进制（where/which），对 git/cargo/npm/pnpm/npx/yarn/bun/docker/kubectl/pytest/python/php/go/dotnet/tsc/eslint/gh/find/grep/rg/ls/tree/cat/diff/curl/wget 等 23 类命令执行 `rtk rewrite` 重写以降低输出 Token；失败回退原始命令；过滤 RTK 宿主 stderr 噪音
- **output_filter 噪音清洗:** RTK git status 噪音清洗（`clean — nothing to commit`）；移除毒性通用折叠（避免吞并代码上下文），仅对 git status 做针对性剔除
- **Read 多模态读取:** 图片读取 + 魔数校验（Magic Bytes）防伪造图片；工具包装器透传 invoke_content 保证多模态内容不丢失
- **Bash 跨平台健壮性:** 多行命令（含字面换行）在 Windows 改走 Git Bash 避免 `cmd /C` 截断；管道超时保留已累积输出；后台 shell 任务在 fork 常驻子进程后不再永久挂起；移除 Git Bash fallback 重试标记（`[Retried with Git Bash]`）混入上下文

## TUI 界面（cc-tui）

- **多会话历史:** `SqliteThreadStore` 持久化会话，`/history` 面板浏览（j/k 导航，d 删除，Enter 打开，Esc 新建）
- **模型别名映射:** 四档别名 opus/sonnet/haiku/fable（`ALL_ALIASES: [&str; 4]`），`/model` 四 Tab 面板（`AliasTab::{Opus,Sonnet,Haiku,Fable}`），`/model <alias>` 快捷切换；模型切换快捷键已废弃 Alt+M/Ctrl+T，统一走 Ctrl+P/Alt+P 命令面板
- **TUI 命令:** 共 30 个 TUI 命令 + 7 个 ACP 命令；含 `/clear` 清空消息、`/help` 命令列表、`/compact` 上下文压缩、`/config` 全局配置、`/cost` 费用统计、`/context` 上下文使用率、`/memory` 编辑 CLAUDE.md、`/mcp` MCP 管理面板、`/recap`(别名 away/catchup) 会话回顾、`/commit`(ci)、`/review`(pr)、`/export`(save)、`/gc`、`/lang`、`/init`、`/setup`、`/tasks`、`/agent`、`/channel`(ch)、`/rename`、`/effort`、`/loop`、`/cron`、`/doctor`、`/hooks`、`/plugin`、`/agents`、`/exit`(quit)、`/model`、`/history`(resume)；Command trait 支持 alias 机制
- **Skills 补全:** 输入 `#` 触发 Skills 浮层，Tab 导航，Enter 补全为 `#skill-name`；发送含 `#skill-name` 的消息时自动通过 `SkillPreloadMiddleware` 将 skill 全文注入 agent state（fake Read 工具调用序列）
- **HITL 弹窗:** `ApprovalNeeded` 事件触发审批弹窗，展示工具名称和参数，支持 Approve / Edit / Reject / Respond
- **AskUser 弹窗:** `AskUserBatch` 事件触发问答弹窗，支持批量问题，单选/多选
- **YOLO 模式:** `-y` 参数启动，自动 Approve 所有 HITL 请求（不影响 ask_user）
- **剪贴板图片粘贴:** `Ctrl+V` 读取 PNG 图片，Base64 编码为 Image ContentBlock，支持多张图片
- **渲染线程分离:** 独立渲染线程（`parking_lot::RwLock<RenderCache>` + `Notify` 驱动），零 sleep，与 Agent 执行线程解耦，按需重绘
- **Headless 测试模式:** `App::new_headless(w, h)` + `ratatui TestBackend`，与生产渲染管道完全一致，用于 CI 集成测试
- **弹窗滚动支持:** 所有面板（AskUser/Model/Agents/Thread）高度限制在屏幕 80%，内容超长可 ↑↓ 滚动
- **Bracketed Paste Mode:** `Ctrl+V` 粘贴多行文本，保留换行不触发 Enter 提交
- **Loading 输入缓冲:** Agent 运行中可继续输入，消息自动缓存，完成后合并发送
- **TODO 状态面板:** 输入框上方固定面板，颜色分类（InProgress 黄/Completed 暗灰/Pending 白）
- **Welcome Card:** 空消息时显示品牌 ASCII Art Logo + 功能亮点 + 命令提示，发送消息后自动消失，窄屏降级为文字标题
- **Sticky Human Message Header:** 聊天区顶部固定显示最后一条 Human 消息（1-3 行截断），滚动时不随之移动，/clear 后消失，打开历史 Thread 自动恢复
- **配色系统（v1.1）:** 橙色仅保留最高优先级交互（命令输入框）；工具名三级分级（bash=ACCENT / 写操作=WARNING / 只读=MUTED）；配置面板边框 MUTED 降噪；HITL/AskUser 弹窗 WARNING
- **Setup Wizard:** 首次启动自动检测配置完整性，三步引导（Provider → API Key → Model Alias），支持 Anthropic/OpenAI Compatible，save_setup() 原子写回 settings.json
- **历史面板工作区过滤:** /history 面板按 cwd 过滤 ThreadMeta，只显示当前工作区的对话，标题包含工作区路径
- **定时任务（cron）:** /loop 注册定时任务（cron 表达式 + prompt），/cron 面板管理（导航/删除/切换启用）；AI 通过 CronRegister/CronList/CronRemove 工具创建管理；内存任务表上限 20，TUI 重启后清空
- **子 Agent 模型切换:** agent.md 的 model 字段生效，LLM Factory 签名升级为 Fn(Option<&str>)，alias 解析在 TUI 层；SkillFrontmatter 增加 model 文档字段
- **工具颜色分层:** 工具名（颜色+BOLD）+ 参数（DarkGray），文件路径自动缩短
- **/compact Thread 迁移:** /compact 执行后创建新 Thread 保留旧历史，新 Thread 以摘要 System 消息开头
- **App 结构体拆分:** App 拆分为 AppCore/AgentComm/LangfuseState 三个子结构体（共 37 字段），对外 API 通过转发方法保持不变
- **Widget 独立 crate:** cc-widgets 提供 11 个通用组件（BorderedPanel、ScrollableArea、SelectableList、InputField、TabBar、RadioGroup、CheckboxGroup、FormState、MarkdownRenderer、Spinner、ToolCall），零内部依赖
- **Spinner 动画:** 动词从 TODO activeForm 获取，Token 计数平滑递增动画，已用时间显示；完成态对齐 Claude Code 风格（`✻ {verb} for {elapsed} · done {HH:MM}`）
- **智能折叠策略:** 只读工具默认折叠、写操作默认展开，SubAgent 步数超过 4 自动折叠
- **syntect 代码高亮:** markdown-highlight feature flag 控制，base16-ocean.dark 主题，单行代码块不高亮
- **鼠标文字选区:** TextSelection 模块管理拖拽状态，WrappedLineInfo 换行映射，Ctrl+C 优先级链（选区复制>中断>退出），REVERSED 反色高亮
- **全局屏幕选区:** ScreenSelection 基于渲染 Buffer 覆盖面板/状态栏/sticky header/bg agent bar/空白区域，与消息区 TextSelection 跨区域衔接；ScreenSnapshot 在 `terminal.draw()` 后克隆 Buffer 作为文本源；双击选整行（消息区用 TextSelection 纯文本，其他区域用 ScreenSelection 整屏行）；松开鼠标自动复制 + "已复制 N 个字符" toast
- **Skills / 触发:** Skills 触发键从 # 统一到 / 前缀，提示浮层合并命令组+Skills 组，命令优先
- **两档权限模式:** `auto`（默认）/ `bypass`，Shift+Tab 在 Auto ↔ Bypass 间循环，未知取值回退 auto，状态栏实时显示
- **Background Agent:** Agent 工具 `run_in_background` 参数触发后台执行，最多 3 并发，`mpsc::unbounded_channel` 通知，完成后 Human 消息注入，主 agent Done 后自动 continuation，ToolBlock 样式显示，状态栏 `[BG: N]` 指示器
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
- **后台 shell 通知 i18n + 进程级语言注册表:** 完成/超时/取消/终止/等待输入通知的展示文案改走 `LcRegistry::tr()`；新增 `i18n::init_global`/`global` 进程级注册表（`LcRegistry: Sync`，`FluentBundle` 用 concurrent 变体），供无 App 上下文的静态 `MessageViewModel` 构造路径读取当前语言，启动与 `/lang` 切换时同步
- **`/gc` 诊断分平台语义 + view_messages 估算:** `active`/`mapped`/`retained` 按 `alloc_name` 条件化标注（jemalloc 真实活跃页 vs mimalloc `page_committed` 历史高水位/`reserved` 虚拟地址），Windows 不再报虚假碎片；新增 `estimate_view_messages_heap()` 遍历 `MessageViewModel` 全部变体（此前 `origin_messages`+`completed` 漏掉 `view_messages`，是「未识别」大头）
- **详细模式长命令运行状态刷新修复:** 详细模式超长命令 header 折成多行后，tick 增量刷新改为**按内容定位** `Running…` 状态行（新增 `is_shell_running_status_line`），不再写死 `lines[1]`、不再用固定行数判「状态行未渲染」，消除「命令续行被覆盖 + 秒数冻结 + 两处 Running… 时间不一致」
- **详细模式工具头前缀稳定（#343）:** 运行中指示器在 `●`/空格间闪烁时，详细模式 header 折行不再 trim 首段行首空白（`push_wrapped_line_keep_first_lead`），`● Bash(` 前缀不再随闪烁左移
- **`/gc` RSS 变化符号修正（#346）:** 方向语义统一为 `after - before`（增加为正），`fmt_signed_delta()` 输出 `+N`/`-N`/`±0`；`allocated - RSS` 带符号并按方向分支文案
- **`/gc` Markdown 缓存内存统计（#351）:** `MarkdownCache::stats()` 单次加锁快照，按 `capacity()` 估算堆占用（总量 / 平均 / 最大条目、行 / Span 数），纳入「已知合计」并标注「非 RSS」
- **排队消息快捷键 Alt+S / Alt+X（#347）:** 「立即发送」由 `Ctrl+S` 迁移为 `Alt+S`、删除由 `Ctrl+X` 迁移为 `Alt+X`（Windows conhost 会截走 `Ctrl+S`）
- **Sticky Header 已禁用:** v0.6.71 起高度固定 0（保留实现，不再展示）
- **权限模式循环:** Auto ↔ Bypass（未知值回退 auto）
- **PageUp/PageDown 半页滚动:** 20 行（输入框为空时生效）

## ACP 服务层（cc-acp）

- **ACP 传输抽象:** `AcpTransport` trait 统一 MpscTransport（TUI 内存通道）和 StdioTransport（IDE stdio），JSON-RPC 2.0 协议
- **Session 管理:** SessionManager 管理会话生命周期（new/prompt/compact/set_model/set_mode/cancel）
- **Agent 构建:** `build_agent()` 统一组装 Middleware Chain + LLM + 工具，TUI 和 stdio 共用
- **事件映射:** `ExecutorEvent` → `SessionNotification` 标准 ACP 通知转换
- **HITL/AskUser 桥接:** `AcpTransportBroker` 通过 ACP RPC（`RequestPermission` + `elicitation/create`）替代 oneshot channel
- **上下文压缩:** auto-compact 触发 + micro/full compact 执行 + resubmit 全部在 executor 循环内完成

## 配置同步（Config Sync）

- **Relay Server:** Hono.js + Cloudflare Durable Objects，WebSocket 密文透传，配对码管理（6 位/5 分钟/一次性）
- **E2E 加密:** PBKDF2-SHA256 密钥派生 + AES-256-GCM 加密，Relay 无法解密
- **同步客户端:** `peri sync sender/receiver` 子命令，crossterm CLI 交互（选择/进度/确认）
- **选择性同步:** receiver 可勾选 settings/skills/mcp/plugins，Sender 打包 MessagePack 序列化传输
- **路径安全:** `validate_and_resolve()` 三层校验拒绝绝对路径/ParentDir/解析后前缀不匹配

## 基础设施

- **SQLite 线程持久化:** sqlx SqlitePool(max=5) 原生异步连接池，WAL 模式，`append_messages` 事务保证 crash-safe，`StateSnapshot` 事件驱动增量写入，数据库文件 + WAL/SHM 应用 0o600 权限（Unix），grandparent 目录权限校验
- **会话回顾/总结持久化:** ThreadMeta 新增 `latest_recap` 与 `last_task_summary`（TaskSummary { verb, elapsed_ms, done_at }）字段并落库，`-c`/`-r` 恢复会话时 recap 与任务完成总结行不再丢失；SQLite 幂等 ALTER TABLE 迁移；ThreadStore 提供 update_latest_recap / update_last_task_summary 单列 UPDATE（避免重写 ~1MB cached_context）
- **OpenTelemetry 追踪:** 内置 OTLP HTTP 导出，`OTEL_EXPORTER_OTLP_ENDPOINT` 环境变量控制开关，tracing-opentelemetry 桥接，兼容 Jaeger
- **结构化日志:** `RUST_LOG` 级别控制，`RUST_LOG_FORMAT=json` 切换 JSON 格式
- **配置持久化:** `~/.cc-code/settings.json` 存储 Provider/Model 配置，`AppConfig` 统一读写，`env` 字段替代 .env 文件注入环境变量
- **日志路径迁移:** 日志默认写入 `~/.cc-code/logs`
- **应用主目录统一 `~/.cc-code`（#349，破坏性）:** 移除对改名前 `~/.peri` 的逐文件回退，`app_home::app_data_path`/`app_data_dir` 一律返回 `~/.cc-code/...`；仅在 `~/.peri` 存在的数据文件不再被读取（需手动迁移）。`hitl` 敏感目录名单中的 `.peri` 保留（安全用途）
- **npm 安装增强:** install.js 自动下载 ripgrep 预编译二进制；存在既有 cc-code 配置时回填缺失模型别名（含 fable）

---
*最后更新: 2026-10-08 — v0.6.103/104：详细模式工具头前缀稳定（#343）、`/gc` RSS 变化符号修正（#346）、排队消息快捷键 Alt+S/X（#347）、`/gc` Markdown 缓存内存统计（#351）、移除 `~/.peri` 兼容统一 `~/.cc-code`（#349）；此前：`/gc` 分平台语义 + `view_messages` 估算（v0.6.100）、详细模式长命令状态刷新按内容定位（v0.6.101）、后台 shell 通知 i18n（v0.6.99）*
