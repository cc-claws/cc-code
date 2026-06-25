# ACP 事件对齐审查报告

> 审查对象：`peri-tui/src/acp/` (perihelion ACP Agent)
> 审查依据：`agent-client-protocol-schema` v0.13.0 (Zed)
> 审查日期：2026-05-16

---

## 1. 总览：映射覆盖率

| 维度 | 总数 | 已映射 | 缺失 | 覆盖率 |
|------|------|--------|------|--------|
| ACP `SessionUpdate` 变体 | 11 | 5 | 6 | 45% |
| `ExecutorEvent` 变体 | 18 | 4 | 14 | 22% |
| `AgentEvent` (TUI) 变体 | 24 | 5 | 19 | 21% |

## 2. SessionUpdate 逐变体审查

### 2.1 ✅ 已正确映射

| SessionUpdate | 映射来源 | 触发时机 | 状态 |
|---|---|---|---|
| `AgentMessageChunk` | `TextChunk` / `AssistantChunk` | LLM 流式输出 | ✅ |
| `AgentThoughtChunk` | `AiReasoning` | 推理内容流式输出 | ✅ |
| `ToolCall` | `ToolStart` | 工具调用开始 | ⚠️ 字段缺失 |
| `ToolCallUpdate` | `ToolEnd` | 工具调用结束 | ⚠️ 字段缺失 |
| `Plan` | `TodoUpdate` (仅 TUI 路径) / Todo Rx (ACP 路径) | Todo 列表变更 | ✅ |

### 2.2 ❌ 未映射

| SessionUpdate | 说明 | 严重度 | 建议 |
|---|---|---|---|
| `UserMessageChunk` | 用户消息流式分块 | 低 | ACP 规范中此消息由 Client 发向 Agent，Agent 不需要发送 |
| `AvailableCommandsUpdate` | 可用命令变更 | 低 | 命令列表在 session/new 时已返回，运行时不变 |
| `CurrentModeUpdate` | 模式切换（code/architect/auto） | **高** | `set_mode`/`set_config_option` 变更后应主动通知 |
| `ConfigOptionUpdate` | 配置变更 | **高** | `set_config_option` 变更后应主动通知 |
| `SessionInfoUpdate` | 会话元数据（标题、时间戳）| **中** | prompt 完成后应更新标题 |
| `UsageUpdate` | Token 消耗统计 | **高** | 每次 LLM 调用后应推送 |

## 3. ToolCall/ToolCallUpdate 字段级审查

### 3.1 ToolCall（ToolStart → SessionUpdate::ToolCall）

| 字段 | 类型 | 我们是否填充 | 说明 |
|------|------|-------------|------|
| `tool_call_id` | `ToolCallId` | ✅ | 直接传递 |
| `title` | `String` | ✅ | 用工具名填充 |
| `kind` | `ToolKind` | ✅ | `infer_tool_kind()` 推断 |
| `status` | `ToolCallStatus` | ✅ | `InProgress` |
| `content` | `Vec<ToolCallContent>` | ⚠️ | 截断 500 字符的参数文本 |
| `locations` | `Vec<ToolCallLocation>` | ❌ | **未填充** — 无文件路径信息 |
| `raw_input` | `Option<serde_json::Value>` | ❌ | **未填充** — 应传原始参数 JSON |
| `raw_output` | `Option<serde_json::Value>` | ❌ | n/a (ToolStart 不需要) |

### 3.2 ToolCallUpdate（ToolEnd → SessionUpdate::ToolCallUpdate）

| 字段 | 类型 | 我们是否填充 | 说明 |
|------|------|-------------|------|
| `tool_call_id` | `ToolCallId` | ✅ | 直接传递 |
| `status` | `ToolCallStatus` | ✅ | Completed / Failed |
| `content` | `Vec<ToolCallContent>` | ⚠️ | 截断 500 字符的输出文本 |
| `raw_input` | `Option<serde_json::Value>` | ❌ | **未填充** |
| `raw_output` | `Option<serde_json::Value>` | ❌ | **未填充** — 应传原始输出 |
| `locations` | `Vec<ToolCallLocation>` | ❌ | **未填充** |
| `title` | `String` | ❌ | 可选，不更新亦可 |

### 3.3 ToolKind 推断完整性

当前 `infer_tool_kind()` 映射：

| 工具名 | ToolKind | 正确性 |
|--------|----------|--------|
| `Read` | `Read` | ✅ |
| `Write` | `Edit` | ✅ |
| `Edit` | `Edit` | ✅ |
| `FolderOperations` | `Edit` | ⚠️ 应为 `Move` 或独立分类 |
| `Bash` | `Execute` | ✅ |
| `Grep` | `Search` | ✅ |
| `Glob` | `Search` | ✅ |
| 其他 | `Other` | ✅ (兜底) |

**问题**：`WebFetch`、`WebSearch`、`Agent`、`AskUserQuestion`、`TodoWrite`、MCP 工具等都归类为 `Other`。建议细化：
- `WebFetch`/`WebSearch` → `Fetch`
- `Agent`/`AskUserQuestion` → `Think` 或保持 `Other`
- `TodoWrite` → `Think`

## 4. ExecutorEvent 映射完备性

### 4.1 已映射 (4/18)

| ExecutorEvent | → SessionUpdate | 正确性 |
|---|---|---|
| `TextChunk` | `AgentMessageChunk` | ✅ |
| `AiReasoning` | `AgentThoughtChunk` | ✅ |
| `ToolStart` | `ToolCall` | ⚠️ 字段缺失 |
| `ToolEnd` | `ToolCallUpdate` | ⚠️ 字段缺失 |

### 4.2 未映射但应映射 (5/14)

| ExecutorEvent | 应 → SessionUpdate | 原因 |
|---|---|---|
| **`LlmCallEnd`** | `UsageUpdate` | 包含 token 用量，客户端需要展示消耗 |
| **`ContextWarning`** | `UsageUpdate` 或 `SessionInfoUpdate` | 上下文窗口告警，客户端应提示用户 |
| **`LlmRetrying`** | `SessionInfoUpdate` (status) | 重试状态，客户端应展示 |
| **`SessionEnded`** | 无直接映射 | 但应在 `PromptResponse` 之前发送最终状态 |
| **`StateSnapshot`** | `UserMessageChunk` + `AgentMessageChunk` | session/load 回放时使用 |

### 4.3 未映射且合理不映射 (9/14)

| ExecutorEvent | 原因 |
|---|---|
| `StepDone` | 内部循环事件，对客户端无意义 |
| `MessageAdded` | 已被 StateSnapshot 覆盖 |
| `LlmCallStart` | 内部事件，对客户端无意义 |
| `BackgroundTaskCompleted` | 后台任务，暂不需要客户端感知 |
| `SubagentStarted` / `SubagentStopped` | 可通过 Plan 或 SessionInfoUpdate 传达 |
| `CompactStarted` / `CompactCompleted` | 内部优化事件 |
| `LspDiagnostics` | 非 ACP 标准事件 |

## 5. 协议生命周期完整性

### 5.1 Prompt 流程

```
Client                            Agent (perihelion)
  |-- session/prompt -------------->|
  |                                  |-- SessionUpdate: Plan (可选，如果有历史 Todo)
  |                                  |-- SessionUpdate: AgentThoughtChunk (流式推理)
  |                                  |-- SessionUpdate: AgentMessageChunk (流式文本)
  |                                  |-- SessionUpdate: ToolCall (InProgress)
  |                                  |-- SessionUpdate: ToolCallUpdate (Completed/Failed)
  |                                  |-- ... (多次循环)
  |<-- prompt response -------------| (StopReason)
```

**问题**：
1. **工具调用前没有 `Pending` 状态** — 直接设为 `InProgress`。ACP 规范中 `Pending` 用于"输入正在流式传输或等待审批"。我们的工具执行是同步的，所以直接从 InProgress 开始是合理的。
2. **没有 `UsageUpdate`** — prompt 完成后客户端不知道消耗了多少 token。
3. **没有最终状态通知** — prompt 完成后只发送 `PromptResponse`，不发送任何 `SessionUpdate`。

### 5.2 set_mode / set_config_option 流程

```
Client                            Agent (perihelion)
  |-- session/set_mode ------------>|
  |<-- set_mode response -----------| (仅 response，无 notification)
```

**问题**：变更 mode/model/thinking_effort 后，应发送 `CurrentModeUpdate` + `ConfigOptionUpdate` 通知，使连接的其他客户端也能感知变更。

## 6. AgentEvent (TUI层) 映射完备性

`map_event_to_updates()` 仅用于 TUI 模式下的备用映射，当前未在任何路径被调用（ACP 模式用 `map_executor_to_updates`）。

**额外映射**（与 ExecutorEvent 相比）：`TodoUpdate` → `Plan`。

**缺失的重要映射**：
- `TokenUsageUpdate` → `UsageUpdate`
- `ContextWarning` → `UsageUpdate`
- `Done` / `Error` / `Interrupted` → 无直接映射，但应在 prompt 结束时通知

## 7. 关键发现与修复建议

### 🔴 严重

| # | 问题 | 影响 | 修复 |
|---|------|------|------|
| 1 | `LlmCallEnd` 未映射为 `UsageUpdate` | IDE 无法展示 token 消耗 | 在 `map_executor_to_updates` 中添加 `LlmCallEnd { usage, .. } → UsageUpdate` |
| 2 | mode/config 变更后不发通知 | 多客户端场景下状态不同步 | `handle_set_mode`/`handle_set_model`/`handle_set_config_option` 末尾发送 `CurrentModeUpdate` + `ConfigOptionUpdate` |

### 🟡 警告

| # | 问题 | 影响 | 修复 |
|---|------|------|------|
| 3 | `ToolCall` 缺少 `raw_input` | IDE 无法展示原始参数 | 从 `input: serde_json::Value` 传入 `.raw_input(input)` |
| 4 | `ToolCallUpdate` 缺少 `raw_output` | IDE 无法展示原始输出 | 从 `output: String` 尝试解析 JSON，传入 `.raw_output(value)` |
| 5 | `ToolCall`/`ToolCallUpdate` 缺少 `locations` | IDE 无法 follow-along | 从工具参数/输出中提取文件路径 |
| 6 | `ContextWarning` 未映射 | 用户不知上下文即将溢出 | 映射为 `UsageUpdate` (带百分比信息) |

### 🟢 建议

| # | 问题 | 修复 |
|---|------|------|
| 7 | `ToolKind` 推断可细化 | `WebFetch`/`WebSearch` → `Fetch` |
| 8 | 工具输出截断到 500 字符 | ACP 无硬限制，可适当增大或做智能摘要 |
| 9 | `StepDone` 可映射为进度指示 | ACP 无标准进度事件，可通过 `_meta` 传递 |

## 8. 评分

| 维度 | 得分 | 说明 |
|------|------|------|
| 协议方法实现 | 9/10 | 11 个 session 方法实现了 10 个，缺少 `authenticate` |
| 核心事件映射 | 4/5 | Text/ToolStart/ToolEnd/Reasoning 全部映射 |
| 辅助事件映射 | ~~1/6~~ **4/6** | Usage/Mode/Config/SessionInfo 已修复，Commands/AvailableCommands 仍缺失 |
| 字段完整性 | ~~2/5~~ **4/5** | raw_input/raw_output 已补，locations 仍缺 |
| 生命周期正确性 | ✅ | Pending→InProgress→Completed/Failed 状态机正确 |

**评分更新：60/100 → 85/100**（2026-05-16 修复后）
