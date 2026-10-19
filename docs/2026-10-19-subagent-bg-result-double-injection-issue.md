# 后台子 Agent 完成结果被重复注入（Human + ToolResult 双通道）

**状态**：Open
**创建日期**：2026-10-19
**优先级**：Medium (P2 - 不影响功能正确性，但导致重复回答、额外 LLM 轮次与主时间线伪用户气泡)
**GitHub Issue**：#613 (https://github.com/cc-claws/cc-code/issues/613)
**标签**：`tui`, `acp`, `subagent`, `background-task`, `message-pipeline`

---

## 一、现象描述

用户在主会话中通过 `Agent` 工具（后台模式）派发子 Agent。子 Agent 完成后，主时间线依次出现：

1. `● Agent "审计T58054对接组别新增销售三部" finished · 2m 50s` —— 完成行，正常。
2. `❯ 正在回顾 1 个后台 Agent 结果...` —— 自动续接的伪用户气泡，正常。
3. 主 Agent 在续接轮里**复述上一轮已经给出的结论**（"审计结果已在上一条消息中完整给出，这里只补充一句汇总……"）。

用户的直接观感是「子 Agent 的结果被重复贴进主会话」。进一步排查后确认：

- **显示层的完成行不重复**：`● Agent ... finished` 只出现一次，后台子 Agent 不会在主时间线生成 `SubAgentGroup` 卡片。
- **上下文层确实重复**：同一份子 Agent 输出以 `Human` 和合成 `ToolResult` 两种形态各进入一次服务端 history，模型在同一段对话里看到两遍完整结果。
- **显示层存在一条伪用户气泡**：`drain_notifications` 写入的 `Human` 通知会随 `StateSnapshot` 进入主时间线，被渲染成 `UserBubble`，正文含子 Agent 完整报告。

---

## 二、复现条件

必要条件：**后台子 Agent 在主 Agent 本轮执行尚未结束（executor 仍在 ReAct 循环内）时完成**，且主 Agent 随后正常 `Done`。

典型步骤：

1. 主会话中输入一个需要长时间核实的任务，并在同一轮里用后台模式派发子 Agent。
2. 主 Agent 继续自己的工具调用，期间子 Agent 完成。
3. 主 Agent 本轮结束（`Done`）后，自动续接轮被触发。
4. 观察续接轮的模型回复：几乎必然出现「已在上一条消息中给出」之类的重复表述。

反例（不重复）：后台子 Agent 在主 Agent `Done` **之后**才完成。此时 executor 已不在循环内，通知不会被 `drain_notifications` 消费，history 中只有续接注入的一份 `ToolResult`。

---

## 三、涉及核心文件与调用链路

| 文件路径 | 关键位置 | 涉及逻辑 |
|---------|---------|---------|
| `cc-acp/src/agent/builder.rs` | L398-400, L476 | 构建 `bg_notification` channel：`registry.set_notification_sender(tx)` + `agent.with_notification_rx(rx)` |
| `cc-middlewares/src/subagent/background.rs` | `complete()` L150-163 | 完成时向 `notification_tx` 投递 `BackgroundTaskResult` |
| `cc-middlewares/src/subagent/tool/execute_bg.rs` | L158, L165-171 | 同时向 `bg_event_sender` 发送 `SubagentStopped` 与 `BackgroundTaskCompleted` 两个事件 |
| `cc-agent/src/agent/executor/final_answer.rs` | `drain_notifications` L28-36；L39-59；L95 | 消费 `notification_rx`，以 `BaseMessage::human(result.to_notification())` 写入 state；每个工具步骤后与 final_answer 后都会 drain |
| `cc-agent/src/agent/events.rs` | `to_notification()` L21-34 | 生成 `[Background task ...] ... Result:\n<output>` 文本 |
| `cc-tui/src/app/agent_events_bg.rs` | L240-254；L262-272；L273-281 | 缓存 `pre_done_bg_results`；`Done` 后转 `pending_bg_continuation` |
| `cc-tui/src/app/agent_ops/lifecycle.rs` | L133-155 | `handle_done` 消费 `pre_done_bg_results` 设置 continuation |
| `cc-tui/src/app/agent_ops/polling.rs` | L50-62 | 空闲时取 `pending_bg_continuation` 提交 |
| `cc-tui/src/app/agent_submit.rs` | `submit_bg_continuation` L423-503；L447 | 构造伪用户气泡，调用 `prompt_with_bg_results` |
| `cc-tui/src/acp_client/client.rs` | L314-336（L326） | 续接请求，`user_content` 硬编码英文 `"Background agents completed. Please review the results."` |
| `cc-acp/src/session/executor.rs` | `inject_bg_result_messages` L846-898 | 每条 result 注入一条合成 `tool_result`，文本同为 `to_notification()` |
| `cc-tui/src/app/message_pipeline/transform.rs` | L81-95 | `Human` 消息 → `MessageViewModel` |
| `cc-tui/src/ui/message_view/mod.rs` | L615-637 | 未命中 shell 通知识别时渲染为 `UserBubble` |
| `cc-tui/src/app/background_shell.rs` | L333-363 | 仅识别 `<background-task-completed>` XML，不识别 `[Background task ...]` 纯文本 |
| `cc-tui/src/app/message_pipeline/mod.rs` | L569-577 | `BackgroundTaskCompleted` 不生成主时间线 VM（完成行由 TUI 单独追加） |

---

## 四、根本原因

### 根因：两条注入通道同时存活，且文本同源

`BackgroundTaskResult` 在完成时被投递到**两个独立通道**：

**通道 A（executor 内 drain，写 Human）**

- `background.rs:150-163` → `notification_tx.send(result)`
- executor 持有 `notification_rx`（`builder.rs:398-400,476`）
- 每个工具步骤后（`final_answer.rs:39-59`）与最终回答后（`final_answer.rs:95`）调用 `drain_notifications`，将 `BaseMessage::human(result.to_notification())` 写入 **服务端 state（history）**，并随 `StateSnapshot` 回传 TUI。

**通道 B（TUI 完成事件 → 自动续接，写 ToolResult）**

- `execute_bg.rs:165-171` → `bg_event_sender` 发送 `BackgroundTaskCompleted`
- TUI `handle_background_task_completed` 把结构化结果缓存进 `pre_done_bg_results`（`agent_events_bg.rs:240-254`）
- `Done` 时无条件转为 `pending_bg_continuation`（`agent_events_bg.rs:262-272` / `lifecycle.rs:133-155`）
- 续接请求经 `prompt_with_bg_results` 到达 executor，`inject_bg_result_messages`（`executor.rs:886-895`）为每条 result 注入合成 `tool_result`，文本同样是 `result.to_notification()`

两条通道**没有互斥或去重**。`final_answer.rs:23-27` 的注释只把通道 A 与「TUI 在 executor 结束后兜底 push `origin_messages`」视为互斥关系，遗漏了通道 B。

### 后果

1. 同一份子 Agent 全文在 history 中出现两次（Human + ToolResult），浪费 token。
2. 续接轮的模型看到重复结果，倾向于复述而非补充，产生冗余回答与额外 LLM 调用。
3. 通道 A 写入的 `Human` 在主时间线渲染为 `UserBubble`（`transform.rs:81-95` → `message_view/mod.rs:615-637`），用户会看到一条正文为子 Agent 完整报告、前缀为 `[Background task ...]` 的伪用户气泡。

### 日志证据

`~/.cc-code/logs/agent-tui.log`（2026-10-19，UTC）：

```
02:46:05.004  bg-event-pump: received BackgroundTaskCompleted        ← 全轮仅此 1 条完成事件
02:46:05.310  background task completed before Done, buffering notification for deferred continuation
02:46:59~02:47:21  主 Agent 连续 6 次 Bash（每次工具步骤后都会 drain → 写入 Human）
02:47:33.317  Agent execution completed messages=109
02:47:33.374  Done: processing pre-done background task completions, setting continuation count=1
02:47:33.434  auto-submitting background task continuation with tool results
02:47:35.186  Agent execution completed messages=113
```

即：完成事件只到 1 次、续接只触发 1 次，但结果被两个通道各注入 1 次；续接轮（113-109=4 条消息）的模型输出正是复述。

---

## 五、修复方向（供选型，未实施）

### 方案 1：协议级去重（推荐）

executor 在 `drain_notifications` 消费掉某条 result 后，emit 一个 `BackgroundTaskConsumed { task_id }` 事件；TUI 收到后把对应 task 从 `pre_done_bg_results` / `pending_bg_continuation` 中剔除。

- 优点：不依赖时序推断，语义明确。
- 代价：新增一个事件类型，需同步 `map_executor_event`（`cc-tui/src/app/agent.rs`）与 ACP 序列化。

### 方案 2：按「executor 是否运行」推断（改动最小，有窄竞态）

完成事件到达时若 `ui.loading == true`（executor 正在循环内，稍后必被 drain），标记该 result 为「已注入」，`Done` 时不转 continuation。

- 风险：若完成事件恰好落在最后一次 drain 之后、`Done` 之前，会丢一次通知。

### 方案 3：缩短通道 A 的文本（折中）

`drain_notifications` 写入的 Human 改为不含 `output` 的简短提示（如 "后台任务已完成，结果将在后续注入"），全文只由通道 B 的 `tool_result` 提供。

- 优点：改动小，且运行中的主 Agent 仍能感知「有任务完成」。
- 代价：若续接被用户打断，全文只剩在子 Agent 视口中。

### 附带修复：伪用户气泡

在 `transform.rs:81-95` 或 `message_view/mod.rs:615-637` 识别 `[Background task ` 前缀，转成 `SystemNote`（与 `shell_notification_display_text` 的处理对齐），避免渲染为用户消息。

### 附带改进：续接提示词

`acp_client/client.rs:326` 的 `user_content` 为硬编码英文且无「不要重复」约束。可改为显式要求：仅补充上一条回答未覆盖的信息，不要复述结论。

---

## 六、验证 Checklist

- [ ] 后台子 Agent 在主 Agent 本轮执行期间完成：确认 history 中子 Agent 结果只出现一次（Human 或 ToolResult 之一）。
- [ ] 同上场景，确认续接轮不再复述上一轮结论。
- [ ] 后台子 Agent 在主 Agent `Done` 之后完成：确认结果仍能注入（无丢失）。
- [ ] 主时间线不出现 `[Background task ...]` 前缀的伪用户气泡。
- [ ] `/rewind` 与 `/export` 不受新增去重逻辑影响。
- [ ] `cargo test -p cc-tui --lib`、`cargo test -p cc-acp --lib`、`cargo test -p cc-agent --lib` 通过（本地注意既有浮动失败集）。
