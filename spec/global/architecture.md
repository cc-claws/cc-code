# 架构全景

## 系统组件

| 组件 | 类型 | 职责 |
|------|------|------|
| `cc-agent` | 核心库 | ReAct 执行器、LLM 适配层、Middleware trait、工具系统、消息类型、线程持久化（SQLite + Filesystem） |
| `cc-middlewares` | 中间件库 | 文件系统、终端（RTK 输出压缩代理）、HITL（含 SharedPermissionMode/Auto 分类器）、SubAgent、Skills、SkillPreload、AgentsMd、AgentDefine、Todo、CronMiddleware、MCP（Client 连接池、OAuth 2.0、工具桥接）、rg 双引擎文件搜索 等具体实现 |
| `cc-acp` | ACP 服务层 | Agent Client Protocol 实现：Session 管理、Agent 构建（Middleware Chain 组装）、事件映射（ExecutorEvent→SessionNotification）、HITL/AskUser 桥接（AcpTransportBroker）、Langfuse 追踪、Hooks、LSP、系统提示词、Provider/Model 解析、上下文压缩执行 |
| `cc-tui` | 可执行文件 | 基于 ratatui 的交互式 TUI，通过 ACP 协议与 Agent 通信（AcpTuiClient），MessagePipeline 消费 SessionNotification，异步渲染、多会话管理、HITL/AskUser 弹窗、配置面板 |
| `cc-widgets` | Widget 库 | 独立 UI 组件库，无 workspace 内部依赖（外部依赖 ratatui、pulldown-cmark、syntect 等） |

## Workspace 依赖关系

```
cc-agent           ← 零内部依赖，纯核心框架
    ↑
cc-middlewares      ← 依赖 cc-agent
    ↑
cc-acp              ← 依赖 cc-agent + cc-middlewares + cc-lsp + langfuse-client
    ↑
cc-widgets          ← 零内部依赖（外部依赖 ratatui、pulldown-cmark、syntect 等）
    ↑
cc-tui              ← 依赖 cc-widgets + cc-acp
```

## 模块划分

### cc-agent 内部模块

```
src/
├── agent/
│   ├── react.rs          — ReactLLM trait 与推理步抽象（循环主体在 executor/）
│   ├── executor/         — ReActAgent 执行器：ReAct 循环（默认 max_iterations=10，cc-acp 构建时设 500）+ middleware chain + 取消令牌
│   ├── state.rs          — AgentState：消息历史（只追加）、cwd、工具注册表
│   ├── token.rs          — TokenTracker / ContextBudget（Token 累积追踪与上下文窗口预算）
│   ├── compact/          — Micro/Full Compact 实现
│   │   ├── config.rs     — CompactConfig（阈值、策略配置）
│   │   ├── micro.rs      — Micro-compact：清除可压缩工具结果/图片/文档
│   │   ├── full.rs       — Full Compact：LLM 生成自由 Markdown 摘要替换历史
│   │   ├── re_inject.rs  — 重新注入最近文件 + Skills
│   │   └── invariant.rs  — Compact 不变量校验
│   └── events.rs         — AgentEvent 枚举（25 种变体，见下方事件系统）
├── llm/
│   ├── adapter.rs        — BaseModel trait 定义（invoke → LlmResponse）
│   ├── anthropic/        — ChatAnthropic：Prompt Cache + Extended Thinking + system blocks
│   ├── openai/           — ChatOpenAI：SSE streaming + reasoning_content（DeepSeek-R1/o系列）
│   ├── react_adapter.rs  — BaseModelReactLLM：BaseModel → ReactLLM trait 适配
│   ├── retry.rs          — RetryableLLM<L> 装饰器（指数退避+随机抖动）
│   └── types.rs          — TokenUsage、LlmRequest/LlmResponse 类型定义
├── middleware/
│   ├── trait.rs          — Middleware<S> trait（7 个生命周期钩子 + collect_tools）
│   ├── chain.rs          — MiddlewareChain：按注册顺序执行所有中间件
│   └── base.rs           — LoggingMiddleware / MetricsMiddleware / NoopMiddleware
├── messages/
│   ├── message.rs        — BaseMessage（Human/Ai/System/Tool）、MessageContent、MessageId（UUID v7）
│   ├── content.rs        — ContentBlock 7 种变体（Text/Image/Document/ToolUse/ToolResult/Reasoning/Unknown）
│   └── adapters/         — MessageAdapter trait：OpenAiAdapter / AnthropicAdapter 双向转换
├── tools/
│   └── mod.rs            — BaseTool trait + ToolDefinition（JSON Schema），工具经 middleware collect_tools 汇总
├── thread/
│   ├── store.rs          — ThreadStore trait（异步，list/get/create/append/delete）
│   ├── sqlite_store.rs   — SqliteThreadStore：sqlx SqlitePool 连接池，WAL 模式，原生 async
│   ├── filesystem.rs     — FilesystemThreadStore：文件系统持久化备选实现
│   └── types.rs          — ThreadId（UUID v7）、ThreadMeta
├── hitl/                 — HitlDecision 枚举（Approve/Edit/Reject/Respond）、HitlHandler trait、BatchItem
├── ask_user/             — AskUserInvoker trait、AskUserBatchRequest、AskUserQuestionData、AskUserOption
├── error.rs              — AgentError / AgentResult 统一错误类型、LlmHttpError（携带 HTTP status code）
└── telemetry/
    └── subscriber.rs     — tracing-subscriber 初始化（env-filter + fmt + json）
```

### cc-middlewares 内部模块

```
src/
├── middleware/
│   ├── filesystem.rs     — FilesystemMiddleware（提供 5 个工具，见工具清单）
│   ├── terminal.rs       — TerminalMiddleware（bash 工具，120s 超时，跨平台）
│   └── todo.rs           — TodoMiddleware（after_tool 解析 TodoWrite，推送 channel）
├── mcp/                   — McpMiddleware（config/transport/client/tool_bridge/resource_tool/middleware）
│   ├── config.rs         — 三层配置加载合并（全局 < 插件 < 项目），${VAR} 展开
│   ├── transport.rs      — stdio（子进程）/ StreamableHTTP（远程）传输工厂
│   ├── client.rs         — McpClientPool 连接池管理
│   ├── tool_bridge.rs    — MCP 工具 → BaseTool 桥接
│   ├── resource_tool.rs  — mcp_read_resource 工具
│   └── middleware.rs     — Middleware trait 实现
├── hitl/
│   ├── mod.rs            — HumanInTheLoopMiddleware（before_tool 拦截 + requires_approval 判断）
│   ├── shared_mode.rs    — SharedPermissionMode (Arc<AtomicU8> 无锁共享权限模式)
│   └── auto_classifier.rs — LlmAutoClassifier (Auto 模式分类器)
├── subagent/
│   ├── mod.rs            — SubAgentMiddleware（挂载 Agent 工具 + LLM 工厂 + system builder）
│   ├── tool/             — SubAgentTool（读 agent 定义、创建子 Agent、工具过滤/防递归）
│   └── skill_preload.rs  — SkillPreloadMiddleware（before_agent 注入 skill 全文为 fake tool 调用序列）
├── skills/
│   ├── loader.rs         — 多路径扫描（~/.claude/skills/ → skillsDir → ./.claude/skills/），同名先到先得
│   └── mod.rs            — SkillsMiddleware（before_agent prepend 摘要到 system prompt）
├── agents_md/            — AgentsMdMiddleware（指引文件加载：同目录合并去重 + 跨目录 root→cwd 拼接 + 限额，见 spec/global/domains/agent-instructions.md）
├── agent_define/         — AgentDefineMiddleware + AgentOverrides（覆盖 model/tone/maxTurns 等）
├── claude_agent_parser/  — .claude/agents/*.md 文件解析器（YAML frontmatter 提取）
├── ask_user/             — parse_ask_user() 工具输出解析
└── tools/
    ├── filesystem/       — 5 个文件系统工具各自独立文件
    │   ├── read.rs       — ReadFileTool
    │   ├── write.rs      — WriteFileTool
    │   ├── edit.rs       — EditFileTool
    │   ├── glob.rs       — GlobFilesTool
    │   └── grep.rs       — GrepTool（rg 双引擎：优先外部 ripgrep 二进制，回退 grep+grep-regex crate）
    ├── ask_user_tool.rs  — AskUserTool（oneshot channel 挂起等待用户输入）
    ├── todo.rs           — TodoWriteTool + TodoItem / TodoStatus
    └── mod.rs            — BoxToolWrapper / ArcToolWrapper 适配器
```

### cc-acp 内部模块

```
src/
├── lib.rs                — 模块入口
├── transport/
│   └── mod.rs            — AcpTransport trait（MpscTransport/StdioTransport）
├── session/
│   ├── mod.rs            — Session 管理（SessionManager + SessionState）
│   ├── event_sink.rs     — EventSink trait + TransportEventSink/StdioEventSink
│   ├── executor.rs       — execute_prompt() 共享 Agent 执行管线
│   └── state_builders.rs — ACP 协议状态构建器（modes/models/configOptions）
├── agent/
│   └── builder.rs        — build_agent_with_embedded_memory()：组装 Middleware Chain + LLM + 工具
├── broker/               — AcpTransportBroker（实现 UserInteractionBroker，HITL/AskUser 桥接）
├── dispatch/             — ACP 请求分发（session/new/prompt/set_model/set_mode 等）
├── event/                — ExecutorEvent → SessionNotification 事件映射
├── langfuse/
│   └── mod.rs            — Langfuse 追踪（Session/Tracer）
├── hooks/
│   └── mod.rs            — Hooks 系统集成
├── lsp/
│   └── mod.rs            — LSP 中间件集成
├── prompt/
│   └── mod.rs            — 系统提示词构建（PromptFeatures 定义于此）
└── provider/
    └── mod.rs            — Provider/Model 解析
```

### cc-tui 内部模块（更新后）

```
src/
├── main.rs               — 入口：CLI 参数解析、terminal 初始化、事件循环
├── acp_client/
│   └── client.rs         — AcpTuiClient：TUI 端 ACP 封装（new_session/prompt/compact/set_model/cancel 等）
├── acp_server/
│   ├── mod.rs            — ACP Server 配置（SessionState/AcpServerConfig）
│   ├── requests.rs       — handle_request()：处理 session/new/prompt/compact 等 ACP 请求
│   ├── prompt.rs         — TUI 侧 prompt 执行入口，委托 executor::execute_prompt()
│   ├── compact.rs        — 手动 compact 入口
│   └── notify.rs         — 通知推送
├── app/
│   ├── mod.rs            — App 结构体
│   ├── agent.rs          — map_executor_event()：ExecutorEvent → AgentEvent 映射
│   ├── agent_ops/        — Agent 事件处理（acp_bridge/lifecycle/subagent/polling）
│   ├── agent_submit.rs   — submit_message()
│   ├── agent_compact.rs  — TUI 侧 compact UI 处理
│   ├── message_pipeline/ — MessagePipeline 统一消息管线
│   ├── text_selection.rs — TextSelection 鼠标文字选区
│   ├── hitl.rs           — 审批/HITL 弹窗
│   ├── ask_user.rs       — AskUser 弹窗
│   ├── model_panel.rs    — /model 面板
│   ├── provider.rs       — Provider/Model 配置管理
│   ├── thread_ops.rs     — 线程操作
│   └── ...
├── ui/
│   ├── main_ui/          — 主渲染入口
│   ├── message_render/   — 消息行渲染
│   ├── message_view/     — MessageViewModel
│   └── render_thread.rs  — 独立渲染线程
├── config/
│   └── mod.rs            — re-export cc-acp 的 PeriConfig（定义在 cc-acp/src/provider/config.rs）+ TUI 本地配置类型
├── command/
│   ├── mod.rs            — CommandRegistry
│   └── ...
└── event/                — crossterm 事件适配（系统提示词构建已迁至 cc-acp，本地 prompt.rs 已删除）
```

## 事件系统

### AgentEvent（核心层，25 种变体，下表为主要变体）

| 事件 | 说明 | 携带信息 |
|------|------|----------|
| `AiReasoning` | AI 推理/CoT 内容 | reasoning_text |
| `TextChunk` | LLM 最终文字输出 | message_id + chunk |
| `ToolStart` | 工具调用开始 | message_id + tool_call_id + name + input |
| `ToolEnd` | 工具调用结束 | message_id + tool_call_id + name + output + is_error |
| `StateSnapshot` | 完整消息快照 | Vec\<BaseMessage\>（用于持久化） |
| `MessageAdded` | 增量消息 | 单条 BaseMessage（用于持久化和遥测） |
| `LlmCallStart` | LLM 调用开始 | step + messages 快照 + tools 定义（Langfuse） |
| `LlmCallEnd` | LLM 调用结束 | step + model + output + TokenUsage（Langfuse） |
| `LlmRetrying` | LLM 重试中 | attempt, max_attempts, delay_ms, error |
| `BackgroundTaskCompleted` | 后台 agent 任务完成 | task_id, agent_name, success, output, tool_calls_count, duration_ms |
| `AiMessageSnapshot` | 流式 AI 消息快照 | 完整 Ai 消息（流式路径专用） |
| `SubagentEvent` | 子 Agent 事件信封 | 以实例 ID 包装的完整子事件 |
| `CompactionStarted` / `CompactionCompleted` | 压缩开始/完成 | 策略、触发原因 / 保留与丢弃条数 |
| `AgentCancelled` | Agent 被取消 | — |

### TUI AgentEvent（应用层，扩展变体）

TUI 层另有独立的 AgentEvent 枚举（与核心层同名但不同类型），含 `Done` / `Error` 等变体驱动 TUI 状态机；审批不走事件变体（无 ApprovalNeeded/AskUserBatch）。

## 数据流

### ReAct 循环（核心执行路径）

```
AgentInput（用户消息）
  ↓
state.add_message(Human)
  ↓
chain.collect_tools(cwd)        ← 各中间件 collect_tools 汇总工具集，手动注册优先
  ↓
chain.before_agent(state)       ← AgentsMd → Memory → AgentDefine → Skills → SkillPreload → AtMention → …
  ↓
┌─── ReAct 循环（max 500 次，cc-acp 构建时设定；executor 默认 10）──────────────────────────────────┐
│  emit(LlmCallStart{step, messages, tools})                   │
│  llm.generate_reasoning(messages, tools)                     │
│  emit(LlmCallEnd{step, model, output, usage})                │
│    ↓ stop_reason==ToolUse                                    │
│  state.add_message(Ai{tool_calls})                           │
│  emit(MessageAdded(Ai))                                      │
│  for each tool_call (并发 join_all):                         │
│    chain.before_tool()  ← HITL 可能在此阻塞等待审批          │
│    emit(ToolStart{...})                                      │
│    tool.invoke(input)   ← AskUser 可能在此阻塞等待输入       │
│    emit(ToolEnd{...})                                        │
│    chain.after_tool()   ← TodoMiddleware 解析结果             │
│    state.add_message(Tool{result})                           │
│    emit(MessageAdded(Tool))                                  │
│    ↓ stop_reason==EndTurn                                    │
│  emit(TextChunk) → 最终答案                                  │
│  emit(StateSnapshot) → 持久化                                │
└──────────────────────────────────────────────────────────────┘
  ↓
chain.after_agent(state, output)
  ↓
AgentOutput（最终结果）
```

### TUI/ACP 通信

```
TUI 路径:
  TUI 输入 → AcpTuiClient.new_session() / .prompt()
           → MpscClientTransport.send_request/notification()
           → MpscServerTransport.recv() (ACP Server, tokio::spawn)
           → acp_server::requests::handle_request() → executor::execute_prompt()
           → build_agent_with_embedded_memory() → agent.execute()
           → ExecutorEvent → TransportEventSink.push_event()
             → peri/agent_event (TUI) + session/update (标准ACP)
           → AcpTuiClient.pump_notifications() → AgentEvent
           → MessagePipeline → View Models → Render

Stdio 路径:
  SDK on_receive_request("session/prompt")
    → executor::execute_prompt() + StdioEventSink
    → ExecutorEvent → SessionNotification → stdout JSON-RPC
```

### Langfuse 追踪层次

```
LangfuseSession（Thread 级别，跨多轮复用）
  └─ LangfuseTracer（Turn 级别，每次 submit_message 创建）
       └─ Trace（trace_id = turn UUID）
            └─ Observation: "agent-run"（type=Agent，根节点）
                 ├─ Generation: "llm-step-{n}"（LlmCallStart → LlmCallEnd）
                 │    └─ input: messages 快照, output: LLM 回复, usage: token 统计
                 ├─ Span: "Tools"（工具批次）
                 │    ├─ Span: "tool:{name}"（ToolStart → ToolEnd）
                 │    ├─ Span: "tool:{name}"
                 │    └─ ...
                 └─ Generation: "llm-step-{n+1}"
                      └─ ...
```

### 上下文压缩流程

```
LlmCallEnd 携带 usage
  → TokenTracker.accumulate()
  → context_usage_percent() > threshold
  → Micro-compact: 清除可压缩工具结果/图片/文档
  → Full Compact: LLM 生成自由 Markdown 摘要替换历史
  → re_inject: 重新注入最近文件 + Skills
```

## 中间件链执行顺序

中间件按注册顺序执行，典型组装顺序：

```
主 Agent（cc-acp 的 build_agent_with_embedded_memory 组装；完整清单以 builder.rs 为准）：
1. AgentsMdMiddleware         ← 注入指引整段（AGENTS.md/CLAUDE.md 及变体，单条 System 消息）
2. AgentDefineMiddleware      ← 解析 agent 定义，设置 model/maxTurns 等覆盖
3. SkillsMiddleware           ← 扫描 Skills 目录，摘要注入 system
4. SkillPreloadMiddleware     ← 消息含 /skill-name 时注入 skill 全文（fake tool 序列）
5. AtMentionMiddleware        ← @path 解析，注入 Read 工具调用
6. FilesystemMiddleware       ← 提供文件系统工具
7. GitAttributionMiddleware   ← 追踪 Write/Edit 贡献字符数
8. TerminalMiddleware        ← 提供 bash 工具
9. WebMiddleware             ← WebFetch/WebSearch
10. TodoMiddleware            ← after_tool 解析 TodoWrite 结果
11. CronMiddleware            ← CronRegister/CronList/CronRemove
12. HookMiddleware            ← hooks 事件拦截（多组实例）
13. HumanInTheLoopMiddleware  ← before_tool 拦截敏感工具
14. SubAgentMiddleware        ← 提供 Agent 工具（支持 fork/normal/background 三路径）
15. McpMiddleware             ← MCP 工具和资源注入（仅 pool 初始化成功时注册）
16. ToolSearchMiddleware      ← SearchExtraTools/ExecuteExtraTool 代理
17. LspMiddleware             ← LSP 工具 + after_tool 文件变更同步
18. CompactMiddleware         ← before_model 钩子触发上下文压缩（条件注册）
[ReActAgent.with_system_prompt()] ← system prompt 固定在 run_before_agent 之后 prepend，不依赖中间件顺序

子 Agent（SubAgentTool 内部组装）：
1. AgentsMdMiddleware
2. SkillsMiddleware
3. SkillPreloadMiddleware     ← 读取 agent 定义 frontmatter.skills 列表
4. TodoMiddleware
5. AgentSystemBuilder         ← 子 agent 的 system prompt 由 builder 动态构建（非中间件）
```

手动注册工具（`register_tool`）优先级最高，覆盖同名中间件工具。

记忆由宿主在会话创建时读取并冻结，系统提示词携带一份精简读取规则与最多 2 KiB 索引（含截断提示），详情按需读取。宿主不注册 `MemoryMiddleware`，不触发后台记忆生成；写入、更新与删除仅响应用户明确要求。旧 `build_agent` 与独立中间件为自定义提示词调用方保留。

## 外部集成

| 外部服务 | 协议 | 认证 | 端点 |
|---------|------|------|------|
| Anthropic API | HTTPS REST + SSE | `ANTHROPIC_API_KEY` header | `https://api.anthropic.com/v1/messages` |
| OpenAI 兼容 | HTTPS REST + SSE | `OPENAI_API_KEY` bearer | `OPENAI_BASE_URL` 环境变量 |
| SQLite | 本地文件 | — | `~/.cc-code/threads/threads.db` |
| Langfuse | HTTPS REST | `LANGFUSE_PUBLIC_KEY` + `LANGFUSE_SECRET_KEY` | `LANGFUSE_BASE_URL`（默认 cloud） |

## 部署拓扑

**标准模式（本地 TUI）：**

```
用户终端
  └─ cargo run -p cc-tui
       ├─ 直接调用 Anthropic/OpenAI API（reqwest HTTP）
       ├─ 读写本地文件系统（FilesystemMiddleware）
       ├─ 执行 bash 命令（TerminalMiddleware）
       ├─ 写入 ~/.cc-code/threads/threads.db（SQLite WAL）
       └─ 上报 Langfuse（可选，环境变量控制）
```

**可观测性（可选）：**

```
cc-acp / cc-tui（tracing spans）
  └─ Langfuse（LangfuseTracer → Langfuse API）
       └─ Trace > Span > Generation 三级层次
```

---
*最后更新: 2026-10-22 — 对照 v0.6.182 代码维护：修正模块路径与事件变体数、删除 OpenTelemetry 描述、补入 MemoryMiddleware、修正数据路径*
