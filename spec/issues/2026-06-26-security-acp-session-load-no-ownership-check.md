# session/load 和 session/resume 接受任意 sessionId 无所有权校验

**状态**：Fixed（部分） — UUID 格式校验 + 存在性校验已实现（M5 最小修复），所有权校验（owner 字段）未实现
**优先级**：中
**创建日期**：2026-06-26
**来源**：cc-code 全项目安全审计 2026-06-26（Finding M5，置信度 8/10）
**修复日期**：2026-06-27（PR #74，commit 07802254 / cbafed6d）

## 问题描述

`session/load` 和 `session/resume` 处理器把客户端提供的 `sessionId` 直接当 SQLite 主键使用（已用 `bind()` 参数化，**非** SQL 注入）。但 `ThreadId` 是 `String` 类型，从未做 UUID 格式校验，也无所有权校验。任意 ACP 客户端给定任意 ID 即可读取对应线程全部消息，或静默插入一条空的 SessionState。

## 当前行为

```rust
// peri-tui/src/acp_server/requests.rs:224-294 (session/load)
// peri-tui/src/acp_server/requests.rs:346-389 (session/resume)
// 客户端提供的 sessionId 直接当作 thread_id 用于 SQLite 查询，
// 参数化绑定 → 不是 SQL 注入
// 但缺少：
//   - UUID 格式校验（ThreadId 是 String，可传任意字符串）
//   - 线程存在性校验（缺失时静默插入新 SessionState）
//   - 所有权校验（任何 ACP client 都可读任意 thread）
```

```rust
// peri-acp/src/session/mod.rs:91-101
// new_session_with_id 接受任意 String 作为 ID，无格式校验
```

## 预期行为

| 操作 | 当前 | 预期 |
|------|------|------|
| `session/load` 传入不存在的 sessionId | 静默插入新 SessionState | 返回错误 `session_not_found` |
| `session/load` 传入他人 sessionId | 直接读取消息 | 返回 `forbidden` 或要求所有者 token |
| `session/load` 传入 `../../etc/passwd` 等非法 ID | 尝试匹配（SQLite TEXT 主键，无文件系统影响） | UUID 格式校验拒绝 |
| 多客户端访问同一 ACP server | 任意 client 可访问任意 thread | 至少 client 隔离 |

## 利用场景

威胁模型基本是本地单用户（用户运行自己的 agent），所以跨线程读取≈直接读 SQLite 文件（见 H3）。但以下场景风险升级：

1. **多客户端场景**：用户启动一个 ACP server，同时连接多个 IDE 插件 / SDK 客户端。某个不可信的 SDK 客户端（例如自动测试工具）可以通过枚举 sessionId 读其他线程的消息。
2. **SubAgent 隔离失败**：后台 agent 或 fork 的 sub-agent 拿到 ACP 句柄时可越权读主 agent 的其他会话。
3. **远程 IDE 场景**：未来如果 ACP server 暴露到网络（vscode remote、SSH），跨用户读写。

## 修复方案

任选其一，按优先级：

1. **UUID 格式校验**（最小修复）：
   ```rust
   fn validate_session_id(id: &str) -> Result<(), AcpError> {
       Uuid::parse_str(id).map_err(|_| AcpError::invalid_params("sessionId must be UUID"))?;
       Ok(())
   }
   ```
   在 `session/load`、`session/resume`、`new_session_with_id` 入口校验。

2. **存在性校验**：`session/load` 在 SQLite 中查询 thread 是否存在，不存在则返回 `session_not_found`，不静默插入。

3. **所有权校验**（长期）：thread 表加 `owner` 字段，session 启动时绑定 owner，load/resume 时验证 caller 与 owner 匹配。

4. **文档化威胁模型**：若 ACP server 始终单用户本地，在 ACP spec 中明确"sessionId 是可信客户端输入"。

## 涉及文件

- `peri-tui/src/acp_server/requests.rs:224-294` — `session/load` 处理器
- `peri-tui/src/acp_server/requests.rs:346-389` — `session/resume` 处理器
- `peri-acp/src/session/mod.rs:91-101` — `new_session_with_id` 入口

## 状态变更记录

| 日期 | 从 | 到 | 操作人 | 说明 |
|------|-----|-----|--------|------|
| 2026-06-26 | — | Open | agent | 创建（安全审计 M5） |
| 2026-06-27 | Open | Fixed（部分） | agent | PR #74 合入：`session/load` + `session/resume` 增加 UUID 格式校验 + 线程存在性校验（不存在的 sessionId 返回 `session_not_found`）。所有权校验（owner 字段）未实现，威胁模型仍为单用户本地 |
