# 消息双重累积存储导致 118MB RSS 中 40-80MB 为冗余数据

**状态**：Open
**优先级**：高
**创建日期**：2026-06-13

## 问题描述

Peri TUI 运行时 RSS 约 118MB。经代码级审查发现，消息历史在两个独立数据结构中**完整存储两份**，且随会话长度线性增长。一个包含 50 条消息（含工具输出）的会话可产生 40-80MB 冗余内存。

## 症状详情

### 数据流追踪

每轮 ReAct 循环结束后，Agent 发出**增量** `StateSnapshot`（`final_answer.rs:46-58`，通过 `snapshot_anchor` 截取新增消息）。

TUI 收到 `StateSnapshot` 后，在 `agent_ops/mod.rs:277-298` 中执行**两处 extend**：

```rust
// agent_ops/mod.rs:283-287 — 存储点 #1
self.session_mgr.current_mut().agent.origin_messages.extend(msgs.clone());

// agent_ops/mod.rs:288-293 → pipeline.set_completed → 存储点 #2
let actions = self.session_mgr.current_mut().messages.pipeline
    .handle_event(AgentEvent::StateSnapshot(msgs));
```

`set_completed()` 内部也是 `extend`（`message_pipeline/mod.rs:1039`）：

```rust
pub fn set_completed(&mut self, msgs: Vec<BaseMessage>) {
    self.completed.extend(msgs);  // 追加到 pipeline.completed
    // ...
}
```

### 重复存储证据

| 存储位置 | 类型 | 更新方式 | 证据 |
|----------|------|----------|------|
| `SessionState.origin_messages` | `Vec<BaseMessage>` | 每次 StateSnapshot `.extend()` | `agent_ops/mod.rs:287` |
| `MessagePipeline.completed` | `Vec<BaseMessage>` | 每次 StateSnapshot `.extend()` | `message_pipeline/mod.rs:1039` |

两者都接收**相同的增量消息**，经过 N 轮 ReAct 后，持有**完全相同的全量消息历史**。

### 内存分布

| 类别 | 估算 MB | 说明 |
|------|---------|------|
| **origin_messages + pipeline.completed 双重存储** | **40-80** | 同一数据存两份，随会话线性增长 |
| Rust 二进制 + Tokio + jemalloc 基线 | 15-20 | 固定开销 |
| AgentPool LLM 客户端（reqwest TLS 缓存） | 5-10 | 3-5 个 reqwest::Client 各 ~1-2MB |
| MCP pool + LSP pool | 3-13 | 取决于配置的 server 数量 |
| view_messages + RenderCache 渲染管线 | 2-5 | 预渲染的 Text<'static> + wrap_map |
| ToolSearchIndex + shared_tools | 1-2 | HashMap 元数据重复 |
| sysinfo::System | 1-2 | 进程快照 |

## 复现条件

- **复现频率**：必现
- **触发步骤**：
  1. 启动 Peri TUI
  2. 进行多轮对话（尤其是包含工具调用的对话）
  3. 执行 `/gc` 查看 origin_messages 和 pipeline.completed 的条数——两者相同
- **环境**：所有平台

## 涉及文件

- `peri-tui/src/app/agent_ops/mod.rs:283-293` — StateSnapshot 处理，两处 extend
- `peri-tui/src/app/message_pipeline/mod.rs:1038-1047` — `set_completed()` 内部 extend
- `peri-agent/src/agent/executor/final_answer.rs:38-58` — `emit_snapshot_and_drain_notifications()` 增量快照发射

## 建议修复

让 `pipeline.completed` 作为唯一消息存储，`origin_messages` 改为按需从 `completed` 重建（或使用 `Arc<Vec<BaseMessage>>` 共享同一份数据）。预期节省 20-40MB。

## 状态变更记录

| 日期 | 从 | 到 | 操作人 | 说明 |
|------|-----|-----|--------|------|
| 2026-06-13 | — | Open | agent | 创建，基于代码审查和 /gc 诊断数据 |

## 修复记录

（由 fix-issue 或 issue-verify skill 追加，创建时留空）
