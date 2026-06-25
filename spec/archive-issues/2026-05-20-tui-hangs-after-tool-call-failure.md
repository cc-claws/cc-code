> 归档于 2026-05-27，原路径 spec/issues/2026-05-20-tui-hangs-after-tool-call-failure.md

# 工具调用失败后 TUI 卡住（spinner 持续旋转）

**状态**：Verify
**优先级**：高
**创建日期**：2026-05-20

## 问题描述

工具调用失败后，TUI 进入"卡住"状态：agent spinner 一直旋转，看起来像 agent 还在运行中，但实际上 agent 已经没有新输出了。用户无法继续操作，只能取消或强制退出。

典型场景：agent 调用了不存在的工具（如 `Task`），工具调用超时或返回错误后，TUI 不再有任何进展，spinner 持续转动。

## 症状详情

| 表现 | 说明 |
|------|------|
| Spinner 状态 | 持续旋转（`loading = true`），agent 显示为"运行中" |
| 事件流 | 无新的 `AgentEvent` 到达 |
| Agent 循环 | spinner 一直在转，agent 不再产生输出 |
| 用户操作 | TUI 本身可以滚动，但 agent 不会自动恢复 |
| 复现方式 | 触发一个会失败/超时的工具调用即可复现 |

**具体复现场景**：agent 尝试调用 `Task` 工具（不存在），调用超时后 TUI 显示 `✗ 工具 'Task' 不存在`，但之后 spinner 持续旋转，agent 不再继续执行。

## 复现条件

- **复现频率**：必现（工具调用超时/失败时）
- **触发步骤**：
  1. 让 agent 执行一个会调用不存在工具的任务（或触发工具调用超时）
  2. 观察工具调用失败后的 TUI 状态
  3. Spinner 持续旋转，agent 不会继续
- **环境**：所有模型，macOS

## 调查发现

对完整数据流链路进行了代码审查：

**agent 层（`peri-agent`）**：

- `tool_dispatch.rs`：`ToolNotFound` 被正确处理为非致命 `ToolResult::error`，ReAct 循环应继续
- `llm_step.rs`：`call_llm` 使用 `tokio::select!` 与 cancel token 竞争，LLM 调用本身不应无限阻塞
- `mod.rs`（ReAct 循环）：工具错误后循环继续，下一次 `call_llm` 应被调用

**executor 层（`peri-acp`）**：

- `executor.rs`：agent 执行完成后（无论成功/失败），会发送 `AgentExecutionFailed` 事件，然后关闭 channel，pump drain 后调用 `push_done`
- 事件泵使用 unbounded channel，`send_notification` 非阻塞

**TUI 层（`peri-tui`）**：

- `acp_bridge.rs`：`AgentDone` → `AgentEvent::Done`，`AgentExecutionFailed` → `AgentEvent::Error`
- `lifecycle.rs`：`handle_done` 和 `handle_error` 都调用 `cleanup_agent_state` → `set_loading(false)`
- `polling.rs`：`try_recv` 非阻塞，`Disconnected` 路径也会清理状态

**代码路径结论**：错误路径的完成信号传递在代码层面看起来正确。问题可能在于：

1. **agent 执行本身未返回**——工具错误后的下一轮 LLM 调用可能因特定原因挂起
2. **超时场景特殊**——工具调用超时（而非立即返回错误）可能触发了不同的代码路径
3. **`run_on_error` / `after_tool` 中间件阻塞**——错误处理链中的某个中间件可能在特定条件下阻塞

## 涉及文件

- `peri-agent/src/agent/executor/tool_dispatch.rs` — 工具调度，ToolNotFound 处理
- `peri-agent/src/agent/executor/mod.rs` — ReAct 循环主入口
- `peri-agent/src/agent/executor/llm_step.rs` — LLM 调用与 cancel token
- `peri-acp/src/session/executor.rs` — 共享 agent 执行管线
- `peri-acp/src/session/event_sink.rs` — EventSink trait 与 TransportEventSink
- `peri-tui/src/app/agent_ops/lifecycle.rs` — Done/Interrupted/Error 处理器
- `peri-tui/src/app/agent_ops/polling.rs` — TUI 事件轮询
- `peri-tui/src/app/agent_ops/acp_bridge.rs` — ACP 通知桥接
