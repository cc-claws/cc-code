# ACP 领域

## 领域综述

ACP（Agent Client Protocol）领域负责通过 stdio 传输为 IDE（如 Cursor）提供 Agent 服务端能力。

核心职责：

- Session 生命周期：initialize、new、load、resume、cancel、logout、close
- 请求处理：RequestPermission（HITL 桥接）、$/cancel_request（单请求取消）
- 更新推送：AvailableCommandsUpdate、SessionNotification 事件流
- Agent 构建复用：与 TUI 共享 build_bare_agent() 入口

## 核心流程

```
ACP Client (IDE) → stdio → handle_initialize/session/new/load...
  → assemble_agent() → executor.execute() → SessionNotification 流
  → RequestPermission RPC → AcpInteractionBroker → HITL 审批桥接
  → $/cancel_request → oneshot cancel → 中断 pending request
```

## 技术方案总结

| 维度 | 选型 |
|------|------|
| 传输层 | stdio（stdin/stdout），JSON-RPC 2.0 |
| Session 管理 | AcpSession，DashMap<SessionId, ...>，支持多 session |
| Agent 构建 | build_bare_agent() 共享入口，中间件链一致 |
| 权限桥接 | AcpInteractionBroker 实现 UserInteractionBroker trait |
| Pending Request | DashMap<RequestId, PendingRequestEntry> + oneshot::Sender |
| 命令推送 | AvailableCommandsUpdate，在 session/new/load/resume 三个入口统一发送 |

## Issue 经验附录

### issue_2026-05-16-acp-cancel-request-unimplemented

**摘要:** ACP 未实现 $/cancel_request 与 AvailableCommandsUpdate
**状态:** Closed
**归档日期:** 2026-05-17
**关键词:** ACP, cancel_request, oneshot, pending requests, AvailableCommandsUpdate
**问题本质:** ACP 协议实现中的两个缺口：(1) $/cancel_request 通知未处理导致无法取消单个请求；(2) AvailableCommandsUpdate 从未发送导致 IDE 端命令补全不可用
**通用模式:** 协议级通知（notification）与请求（request）是独立通道，废弃的 notification handler 会静默丢弃客户端通知；pending request 追踪需要支持一对一取消（oneshot channel）而非仅全局取消
**技术决策:** DashMap<RequestId, PendingRequestEntry> 追踪 pending requests，oneshot::Sender<()> 实现单请求取消；build_available_commands() 在 session/new、load、resume 三个入口统一发送
**涉及文件:** peri-tui/src/acp/dispatch.rs
**CLAUDE.md 链接:** false

### issue_2026-05-17-acp-dispatch-heavy-file

**摘要:** ACP dispatch.rs 请求分发逻辑过度集中（1044 行），14 个 pub 函数和所有 handler 实现集中在一个文件
**状态:** Fixed
**归档日期:** 2026-05-18
**涉及文件:** peri-tui/src/acp/dispatch.rs
**说明:** 纯代码组织优化——按 handler 分组拆分为子目录（initialize/session/prompt/permission/helpers），无领域认知提炼。

### issue_2026-05-19-acp-missing-capabilities

**摘要:** ACP 协议能力缺失：Session 生命周期路由、能力声明、SessionUpdate 通知变体
**状态:** Fixed
**归档日期:** 2026-05-20
**关键词:** Session 生命周期, ACP 路由, 能力声明, SessionUpdate
**问题本质:** TUI ACP Server 的路由层未接通——SessionManager 已有完整基础设施，但 acp_server.rs 缺少 session/load/resume/close/list/fork 路由处理，且 InitializeResponse 的能力声明为空。这是典型的"底层能力已存在但协议层未暴露"的分层架构问题。
**通用模式:** 协议层路由是基础设施能力的"出口"。当 SessionManager 支持了某操作但 ACP handler 无路由时，外部客户端完全无法使用。新增基础设施能力时，需同步检查所有协议/传输层的路由表是否齐备。
**架构影响:** ACP 协议中 fs/terminal 代理是面向远程 Agent 场景的——本地 Agent（perihelion）通过 FilesystemMiddleware 直接操作 std::fs 是正确的架构选择，仅在 stdio 连接外部 IDE 时才有实现意义。
**涉及文件:** peri-tui/src/acp_server.rs, peri-acp/src/event/mapper.rs, peri-acp/src/session/state_builders.rs
**CLAUDE.md 链接:** false

### issue_2026-05-19-acp-stopreason-hardcoded-endturn

**摘要:** ACP PromptResponse 的 StopReason 全部硬编码为 EndTurn，无法区分取消/超限/正常完成
**状态:** Fixed
**归档日期:** 2026-05-20
**关键词:** StopReason, PromptResult, 枚举映射, 终止原因
**问题本质:** executor 返回的 PromptResult 只携带 ok: bool，不包含终止原因的语义信息。协议边界处的语义压缩（正常完成/取消/超限 → 统一 EndTurn）导致 IDE 客户端无法给出差异化 UI 反馈。
**通用模式:** 内部错误类型的语义信息应在跨越协议/API 边界时保有足够粒度。使用 bool 做结果标识会导致语义坍缩——应使用枚举携带区分性的终止原因。新增 PromptStopReason 中间层枚举是解耦内部 AgentError 和外部 ACP StopReason 的正确模式。
**涉及文件:** peri-acp/src/session/executor.rs, peri-tui/src/acp_server.rs, peri-tui/src/acp_stdio.rs
**CLAUDE.md 链接:** false

### issue_2026-05-19-acp-missing-state-notifications

**摘要:** ACP 状态变更后不发通知：ConfigOptionUpdate / AvailableCommandsUpdate / SessionInfoUpdate
**状态:** Resolved
**归档日期:** 2026-05-20
**关键词:** SessionUpdate, ConfigOptionUpdate, 状态同步, 多客户端
**问题本质:** set_mode/set_config_option/set_model 等请求处理器变更状态后只返回 response，不推送 session/update 通知。多客户端场景下其他客户端无法感知变更，UI 控件不同步。
**通用模式:** 任何改变共享状态的请求处理器，在返回 response 后必须主动推送状态变更通知。这是"写操作 + 广播通知"模式——状态变更方可能不是 UI 更新方。遵循 CQRS 思想：命令处理完成后推送事件通知所有订阅者。
**涉及文件:** peri-tui/src/acp_server.rs, peri-tui/src/acp_stdio.rs, peri-acp/src/event/mapper.rs, peri-acp/src/session/event_sink.rs
**CLAUDE.md 链接:** false

### issue_2026-05-23-tui-image-sent-as-text

**摘要:** TUI 粘贴图片后 LLM 仅收到文本而非图片内容
**状态:** Fixed
**归档日期:** 2026-05-24
**关键词:** 图片粘贴, MessageContent, 多模态数据流, Base64 Image
**问题本质:** TUI → ACP → Executor 整条链路的 content 参数类型是 String，图片在 submit_message 阶段被丢弃
**通用模式:** 新增多模态支持时，数据链路必须从输入端到消费端全链路升级（String → MessageContent），任何一环用 String 都会丢失非文本数据
**架构影响:** ACP 协议层的 content 类型从 String 升级为 MessageContent（支持 blocks 数组）
**涉及文件:** peri-acp/src/session/executor.rs, peri-tui/src/acp_server/prompt.rs, peri-tui/src/acp_client/client.rs, peri-tui/src/app/agent_submit.rs, peri-tui/src/acp_stdio.rs, peri-tui/src/cli_print.rs
**CLAUDE.md 链接:** false

### issue_2026-05-21-acp-stdio-missing-session-capabilities

**摘要:** ACP Stdio InitializeResponse 缺少 session 能力声明，Zed 客户端报错
**状态:** Fixed
**归档日期:** 2026-05-24
**关键词:** ACP InitializeResponse, session_capabilities, load_session, session/list
**问题本质:** ACP stdio 路径的 initialize 响应只声明了 promptCapabilities，遗漏了 load_session 和 session_capabilities
**通用模式:** 两条 ACP 路径（stdio/TUI）的能力声明必须统一；提取到 dispatch 模块共享，避免重复维护
**涉及文件:** peri-acp/src/dispatch/init.rs, peri-acp/src/dispatch/list_sessions.rs, peri-acp/src/dispatch/mod.rs, peri-tui/src/acp_stdio.rs, peri-tui/src/acp_server/requests.rs
**CLAUDE.md 链接:** false

### issue_2026-05-21-acp-stdio-default-permission-should-be-bypass

**摘要:** ACP stdio 默认权限模式为 AutoMode，与 TUI/-p 模式不一致
**状态:** Fixed
**归档日期:** 2026-05-24
**关键词:** ACP stdio权限, PermissionMode, 默认Bypass, 行为一致性
**问题本质:** acp_stdio.rs 硬编码 AutoMode，未考虑与其他模式（TUI Bypass、-p Bypass）的一致性
**通用模式:** 多入口系统的默认行为必须统一；硬编码默认值应提取为共享常量
**涉及文件:** peri-tui/src/acp_stdio.rs:184-185, peri-tui/src/main.rs:441-467, peri-tui/src/cli_print.rs:96-109
**CLAUDE.md 链接:** false

### issue_2026-05-21-skills-not-passed-as-acp-commands

**摘要:** Skills 未作为 ACP AvailableCommands 传递给 IDE 客户端
**状态:** Fixed
**归档日期:** 2026-05-24
**关键词:** AvailableCommands, Skills命令, FrozenSessionData, skill_summary
**问题本质:** ACP 协议的 AvailableCommands 通知只返回硬编码静态命令，未包含动态发现的 Skills
**通用模式:** 动态内容（Skills、插件命令）必须在 session/new 时冻结并注入协议通知，与系统提示词的 frozen 模式对齐
**涉及文件:** peri-tui/src/acp_stdio.rs:79-108, peri-tui/src/acp_server/notify.rs:118-146, peri-tui/src/acp_stdio.rs:362-363, peri-middlewares/src/skills/mod.rs:115-122
**CLAUDE.md 链接:** false

### issue_2026-05-19-acp-missing-prompt-capabilities

**摘要:** ACP InitializeResponse 缺少 prompt_capabilities 声明
**状态:** Fixed
**归档日期:** 2026-05-31
**关键词:** ACP, prompt_capabilities, 协议声明, 能力声明
**问题本质:** InitializeResponse 未按 ACP 规范声明 prompt_capabilities（图片/音频/嵌入上下文），客户端行为不确定
**通用模式:** 协议实现必须完整声明所有能力字段（即使不支持也要显式声明为空或 false），不能省略
**涉及文件:** peri-tui/src/acp_server.rs
**CLAUDE.md 链接:** false

---

## 相关 Feature

- → [tui.md](./tui.md) — ACP 与 TUI 共享 build_bare_agent() Agent 构建入口
