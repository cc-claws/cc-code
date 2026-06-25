# ACP 协议对接缺口全景报告

> perihelion `peri-tui/src/acp/` vs. `agent-client-protocol-schema` v0.13.0
> 审查日期：2026-05-16

---

## 一、协议方法覆盖率总览

### Agent 侧（Client → Agent，Agent 必须处理）

| # | 方法 | 稳定性 | 已实现 | 说明 |
|---|------|--------|--------|------|
| 1 | `initialize` | 稳定 | ✅ | 能力协商 |
| 2 | **`authenticate`** | 稳定 | ❌ | 用户认证，**稳定方法中唯一缺失** |
| 3 | `session/new` | 稳定 | ✅ | 创建会话 |
| 4 | `session/load` | 稳定 | ✅ | 加载历史会话 |
| 5 | `session/list` | 稳定 | ✅ | 列出会话 |
| 6 | `session/resume` | 稳定 | ✅ | 恢复会话 |
| 7 | `session/close` | 稳定 | ✅ | 关闭会话 |
| 8 | `session/set_mode` | 稳定 | ✅ | 切换模式 |
| 9 | `session/set_config_option` | 稳定 | ✅ | 设置配置 |
| 10 | `session/prompt` | 稳定 | ✅ | 发送提示词 |
| 11 | `session/cancel` | 稳定 | ✅ | 取消 |
| 12 | `session/fork` | unstable | ✅ | **超越协议：已实现不稳定方法** |
| 13 | `session/set_model` | unstable | ✅ | **超越协议：已实现不稳定方法** |
| 14 | `providers/list` | unstable | ❌ | LLM 提供商列表 |
| 15 | `providers/set` | unstable | ❌ | 配置提供商 |
| 16 | `providers/disable` | unstable | ❌ | 禁用提供商 |
| 17 | `logout` | unstable | ❌ | 登出 |
| 18-20 | `nes/*` | unstable | ❌ | 代码补全建议 |
| 21-22 | `nes/accept,reject` | unstable | ❌ | - |
| 23-27 | `document/*` | unstable | ❌ | 编辑器文档事件 |
| 28-29 | `mcp/*` (Agent 侧) | unstable | ❌ | MCP-over-ACP |
| 30 | `$/cancel_request` | unstable | ❌ | 协议级取消 |
| - | `_*` (扩展方法) | 稳定 | ⚠️ 未显式处理 | 通过 `handle_dispatch` 返回 `Handled::No` 透传 |

> **稳定方法覆盖率：10/11（91%）**
> 含不稳定方法：12/30（40%）

### Client 侧（Agent → Client，Agent 主动发起）

| # | 方法 | 稳定性 | 已实现 | 说明 |
|---|------|--------|--------|------|
| 1 | `session/update` (通知) | 稳定 | ✅ | 核心流式推送 |
| 2 | `session/request_permission` | 稳定 | ✅ | 权限请求桥接 |
| 3 | **`client/available_commands`** (通知) | 稳定 | ❌ | 从未发送 |
| 4 | `fs/write_text_file` | 稳定 | ❌ | **架构决策：直接操作文件** |
| 5 | `fs/read_text_file` | 稳定 | ❌ | **架构决策：直接操作文件** |
| 6 | `terminal/create` | 稳定 | ❌ | **架构决策：直接操作终端** |
| 7 | `terminal/output` (通知) | 稳定 | ❌ | **架构决策：直接操作终端** |
| 8 | `terminal/release` | 稳定 | ❌ | **架构决策：直接操作终端** |
| 9 | `terminal/wait_for_exit` | 稳定 | ❌ | **架构决策：直接操作终端** |
| 10 | `terminal/kill` | 稳定 | ❌ | **架构决策：直接操作终端** |
| 11-15 | `elicitation/*`, `mcp/*` | unstable | ❌ | 不稳定特性 |

> **稳定方法实际对接：2/10（20%）— 但 fs/terminal 7 个是架构性省略**

---

## 二、能力声明 vs 实际能力

当前声明（`request_handler.rs`）：

```rust
AgentCapabilities {
    load_session: true,
    prompt_capabilities: { image: true },
    session_capabilities: {
        close: Some,
        list: Some,
        resume: Some,
    },
}
```

### 缺失的能力声明

| 能力 | 我们是否支持 | 当前声明 | 应声明 |
|------|-------------|---------|--------|
| `fork` | ✅ 已实现 | ❌ | ✅ |
| `models` (set_model) | ✅ 已实现 | ❌ | ✅ |
| `config` (set_config_option) | ✅ 已实现 | ❌ | ✅ |
| `prompt_capabilities.audio` | ❌ 不支持 | ❌ | ❌ (正确) |
| `prompt_capabilities.embedded_refs` | ❌ 不支持 | ❌ | ❌ (正确) |
| `mcp` | ⚠️ 有 MCP middleware 但未通过 ACP | ❌ | ❌ (正确) |

---

## 三、每个缺失项的详细分析

### 🔴 authenticate（稳定，应实现）

```
Client → Agent:
{
  "method": "authenticate",
  "params": {
    "methods": [
      { "id": "agent", "label": "Sign in with Agent" },
      { "id": "api-key", "label": "API Key", "env_var": { "name": "OPENAI_API_KEY" } }
    ]
  }
}
```

**ACP 规范中的认证方法**：
- `Agent` — 通过 `/auth` 页面或类似机制
- `EnvVar` — 通过环境变量注入凭据
- `Terminal` — 通过终端命令获取凭据

**当前状态**：未实现。`handle_initialize` 返回能力后不处理认证请求。

**影响**：依赖 API Key 的 IDE 客户端无法完成认证流程。

**实现建议**：
- EnvVar 方法：检查 `env_var.name` 是否已设置，返回成功
- Agent 方法：生成回调 URL 或通知用户
- 最低实现：返回所有方法均已通过（假设用户已通过 peri config 配置好）

### 🟡 providers/list、providers/set、providers/disable（unstable）

```
Client → Agent: providers/list
Agent → Client: {
  providers: [{ id: "anthropic", label: "Anthropic", models: [...], ... }]
}
```

**当前状态**：未实现。但我们有 `PeriConfig` 中的 providers 配置。

**实现难度**：低 — 从 `PeriConfig.config.providers` 读取并转换为 ACP `ProviderInfo`。

### 🟡 logout（unstable）

**当前状态**：未实现。无用户会话概念，登出仅需清理缓存。

### 🟢 NES + document/*（unstable，不需要）

Next Edit Suggestions — 完整的代码补全建议生命周期。需要编辑器文档同步 (`document/didOpen` 等 LSP 风格事件)。

**当前状态**：不属于 Agent 核心功能，**不需要实现**。

### 🟢 MCP-over-ACP（unstable，不需要）

通过 ACP 通道承载 MCP 协议（`mcp/connect`、`mcp/message`、`mcp/disconnect`）。

**当前状态**：perihelion 通过 `McpMiddleware` 直接在 Agent 内部发起 MCP 连接，不依赖 ACP 中继。**不需要此方法**。

### 🟡 client/available_commands（稳定，应发送）

```
Agent → Client (notification):
{
  "method": "session/update",
  "params": {
    "sessionUpdate": "available_commands_update",
    "commands": [
      { "name": "/help", "description": "Show help" },
      ...
    ]
  }
}
```

**当前状态**：从未发送。ACP 客户端不知道该 Agent 支持哪些斜杠命令。

**可用命令**：`/help`、`/clear`、`/compact`、`/cost`、`/doctor` 等（`peri-tui/src/command/` 中定义的命令）。

**影响**：低 — IDE 端的命令补全不可用。

### 🟡 $/cancel_request（unstable，双向）

协议级通知，用于取消正在进行的请求（不同于 `session/cancel`）。

**当前状态**：未实现。已有 `session/cancel`（取消整个会话），`$/cancel_request` 用于取消单个请求。

### 🟢 fs/* 和 terminal/*（稳定，架构性省略）

这些是 Agent 请求 Client 代表自己执行文件/终端操作。**perihelion 作为本地 Agent，直接操作文件系统和终端**，不需要通过 ACP 请求这些操作。

这是**正确的架构选择**，不是缺失。

---

## 四、SessionUpdate 语义对齐

已在上一个报告中详细分析，这里归纳：

| 变体 | 对接状态 | 详情 |
|------|---------|------|
| `AgentMessageChunk` | ✅ 完整 | |
| `AgentThoughtChunk` | ✅ 完整 | |
| `ToolCall` | ⚠️ 字段缺失 | 缺 `raw_input`、`locations` |
| `ToolCallUpdate` | ⚠️ 字段缺失 | 缺 `raw_output`、`locations` |
| `Plan` | ✅ 完整 | |
| `AvailableCommandsUpdate` | ❌ 从未发送 | |
| `CurrentModeUpdate` | ❌ 从未发送 | mode 变更后应通知 |
| `ConfigOptionUpdate` | ❌ 从未发送 | config 变更后应通知 |
| `SessionInfoUpdate` | ❌ 从未发送 | 标题/状态/时间戳 |
| `UsageUpdate` | ❌ 从未发送 | token 消耗 |
| `UserMessageChunk` | ❌ 合理不发送 | ACP 中这是上行消息 |

---

## 五、StopReason 映射正确性

`handle_prompt` 中 `PromptResponse` 的 stop_reason 映射：

| AgentError | StopReason | 正确性 |
|---|---|---|
| `Ok(())` | `EndTurn` | ✅ 默认完成 |
| `Err(Interrupted)` | `Cancelled` | ✅ 用户取消 |
| `Err(Refusal)` | → `EndTurn` | ❌ **应映射为 `Refusal`** |
| `Err(MaxIterations)` | → `EndTurn` | ⚠️ 应映射为 `MaxTurnRequests` |
| `Err(MaxTokens)` | → `EndTurn` | ⚠️ 应映射为 `MaxTokens` |

**ACP StopReason 定义**：
```rust
enum StopReason {
    EndTurn,        // 正常完成
    MaxTokens,      // 达到 token 限制
    MaxTurnRequests,// 达到最大循环次数
    Refusal,        // 模型拒绝
    Cancelled,      // 用户取消
}
```

当前所有错误都映射为 `EndTurn`，丢失了语义信息。

---

## 六、优先级排序

### 必须修（影响协议合规）

| # | 项目 | 工作量 |
|---|------|--------|
| 1 | **实现 `authenticate`** | 小（30 行） |
| 2 | **声明已实现的能力**（fork、models、config） | 极小（5 行） |
| 3 | **StopReason 精确映射** | 小（15 行） |

### 应该修（提升 IDE 体验）

| # | 项目 | 工作量 |
|---|------|--------|
| 4 | `LlmCallEnd` → `UsageUpdate` | 小（20 行） |
| 5 | mode/config 变更后发送通知 | 中（50 行） |
| 6 | `ToolCall.raw_input` / `ToolCallUpdate.raw_output` | 小（15 行） |
| 7 | `ToolCall.locations`（文件路径） | 中（需要解析工具参数） |
| 8 | `SessionInfoUpdate`（prompt 完成后更新标题/状态） | 小（15 行） |

### 可选

| # | 项目 | 工作量 |
|---|------|--------|
| 9 | `providers/list` 实现 | 中（需要映射 PeriConfig） |
| 10 | `client/available_commands` 通知 | 中（需要收集命令列表） |
| 11 | `ToolKind` 细化（WebFetch→Fetch） | 极小（5 行） |

---

## 七、总结

```
稳定方法实现:  ██████████░  10/11 (91%)  ← authenticate 是唯一缺口
SessionUpdate: █████████░░   8/11 (73%)  ← AvailableCommands/SessionInfo 缺失
字段完整性:    ████████░░   raw_input/raw_output 已补，locations 缺
能力声明:      ███░░░░░░░   漏声明 fork/models/config
StopReason:    ████████░░   已补 MaxIterationsExceeded→MaxTurnRequests
```

**2026-05-16 修复后更新**：
- SessionUpdate: 5/11 → 8/11（新增 UsageUpdate、CurrentModeUpdate、ConfigOptionUpdate、AgentThoughtChunk+AgentMessageChunk 已经正常）
- ToolCall 字段：`raw_input`/`raw_output` 已补
- StopReason：`MaxIterationsExceeded` → `MaxTurnRequests` 已修复
- `context_window` 从 model 获取（含 `context_1m` 覆盖），不再硬编码
- Agent 构建：`build_bare_agent()` 统一 TUI/ACP 入口
- 仍缺失：`authenticate`、`AvailableCommandsUpdate`、`locations`、能力声明

**最大缺口不是方法数量，而是运行时的通知丰富度**——IDE 客户端看不到 token 消耗、模式变更、配置更新、命令列表。这些直接决定 Cursor 等 IDE 中的用户交互体验。
