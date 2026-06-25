> 归档于 2026-05-24，原路径 spec/issues/2026-05-24-cancel-ineffective-during-streaming-and-tool-execution.md

# Cancel（Ctrl+C）在流式输出和工具执行中失效——UI 中断但底层请求未停止

**状态**：Fixed
**优先级**：高
**创建日期**：2026-05-24
**修复日期**：2026-05-24

## 问题描述

用户在 Agent 流式输出文本或工具执行过程中按 Ctrl+C 取消时，UI 层面显示已中断（spinner 停止、出现中断提示），但底层的 LLM 请求或工具执行实际上仍在继续运行。Cancel 信号未真正传递到执行层，导致资源持续消耗且用户以为已停止。

等待 LLM 响应阶段（尚未开始输出）的 Ctrl+C 可以正常取消。

## 症状详情

| 表现 | 说明 |
|------|------|
| 触发阶段 | LLM 流式输出中 或 工具执行中 |
| Ctrl+C 后 UI | Spinner 停止，显示中断提示 |
| Ctrl+C 后实际行为 | LLM 请求/工具执行继续运行，未真正停止 |
| 等待响应时 Ctrl+C | 正常工作，可成功取消 |
| 期望行为 | Ctrl+C 应真正取消底层 LLM 请求和工具执行 |

**具体表现**：
- 场景 1：Agent 正在流式输出文本（打字效果），按 Ctrl+C → UI 显示已停止，但 API 请求实际仍在消费 token
- 场景 2：Agent 正在执行工具（如 Bash 命令），按 Ctrl+C → UI 显示已停止，但工具进程实际仍在运行

## 复现条件

- **复现频率**：经常
- **触发步骤**：
  1. 向 Agent 发送一个需要较长时间响应的 prompt（或触发工具执行）
  2. 等待 LLM 开始流式输出，或工具开始执行
  3. 按 Ctrl+C
  4. 观察：UI 显示已中断，但实际请求未停止
- **环境**：macOS，所有模型

## 涉及文件

- `peri-tui/src/app/agent_ops/polling.rs` — TUI 事件轮询与 cancel 处理
- `peri-tui/src/app/agent_ops/lifecycle.rs` — Interrupted/Error 处理器
- `peri-tui/src/acp_server/requests.rs` — `$/cancel_request` 处理
- `peri-acp/src/session/executor.rs` — 共享 agent 执行管线，cancel token 传递
- `peri-agent/src/agent/executor/llm_step.rs` — LLM 调用与 cancel token 竞争
- `peri-agent/src/agent/executor/mod.rs` — ReAct 循环中的 cancel 检查点
- `peri-agent/src/agent/executor/tool_dispatch.rs` — 工具执行中的 cancel 处理

## 根因分析

`App::interrupt()` 在 ACP 路径下同时执行两条路径：
1. **异步 cancel**：`tokio::spawn(client.cancel())` — fire-and-forget 发送 `$/cancel_request` 通知
2. **同步强制清理**：因 `AgentComm.cancel_token` 始终为 None（TUI+ACP 路径下不设置），进入 `else if loading` 分支立即清理 UI

问题：路径 2 的强制清理与后续 ACP server 端 `Interrupted`/`Done` 事件竞态。UI 立即显示"已中断"，但 agent 在 cancel 生效前继续运行。ACP 事件到达后触发 `handle_interrupted()`/`handle_done()` 二次清理，导致 UI 不一致。

Cancel 信号传播链本身完整（LLM 流式循环和工具执行均有 `tokio::select! { cancel.cancelled() }`），问题仅在于 `interrupt()` 的 UI 层面处理。

## 修复方案（已实施，commit 0211c41）

1. **核心修复**：`interrupt()` 在 ACP 路径下只发送 cancel 通知后 `return`，不执行强制 UI 清理。UI 清理延迟到 ACP server 发回的 `Interrupted`/`Done` 事件通过正常事件流处理。

2. **安全网**：记录 `cancel_sent_at` 时间戳，5 秒后如果仍在 loading 且未收到任何事件，执行 fallback 强制清理（防止 cancel 通知丢失导致的永久 loading）。

**修改文件**：
- `peri-tui/src/app/mod.rs` — `interrupt()` ACP 路径添加 `cancel_sent_at` 记录 + `return;`
- `peri-tui/src/app/agent_comm.rs` — 添加 `cancel_sent_at: Option<Instant>` 字段
- `peri-tui/src/app/agent_ops/polling.rs` — `poll_agent()` 中 5 秒超时检查
- `peri-tui/src/app/agent_ops/lifecycle.rs` — `handle_done/handle_interrupted/handle_error` 中清除 `cancel_sent_at`
