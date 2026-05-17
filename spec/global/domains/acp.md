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

---

## 相关 Feature

- → [tui.md](./tui.md) — ACP 与 TUI 共享 build_bare_agent() Agent 构建入口
