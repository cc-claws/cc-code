# CLAUDE.md

## 项目概述

Rust Agent 框架，7 个 Workspace Crate + 1 个独立 Node.js CLI（`peri-cli/`）。

| Crate | 职责 |
|-------|------|
| `peri-agent` | 核心：ReAct 循环、Middleware trait、LLM 适配器、工具系统、持久化（SQLite）、遥测 |
| `peri-middlewares` | 中间件：文件系统、终端、HITL、SubAgent、Skills、Todo、Cron、MCP、Hooks、Plugin、LSP |
| `peri-widgets` | Widget 组件库，仅依赖 ratatui + pulldown-cmark |
| `peri-acp` | **ACP 服务层**：Agent Client Protocol 实现，通过 MpscTransport/StdioTransport 桥接 TUI/IDE 与 Agent |
| `peri-tui` | TUI 应用，依赖 peri-acp（通过 ACP 协议与 Agent 通信）+ peri-widgets |
| `langfuse-client` | Langfuse 遥测客户端（独立） |
| `peri-lsp` | LSP 客户端库（独立，被 middlewares 使用） |

`rmcp` crate（v1.7）直接引用，不再需要本地 patch。

**其他目录**：`peri-cli/`（Node.js CLI，版本管理/安装工具）、`scripts/`（启动脚本）、`side-projects/`（实验性/空壳，未纳入 workspace）。

## 依赖关系

依赖关系（A → B 表示 A 依赖 B）：

- `peri-widgets`、`peri-lsp`、`langfuse-client` → 无 workspace 内部依赖（独立基础库）
- `peri-middlewares` → `peri-agent`、`peri-lsp`
- `peri-acp` → `peri-agent`、`peri-middlewares`、`peri-lsp`、`langfuse-client`
- `peri-tui` → `peri-acp`（运行时通信）+ `peri-agent`、`peri-middlewares`、`peri-lsp`、`langfuse-client`、`peri-widgets`（类型依赖，用于 UI 渲染的类型如 `BaseMessage`/`ContentBlock`）
- `peri-cli` → 独立（Node.js）

**TUI→ACP 通信**: TUI 运行时仅通过 `peri-acp` 的 `MpscTransport`（in-memory channel pair）与 ACP Server 通信，不经过 `peri-agent`/`peri-middlewares` 的运行时路径。ACP Server 持有 Agent 构建和执行逻辑，TUI 作为纯 ACP client 前端消费 `AcpNotification` 事件。

## 开发命令

```bash
cargo build                          # 构建所有 crate
cargo build -p <crate>               # 构建指定 crate
cargo run -p peri-tui          # 运行 TUI
cargo run -p peri-tui -- -a    # HITL 审批模式
cargo test                           # 全量测试
cargo test -p <crate> --lib -- <test_name>  # 单个测试
lefthook install                     # 安装 git hooks
lefthook run pre-commit              # pre-commit（fmt/check/clippy）
scripts/start-tui.sh                 # 启动 TUI（RELAY_PORT=3001）
```

**peri-cli**（Node.js）：`install`/`update`/`list`/`add-env`/`uninstall`/`clean`，用于版本管理和安装。

## 架构要点

**ReAct 循环**（`peri-agent`）：AgentInput → collect_tools → before_agent → loop(500) { before_model → LLM → after_model → [工具调用] before_tool → 并发执行 → after_tool → emit | [回答] → emit TextChunk + StateSnapshot → after_agent }。TUI 覆盖 `max_iterations(500)`（核心默认 10）。

**[TRAP]** `tool_dispatch.rs` 延迟写入：`collect_tool_results` 执行 before_tool + 并发调用 + 收集结果，**不写 state**；`dispatch_tools` 最后统一写入 AI 消息 + 所有 tool_result。禁止在 `collect_tool_results` 中调用 `state.add_message`。错误路径：before_tool 错误/Cancel 返回 `Err`（state 未修改）；执行阶段 Cancel/deferred_error 返回 `Ok((.., true, ..))`，`dispatch_tools` 写入 state 后再返回 `Err`。链上 17 个中间件的 `before_tool`/`after_tool`/`on_error` 均不读 `state.messages()`，新增中间件必须遵守。`ExecutorEvent::MessageAdded` 被 TUI 丢弃，TUI 通过 `StateSnapshot` + 流式事件维护状态。（详见 spec/global/domains/agent.md#issue_2026-05-15-orphaned-tool-use-after-concurrent-tool-error）

**[TRAP]** 新增/修改事件类型语义（如工具前文本从 AiReasoning 改为 TextChunk）时，必须同步检查 TUI 侧事件映射层（`map_executor_event`）。新增 ExecutorEvent 变体时必须同步更新映射，事件丢弃会导致下游状态不一致。（详见 spec/global/domains/agent.md#issue_2026-05-11-streaming-text-invisible-with-tools，spec/global/domains/message-pipeline.md#issue_2026-05-13-streaming-text-tool-aggregation-visual-issues）

**[TRAP]** 多工具并发的结果处理循环中，P3/P4 错误路径提前返回会导致后续 tool_result 缺失。必须用 deferred_error 模式——先收集所有错误，循环结束后统一判断。所有 tool_result 必须始终写入 state。（详见 spec/global/domains/agent.md#issue_2026-05-14-orphaned-tool-use-without-tool-result，spec/global/domains/agent.md#issue_2026-05-15-tool-execution-error-stops-agent，spec/global/domains/agent.md#issue_2026-05-18-agent-tool-calls-execute-serially）

**[TRAP]** `prepended_ids` 只追踪 `prepend_message`（头部 insert System），不能计入 `add_message`（尾部 push）。cleanup 用 `take_while(|m| m.is_system())` 只收集头部连续 System 消息，禁止用长度差计算。新增中间件的 `add_message` 注入不受 cleanup 影响，也不能假设 cleanup 会清理它们。（详见 spec/global/domains/agent.md#issue_2026-05-26-skillpreload-anthropic-400-tool-result-orphan）

**消息类型**：`BaseMessage`（Human/Ai/System/Tool），`ContentBlock`（Text/Image/Document/ToolUse/ToolResult/Reasoning/Unknown）。

**LLM 适配层**：`BaseModel` trait（OpenAI/Anthropic）→ `BaseModelReactLLM` → `ReactLLM`。`RetryableLLM<L>` 指数退避重试。

**TUI 消息渲染**（`peri-tui`）：所有消息更新通过统一 `RebuildAll` 路径触发（无增量更新）。`MessagePipeline`（`message_pipeline.rs`）维护规范状态，`build_tail_vms()` 构建尾部 VMs，`messages_to_view_models()` 是唯一转换入口。流式文本通过 100ms 节流触发 RebuildAll，非流式事件立即触发。独立 `RenderThread` 处理渲染，通过 `RenderCache(RwLock)` 与 UI 线程同步。

**[TRAP]** Ephemeral VM（SystemNote/CacheWarning）依赖锚点机制：`ephemeral_notes: Vec<(usize, MessageViewModel)>` 记录插入时的 `view_messages.len()` 作为位置索引（非 MessageId）。RebuildAll 时通过 `(anchor - prefix_len).min(tail_len) + prefix_len` 计算插入位置。`retain()` 路径通过 `anchor >= prefix_len` 过滤过期锚点。新增 ephemeral VM 类型必须同步更新过滤逻辑。（详见 spec/global/domains/message-pipeline.md#issue_2026-05-12-systemnote-position-drift-on-rebuild）

**[TRAP]** BaseMessage vs MessageViewModel 维度混淆：`completed_len_at_round_start` 是 BaseMessage 长度，`prefix_len` 是 VM 索引，两者非 1:1。`prefix_len` 必须用 `round_start_vm_idx`，`drain` 必须钳位。**禁止 Pipeline 内部返回 `RebuildAll`**——Pipeline 不拥有 `round_start_vm_idx`。（详见 spec/global/domains/message-pipeline.md#issue_2026-05-20-llm-error-message-area-clear-flicker）

**[INFO]** `MessageViewModel` 已不再包含 `message_id` 字段。SubAgentGroup 使用 `instance_id: Option<String>` 标识。新增 VM 变体时注意身份标识字段与显示字段分离。

**[TRAP]** `Interrupted`/`Error` + `Done` 互斥：`Interrupted`/`Error` 先 `request_rebuild()` + 添加通知，设 `reconcile_already_done=true`，后续 `Done` 跳过 `request_rebuild()` 防止覆盖通知。（详见 spec/global/domains/agent.md#issue_2026-05-25-interrupt-undo-last-user-message）**[TRAP]** Cancel 后历史不应无条件截断：ACP server 在 `result.ok==false` 时无条件 truncate history 会丢失 agent 已写入 state 的消息，导致 agent 失忆。应检查 `result.messages.len()` 判断是否有进展，有则保留。（详见 spec/global/domains/agent.md#issue_2026-05-26-ctrl-c-interrupt-causes-agent-amnesia）

**[TRAP]** frozen_subagent_vms 按 agent_id + 位置匹配（先 instance_id 精确匹配，失败后按顺序 agent_id 匹配）。`begin_round()` 清空 frozen_vms 和 ephemeral_notes，但 `done()` 不清空 frozen_subagent_vms（允许 Done→下一轮之间消费）。（详见 spec/global/domains/message-pipeline.md#issue_2026-05-16-frozen-subagent-vms-cross-round-accumulation-duplication）

**系统提示词**：`build_system_prompt()` 在 `session/new` 时调用一次，产出 `frozen_system_prompt` 存入 `SessionState`，后续轮次直接复用。段落文件位于 `peri-tui/prompts/sections/`（01-06 静态 + 07+10-13 动态，共 11 个），通过 `__SYSTEM_PROMPT_DYNAMIC_BOUNDARY__` 边界标记分隔——标记前可缓存，标记后不影响前缀缓存。`PromptFeatures` 控制条件段落注入。Agent 构建在 system prompt 末尾追加 Git Attribution 段落（动态区域内不影响缓存前缀）。

## Thinking/推理模式

`ThinkingConfig` 控制推理参数。Anthropic 用 `thinking + output_config.effort`，OpenAI 用 `reasoning_effort`。`budget_tokens` 最小 1024，`max_tokens` 必须 > `budget_tokens`。

**OpenAI Reasoning 回传**（`openai.rs`）：

- `reasoning_content` 顶层字段：所有模型无条件回传
- content 数组 `thinking` 类型：默认关闭，通过 `with_thinking_content(true)` 手动开启（如 deepseek-v4-pro）

**[TRAP]** DeepSeek `unknown variant 'thinking'`：不要把 `Reasoning` block 序列化为 `{"type":"thinking"}` 发给不支持的 provider。**[TRAP]** `reasoning_content must be passed back`：过滤 `Reasoning` 时必须同时作为顶层字段回传。两个陷阱互相关联。（详见 spec/global/domains/agent.md#issue_2026-05-12-glm-reasoning-field-not-parsed，spec/global/domains/agent.md#issue_2026-05-14-deepseek-anthropic-thinking-block-dropped，spec/global/domains/agent.md#issue_2026-05-12-thinking-reasoning-dataflow-issues）

**OpenAI 兼容适配层 Provider 特定处理**（`invoke.rs` `build_request_body`/`messages_to_json`）：

- **`reasoning` 字段已移除**：`messages_to_json` 中 assistant 消息仅回传 `reasoning_content`（OpenAI 标准字段），不再同时设置 `reasoning`。GLM 等需要 `reasoning` 字段的模型在接收端（`parse_assistant_message`）仍兼容双字段解析。
- **`stream_options` 仅 Qwen**：`stream_options.include_usage` 仅在模型名含 `qwen` 时发送（Qwen API 需要此字段在流式末尾返回 usage），其他 provider 不发送。
- **Kimi thinking/reasoning_effort 互斥**：Kimi k2.6 不支持 `thinking` 和 `reasoning_effort` 同时出现（400 Bad Request）。当 `thinking_enabled` 为 true 且模型名含 `kimi` 时，请求体中移除 `reasoning_effort`。

## 系统提示词稳定性（第一优先级）

**[原则] 系统提示词稳定性是第一优先级**：会话开始后，系统提示词必须完全稳定、不可变更。任何在会话进行中修改系统提示词的行为（包括通过 runtime config、模型切换、技能加载、中间件注入等方式间接改变其内容）都是禁止的。系统提示词内容的任何变化都会导致 Prompt Cache 失效、模型行为漂移，严重影响会话质量。

唯一例外是 `__SYSTEM_PROMPT_DYNAMIC_BOUNDARY__` 边界标记之后动态区域内的占位符值变化（如日期、cwd），但即使是动态区域，其**结构/模板/段落数量**也必须在会话内保持不变。新增中间件在 `before_agent` 阶段注入 System 消息时，必须确保注入内容和位置跨轮次稳定。

## Tool Search 延迟加载

工具分三层：**Core（12 个）**——Read/Write/Edit/Glob/Grep/folder_operations/Bash/WebFetch/WebSearch/Agent/AskUserQuestion/TodoWrite，始终对 LLM 可见；**Meta（2 个）**——`SearchExtraTools`/`ExecuteExtraTool`，始终可见，用于按需发现和执行 deferred tools；**Deferred（其余）**——Cron*、MCP 工具、LspTool 等，LLM 不直接可见，通过 Meta 工具桥接。核心工具定义以 `tool_search/core_tools.rs` 中的 `CORE_TOOLS` 为准，Meta 工具定义在 `META_TOOLS`。新增工具优先配置为 deferred tool，避免膨胀核心工具列表。

**[TRAP]** `Box<dyn BaseTool>` 不能直接转 `Arc<dyn BaseTool>`，用 `box_to_arc()` 通过 `ToolWrapper(ManuallyDrop<Box>)` 透传。**绝不能用 `Box::into_raw` + `Arc::from_raw`**——布局不同导致 UB。

**[TRAP]** Prompt Cache 前缀稳定性——通用原则：所有参与缓存前缀的数据（system prompt、tools 数组、消息顺序）必须保证跨请求稳定。这是实现上述"系统提示词稳定性"原则的技术手段。具体规则：

- （a）system prompt 中用 `__SYSTEM_PROMPT_DYNAMIC_BOUNDARY__` 边界标记分隔静态/动态内容，标记前可缓存，标记后不缓存
- （b）非 System 消息必须用 `add_message`（尾部追加），禁止 `prepend_message`（头部插入会改变 Anthropic cache_control 标记位置）；System 消息用 `prepend_message` 插入头部是安全的
- （c）动态占位符（日期、cwd、环境变量）放在边界标记之后
- （d）middleware 注入的 System 消息天然在边界标记之后（非缓存块）

历史踩坑已全部修复并固化到 frozen_system_prompt + cached_prompt + boundary 标记机制中。（详见 spec/global/domains/message-pipeline.md，spec/global/domains/message-pipeline.md#issue_2026-05-14-cache-breakpoint-structural-inefficiency，spec/global/domains/system-prompt.md#issue_2026-05-23-mcp-tools-instability-breaks-anthropic-cache）

**[TRAP]** `prepend_message` 的 `insert(0)` 右移导致 StateSnapshot 快照范围扩大，泄露 System 消息到 `agent_state_messages`。StateSnapshot 应始终 `.filter(|m| !m.is_system())`，`agent_state_messages` 不应包含 System 变体。（详见 spec/global/domains/system-prompt.md#issue_2026-05-13-system-prompt-dynamic-parts-duplicated-in-consecutive-calls，spec/global/domains/agent.md#issue_2026-05-14-deepseek-multi-turn-tool-result-duplication，spec/global/domains/system-prompt.md#issue_2026-05-20-rapid-context-expansion）

## 中间件链执行顺序

```
1.  AgentsMdMiddleware       ← CLAUDE.md/AGENTS.md 注入
2.  AgentDefineMiddleware    ← agent 定义，model/maxTurns 覆盖
3.  SkillsMiddleware         ← Skills 摘要注入（含插件 extra_dirs）
4.  SkillPreloadMiddleware   ← #skill-name 全文注入
5.  AtMentionMiddleware      ← @path 解析，注入 Read 工具调用
6.  FilesystemMiddleware     ← 6 个文件系统工具
7.  GitAttributionMiddleware ← before_tool/after_tool 追踪 Write/Edit 贡献字符数
8.  TerminalMiddleware       ← Bash
9.  WebMiddleware            ← WebFetch/WebSearch
10. TodoMiddleware           ← after_tool 解析 TodoWrite
11. CronMiddleware           ← Cron 调度
12. HookMiddleware           ← hooks 事件拦截（多组实例）
13. HumanInTheLoopMiddleware ← before_tool 拦截
14. SubAgentMiddleware       ← Agent 工具
15. McpMiddleware            ← MCP 工具和资源（pool 成功时注册）
16. ToolSearchMiddleware     ← SearchExtraTools/ExecuteExtraTool 代理
17. LspMiddleware            ← LSP 工具 + after_tool 文件变更同步
[ReActAgent.with_system_prompt()] ← prepend
```

插件通过 `plugin_skill_dirs` → `SkillsMiddleware.with_extra_dirs()`、`plugin_hooks` → `HookMiddleware` 注入，无独立 PluginMiddleware。

## ACP/TUI 分层架构

**概述**：`peri-acp` 是独立的 ACP 服务层 crate（依赖 peri-agent + peri-middlewares + peri-lsp + langfuse-client）。`peri-tui` 降级为纯 ACP client 前端，通过 `MpscTransport`（in-memory channel pair）与 `peri-acp` 通信。

**数据流**：
```
TUI 路径:
  TUI 输入 → AcpTuiClient.new_session() / .prompt()
           → MpscClientTransport.send_request/notification()
           → MpscServerTransport.recv() (ACP Server, tokio::spawn)
           → acp_server::requests::handle_request() → acp_server::prompt::execute_prompt()
           → peri_acp::session::executor::execute_prompt() + TransportEventSink
           → peri_acp::agent::builder::build_agent() → agent.execute()
           → ExecutorEvent → TransportEventSink.push_event()
             → peri/agent_event (TUI) + peri/* (compact) + session/update (标准ACP)
           → AcpTuiClient.pump_notifications() → AcpNotification::AgentEvent
           → agent_ops::acp_bridge::handle_acp_notification() → map_executor_event() → AgentEvent
           → agent_ops::handle_agent_event() → UI 更新

Stdio 路径:
  SDK on_receive_request("session/prompt")
    → peri_acp::session::executor::execute_prompt() + StdioEventSink
    → ExecutorEvent → StdioEventSink.push_event() → SessionNotification
    → SDK cx.send_notification() → stdout JSON-RPC
```

**Stdio 路径约束**：Stdio 和 TUI 路径共享 `executor::execute_prompt()`，但请求入口和错误处理独立。Stdio 当前支持的方法：`session/new`、`session/prompt`、`session/cancel`、`session/set_config_option`。Slash Commands（如 `/compact`、`/clear`）通过 `session/prompt` 发送，两条路径天然统一。缺失方法（如 `session/load`、`session/list`）会返回 "Method not found" 错误。新增 ACP 方法时必须检查两条路径是否都需要实现。

**[TRAP]** Stdio `initialize` 响应必须声明 session capabilities（与 TUI 路径的 `AcpServerConfig` 对齐），新增 capability 时必须同步更新 `StdioTransport` 的 initialize 响应。

**Frozen Data Flow**（会话内不可变数据）：

**真正冻结（session/new 一次性捕获，存 SessionState.frozen_*）：**
```
session/new → chrono::Local::now() → frozen_date
            → AgentsMdMiddleware::read_frozen_content(cwd) → frozen_claude_md + frozen_claude_local_md
            → SkillsMiddleware::build_frozen_summary(cwd, plugin_skill_dirs) → frozen_skill_summary
            → build_system_prompt(None, cwd, features, agent_dirs, Some(&frozen_date)) → frozen_system_prompt
            → SessionState.frozen_*
            → TUI prompt::execute_prompt → FrozenSessionData（+ is_git_repo 实时计算）
            → executor::execute_prompt → AcpAgentConfig.frozen_*
            → AgentsMdMiddleware::with_frozen_content / SkillsMiddleware::with_frozen_summary / system_builder(frozen_date)
```

**每轮 prompt 重新计算（非冻结）：**
- `is_git_repo`：`prompt.rs:124` 实时检查 `.git` 目录是否存在，不存 SessionState
- `YOLO_MODE`：`PromptFeatures::detect()` 在 system_builder closure 中每次 SubAgent 构建时重新读取（主 Agent 的 system prompt 中已冻结）
- `DISABLE_COMPACT` / `DISABLE_AUTO_COMPACT` / `COMPACT_THRESHOLD`：每轮读取 env，且被 executor 和 builder **重复读取两次**
- `peri_config`、Provider Snapshot、context_window、context_1m：每轮从 `Arc<RwLock<>>` 克隆快照
- 整个中间件链、AgentState、Cancel Token、Langfuse Tracer：每轮全新构造
- **SubAgent 系统提示词**：调用 `build_system_prompt(None, ...)` 不走 frozen 路径，完全重建。**[TRAP]** 这会导致 SubAgent 使用当前 runtime config 而非 frozen 值（如 language），在同一会话中与 Main Agent 产生漂移。所有 frozen 字段必须通过 `AcpAgentConfig` 传递到 SubAgent builder。（详见 spec/global/domains/system-prompt.md#issue_2026-05-27-language-injection-subagent-drift-cache-isolation）

**核心文件**：
| 文件 | 职责 |
|------|------|
| `peri-acp/src/session/executor.rs` | 共享 agent 执行管线（TUI/stdio 共用） |
| `peri-acp/src/session/event_sink.rs` | `EventSink` trait + Transport/Stdio 实现 |
| `peri-tui/src/acp_server/requests.rs` | TUI 侧 ACP 请求路由 |
| `peri-tui/src/acp_client/client.rs` | TUI 端 ACP client 封装 |
| `peri-tui/src/app/agent_ops/acp_bridge.rs` | AcpNotification → AgentEvent 桥接 |
| `peri-tui/src/app/agent.rs` | ExecutorEvent → AgentEvent 映射 |
| `peri-tui/src/app/agent_submit.rs` | 用户输入提交入口 |
| `peri-tui/src/app/agent_ops/lifecycle.rs` | Agent 生命周期处理 |

**AcpNotification 变体**（`acp_client/client.rs`）：
- `AgentEvent { session_id, event }` — 携带 `AgentEvent` 枚举，由 `map_executor_event()` 转换为 TUI `AgentEvent`
- `AgentDone { session_id }` — Agent 执行完成通知
- `RequestPermission { id, params }` — HITL 审批请求
- `Elicitation { id, params }` — AskUser 问答请求
- `SessionUpdate { session_id, params }` — 标准 ACP SessionUpdate（保留给外部 IDE client）
- `Peri { session_id, method, params }` — `peri/*` 自定义通知（compact/session 生命周期等）
- `Other { msg }` — 未识别的通知

**ACP Server 请求处理**（`acp_server/requests.rs`）：
- `initialize` → 协议握手，声明 session capabilities
- `session/new` → 创建 session state，分配 session_id，推送 `AvailableCommandsUpdate`
- `session/prompt` → Slash Commands（`/compact` 等）和普通 prompt 统一入口；executor 入口拦截 `/` 前缀命令
- `session/set_model` → 模型切换（通过 peri_config.alias）
- `session/set_mode` → 权限模式切换
- `session/set_config_option` → 统一配置选项（mode/model/thinking_effort）
- `session/load` → 加载已有 session
- `session/list` → 列出所有 session
- `session/close` → 关闭 session
- `session/resume` → 按 ID 恢复 session
- `session/fork` → 分叉 session
- `session/cancel` → 取消当前 session 的 Agent 执行（notification，TUI 和 stdio 均使用）

**ACP Slash Commands**（符合 https://agentclientprotocol.com/protocol/slash-commands）：
- Agent 通过 `AvailableCommandsUpdate` 通知广播可用命令列表（`session/new` response + 增量更新）
- Client 将 `/compact` 等命令作为 `session/prompt` 发送（text = `/compact`）
- Executor 入口拦截 `/` 前缀，按 `CommandKind`（`Immediate`/`Passthrough`/`Transform`）分类执行
- `Immediate` 命令（compact/clear）直接执行，不构建 agent；结果通过 EventSink 推送事件。**[TRAP]** Immediate 命令路径绕过 agent event pump，必须手动调用 `sink.push_done()` 发送 AgentDone 事件，否则 TUI 永久卡在 loading。新增 Immediate 命令时必须确保所有退出路径都调用了 push_done。（详见 spec/global/domains/agent.md#issue_2026-05-29-immediate-command-missing-push-done）
- `Passthrough` 命令（skill）透传给 agent，由 middleware 处理
- TUI 通过 `agent_commands` HashSet（从 `AvailableCommandsUpdate` 学习）区分 UICommand 和 AgentCommand
- `/clear` 保留为 UICommand（`app.new_thread()` 创建新 session），不走 ACP

**依赖关系说明**：TUI 保留 `AgentEvent` 枚举和 `handle_agent_event()` 处理器（`agent_ops/mod.rs:handle_agent_event`）以复用 UI 逻辑。Config/LlmProvider 类型已统一（TUI re-export `peri-acp` 的定义）。Agent 执行逻辑通过 `EventSink` trait + `executor::execute_prompt()` 统一在 `peri-acp`，TUI 和 stdio 各自提供 EventSink 实现。`peri-tui/Cargo.toml` 保留 `peri-agent`/`peri-middlewares` 作为**类型依赖**（UI 渲染所需的 `BaseMessage`/`ContentBlock` 等类型），运行时通信仅通过 `peri-acp`。

**[TRAP]** Agent 构建和执行统一通过 `peri_acp::session::executor::execute_prompt()`（内部调用 `peri_acp::agent::builder::build_agent()`）。禁止在 TUI 层直接构建 ReActAgent 或手写事件泵——使用 `EventSink` 实现委托给 executor。`build_agent()` 每轮重建的大对象（LLM 实例、middleware）已通过 `AgentPool` session 级缓存复用，避免 jemalloc arena 碎片化。（详见 spec/global/domains/agent.md#issue_2026-05-24-build-agent-per-turn-arc-transient-fragmentation）

**[TRAP]** TUI 层数据必须通过 ACP 协议到达 ACP 层，禁止直连。所有 TUI → ACP Server 的状态变更必须通过 `acp_client` 的协议方法（`new_session()`/`load_session()`/`prompt()`/`set_config_option()` 等）。TUI 本地清空状态（如 `new_thread()` 清空 `view_messages`/`agent_state_messages`/`pipeline`）不等于 ACP Server 端状态同步——必须同时通过 ACP 协议通知 Server 侧（如调用 `new_session()` 创建新 session）。违反此原则会导致 TUI 与 Server 状态不一致（如 `/clear` 后旧 history 泄漏到新对话）。（详见 spec/global/domains/agent.md#issue_2026-05-29-clear-keeps-acp-server-history）

**[TRAP]** Session Config Options 覆盖旧的 Session Modes API。ACP 规范明确指出 `configOptions` 取代 `modes`/`models`，但过渡期内需同时发送两者以兼容旧客户端。IDE 客户端通过 `configOptions` 中条目的 `category` 字段决定渲染哪些 UI 控件：`category: "mode"` → 权限模式选择器，`category: "model"` → 模型下拉，`category: "thought_level"` → 推理强度。`build_config_options()` 必须按优先级顺序返回（mode → model → thinking_effort），`session/set_config_option` 处理器必须处理 `"mode"` 和 `"model"` config ID（除了已有的 `"thinking_effort"`）。仅发送 `modes`/`models` 而缺少对应 `configOptions` 条目的，已迁移到新 API 的 IDE 不会显示任何控件。

## HITL 审批

默认需审批：`Bash`、`folder_operations`、`Agent`、`Write`、`Edit`、`delete_*`、`rm_*`、`mcp__*`、`WebFetch`、`WebSearch`。

## 上下文压缩

**架构**：Compact 由 `CompactMiddleware` 在 ReAct 循环内通过 `before_model` 钩子就地处理，不再需要外层 resubmit 循环。TUI 和 stdio 共享 executor 中的 compact 触发逻辑。TUI 侧 `handle_compact_completed` 只负责 UI 通知和 pipeline 状态清理。

**触发**：`CompactMiddleware::before_model` 在每轮 LLM 调用前检查 `ContextBudget`：0.70 micro-compact（清除 ≥5 步前的工具结果），0.85 full compact（LLM 生成摘要 + re_inject）。环境变量覆盖：`DISABLE_COMPACT`、`DISABLE_AUTO_COMPACT`、`COMPACT_THRESHOLD`（0.0-1.0）。

**手动 /compact**：通过 ACP Slash Command `/compact` → `session/prompt` → executor 入口拦截 → `CompactCommand`（`peri-acp/src/session/command/compact.rs`）→ `full_compact()` + `re_inject()` → EventSink 推送 `CompactStarted`/`CompactCompleted` 事件。

**核心文件**：
| 文件 | 职责 |
|------|------|
| `peri-agent/src/agent/events.rs` | `CompactStarted`/`CompactCompleted`/`CompactError` 事件变体，`CompactCompleted` 携带 `messages`、`files`、`skills` |
| `peri-agent/src/agent/compact/full.rs` | `full_compact()`：LLM 生成摘要 |
| `peri-agent/src/agent/compact/micro.rs` | `micro_compact_enhanced()`：清除陈旧工具结果 |
| `peri-agent/src/agent/compact/re_inject.rs` | `re_inject()`：重新注入文件/技能上下文 |
| `peri-agent/src/agent/compact/config.rs` | `CompactConfig`：阈值、开关、环境变量覆盖 |
| `peri-agent/src/agent/compact/invariant.rs` | 消息轮次分组（工具调用配对完整性检查） |
| `peri-middlewares/src/compact_middleware.rs` | `CompactMiddleware`：`before_model` 钩子，在 ReAct 循环内触发 compact。**[TRAP]** Micro compact 必须加 once-per-prompt 守卫（AtomicBool），否则每轮都重复触发。（详见 spec/global/domains/compact.md#issue_2026-05-23-micro-compact-repeated-triggering） |
| `peri-acp/src/session/command/compact.rs` | `/compact` Slash Command 实现（`CommandKind::Immediate`） |
| `peri-tui/src/app/agent_compact.rs` | TUI 侧 compact 事件处理：pipeline 清理 + UI 通知 |

**[TRAP]** `handle_compact_completed` 必须三步清理：① `pipeline.clear()` ② `pipeline.restore_completed(messages)` ③ `RebuildAll { prefix_len: 0 }`。缺少任一步都会导致旧消息残留或 system 消息泄漏到显示。`CompactCompleted` 事件必须携带 `messages: Vec<BaseMessage>`，TUI 用它更新 `agent_state_messages` 和 pipeline。禁止在 TUI 层触发 auto-compact——所有触发判断在 executor 内部。（详见 spec/global/domains/compact.md#issue_2026-05-20-compact-command-not-triggering）

**[TRAP]** compact 后消息结构必须以 `BaseMessage::human(summary + continuation)` 开头（与 Claude Code 实现对齐）。禁止将摘要放在 `BaseMessage::system()` 中——LLM 适配器（`messages_to_json`/`messages_to_anthropic`）将 System 消息提取到 system 字段不进入 messages 数组，导致发给 API 的 messages 数组中无 user/assistant 消息，DeepSeek/OpenAI 兼容 API 返回 400。compact 后的完整结构：`[Human(摘要+续接指令), System(文件)..., System(Skills)...]`。此约束适用于 `CompactMiddleware::do_full_compact()` 和 `acp_server::execute_compact()` 两条路径。（详见 spec/global/domains/compact.md#issue_2026-05-20-auto-compact-empty-messages-400）

**[TRAP]** `restore_completed(messages)` 会把 messages 中的 system 消息放入 pipeline 的 completed 列表。compact 后的 messages 中 re_inject 产生的 System 消息是内部状态，不应被渲染。pipeline 的 reconcile 逻辑通过 `messages_to_view_models` 跳过 System 消息、`build_tail_vms` 用 `rposition(Human)` 定位到摘要 Human 消息作为 reconcile 起点。`round_start_vm_idx` 和 `completed_len_at_round_start` 必须正确设置，否则 view_messages 会泄漏 system 消息。（详见 spec/global/domains/message-pipeline.md#issue_2026-05-20-session-restore-renders-system-prompt）

## MCP 中间件

`McpMiddleware` 基于 `rmcp` crate。配置合并：全局 `~/.peri/settings.json` + 项目 `{cwd}/.mcp.json`（同名覆盖）。工具命名 `mcp__{server_name}__{tool_name}`。插件 MCP 使用 `{plugin_name}__{server_name}` 前缀命名空间。

**[TRAP]** `ClaudeSettings` 的 `extraKnownMarketplaces` 和 `enabledPlugins` 需同时支持对象和数组格式。**`enabledPlugins` 写入必须用对象格式** `{"id": true}`。

**Plugin Sources 旁路表**：`load_merged_config_full` 返回 `(McpConfigFile, HashMap<String, String>)`，key 格式 `"plugin:{name}:{server}"`，value `"name@marketplace"`。`load_installed_plugins` 路径需从 `claude_home` 参数推导。

## 插件系统

兼容 Claude Code 插件生态。配置：`~/.peri/settings.json`（全局）+ `~/.claude/plugins/cache/`（插件 manifest）。

**Hooks**（`peri-middlewares/src/hooks/`）：4 种执行类型（Command/Prompt/Http/Agent），14 种事件。exit code 控制流程：0=Allow，1=Warn，2=Block。SSRF 防护阻止内网地址（`ssrf_guard.rs`），回环地址允许。

**Frontmatter 解析**：skill 和插件命令用 `gray_matter` crate（YAML engine），必须复用 `Matter::<YAML>::new()` 模式。

**Skills**：搜索顺序 `~/.claude/skills/` → `skillsDir` → `./.claude/skills/` → 插件 skills。`SkillsMiddleware.with_extra_dirs()` 是插件扩展点。

**[TRAP]** Manifest `skills` 字段语义：`skills` 数组条目是相对于插件根目录的路径（如 `"./skills/"`、`"skills/tdd"`），不是 skill 名称。`extract_skills_paths` 用 `base_dir.join(entry)` 解析路径。如果路径本身含 `SKILL.md` 则直接作为 skill 目录；否则视为容器目录，扫描其子目录找含 `SKILL.md` 的。绝不能把条目当名称拼接到 `base_dir/skills/` 下——会生成 `base_dir/skills/./skills/` 这样的无效路径。

**[TRAP]** Manifest `commands` 字段类型：Claude Code 插件 manifest 的 `commands` 支持混合数组（字符串路径 + 对象），如 `["./commands/", {"path":"x.md","name":"x"}]`。`PluginManifest.commands` 类型是 `Option<Vec<PluginCommandEntry>>`（`PluginCommandEntry` 枚举：`Path(String)` | `Full(PluginCommand)`）。`extract_commands` 必须用 match 分支处理两种变体：`Path` 变体支持目录扫描（扫描 .md 文件）和单文件路径；`Full` 变体使用显式 name/description。禁止假设所有条目都是 `PluginCommand` 对象——字符串路径是 Claude Code 插件的常见格式（如 ECC 的 `"commands": ["./commands/"]`）。

**[TRAP]** Agent 目录回退扫描：`extract_agents_paths` 在 manifest 无 `agents` 字段时必须回退扫描插件根目录下的 `agents/` 和 `.agents/` 子目录。Claude Code 插件（如 ECC）常把 agent 定义放在 `.agents/` 目录但不在 manifest 中声明。新增 agent 目录约定时必须同步更新回退扫描的目录列表。

**[TRAP]** 插件 MCP `.mcp.json` 回退：`extract_mcp_servers` 有两层加载逻辑——先处理 manifest `mcpServers` 字段（内联配置或文件路径引用），结果为空时回退加载 `install_path/.mcp.json`。当 manifest 声明 `mcpServers: {}`（空对象）时，`manifest.mcp_servers` 是 `Some(empty HashMap)`，迭代无结果后 `result.is_empty()` 为 true，会正确走到 `.mcp.json` 回退。MCP pool 初始化（`McpClientPool::run_initialize`）通过 `load_merged_config_full` 独立调用 `load_enabled_plugins_aggregated`，不依赖 TUI 层传递插件 MCP 数据。

## SubAgents

`.claude/agents/{agent_id}/agent.md` 定义。`tools` 为空继承父工具（排除 Agent 防递归），有值仅保留允许列表，`disallowedTools` 额外排除。插件 agent 通过 `scan_agents_with_extra_dirs` 追加搜索路径。

**[TRAP]** Background agent 工具完全依赖 `register_tool` 传递，跨 async 边界需确保 Arc 引用生命周期。多语义叠加（fork+background）需明确优先级，跨轮次累积数据（frozen_vms）必须有清理机制。**[TRAP]** Normal/Fork 子 Agent 透传 event_handler 导致事件溢出，StateSnapshot/ContextWarning/LlmRetrying 缺少 in_subagent() 守卫——新增事件类型时必须同步检查所有事件处理路径的守卫。**[TRAP]** 并发 SubAgent 场景：事件路由必须用 `source_agent_id` 精确匹配而非位置堆栈；流式循环必须 `tokio::select!` 竞争取消令牌防止 Ctrl+C 死锁；事件通道容量：主 executor 用 `unbounded_channel()`（无界），`bg_event_tx` 用 `channel(128)`。（详见 spec/global/domains/agent.md#issue_2026-05-12-background-agent-display-and-continuation-bugs，spec/global/domains/agent.md#issue_2026-05-13-sync-subagent-events-leak-to-parent，spec/global/domains/tui.md#issue_2026-05-15-concurrent-subagent-display-delay，spec/global/domains/agent.md#issue_2026-05-16-concurrent-subagent-tool-call-routing-and-background，spec/global/domains/agent.md#issue_2026-05-18-agent-tool-calls-execute-serially，spec/global/domains/agent.md#issue_2026-05-19-concurrent-subagent-duplicate-id，spec/global/domains/agent.md#issue_2026-05-24-concurrent-bg-agent-only-one-completion，spec/global/domains/agent.md#issue_2026-05-26-sync-subagent-cancel-fix-attempts-log）**[TRAP]** 同步 SubAgent 取消传播：父 Agent 的 cancel token 必须传播到同步 SubAgent 执行上下文，否则 Ctrl+C 无法中断 SubAgent。（详见 spec/global/domains/agent.md#issue_2026-05-25-ctrl-c-cannot-interrupt-sync-subagent）

## LSP 中间件

`LspMiddleware` + `LspTool` + `peri-lsp` 客户端库。10 种操作（goToDefinition/findReferences/hover 等），`after_tool` 自动同步文件变更（`didChange` + `didSave`）。

## Sync 模块

**[TRAP]** 路径穿越防护：`validate_and_resolve()` 是项目标准的路径穿越防护入口，使用三层校验（绝对路径拒绝 + 深度计数器检测 ParentDir + 解析后前缀验证）。任何需要接收用户侧相对路径并写入 base_dir 的场景都必须复用此函数。新增类似写入功能时禁止自行实现路径校验。

**[TRAP]** sync 模块中 FileEntry.path 来自外部不可信数据（网络传输的解密结果），写入前必须经过 `validate_and_resolve` 校验，禁止直接拼接路径或使用未校验的相对路径。

**[TRAP]** `Path::strip_prefix()` + `to_string_lossy()` 在 Windows 上产生 `\` 分隔符，但 sync 协议要求 `FileEntry.path` 使用 `/`（跨平台兼容 + 序列化一致性）。`scan_dir_recursive()` 构造 `FileEntry.path` 时必须 `.replace('\\', "/")`。新增类似路径字符串化场景必须归一化分隔符，否则 Windows 测试（如 `paths.contains(&"my-skill/SKILL.md")`）会失败。（详见 spec/global/domains/sync.md#issue_2026-05-20-windows-path-separator-breaks-tests）

`peri sync` 子命令使用标准终端 CLI（crossterm 交互 + println!），不经过 TUI 主循环。`Commands::Sync` 分支在 main.rs 中创建独立 tokio runtime。sender.rs/receiver.rs/ui.rs 使用 crossterm 通过 ratatui 重导出路径 `ratatui::crossterm::*`，不引入独立 crossterm 依赖。

## 环境变量

| 变量 | 说明 |
|------|------|
| `ANTHROPIC_API_KEY` | Anthropic API Key |
| `ANTHROPIC_BASE_URL` | Anthropic 自定义 Base URL |
| `ANTHROPIC_MODEL` | 默认 Anthropic 模型名 |
| `OPENAI_API_KEY` | OpenAI 兼容 API Key |
| `OPENAI_API_BASE` | API Base URL（优先于 `OPENAI_BASE_URL`） |
| `OPENAI_BASE_URL` | API Base URL（fallback） |
| `OPENAI_MODEL` | 模型名称（默认 gpt-4o） |
| `MODEL_PROVIDER` | Provider 选择提示（auto-detect） |
| `YOLO_MODE=true/false` | 跳过/启用 HITL 审批 |
| `RUST_LOG` | 日志级别（默认 info） |
| `RUST_LOG_FORMAT` | `"json"` 时输出 JSON 格式日志 |
| `RUST_LOG_FILE` | 日志文件路径 |
| `LANGFUSE_PUBLIC_KEY` | Langfuse 公钥（缺一则禁用遥测） |
| `LANGFUSE_SECRET_KEY` | Langfuse 密钥 |
| `LANGFUSE_BASE_URL` | Langfuse 服务地址（默认 cloud.langfuse.com） |
| `DISABLE_COMPACT` | 禁用所有 compact（含 auto + micro） |
| `DISABLE_AUTO_COMPACT` | 仅禁用 auto compact |
| `COMPACT_THRESHOLD` | 覆盖 auto compact 阈值（0.0-1.0，默认 0.85） |

配置通过 `~/.peri/settings.json` 的 `env` 字段注入。

## CLI 参数

对齐 Claude Code 核心参数体系。所有 camelCase 参数同时支持 kebab-case 别名（如 `--allowedTools` = `--allowed-tools`）。clap 4 derive 解析。

**参数列表**：

| 参数 | 说明 | 模式 |
|------|------|------|
| `-p/--print [PROMPT]` | 非交互模式：执行单轮问答后输出到 stdout 并退出 | print only |
| `--output-format` | 输出格式：text / json / stream-json（配合 `-p`） | print only |
| `--max-turns` | 最大 agentic 轮数（配合 `-p`） | print only |
| `--bare` | 极简模式：跳过 hooks/LSP/插件/MCP 初始化（配合 `-p`） | print only |
| `--permission-mode` | 权限模式：bypass / default / dont-ask / accept-edit / auto-mode | both |
| `--dangerously-skip-permissions` | 绕过所有权限检查（等同 permission-mode bypass） | both |
| `--model` | 指定模型（别名如 sonnet 或全名） | both |
| `--effort` | 推理强度：low / medium / high / max | both |
| `-c/--continue` | 继续当前目录最近的对话 | TUI |
| `-r/--resume [ID]` | 按 session ID 恢复对话 | TUI |
| `--session-id` | 指定会话 ID | TUI |
| `-n/--name` | 设置会话显示名称 | TUI |
| `--no-session-persistence` | 禁用会话持久化 | TUI |
| `--allowedTools` | 允许的工具列表 | both |
| `--disallowedTools` | 禁止的工具列表 | both |
| `--settings` | 加载额外 settings 文件或 JSON 字符串 | both |
| `-a/--approve` | 启用 HITL 审批模式（等同 --permission-mode default） | TUI |
| `-y/--yolo` | 向后兼容，无操作（YOLO 已是默认行为） | TUI |

**子命令**：`plugin list [--json]` / `plugin install <name@marketplace> [--scope user/project/local]` / `plugin uninstall <id>`。`acp`/`update`/`sync` 子命令保持不变。

**`-p` 模式架构**：复用 ACP executor（`peri_acp::session::executor::execute_prompt()`），通过自定义 `EventSink` 实现（`PrintEventSink`）收集事件并输出。不启动 TUI、不维持 session。`PrintBroker` 自动批准所有交互。

**TUI 模式参数接入**：`TuiOptions` 结构体桥接 CLI 解析到 `run_tui()`/`run_app()`。`--permission-mode`/`--dangerously-skip-permissions` 映射到 `SharedPermissionMode`，`--model` 通过 `LlmProvider::from_config_for_alias` 覆盖，`--settings` 通过 `inject_settings_override()` 合并。`-p` 专属参数在 TUI 模式下产生警告（`validate_args`）。

**核心文件**：
| 文件 | 职责 |
|------|------|
| `peri-tui/src/main.rs` | Cli struct（clap derive）+ 命令分发 + TuiOptions 桥接 |
| `peri-tui/src/cli_args.rs` | OutputFormat/EffortLevel/PluginScope 枚举 + RunOptions + validate_args |
| `peri-tui/src/cli_print.rs` | `-p` 模式：PrintBroker + PrintEventSink + PrintCollector |
| `peri-tui/src/cli_plugin.rs` | plugin list/install/uninstall 子命令实现 |
| `peri-tui/src/cli_integration_test.rs` | CLI 参数解析集成测试（9 个） |

运行时 `Shift+Tab` 切换权限模式，`Ctrl+T` 切换模型，`Ctrl+Shift+T` 切换 Provider。支持多 session 分屏。

## 编码规范

- Rust 2021 edition，tokio async/await + async-trait
- 库用 `thiserror`，应用层用 `anyhow::Result`
- 日志用 `tracing`，禁止 `println!`/`eprintln!`
- 测试与源码分离为同目录 `_test.rs` 文件（≥30 行必须分离）
- bin crate 集成测试在 `src/` 内（不支持 `tests/` 目录）
- 每模块一目录，`mod.rs` 入口；Workspace resolver = "2"，禁止下层依赖上层
- 禁止 `ℹ`（U+2139）符号和 `[i]` 前缀
- **字符串截断必须用字符级操作**：`s.chars().take(N).collect()` 或 `s.char_indices().nth(N)`，`&s[..N]` 对 CJK 会 panic
- 终端列宽用 `unicode-width` crate（CJK 占 2 列）
- **终端 UI 鼠标坐标转换**：鼠标事件坐标是显示列（unicode-width），光标位置是字符索引，需逐字符累加转换。Block padding/border 和水平滚动也要纳入偏移计算。（详见 spec/global/domains/tui.md#issue_2026-05-12-textarea-mouse-click-cursor-misposition-cjk）
- **快捷键设计**：禁止 `Shift+字母`（编辑态等同大写输入）。全局用 `Ctrl+字母`，面板用方向键/Space/Enter/Esc。
- **快捷键跨平台兼容 [TRAP]**：`Alt+Enter`/`Alt+M`/`Alt+Shift+M` 在 Windows 终端被截获（Alt+Enter 触发全屏切换、Alt 激活菜单栏），应用无法收到事件。新增快捷键必须优先用 `Ctrl+字母`，避免 `Alt` 修饰键。macOS Option 键产生组合字符（如 Option+M → `µ`），`KeyBinding` 系统通过 `macos_char` 字段兼容两种路径。快捷键标签在 UI 中统一用 `Ctrl+字母` 形式，不按平台差异化显示。
- **面板系统**：`PanelManager` + `PanelComponent` trait（`panel_manager.rs`/`panel_component.rs`），新增面板只需定义变体 + 实现 trait。面板内禁止渲染提示行，由 `status_bar_hints()` 统一描述。面板内列表组件使用统一的 `ListState` 管理（选中/滚动/过滤），支持鼠标交互（滚轮/点击/拖拽）。
- **`Event::Paste`**：独立于 key event 链，必须单独拦截。
- **翻页快捷键**：不使用 PageUp/PageDown 做滚动。macOS 终端会将 Cmd+Backspace 映射为 Ctrl+U（非 PageUp），导致误触发滚动。滚动统一用 `Ctrl+U`/`Ctrl+D`（textarea 空时）。PageUp/PageDown 静默消费不传 textarea。Ctrl+U 在 textarea 有内容时执行 `delete_line_by_head`（删除到行首，匹配 macOS Cmd+Backspace 标准行为）。禁止添加 PageUp/PageDown 滚动行为。
- **鼠标事件合并**：`next_event()` 中 `coalesce_mouse_events()` 对 ScrollUp/ScrollDown/Drag(Left) 事件做非阻塞 drain 合并——用 `event::poll(ZERO)` 排空队列中同类事件，只保留最后一个，消除滚动/拖拽时的冗余 redraw。Tradeoff：N 个 scroll 事件合并为 1 个，只移动 ±3 行而非 N×3（接受精度损失换取 CPU 降低）。非 scroll/drag 事件终止 drain 并作为结果返回（不丢弃）。

## i18n 国际化

i18n 模块位于 `peri-tui/src/i18n/`，仅 TUI crate 使用，不向 `peri-widgets` 或 `peri-agent` 传递。`LcRegistry` 存储在 `ServiceRegistry.lc` 中，翻译资源通过 `include_str!` 编译时嵌入 `locales/{lang}/main.ftl`。

**[TRAP]** `FluentBundle::get_message` 返回的 `FluentMessage` 生命周期绑定在 bundle 上，`tr()` 方法必须返回 `String` 而非 `&str`，否则无法满足借用检查。`format_pattern` 的 args 参数类型是 `FluentArgs` 而非 `HashMap`。

Command trait 的 `description()` 接收 `&LcRegistry` 参数并返回 `String`，新增命令实现时必须遵循此签名。`PluginCommandAdapter` 的 description 直接返回 manifest 中的文本（不经过 LcRegistry）。`CommandRegistry::match_prefix()` 和 `list()` 均需 `&LcRegistry` 参数，调用时统一通过 `&app.services.lc` 获取。测试中使用 `LcRegistry::default()` 构造。

## 测试编写风格

- 注释、断言消息用中文；命名 `test_<被测对象>_<场景>`
- Arrange-Act-Assert，无空行分隔
- 断言优先 `assert_eq!`/`assert!`，`.unwrap()` 仅用于构造测试数据
- Mock 命名 `make_` 前缀（函数），`Mock` 前缀（结构体），不跨文件共享
- 最小依赖：`assert!`/`assert_eq!`/`matches!` + `tempfile` + `tokio-test`

## 开发注意事项

- **测试隔离**：禁止写入全局配置。用 `App::save_config(cfg, self.config_path_override.as_deref())`。
- **`std::sync::RwLockReadGuard` 不是 `Send`**，async 中不能跨 `.await` 持有，用 `parking_lot::RwLock`。（详见 spec/global/domains/lsp.md#issue_2026-05-12-lsp-transport-no-fast-fail-on-process-exit）
- **`CommandRegistry::dispatch` 借用限制 [TRAP]**：`&self` + `&mut App` 冲突，当前用 `std::mem::take` + put-back 解决。在 dispatch 期间改变 `app.session_mgr.active` 的命令（如 `/split`）存在 session index 竞态风险——take 前保存 index，归还时使用保存值。（详见 spec/global/domains/tui.md#issue_2026-05-12-split-session-command-hint-only-shows-active）
- **`ServiceRegistry` 与 `GlobalUiState`**：`App` 状态已拆分为 `ServiceRegistry`（跨会话共享服务：config/MCP/cron/provider）和 `GlobalUiState`（纯 UI 临时状态：高亮计时器/弹窗/鼠标检测）。面板 dispatch 宏（`with_global_panels!`/`with_session_panels!`）位于 `event/macros.rs`，封装了 `mem::take` 模式。
- **`app/mod.rs` 模块组织**：使用 `include!` 按功能类别分组声明（`modules_panels.inc`/`modules_agent.inc`/`modules_state.inc`/`modules_system.inc`），每个 `.inc` 文件声明一组相关模块。新增模块按类别加入对应的 `.inc` 文件，避免 `app/mod.rs` 膨胀。
- **`app/edit_utils.rs`**：从 `app/mod.rs` 拆出的文本编辑辅助函数（`build_textarea`/`handle_edit_key`/`ensure_cursor_visible`/`edit_display_parts`），供多面板共用。
- **插件面板拆分**：handler 从 `plugin_panel/handlers.rs` 拆分为 `handlers/plugin_handlers/` 9 个子模块（delete/discover_detail/discover_list/discover_search/install/installed_detail/marketplace/persistence/mod）。渲染从 `panels/plugin.rs` 拆分为 `panels/plugin/mod.rs` + `plugin_render/` 6 个子模块（add_marketplace/detail/discover_detail/discover_list/discover_search/list）。
- **跨平台 spawn [TRAP]**：所有子进程 spawn 必须通过平台感知的统一 wrapper（`shell_command()/spawn_shell()`），Windows 用 `cmd /C`、Unix 用 `bash -c`。三处调用点（MCP transport、Hooks executor、Bash 工具）已统一，新增 spawn 时必须复用。（详见 spec/global/domains/mcp.md#issue_2026-05-27-cross-platform-spawn-wrapper）
- **MultiplexBroker 竞速 [TRAP]**：ChannelBroker 不支持 Questions 交互类型，不应与 TUI broker 参与竞速，否则空答案会被优先采纳导致 AskUserQuestion 弹窗失效。（详见 spec/global/domains/agent.md#issue_2026-05-29-ask-user-tool-auto-complete）
