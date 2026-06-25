> 归档于 2026-05-17，原路径 spec/issues/2026-05-16-acp-cancel-request-unimplemented.md
# ACP 未实现 `$/cancel_request` 与 `AvailableCommandsUpdate`

**状态**：Closed
**优先级**：中
**创建日期**：2026-05-16
**关闭日期**：2026-05-17

## 问题描述

ACP 协议中有两项能力当前未实现：

1. **`$/cancel_request`** 协议级取消通知——取消单个正在进行的请求（而非整个 session）
2. **`AvailableCommandsUpdate`** SessionUpdate 变体——Agent 未向 Client 推送可用命令列表

## 症状详情

### 缺口 1：`$/cancel_request` 未处理

- ACP Client 发送 `$/cancel_request` 时，`handle_dispatch` 返回 `Handled::Yes` 静默丢弃
- 无法取消单个 `RequestPermission` 等请求

**ACP 规范行为**（接收方）：
1. MUST 取消对应请求活动
2. MUST 返回 error code `-32800` (Cancelled)

**数据格式**：`{ "method": "$/cancel_request", "params": { "requestId": 1 } }`

### 缺口 2：`AvailableCommandsUpdate` 从未发送

- ACP Client 不知道该 Agent 支持哪些命令
- IDE 端命令补全不可用

**可用命令示例**：`/help`、`/clear`、`/compact`、`/cost`、`/doctor` 等（`peri-tui/src/command/` 中定义）

## 涉及文件

- `peri-tui/src/acp/dispatch.rs` —— `handle_dispatch`（缺口 1）、session/new handler（缺口 2）
- `agent-client-protocol-schema` —— 已有 `CancelRequestNotification`、`AvailableCommandsUpdate` 类型

## 修复记录

### 缺口 1：`$/cancel_request` 已实现

- `684ca36` feat(acp): add pending request tracking to AcpSession for $/cancel_request support
- `469c48c` feat(acp): make permission forwarding loop cancellable via $/cancel_request
- `6a96048` feat(acp): handle $/cancel_request notification in dispatch handler
- `c063ac6` fix(acp): cancel all pending requests when session/cancel received
- `a96e398` fix(acp): check session cancel_token before each permission request

实现要点：
- `AcpSession` 新增 `pending_requests: DashMap<RequestId, PendingRequestEntry>` 和 `pending_gen: AtomicU64` 追踪每个待处理请求
- 每个 `RequestPermission` 注册到 `pending_requests`，附带 `cancel_tx: oneshot::Sender<()>`
- `handle_dispatch` 中 `$/cancel_request` 解析 `requestId`，调用 `mgr().cancel_pending_request()` 触发 oneshot
- permission forwarding loop 的 `tokio::select!` 竞争 `cancel_rx` 和 client 响应
- `cancel_session` 同步取消所有 pending requests，防止取消后权限请求卡住

### 缺口 2：`AvailableCommandsUpdate` 已实现

- `519372c` feat(acp): send AvailableCommandsUpdate on session/new, load, resume

实现要点：
- `build_available_commands()` 构建命令列表（24 个命令）
- `send_available_commands()` 封装 `SessionNotification` 发送
- 在 `handle_new_session`、`handle_load_session`、`handle_resume_session` 三个入口统一调用
