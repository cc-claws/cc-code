# 实现 ACP session_info_update 协议迁移——动态 Session 标题与元数据

**状态**：Open
**优先级**：中
**创建日期**：2026-05-29

## 问题描述

ACP 协议 RFD [session-info-update](https://agentclientprotocol.com/rfds/session-info-update) 定义了 `SessionUpdate::SessionInfoUpdate` 变体，允许 agent 实时推送 session 元数据（title、updatedAt、_meta）。当前 Perihelion 已有基础发送路径，但仅使用 `updatedAt` 字段，未利用 `title` 和 `_meta`。TUI 侧则完全忽略该事件。需完成完整协议迁移，实现动态 session 标题自动生成和展示。

## 现状

### SDK 侧（agent-client-protocol crate）

`SessionInfoUpdate` struct 已存在于 ACP SDK，当前支持：
- `title: Option<String>` — 可设置但未使用
- `updated_at: Option<String>` — 已使用（仅 timestamp）
- `_meta: Option<Value>` — 未使用

### Server 发送侧

| 位置 | 触发时机 | 当前发送内容 |
|------|----------|-------------|
| `peri-tui/src/acp_server/notify.rs:104-123` | prompt/compact 完成后 | 仅 `updatedAt` = RFC3339 timestamp |
| `peri-acp/src/event/mapper.rs:203-208` | LLM retry 时 | `title` = "Retrying LLM call (attempt N/M, Dms delay)" |

两处均未发送有意义的会话标题（如从用户首条消息自动生成）。

### TUI 接收侧

`peri-tui/src/app/agent_ops/acp_bridge.rs:204` 明确忽略：

```rust
"usage_update" | "session_info_update" => {
    // Peri 模式忽略 — 完整数据通过 peri/agent_event（类别②）获取
    (false, false, false)
}
```

### stdio 客户端

stdio 模式下 `session_info_update` 通过 `session/update` 推送，客户端可收到但 Perihelion 未提供有价值的 title 数据。

## RFD 要求摘要

来源：[RFD: session-info-update](https://agentclientprotocol.com/rfds/session-info-update)

- `SessionInfoUpdate` 作为 `SessionUpdate` 的新变体，通过 `session/update` 通知发送
- 字段全部可选，支持增量更新：`title?`、`updatedAt?`、`_meta?`
- 不含 `sessionId`（已在 params 中）和 `cwd`（创建时设定不可变）
- `_meta` 使用合并语义：新字段与已有字段递归合并
- 典型用例：首条消息后自动生成标题、对话主题变化时更新标题

## 期望改进方向

### 1. 自动标题生成（核心功能）

- prompt 完成后（首条消息时），根据用户输入和 agent 首轮回复自动生成简短标题
- 后续对话主题明显变化时更新标题
- 替代当前仅发送 `updatedAt` timestamp 的行为

### 2. TUI 展示（接收侧）

- `acp_bridge.rs` 中处理 `session_info_update`，更新本地 session 标题缓存
- 在 session 列表/侧边栏中展示动态标题（而非 "Session abc123..."）

### 3. _meta 扩展点

- 预留 `_meta` 字段用于未来扩展（标签、状态、优先级等）
- 当前阶段可暂不填充，但发送时需保留字段

## 涉及文件

- `peri-tui/src/acp_server/notify.rs:104-123` — `send_session_info_update` 当前仅发 updatedAt，需增加 title 生成逻辑
- `peri-acp/src/event/mapper.rs:203-208` — LLM retry 场景的 SessionInfoUpdate 发送
- `peri-tui/src/app/agent_ops/acp_bridge.rs:204` — TUI 侧需处理 session_info_update 而非忽略
- `agent-client-protocol` crate — SessionInfoUpdate struct 定义（确认与 RFD 对齐）

## 参考

- [ACP RFD: session-info-update](https://agentclientprotocol.com/rfds/session-info-update)
- `spec/issues/2026-05-29-available-commands-update-format-mismatch.md` — 同属 SessionUpdate 通知体系的 format 修复
