# ACP 协议一致性审查：11 项偏离（session/load 未回放、ResourceLink 静默丢弃、capabilities 失真）

**状态**：Open
**优先级**：中
**创建日期**：2026-09-29
**GitHub Issue**：cc-claws/cc-code#257

## 问题描述

对照 ACP v1 规范对现有实现做了一次逐方法比对。结论：**stdio 路径（`peri acp` → `run_acp_stdio`，基于官方 `agent-client-protocol` SDK）基本合规**，但存在 5 项 MUST 级偏离；另一条 mpsc 路径复用了 ACP 的响应类型，但请求契约并非 ACP。本次为审查记录，未做任何代码改动。

## 审查基准

| 项 | 值 |
|----|-----|
| 规范 | ACP v1（agentclientprotocol.com，`/protocol/v1/*`） |
| `agent-client-protocol-schema` | 0.12.0（crate 侧 features：`unstable` + `unstable_elicitation`） |
| SDK | `agent-client-protocol` 0.11.1 / `-tokio` 0.11.1 |
| 审查方式 | 逐方法对照规范 JSON 结构与 MUST/SHOULD 措辞，回溯到具体代码位置 |

## 现状：两条路径 + 一处死代码

**路径 A —— stdio（对外唯一合规入口）**
`peri-tui/src/acp_stdio.rs` 的 `run_acp_stdio()` 用 SDK 的 `Agent.builder().on_receive_request(...)` 注册全部方法，事件经 `StdioEventSink`（`peri-acp/src/session/event_sink.rs:122-159`）回推 `session/update`。协议合规性由 SDK 的类型系统兜底。

**路径 B —— mpsc（TUI 内部）**
`peri-tui/src/acp_server/*` 是手写 JSON-RPC 分发。它复用了 ACP 的**响应**类型（`NewSessionResponse` / `PromptResponse` / `AcpError`），但**请求**契约是自定的：

- `session/prompt` 收 `{sessionId, message:{role,content}}`（`peri-tui/src/acp_client/client.rs:290-293`），规范要求 `{sessionId, prompt:[ContentBlock]}`
- 内容反序列化为 peri 自己的 `MessageContent`（`peri-tui/src/acp_server/prompt.rs:58-61`），而非 ACP `ContentBlock`
- `session/new` 额外接受非规范参数 `model`（`client.rs:229`、`peri-tui/src/acp_server/requests.rs:121-130`）

外部 ACP 客户端接上该路径会直接失败。目前只有自家 TUI 使用，因此不构成线上问题，但「看起来合规、实际不合规」是后续返工的隐患。

**死代码**
`peri-acp/src/transport/stdio.rs` 的 `StdioTransport`（约 300 行，含 stdin pump、pending map、envelope 编解码）**从未被任何地方实例化**（全仓仅 `mod.rs` 的文档注释与自身单测引用）。真正的 stdio 走 SDK 的 `Stdio::new()`（`acp_stdio.rs:995-1011`）。两套 stdio 实现并存，其中一套无人使用。

---

## A. 违反 MUST

### A1 `session/load` 从不回放历史

| 项 | 内容 |
|----|------|
| 规范 | "The Agent **MUST** replay the entire conversation to the Client in the form of `session/update` notifications (like `session/prompt`) … When **all** the conversation entries have been streamed to the Client, the Agent **MUST** respond to the original `session/load` request."（`/protocol/v1/session-setup#loading-a-session`） |
| 代码 | `peri-tui/src/acp_stdio.rs:760-842`（`:824` 先 respond，`:833-837` 才发 `AvailableCommandsUpdate`）；`peri-tui/src/acp_server/requests.rs:283-373`（`:371` respond） |
| 现象 | 历史消息只被装进 `SessionInfo.history`，没有任何 `user_message_chunk` / `agent_message_chunk` 通知发出 |
| 影响 | IDE（Zed）恢复历史会话 → 界面空白，但 agent 上下文是完整的；用户看到空对话却能追问，语义割裂。TUI 路径因本地持有消息视图而被掩盖 |
| 修复方向 | 新增 `BaseMessage → SessionUpdate` 转换（Human→`UserMessageChunk`，Ai 文本→`AgentMessageChunk`，Reasoning→`AgentThoughtChunk`，ToolUse/ToolResult→`ToolCall`/`ToolCallUpdate`），在 respond **之前**逐条发送。可参考 `peri-acp/src/event/mapper.rs` 的现有映射思路，但输入是历史消息而非 `ExecutorEvent` |
| TUI 影响 | 无（只改 stdio 分支）。mpsc 路径建议不动——TUI 本身维护视图，回放反而会重复 |

### A2 `ResourceLink` 提示块被静默丢弃

| 项 | 内容 |
|----|------|
| 规范 | "All agents **MUST** support text content blocks in prompts." / "All agents **MUST** support resource links in prompts."（`ContentBlock` 定义）；`ContentBlock::Resource`（内嵌内容）需 `promptCapabilities.embeddedContext` |
| 代码 | `peri-tui/src/acp_stdio.rs:401-411`：`Text`/`Image` 有分支，其余 `_ => None` |
| 现象 | 客户端发来的 `ResourceLink` / `Resource` / `Audio` 被静默过滤。若整个 prompt 只有 resource link，`blocks` 为空 → 回退成 `MessageContent::text("")`（`:412-416`）→ agent 收到**空 human message** |
| 影响 | Zed 中 `@` 引用文件是常见操作；用户以为把文件给了 agent，实际 agent 只收到空串（还可能因空消息触发上游 400） |
| 修复方向 | `ResourceLink` → 文本引用（如 `@file: <uri>`，让 agent 用 Read 自行取内容）；`Resource` → 内嵌文本；确实不支持的 `Audio` 应返回 `-32602` 而不是静默丢弃 |
| TUI 影响 | 无（TUI 走 `MessageContent`，不经过该分支） |

### A3 错误路径返回「成功 + 伪造 sessionId」

| 项 | 内容 |
|----|------|
| 规范 | JSON-RPC 2.0 错误语义；`session/close` 段落明确 "Agents **MAY** return an error if the session does not exist or is not currently active" |
| 代码 | `peri-tui/src/acp_stdio.rs:293`（create_thread 失败 → `NewSessionResponse::new(SessionId::new("error"))`）、`:862`、`:868`、`:882`（fork 各失败分支 → `ForkSessionResponse::new(SessionId::new("error"))`） |
| 现象 | 失败时仍回 200/result，sessionId 字面量是 `"error"` |
| 影响 | 客户端把 `"error"` 当合法会话继续用；后续 `session/prompt` 落到不存在的 session → 回 `EndTurn` 空响应（`acp_stdio.rs:431`），错误被彻底吞掉、无法诊断。对比 mpsc 路径在同类场景返回的是 `-32602`（`peri-tui/src/acp_server/prompt.rs:77`） |
| 修复方向 | handler 内返回 `Err(agent_client_protocol::Error::...)`。注意 SDK 闭包的 Ok 类型在现有代码里混用了 `Ok(())` 与 `Ok(Handled::No/Yes)`（`acp_stdio.rs:930/990` vs `:359`），改动前需先确认 `on_receive_request` 的实际签名与两者如何统一 |
| TUI 影响 | 无 |

### A4 `capabilities` 声明超出实现

| 项 | 内容 |
|----|------|
| 代码 | `peri-acp/src/dispatch/init.rs:13-25`（两条路径共享） |
| 现象 | ① `load_session(true)` 但 A1 未实现 → 声明本身即有害；② 声明 `sessionCapabilities.resume`，但 `session/resume` 插入的是**空历史**（`acp_stdio.rs:736-748`、`requests.rs:425-471`） |
| 规范 | `session/resume`：「Unlike `session/load`, the Agent **MUST NOT** replay the conversation history … it restores the session context, reconnects to the requested MCP servers, and returns once the session is ready to continue」 |
| 影响 | 当前 resume 语义等于「挂旧 ID 的全新会话」——上下文丢失但对外宣称已恢复 |
| 修复方向 | 二选一：从 `ThreadStore` 加载历史但**不发通知**（与 A1 共用读取逻辑，只是不推送），或撤下 `resume` 声明直到实现 |
| TUI 影响 | 无。`acp_client/client.rs` 只调用 `session/load`，从不调用 `session/resume` / `fork` / `list` / `initialize`（已 grep 确认） |

### A5 客户端传入的 `mcpServers` 被忽略

| 项 | 内容 |
|----|------|
| 规范 | `session/new` 的 `mcpServers` "Agents **SHOULD** connect to all MCP servers specified by the Client"；stdio transport 为 Agent **MUST** 支持 |
| 代码 | `peri-tui/src/acp_stdio.rs:286`：`req: NewSessionRequest` 只使用了 `req.cwd`，`req.mcp_servers` 从未被读取 |
| 现状 | MCP 连接池完全由 peri 自身配置构建（`acp_stdio.rs:142-163`，`McpClientPool::run_initialize(pool, cwd, claude_home, ...)`，与 ACP 请求无关）；`mcpCapabilities` 也未声明 |
| 影响 | IDE 侧配置的 MCP server 永远不生效 |
| 关联 | `spec/issues/2026-05-16-acp-mcp-over-acp-unimplemented.md` 记录的是 IDE 端托管 MCP 的**隧道**方案（`mcp/connect` / `mcp/message` / `mcp/disconnect`）。两者是同一需求的不同承载方式（隧道 vs 直连参数），需一并决策，避免做两遍 |
| TUI 影响 | 无 |

---

## B. 能力与语义失真

### B1 `promptCapabilities` 全为 false，但 Image 实际已实现

`peri-acp/src/dispatch/init.rs:16` 用 `PromptCapabilities::new()`（image/audio/embeddedContext 全 false），而 `acp_stdio.rs:406` 明确处理了 `ContentBlock::Image`。遵守能力位的客户端（Zed）将永远不发图片。

修复：`.prompt_capabilities(PromptCapabilities::new().image(true))`。`embeddedContext` 暂不声明（Resource 尚未处理，见 A2）。
TUI 影响：无（TUI 不调 `initialize`）。

### B2 stdio 路径没有 `session/request_permission`

`StdioBroker`（`acp_stdio.rs:68-107`）对 Approval 一律 `Approve`、Questions 一律空答案，因此 client 的基线方法 `session/request_permission` 从未被调用；`acp_stdio.rs:190-192` 里 permission_mode 更是硬编码 `Bypass`——虽然 `session/set_mode` 会改它（`:565-581`），但改了也没人用。对比 `AcpTransportBroker`（`peri-acp/src/broker/transport_broker.rs:50-102`）已正确实现 `session/request_permission` + `elicitation/create`，却只接在 mpsc 路径上（`acp_server/prompt.rs:102-104`）。

修复方向：为 SDK 的 `ConnectionTo<Client>` 写一个等价 broker（现有 broker 依赖 `AcpTransport` 抽象，不能直接复用），替换 stdio 的 `StdioBroker`。
TUI 影响：无（不动 mpsc broker）。

### B3 广播了 20 个 ACP 侧不存在的「幽灵命令」

`peri-acp/src/dispatch/commands.rs:8-54` 固定发布 27 个命令 + skills；`default_command_registry()`（`peri-acp/src/session/command/mod.rs:143-153`）只注册 7 个（compact / clear / rewind / init / recap / commit / review）。`help`、`exit`、`doctor`、`mcp`、`hooks`、`plugin`、`cron`、`agents`、`memory`、`login`、`rename`、`lang`、`model`、`mode`、`effort`、`loop`、`history`、`context`、`cost`、`away`/`catchup` 均为 TUI 本地命令（`peri-tui/src/command/`），ACP 侧不存在。`executor.rs:184-187` 查不到命令就当作普通 prompt 文本转给 LLM。

影响：IDE 命令面板里选了 `/doctor`，实际是把字符串 "/doctor" 发给模型。

**注意（TUI 耦合点）**：`build_available_commands()` 是两条路径共享的，而 **TUI 确实消费该通知**——`peri-tui/src/app/agent_ops/acp_bridge.rs:268-287` 会把命令名学进 `CommandSystem`，`app/hint_ops.rs:50` 用于命令提示。因此**不能直接改这个函数**；应为 stdio 新增独立列表（或在 stdio 调用点做过滤），TUI 侧保持现状 → 对 TUI 零影响。

### B4 配置状态是进程级全局，非 session 级

`StdioContext`（定义 `acp_stdio.rs:20-40`，实例化 `:222-238`）只有一份 `provider` / `peri_config` / `permission_mode`；`session/set_mode`（`:565-581`）、`session/set_config_option`（`:620-679`）直接改全局。同一 IDE 打开多个会话时互相串扰（切 A 的模式影响 B）。

修复成本较高（`SessionInfo` 需持有 per-session override，`executor::execute_prompt` 的 provider/permission 入参需按会话传），建议单独排期。

### B5 自定义扩展命名违反规范的 `_` 前缀约定

| 名称 | 位置 |
|------|------|
| `peri/agent_event`、`peri/agent_event_done` | `peri-acp/src/session/event_sink.rs:94,107`；消费方 `peri-tui/src/acp_client/client.rs:102,163` |
| `peri/session/steer` | `peri-tui/src/acp_server/mod.rs:183` |
| `session/update_config` | `acp_client/client.rs:394`、`acp_server/requests.rs:529`、`acp_stdio.rs:929` |

规范 extensibility：自定义方法/通知应以下划线前缀（`_`）声明，且不应占用规范命名空间。`session/update_config` 尤其危险——它占据 `session/` 保留域，并与规范的 `session/set_config_option` 语义重叠（我们在 `acp_stdio.rs:620-679` 已经实现了后者，两者并存）。另外 `TransportEventSink` 把 `_peri` 塞在 params 顶层键而非 `_meta`（`event_sink.rs:70`）。

影响：目前仅自家 TUI 消费，无外部破坏；但与规范未来方法的冲突风险持续存在（Zed 收到未知通知会忽略）。
TUI 影响：**有** —— 改名需同时改发送侧与 `acp_client` 解析侧，属联合改动，不建议在最小化批次里做。

### B6 细节

- initialize 响应缺少 `agentInfo`（规范 SHOULD），`init.rs:24` 只设了 protocolVersion 与 capabilities。
- `initialize` 完全忽略 `clientCapabilities`（`acp_stdio.rs:276`），因此从不调用 `fs/read_text_file`；编辑器内未保存的 buffer 对 agent 不可见。
- `PromptStopReason`（`peri-acp/src/session/executor.rs:41-47`）只有 EndTurn / Cancelled / MaxTurnRequests，`MaxTokens` / `Refusal` 不可达——因 max_tokens 截断被报成 `EndTurn`（`:623-631`，映射点 `acp_stdio.rs:541-545`）。

---

## C. 结构性问题（非协议违规，但影响后续返工）

- **C1**：`StdioTransport` 死代码（见「现状」）。收敛前，任何人改 stdio 行为都要先分辨「改哪一套」。
- **C2**：路径 B 的请求契约非 ACP（见「现状」）。若未来要让 TUI 之外的客户端接入 mpsc 传输，必须先统一契约。

---

## D. 修复批次建议（按 TUI 风险排序）

**批次 1 —— 最小改动、TUI 零影响**（只动 `peri-tui/src/acp_stdio.rs` 与 `peri-acp/src/dispatch/init.rs`）

1. A3 错误路径改真错误（先确认 SDK handler 返回类型）
2. A1 `session/load` 回放
3. A2 `ResourceLink` 不再静默丢弃
4. B1 `promptCapabilities.image`
5. A4② `resume` 加载历史（stdio 分支）或撤下声明

**批次 2 —— 需要新代码，仍是 stdio 局部**

6. B2 stdio 接入权限/问答 broker
7. B3 stdio 独立命令列表
8. A5 `mcpServers`（需先与 MCP-over-ACP issue 合并决策）

**批次 3 —— 架构级，需专门排期**

9. B4 配置 session 化
10. B5 扩展命名收敛（需 TUI 侧联合改动）
11. C1 / C2 死代码与契约收敛

## 涉及文件

- `peri-tui/src/acp_stdio.rs` —— stdio 路径全部方法处理器（A1-A5、B1、B2、B6 主战场）
- `peri-acp/src/dispatch/init.rs` —— 两条路径共享的能力声明（A4、B1、B6）
- `peri-acp/src/dispatch/commands.rs` —— 幽灵命令来源（B3）
- `peri-acp/src/session/command/mod.rs` —— 实际可执行的 7 个命令（B3 对照基准）
- `peri-acp/src/session/event_sink.rs` —— 事件回推 + `_peri` 顶层键（B5）
- `peri-acp/src/broker/transport_broker.rs` —— 现有正确实现的权限/elicitation broker（B2）
- `peri-tui/src/acp_server/*` —— mpsc 路径（A1/A3 的对照实现，契约非 ACP：C2）
- `peri-acp/src/transport/stdio.rs` —— 未使用的 `StdioTransport`（C1）
- `peri-tui/src/acp_client/client.rs` —— TUI 作为 client 的调用面（决定了哪些改动会波及 TUI）
- `peri-tui/src/app/agent_ops/acp_bridge.rs` —— TUI 消费 `available_commands_update`（B3 的耦合点）

## 参考

- [ACP Overview](https://agentclientprotocol.com/protocol/overview)
- [Initialization](https://agentclientprotocol.com/protocol/v1/initialization)
- [Session Setup](https://agentclientprotocol.com/protocol/v1/session-setup)（load / resume / close / mcpServers）
- [Session Config Options](https://agentclientprotocol.com/protocol/v1/session-config-options)
- [Extensibility](https://agentclientprotocol.com/protocol/v1/extensibility)（`_` 前缀与 `_meta`）
- `spec/issues/2026-05-16-acp-mcp-over-acp-unimplemented.md` —— MCP 相关（A5 关联）
- `spec/issues/2026-05-29-acp-session-info-update-protocol-migration.md` —— `session/update` 体系迁移
- GitHub：cc-claws/cc-code#257
