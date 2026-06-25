> 归档于 2026-05-20，原路径 spec/issues/2026-05-20-compact-command-not-triggering.md

# /compact 命令作为普通文本发给 LLM，未触发压缩

**状态**：Fixed
**优先级**：中
**创建日期**：2026-05-20
**修复日期**：2026-05-20（初步修复），2026-05-20（二次修复）

## 问题描述

在 TUI 中输入 `/compact` 命令后，消息被当作普通用户文本发送给 LLM。LLM 收到 `/compact` 字符串并尝试像普通对话一样回复，但没有触发任何上下文压缩操作。

## 根因

旧代码将 `/compact` 通过 `app.submit_message("/compact")` 发送为普通用户消息。

## 修复方案

`/compact` 命令处理器改为通过 ACP compact 通道（`client.compact()`）触发手动压缩，而非将文本作为普通消息发送。

### 修改文件

- `peri-tui/src/command/session/compact.rs`：`execute()` 调用 `acp_client.compact().await` 而非 `app.submit_message("/compact")`

## 二次修复（2026-05-20）

初步修复引入三个副作用，一并修正：

### 副作用 1：compact 期间无 loading 状态 / spinner 不转

- **根因**：`handle_compact_started` 未设 `loading=true`；compact 期间无流式事件导致 `agent_updated` 持续为 false，跳过渲染
- **修复**：
  - `agent_compact.rs`：`handle_compact_started` 调 `set_loading(true)`，`handle_compact_completed`/`handle_compact_error` 调 `set_loading(false)`
  - `polling.rs`：`poll_agent()` 在 `loading=true` 时返回 `true` 强制每帧渲染

### 副作用 2：历史恢复后的消息 compact 不生效

- **根因**：`open_thread()` 只同步了 TUI pipeline，未调用 `acp_client.load_session()` 同步 ACP 服务器端 `state.history`
- **修复**：`thread_ops.rs`：`open_thread()` 末尾调用 `client.load_session()` 同步服务器端 session 状态

### 副作用 3：compact 执行失败无用户可见反馈

- **根因**：`tokio::spawn` 中的 `.compact().await` 错误只打 tracing 日志
- **修复**：
  - `command/session/compact.rs`：`acp_client=None` 时显示系统消息
  - `agent_comm.rs`：新增 `pending_compact_error: Option<Arc<Mutex<Option<String>>>>` 字段
  - `polling.rs`：`poll_agent()` 检查 `pending_compact_error`，显示错误消息

### 修改文件

- `peri-tui/src/app/agent_compact.rs`
- `peri-tui/src/app/agent_ops/polling.rs`
- `peri-tui/src/app/agent_comm.rs`
- `peri-tui/src/app/thread_ops.rs`
- `peri-tui/src/command/session/compact.rs`
- `peri-tui/locales/en/main.ftl`
- `peri-tui/locales/zh-CN/main.ftl`

## 症状详情

| 操作 | 预期 | 实际（修复前） | 实际（修复后） |
|------|------|------------|------------|
| 输入 `/compact` 回车 | 触发 full compact | LLM 收到 `/compact` 文本 | 通过 ACP compact 通道触发 full compact |
| auto-compact（token 超过阈值） | 正常触发 | 正常（`CompactMiddleware`） | 正常 |
| compact 进行中 | loading spinner 持续动画 | spinner 冻结，无 loading 状态 | loading=true，spinner 持续渲染 |
| 历史会话恢复后 /compact | 正常 compact | 不生效（ACP state.history 未同步） | open_thread 后同步 ACP session |
| compact 失败（无 API key 等） | 用户可见错误提示 | 无反馈（仅 tracing 日志） | 系统消息显示错误 |
