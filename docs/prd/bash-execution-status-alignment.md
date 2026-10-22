# PRD：Bash 执行状态渲染对齐参考实现

日期：2026-10-22

版本：v1.1（实施稿）

状态：L1 已实施（`ToolPhase`、状态行 ` · timeout` 格式、空输出 `Done` / `(No output)`、timeout 透传、Ctrl+B 悬挂提示均已落地）；L2 / L3 尚未实施（代码中暂无 `ToolPending` / `ToolOutput` 事件，`BatchItem` 尚无 `tool_call_id`）。GitHub Issue：[#675](https://github.com/cc-claws/cc-code/issues/675)。

适用范围：`cc-tui`（渲染与事件消费）、`cc-acp`（事件映射）、`cc-agent`（事件与执行调度）、`cc-middlewares`（Bash 执行器与 HITL）。

参考实现：参考源码中的 Bash 工具 UI 与执行链路；行号引用以该快照为准。

视觉依据：[Bash 状态效果稿](../designs/2026-10-22-bash-status-preview.html)（逐状态与时间线 HTML mock）。

## 1. 背景与目标

用户对比后发现：同一条 Bash 命令，参考实现在消息流里依次给出 `⎿  Waiting…`、`⎿  Running… (22s · timeout 5m)`、运行中输出尾部、完成结果；peri 只有工具头 + 一行 `Running… (Xs)`，运行中看不到任何输出，timeout 也从不显示。

目标：**Bash（以及所有工具）的执行全过程在消息流内可读**，状态、耗时、超时、输出尾部与完成结果都能就地看到，观感与参考实现对齐。

### 1.1 目标体验

```
● Bash(npm run build)
  ⎿  Running… (22s · timeout 5m)
     (ctrl+b to run in background)
```

有输出后：

```
● Bash(npm run build)
  ⎿  vite v5.0.0 building for production…
     …
     … +12 lines
     (22s · timeout 5m)
     (ctrl+b to run in background)
```

审批期间：

```
● Bash(npm run build)
  ⎿  Waiting for permission…
```

多工具逐个审批：

```
● Bash(cmd A)
  ⎿  Waiting for permission…
● Bash(cmd B)
  ⎿  Waiting…
● Bash(cmd C)
  ⎿  Waiting…
```

jev 自动判定中（单工具，不弹窗）：

```
● Bash(git status)
  ⎿  Waiting…
```

完成且无输出：

```
● Bash(true)
  ⎿  Done
```

## 2. 现状与差异（代码证据）

> 实施注记（2026-10-22 核对）：下表为实施前现状。L1 落地后 D1–D4 已消除——证据：`cc-tui/src/ui/message_view/mod.rs:26`（`ToolPhase` 枚举）、`cc-tui/src/ui/message_render/mod.rs:68`（`shell_running_text` 使用 ` · ` 分隔）、`cc-tui/src/app/message_pipeline/transform.rs:163`（从入参读 `timeout`）、`cc-tui/src/ui/message_render/bash_status_test.rs:86`（Ctrl+B 悬挂提示断言）。D5–D7 随 L2 / L3 尚未实施。

| # | 维度 | 参考实现 | peri 现状 | 代码位置 |
|---|---|---|---|---|
| D1 | 状态行格式 | `Running… (22s · timeout 5m)`，` · ` 分隔 | `Running… (22s)    (timeout 5m)`，4 空格 | `cc-tui/src/ui/message_render/mod.rs:68-77` |
| D2 | timeout | 仅模型显式传 `timeout` 时显示 | `timeout_ms` 恒为 `None`，永不显示 | `message_pipeline/transform.rs:161`、`reconcile.rs:231`、`message_view/mod.rs:812` |
| D3 | 空输出完成 | 显示 `Done` / `(No output)` | 仍按运行中渲染（转圈） | `message_render/mod.rs:1562` |
| D4 | Ctrl+B 提示 | 状态行之后，5 空格悬挂（`Waiting…` / `Running…` 都显示） | `Running…` 下方独立行且带 `⎿` 前缀 | `message_render/mod.rs:1872-1881` |
| D5 | 审批等待 | 工具行下 `Waiting for permission…` | 只有模态弹窗，工具行**尚未出现** | `cc-agent/.../tool_dispatch.rs:955-994`、`message_render/mod.rs:1535-1537` |
| D6 | 运行中输出 | 每秒 progress，显示最后 5 行 | 无输出流，运行中只有一行状态 | 无对应事件 |
| D7 | 完成后输出 | `OutputLine` + 空输出文案 + timeout 尾注 | 非详细 Bash 前 3 行；空输出无文案 | `message_render/mod.rs:1741-1748` |

补充事实：

- peri 的 `ToolStart` 在 `run_before_tools_batch`（含 HITL/jev 审批）**之后**才 emit（`tool_dispatch.rs:955-994`），这是 D5 的根因。
- AI 消息内的 `ContentBlockView::ToolUse` 不渲染（`message_render/mod.rs:1535-1537`），工具行的唯一来源是 `ToolBlock`。
- peri 的工具**执行**是 `join_all` 全并发（`tool_dispatch.rs:1108`），但 `before_tool` 链存在真实等待窗口：默认实现逐个处理 tool call，HITL 可用批量弹窗一次展示多个，jev 语义门逐个判定（`chain.rs:73-75`、`jev/mod.rs`）。

## 3. 需求清单

| 编号 | 需求 | 优先级 | 所属阶段 |
|---|---|---|---|
| R01 | 状态行改为 `Running… (22s · timeout 5m)`；无 timeout 时为 `Running… (22s)` | P0 | L1 |
| R02 | 从工具入参提取 `timeout` 并透传到 ToolBlock；仅显式传入时展示 | P0 | L1 |
| R03 | 引入显式执行阶段（`ToolPhase`），空输出完成后显示 `Done` / `(No output)`，不再转圈 | P0 | L1 |
| R04 | Ctrl+B 提示位于状态行之后，使用 5 空格悬挂缩进 | P1 | L1 |
| R05 | 审批/语义门期间，工具行先出现并显示 `Waiting for permission…` | P1 | L2 |
| R06 | 运行中每秒更新输出尾部：最后 5 行 + `+N lines` / `~N lines` + 字节数 | P2 | L3 |
| R07 | 增量输出只进 UI 缓存，不写 `AgentState` | P2 | L3 |
| R08 | 完成后输出规则对齐：空输出文案、timeout 尾注保留现有 OutputLine 装饰器 | P1 | L2 |
| R09 | `-p` / stdio 路径行为与 TUI 一致（或明确降级） | P1 | L2/L3 |
| R10 | 未开始执行时显示 `Waiting…`：多工具逐个审批的排队，以及 jev 自动判定期间 | P1 | L2 |

## 4. 状态机

引入统一阶段枚举，放在 `cc-tui/src/ui/message_view/mod.rs`：

```rust
pub enum ToolPhase {
    Queued,            // 已收到 tool_use，尚未进入执行（peri 当前几乎不出现）
    AwaitingApproval,  // HITL / jev 审批中
    Running,           // 已开始执行
    Finished,          // 已有 ToolEnd
}
```

| 当前阶段 | 触发 | 目标阶段 | 渲染 |
|---|---|---|---|
| — | `ToolPending` 事件 | Queued | `● Bash(...)` + `⎿  Waiting…` |
| Queued | HITL 弹窗打开 | AwaitingApproval | `⎿  Waiting for permission…` |
| Queued / AwaitingApproval | `ToolStart` | Running | `⎿  Running… (Xs · timeout Nm)` |
| Running | 首个输出 chunk（L3） | Running | 最后 5 行 + `+N lines` + 计时 |
| Running / AwaitingApproval | `ToolEnd` | Finished | 输出 / `Done` / `(No output)` |
| 任意 | 取消 / 拒绝 | Finished(Failed) | 错误结果行 |

`Queued` 的可见窗口来自 `ToolPending → ToolStart`：多工具逐个审批时，未轮到的工具保持 `Queued`，正在审批的转 `AwaitingApproval`（详见 5.4）。

约束：

- 历史恢复与 `/rewind` 重建的 VM 一律 `Finished`，不显示运行态。
- 子 Agent 事件经 `source_agent_id` 归属，不改变状态机。

## 5. 详细设计

### 5.1 L1：渲染与数据修正（无协议变更）

#### 5.1.1 ToolPhase 落地

`MessageViewModel::ToolBlock` 增加：

```rust
phase: ToolPhase,
timeout_ms: Option<u64>,
```

- `build_tool_start_vm`：`phase = Running`，`timeout_ms = input.get("timeout").and_then(|v| v.as_u64())`；
- `ToolEnd` 处理：`phase = Finished`；
- `messages_to_view_models`（历史重建）：`phase = Finished`，`timeout_ms` 从 tool_use input 读取；
- 渲染：`is_running` 由 `phase == Running` 取代 `content.is_empty()`。

#### 5.1.2 状态行文案

`cc-tui/src/ui/message_render/mod.rs` 的 `shell_running_text`：

```rust
// 复用 shell_timeout_text 的 humanize 逻辑（ms >= 60s → "1m 5s"，整千 ms → "Ns"，否则 "Nms"）
fn shell_timeout_label(ms: u64) -> String { /* 现有逻辑，去掉外层括号 */ }

let suffix = timeout_ms
    .filter(|ms| *ms > 0)
    .map(|ms| format!(" · timeout {}", shell_timeout_label(ms)))
    .unwrap_or_default();
format!("Running… ({elapsed}{suffix})")
```

`shell_timeout_text` 去掉外层括号，只保留 `timeout 5m` 文本，由调用方决定是否加括号；**不得直接格式化毫秒原值**（否则会输出 `timeout 300000`）。

#### 5.1.3 空输出完成

`Finished` 且 `content.trim().is_empty()` 且非后台：

- 非详细模式显示 `Done`；
- 详细模式显示 `(No output)`。

参考实现的 `Done` / `(No output)` 取决于工具结果里的 `noOutputExpected` / `returnCodeInterpretation`（`BashToolResultMessage.tsx:100-112`）；peri 没有这两个字段，按详细模式二选一属于**自定义口径**，仅保证「空输出不再转圈」这一行为对齐。

#### 5.1.4 Ctrl+B 提示

参考实现的实际版式是：状态行（`Waiting…` / `Running…`）在前，提示在下一行，使用 **5 空格悬挂缩进**（不带 `⎿`）：

```
● Bash(cmd)
  ⎿  Running… (2s)
     (ctrl+b to run in background)
```

- 显示条件：工具未后台化，且处于 `Queued` / `AwaitingApproval` / `Running`（任何「未完成」阶段）。参考实现在 `Waiting…` 下也显示该提示；
- 完成、后台化、失败后都不显示；
- L3 有输出时，提示放在计时状态行之后（最后一行），输出尾部在它之前。

#### 5.1.5 完成后 timeout 尾注

`Finished` 且 `timeout_ms.is_some()` 时，在结果块底部追加**独立一行**，使用 `⎿` 前缀而非悬挂缩进：

```
● Bash(cmd)
  ⎿  … +11 lines (ctrl+o to expand)
  ⎿  (timeout 1m 30s)
```

对齐参考实现 `BashToolResultMessage.tsx:113-117`（`ShellTimeDisplay` 只传 `timeoutMs` 时输出 `(timeout …)`）。

### 5.2 L2：ToolPending 事件与审批内联

#### 5.2.1 新事件

`cc-agent/src/agent/events.rs`：

```rust
ToolPending {
    message_id: MessageId,
    tool_call_id: String,
    name: String,
    input: serde_json::Value,
    source_agent_id: Option<String>,
},
```

在 `collect_tool_results` 进入 `run_before_tools_batch` **之前**，对每个 `original_calls` 依次 emit。

时序：

```
emit ToolPending × N
  → run_before_tools_batch（HITL / jev 审批，可能阻塞）
  → emit ToolStart（已批准）
  → 并发执行
  → emit ToolEnd × N
```

错误路径：

- before_tool 取消：为所有已 emit `ToolPending` 但未 `ToolStart` 的调用补发 `ToolEnd(is_error=true, "interrupted by user")`；
- before_tool 返回 `ToolRejected`：现有逻辑已补发 `ToolStart + ToolEnd`，改为 `ToolPending + ToolStart + ToolEnd` 或直接 `ToolPending → ToolEnd`（推荐后者，避免状态抖动）；
- 其余错误沿用现有 deferred_error 模式。

**不得写 state**：`ToolPending` 只是事件，`collect_tool_results` 仍不调用 `state.add_message`。

#### 5.2.2 ACP 映射

`cc-acp/src/event/mapper.rs`：

- `ToolPending` → `SessionUpdate::ToolCall(ToolCall::new(id, name).status(ToolCallStatus::Pending).raw_input(...))`。`ToolCallStatus::Pending` 在 `agent-client-protocol-schema 0.12.0` 中已定义，语义即「尚未开始运行，可能正在流式输出参数或等待审批」，无需兜底；
- `ToolStart` 继续映射为 `ToolCall`/`ToolCallUpdate(status = InProgress)`；
- 必须同步更新 `map_executor_event`（`cc-tui/src/app/agent.rs`）；`PrintCollector::handle_event` 已有 `_ => None` 通配分支，新事件默认不输出，但仍需补 `test_print_sink_ignores_tool_pending` 锁定该行为。

#### 5.2.3 TUI 消费

- ACP 事件经 `acp_bridge.rs` 映射为 `AgentEvent::ToolPending`，由 `MessagePipeline`（`cc-tui/src/app/message_pipeline/mod.rs:715`）写入 `pending_tools`，`phase = Queued`；
- HITL 弹窗打开时（`InteractionPrompt::Approval`），按批量子项里的 `tool_call_id` 将对应 VM 置为 `AwaitingApproval`；用户拒绝直接进入 `Finished(Failed)`，**不回退到 Queued**；批准则由后续 `ToolStart` 转入 `Running`；
- jev 语义门与普通 HITL 共用 broker，因此天然覆盖；若后续新增「分类器检查中」事件，统一映射为 `⎿  Semantic gate checking…`（对齐 参考实现的 `Bash classifier checking…`，P2 可选，口径与 5.4.5 一致）。

#### 5.2.4 渲染

- `Queued`：`⎿  Waiting…`
- `AwaitingApproval`：`⎿  Waiting for permission…`
- 两者均不显示计时。

#### 5.2.5 兼容

- 旧版事件无 `ToolPending`：TUI 按现状（ToolStart 才出现工具行）运行；
- `-p` 模式：`PrintEventSink` 收到 `ToolPending` 不输出内容，仅记录状态，避免污染 stdout。

### 5.3 L3：运行中输出流

#### 5.3.1 执行器改造

`cc-middlewares/src/middleware/terminal_inline_executor.rs` 当前调用 `crate::process::output_with_input_timeout(...)`（一次性捕获 stdout/stderr，无流式回调）。改造为：

1. `spawn` + `Stdio::piped()`；
2. 两个 tokio 任务分别读 stdout/stderr，按块写入共享状态；
3. 每次写入后触发节流器（默认 1s）；
4. 节流器通过 `tokio::sync::mpsc` 把 `OutputChunk` 交给上层执行循环。

输出缓冲：

- 保留尾部 64 KiB（跨 stdout/stderr 合计），用于 UI 尾部展示；
- 同时维护 `total_lines`、`total_bytes`（用于 `+N lines` / `~N lines`）；
- 完整输出仍在结束时一次性返回给模型，不改变工具结果语义。

#### 5.3.2 新事件

```rust
ToolOutput {
    message_id: MessageId,
    tool_call_id: String,
    tail: String,        // 最近 N 行（执行器已裁剪）
    total_lines: usize,
    total_bytes: usize,
    elapsed_ms: u64,
    source_agent_id: Option<String>,
},
```

节流规则：

- 首次输出立即发一次；
- 之后最多每 1000ms 一次；
- 输出停止后不再发（完成由 `ToolEnd` 负责）。

#### 5.3.3 ACP 传输

使用 `SessionUpdate::ToolCallUpdate`，`_meta` 挂在 `ToolCallUpdate` 结构上（`ToolCallUpdateFields` 没有 meta 字段）：

```rust
ToolCallUpdate {
    tool_call_id: id,
    fields: ToolCallUpdateFields::new(),
    meta: Some(Meta::from(serde_json::Map::from_iter([(
        "peri".into(),
        json!({
            "toolOutput": {
                "tail": tail,
                "totalLines": total_lines,
                "totalBytes": total_bytes,
                "elapsedMs": elapsed_ms,
            }
        }),
    )]))),
}
```

不使用标准 `content` 字段，避免把运行中输出误当成工具最终结果。

#### 5.3.4 TUI 渲染

- `pending_tools` 增加 `tail: String`、`total_lines`、`total_bytes`、`elapsed_ms`；
- `Running` 且 `tail` 非空：显示最后 5 行（`tail.lines().rev().take(5).rev()`），底部状态行：
  - `+N lines`（`total_lines - 5 > 0`）
  - `(22s · timeout 5m)`
  - 字节数（`formatFileSize` 等价实现）
- 渲染路径复用 `transform_bash_output_lines`（现有 ANSI 处理），避免二次实现；
- 状态行不再显示 `Running…` 前缀（对齐参考实现 有输出时的形态）。

性能约束：

- 每秒最多触发一次 RebuildAll；`RenderCacheKey` 已含内容哈希，未变化不重绘；
- 无输出时仍按现有 1s 计时刷新（`render_thread.rs` 的状态行内容定位逻辑需扩展为「状态行前缀匹配」）；
- 运行中输出不进入 `BaseMessage`，`/rewind` 与持久化不受影响。

#### 5.3.5 降级开关

`settings.json` 增加 `tool_output_streaming: bool`（默认 `true`）。关闭后：

- 执行器不发 `ToolOutput`；
- TUI 回到 L1 行为（仅 `Running… (Xs)` + 完成后输出）。

目的：出现性能或兼容问题时无需回滚版本。

### 5.4 排队态与自动判定态 `Waiting…`

#### 5.4.1 可见窗口

工具**执行**本身是 `join_all` 全并发，但 `before_tool` 链不是：`run_before_tools_batch` 按中间件顺序执行；中间件可以用 batch 实现把多个 tool call 合并处理（如 HITL 批量审批，`cc-agent/src/middleware/chain.rs:73-75`），默认实现则逐个处理。jev 语义门按工具逐个判定。

此外，即使只有一个工具调用，jev 的语义判定本身也要调用 LLM（`cc-middlewares/src/hitl/jev/mod.rs` 的 semantic judge），耗时通常为几百毫秒到数秒。判定是自动的、不需要用户操作，但工具确实处于「已出现、未执行」状态，这段时间同样显示 `Waiting…`。

因此一次回复里出现 N 个工具调用时：

```
emit ToolPending × N        → 全部 Queued，显示 ⎿  Waiting…
before_tool 逐个处理：
  call 1 弹出审批            → call 1 转 AwaitingApproval，显示 ⎿  Waiting for permission…
  call 1 批准              → 继续等待整批审批完成
  call 2 弹出审批            → call 2 转 AwaitingApproval；call 3..N 仍 Queued，显示 ⎿  Waiting…
整批审批完成 → 已批准调用 emit ToolStart → join_all 并发执行
```

这正是 参考实现的观感：**正在等审批的那个显示 `Waiting for permission…`，还没轮到的显示 `Waiting…`**。

#### 5.4.2 渲染规则

| 阶段 | 文案 | 计时 |
|---|---|---|
| `Queued` | `⎿  Waiting…`（排队，或 jev 自动判定中） | 无 |
| `AwaitingApproval` | `⎿  Waiting for permission…` | 无 |
| `Running` | `⎿  Running… (Xs · timeout Nm)` | 有 |

#### 5.4.3 防闪烁

- 若某工具从 `ToolPending` 到 `ToolStart` 之间没有任何审批（YOLO、allow 规则、无需审批的工具），窗口可能只跨一个事件循环。此时**不人为延时**，但要求 TUI 在**同一渲染批次**内收到 `ToolPending` 与 `ToolStart` 时直接渲染最终状态，不先画一帧 `Waiting…`；
- 同一 tool call 的阶段只允许单调前进（Queued → AwaitingApproval → Running → Finished），禁止回退，避免状态行闪烁；
- **Bypass 模式最终口径**：不人为延时、不强制插帧。`ToolPending` 与 `ToolStart` 同批到达时直接渲染 `Running…`；跨批次时自然显示的一帧 `Waiting…` 保留，不做抑制。

#### 5.4.4 审批归属

**前置改动（当前代码缺口）**：`BatchItem`（`cc-agent/src/hitl/mod.rs:20-23`）目前只有 `tool_name` / `tool_input`，**没有 `tool_call_id`**，无法把审批窗口精确归属到某个工具块。`ApprovalItem`（`cc-agent/src/interaction/mod.rs:8-12`）已经有 `tool_call_id`，无需改动。需要：

1. `BatchItem` 增加 `tool_call_id: String`；
2. HITL 中间件构造批量子项时填充（此时 `ToolCall.id` 可用）；
3. cc-tui 两处 `BatchItem` 构造透传（`agent_ops_interaction.rs:148`、`:329`）；
4. ACP `request_permission` 路径的 `RequestPermissionRequest.tool_call.tool_call_id` 已存在，直接使用。

归属规则：

- HITL 弹窗打开时，按 `tool_call_id` 精确匹配对应 VM，只把该工具置为 `AwaitingApproval`；
- jev 语义门与普通 HITL 共用同一个 broker，归属逻辑一致；
- 批量审批弹窗若一次展示多个工具，则这些工具同时进入 `AwaitingApproval`，其余仍为 `Queued`。

#### 5.4.5 jev 自动判定与 Bypass 路径

`before_tool` 阶段按权限模式分三条路径：

| 模式 | before_tool 行为 | 工具块显示 |
|---|---|---|
| **Bypass** | `PermissionMode::Bypass` 直接放行（`cc-middlewares/src/hitl/mod.rs:433`），不调用 judge | `Queued` 窗口≈0，**不显示 `Waiting…`**，直接进入 `Running…` |
| **Auto + Allow** | LLM judge 返回允许（`jev/mod.rs`） | `Waiting…`（judge 期间）→ `Running…` |
| **Auto + Ask** | 弹人工审批 | `Waiting…` → `Waiting for permission…` → `Running…` |
| **Auto + Block** | 直接拒绝 | `Waiting…` → 错误结果 |
| **Auto + 判定不可用** | 兜底分类器 / Ask | 按上面对应路径 |

要点：

- Bypass 与 Auto 共用同一套状态机（`Queued → Running`），但 Bypass 没有可等待的判定过程；
- Bypass **不人为插入 Waiting 帧**：Pending 与 Start 同批到达时直接渲染 `Running…`；跨批时自然显示一帧 `Waiting…`，不做抑制（见 5.4.3）；
- jev 判定期间**不弹窗**、用户无需操作，因此显示 `Waiting…`，而不是 `Waiting for permission…`；
- 判定耗时来自 LLM 往返，通常几百毫秒到数秒，这是**单工具场景下 `Waiting…` 最主要的可见来源**；
- 若后续 jev 能在调用 judge 前上报一次状态，可把该阶段细化为 `⎿  Semantic gate checking…`（对齐 参考实现的 `Bash classifier checking…`）；该细化列为 P2 可选，不阻塞本 PRD。

### 5.5 展示规格（逐帧 Demo）

#### 5.5.1 版式约定

- header：`● Bash(命令摘要)`，`●` 后一个空格再跟工具名；
- 结果区前缀固定为 `  ⎿  `（`BranchPrefix::PREFIX`，2 空格 + `⎿` + 2 空格，共 5 列）；
- 结果区续行、悬挂缩进固定为 5 个空格（`BranchPrefix::HANG`）；
- 实现时**必须**复用 `cc-tui/src/ui/branch_prefix.rs` 的 `BranchPrefix`，禁止任何调用点手写 `⎿` 字面量（`spec/global/domains/tui.md:289-305`）；
- 行首圆点**必须**使用 `crate::ui::theme::SYMBOL_DOT`（`●` U+25CF），不得自绘或退化为小圆点（`spec/global/domains/tui.md:272-276`）；
- 代码块中的行首空格逐字符有效，实际界面不显示任何占位符。

指示器颜色语义：

| 阶段 | `●` 颜色 |
|---|---|
| Queued / AwaitingApproval / Running | `inactive`（运行中带闪烁动画） |
| Finished 成功 | `success`（绿） |
| Finished 失败 | `error`（红） |
| 后台运行中 | `cyan` |

#### 5.5.2 逐帧示例

**A. 多工具逐个审批（排队态 + 审批态并存）**

```
● Bash(cmd A)
  ⎿  Waiting for permission…
● Bash(cmd B)
  ⎿  Waiting…
     (ctrl+b to run in background)
● Bash(cmd C)
  ⎿  Waiting…
     (ctrl+b to run in background)
```

- 单个审批弹窗时每次只有一个工具处于 `AwaitingApproval`；批量弹窗场景下多个同时进入（见 5.4.4）；
- 批准后继续等待整批审批完成，下一个进入 `AwaitingApproval`；
- 全部批准后三个工具同时进入 `Running`（`join_all`）。

**A2. jev 自动判定中（单工具，不弹窗）**

```
● Bash(git status)
  ⎿  Waiting…
     (ctrl+b to run in background)
```

- 语义门正在调用 LLM judge，**不需要用户操作**，所以是 `Waiting…` 而不是 `Waiting for permission…`；
- 判定 `Allow` → 直接转 `Running…`；判定 `Ask` → 才弹人工审批并转为 `Waiting for permission…`；
- 判定耗时通常几百毫秒到数秒，这是单工具场景下 `Waiting…` 最常见的来源。

**A3. Bypass（直接放行）**

```
● Bash(git status)   ← 同批完成 Pending→Start，不显示 Waiting…
```

Bypass 与 Auto 共用同一套状态机；不人为延时、不强制插帧（见 5.4.3 / 5.4.5）。

**B. AwaitingApproval（正在审批）**

```
● Bash(cmd A)
  ⎿  Waiting for permission…
```

**C. Running，未满 2 秒（不显示运行行）**

```
● Bash(npm run build)
```

只有 header 与闪烁指示器；2 秒内完成则永远不会出现 `Running…`。

**D. Running，满 2 秒且无输出**

```
● Bash(npm run build)
  ⎿  Running… (22s · timeout 5m)
     (ctrl+b to run in background)
```

- 状态行在前，Ctrl+B 提示在后（5 空格悬挂，不带 `⎿`，R04）；
- 未传 `timeout` 时状态行为 `Running… (22s)`；
- 有输出后此形态消失，改为 E。

**E. Running，已有输出（L3）**

```
● Bash(npm run build)
  ⎿  vite v5.0.0 building for production…
     transform 1203 modules…
     rendering chunks…
       dist/index.js   120.31 kB
       dist/index.css    8.42 kB
     … +12 lines
     (22s · timeout 5m)
     (ctrl+b to run in background)
```

规则：

- 只显示**最后 5 个视觉行**；
- 超出部分显示 `… +N lines`（N = 总行数 - 5）；
- 状态行只保留 `(22s · timeout 5m)`，**不再带 `Running…` 前缀**；
- 输出行用正文色 `text_soft`，stderr 合并进同一尾部，不单独分栏。

**F. Finished，有输出（默认折叠）**

```
● Bash(npm run build)
  ⎿  vite v5.0.0 building for production…
     transform 1203 modules…
     rendering chunks…
     … +45 lines (ctrl+o to expand)
```

传了 `timeout` 时（真实形态，注意最后一行是独立 `⎿` 行）：

```
● Bash(cd "D:/code/nt_order" && php storage/probes/probe_t57489_prodlog.php 2>&1 | head -25)
  ⎿  order_status_history_000 总行数: 3323926
     order_status_history_000 今日行数: 4015
     … +11 lines (ctrl+o to expand)
  ⎿  (timeout 1m 30s)
```

- 非详细模式只显示前 3 行（现有规则），超出追加 `… +N lines (ctrl+o to expand)`；
- 详细模式（Ctrl+O）显示全量，无截断提示；
- 传了 `timeout` 时，输出块底部追加独立一行 `⎿  (timeout 5m)`（R08，注意是 `⎿` 前缀而非悬挂）。

**G. Finished，无输出**

```
● Bash(true)
  ⎿  Done
```

- 非详细模式 `Done`，详细模式 `(No output)`；
- 对齐参考实现 `BashToolResultMessage.tsx:100-112`（`Done` / `(No output)` 的选择依据是 参考实现的 `noOutputExpected`，peri 按详细模式二选一，见 5.1.3）；
- 不再出现转圈指示器。

**H. Finished，失败**

```
● Bash(npm run build)
  ⎿  error TS2304: Cannot find name 'foo'.
     … +2 lines (ctrl+o to expand)
```

- 指示器红色，输出行 `error` 色；
- 退出码非零但无 stderr 时，沿用现有错误摘要行规则。

**I. 后台运行**

```
● Bash(long-task)
  ⎿  Running in the background (↓ to manage)
  ⎿  (timeout 5m)
```

- 后台化后不再显示 `Running…` 与计时；
- `(timeout 5m)` 仅在显式传 `timeout` 时出现（现有行为保留）。

**K. 窄终端折行**

```
● Bash(npm run build -- --mode production
       --out-dir dist --sourcemap)
  ⎿  Running… (22s · timeout 5m)
```

- 命令摘要折行时，续行缩进到命令起始列（`● Bash(` 之后，即 7 列；现有 issue #258 行为）；
- 结果区内容按显示宽度折行，续行使用 5 空格悬挂缩进；
- CJK 按 2 列计算宽度，不按字节切断。

**L. 详细模式（Ctrl+O）**

- 命令完整展示，超宽折行；
- 运行中输出显示全量（不截断 5 行），状态行仍在末尾；
- 完成后输出全量；
- 排队/审批行与默认模式一致。

**M. 执行中断**

```
● Bash(long-task)
  ⎿  interrupted by user
```

Esc / Ctrl+C 后进入 `Finished(Failed)`，运行行、计时、Ctrl+B 提示全部消失。

**N. 并发多工具**

```
● Bash(cmd A)
  ⎿  Running… (2s)
     (ctrl+b to run in background)
● Bash(cmd B)
  ⎿  Running… (1s)
     (ctrl+b to run in background)
```

每个工具独立计时与提示，顺序按 tool_use 原始顺序。

**O. 审批拒绝**

```
● Bash(rm -rf build)
  ⎿  User rejected
```

红色错误态，直接进入 `Finished(Failed)`。

**P. 审批取消（Esc）**

```
● Bash(npm run build)
  ⎿  Cancelled
```

与拒绝同路径，工具不执行。

**Q. 跨分钟计时**

```
● Bash(npm run build)
  ⎿  Running… (1m 5s · timeout 5m)
     (ctrl+b to run in background)
```

**R. 跨小时计时**

```
● Bash(npm test)
  ⎿  Running… (1h 2m)
     (ctrl+b to run in background)
```

秒数 `< 60` → `Ns`；`>= 60` → `Nm Ns`；`>= 1h` → `Nh Nm`；timeout 仍只在显式传参时出现。

**S. 运行中 ANSI 输出**

```
● Bash(npm test)
  ⎿  ✓ auth.spec.ts (2.1s)
     ✗ cart.spec.ts (0.8s)
     … +18 lines
     (12s · timeout 5m)
     (ctrl+b to run in background)
```

ANSI 颜色保留（示例中 ✓ 绿 / ✗ 红）。

**T. 运行中 JSON 输出**

```
● Bash(curl -s localhost:8080/status)
  ⎿  {
       "status": "ok",
       "uptime": 3600
     }
     (3s)
     (ctrl+b to run in background)
```

与完成态使用同一套美化与折行口径（见 5.9.4）。

**U. 混合状态**

```
● Bash(cmd A)
  ⎿  done output
● Bash(cmd B)
  ⎿  Running… (2s)
     (ctrl+b to run in background)
● Bash(cmd C)
  ⎿  Waiting…
     (ctrl+b to run in background)
```

同一条回复里可同时出现已完成、运行中、排队三种工具块。

**V. 子 Agent 抽屉**

```
└─ Agent(扫描日志)
  ● Bash(grep -r ERROR logs/)
    ⎿  Running… (3s)
```

抽屉内同样按 `phase` 渲染，但不显示 Ctrl+B；事件经 `source_agent_id` 归属。

**W. 历史恢复**

```
● Bash(npm run build)
  ⎿  built in 2.1s
```

无运行行、计时、提示。

**X. 非 Bash 工具**

```
● Read(src/main.rs)
  ⎿  Read 120 lines
```

不显示 `Running…`；空输出时显示 `Done`。

**Y. 大输出（运行中估算）**

```
● Bash(seq 1 100000)
  ⎿  99996
     99997
     99998
     99999
     100000
     ~100000 lines
     (2s)
     (ctrl+b to run in background)
```

**Z. 极窄终端（40 列）**

```
● Bash(npm run build --prod)
  ⎿  Running… (2s)
     (ctrl+b to run in
     background)
```

状态行与提示按显示宽度折行，续行保持悬挂缩进。

#### 5.5.3 完整时间线：一条执行 3 秒的 Bash

统一前提：计时基准 `t=0` 为 `ToolStart` 被 TUI 处理的时刻；命令实际执行 3 秒。

**场景 A：无审批、无输出（`sleep 3`），L3 关闭**

| 时刻 | 事件 | 内部状态 | 屏幕内容 |
|---|---|---|---|
| -50ms | agent emit `ToolPending`，紧接着 emit `ToolStart`（同一渲染批次） | `Queued → Running`，`started_at=now` | 直接渲染运行态，不闪 `Waiting…` |
| 0ms | 首帧 | `Running`，`elapsed=0s < 2s` | `● Bash(sleep 3)`（指示器闪烁） |
| 1000ms | 每秒 tick | `elapsed=1s` | 与 0ms 相同，无变化不重绘 |
| 2000ms | 每秒 tick 命中阈值 | `elapsed=2s >= 2s`，`tail` 空 | `● Bash(sleep 3)` + `⎿  Running… (2s)` + 悬挂 `(ctrl+b to run in background)` |
| 2999ms | — | 不变 | 同上 |
| 3000ms | agent emit `ToolEnd`，输出为空 | `Finished`，指示器转绿 | `● Bash(sleep 3)` + `⎿  Done`，运行行与 Ctrl+B 提示一起移除 |

结论：这条命令的 `Running…` **只出现约 1 秒**（2s 到 3s 之间），2 秒内不显示任何进度。

**场景 B：有审批（jev / HITL），`sleep 3`**

| 时刻 | 事件 | 内部状态 | 屏幕内容 |
|---|---|---|---|
| 0ms | emit `ToolPending` | `Queued` | `● Bash(sleep 3)` + `⎿  Waiting…` |
| 80ms | 审批弹窗打开 | `AwaitingApproval` | `⎿  Waiting for permission…`（弹窗为独立 UI） |
| 1200ms | 用户点「允许」，emit `ToolStart` | `Running`，`started_at=1200ms` | `Waiting for permission…` 消失；`elapsed=0 < 2s`，只剩 header |
| 2200ms | 每秒 tick | `elapsed=1s` | 仅 header |
| 3200ms | 每秒 tick 命中阈值 | `elapsed=2s` | `⎿  Running… (2s)` + 悬挂 `(ctrl+b to run in background)` |
| 4200ms | emit `ToolEnd`，输出为空 | `Finished` | `⎿  Done` |

注意：计时从 **1200ms（ToolStart）** 起算，不是从 `ToolPending` 的 0ms 起算。

**场景 B2：jev 自动判定（`git status`）**

| 时刻 | 事件 | 内部状态 | 屏幕内容 |
|---|---|---|---|
| 0ms | emit `ToolPending` | `Queued` | `● Bash(git status)` + `⎿  Waiting…` |
| 120ms | 语义门开始调用 LLM judge | `Queued` | 不变（不弹窗） |
| 900ms | judge 返回 `Allow`，emit `ToolStart` | `Running`，`elapsed=0 < 2s` | `Waiting…` 消失，仅 header |
| 1000ms | 命令结束，emit `ToolEnd` | `Finished` | `● Bash(git status)` + `⎿  <输出>`，指示器转绿 |

若 judge 返回 `Ask`（例如 1600ms），则在 1600ms 弹人工审批，工具块从 `Waiting…` 转为 `Waiting for permission…`，后续与场景 B 相同。

结论：**单工具 + jev 自动判定**是 `Waiting…` 最常见的可见场景，等待时长等于一次 LLM judge 的往返。

**场景 C：有持续输出（`for i in 1 2 3; do echo line $i; sleep 1; done`），L3 开启**

| 时刻 | 事件 | 内部状态 | 屏幕内容 |
|---|---|---|---|
| 0ms | emit `ToolStart`；立即产生 `line 1`，执行器聚合，首次 `ToolOutput`（节流起点） | `Running`，`tail="line 1"`，`elapsed=0` | `● Bash(for i in 1 2 3; …)`（`<2s`，**即使已有输出也不显示**） |
| 1000ms | `line 2`，第二次 `ToolOutput` | `tail` 含 2 行，`elapsed=1s` | 仅 header |
| 2000ms | tick 命中阈值 | `elapsed=2s`，`tail` 非空 | `⎿  line 1` + `   line 2` + `   (2s)` + 悬挂 `(ctrl+b to run in background)`（无 `Running…` 前缀） |
| 3000ms | `line 3` 输出后命令结束，emit `ToolEnd` | `Finished` | 完成态输出：前 3 行 `line 1/2/3`，无截断，指示器转绿 |

补充规则：

- **2 秒阈值对输出尾部同样生效**：`elapsed < 2s` 时即使 `ToolOutput` 已到达，也只显示 header，避免短命令刷屏；
- 输出尾部最多 5 行，超出显示 `… +N lines`；
- 每秒 `ToolOutput` 与每秒 tick 合并渲染，同一帧内不重复刷新。

#### 5.5.4 元素与数据来源

| 元素 | 数据来源 | 样式 |
|---|---|---|
| `●` | `ToolPhase` / 结果 | inactive / success / error / cyan |
| 工具名 | `tool_name` | `text_soft`（不随状态变色） |
| 命令摘要 | `args_display` | `text_soft` |
| `Waiting…` | `phase == Queued` | `inactive` |
| `Waiting for permission…` | `phase == AwaitingApproval` | `inactive` |
| `(ctrl+b to run in background)` | 未完成（`Queued`/`AwaitingApproval`/`Running`）且未后台 | `inactive`，5 空格悬挂 |
| `Running… (22s · timeout 5m)` | `started_at.elapsed()` + `timeout_ms` | `inactive` |
| 运行中输出尾部 | `ToolOutput.tail`（L3） | `text_soft` |
| `… +N lines` | `total_lines` / `total_bytes` | `inactive` |
| `Done` / `(No output)` | `phase == Finished` 且输出为空 | `inactive` |
| 完成后输出 | `content`（既有 OutputLine 装饰器） | `text_soft` / `error` |

#### 5.5.5 明确不出现的形态

- 不在 header 行追加秒数（`● Bash(npm run build) 22s` 不出现）；
- 不在有输出的情况下同时显示 `Running…` 与输出尾部；
- 排队行、审批行不带计时；
- `Done` 与非空输出不并存；
- 历史恢复的消息不出现任何运行/排队/审批行。

### 5.6 `Running…` 的精确出现条件

#### 5.6.1 显示条件（全部满足才出现）

| 条件 | 判据 | 数据来源 |
|---|---|---|
| C1 阶段 | `phase == Running` | 已收到 `ToolStart`、未收到 `ToolEnd`；历史重建一律 Finished |
| C2 阈值 | `started_at.elapsed() >= 2s` | `ToolStart` 处理时刻起算的单调时钟 |
| C3 未后台 | `!shell_backgrounded` | 后台化事件 |
| C4 无输出尾部 | L3 开启时 `tail.is_empty()`；L3 关闭时恒真 | `ToolOutput.tail` |

判定的唯一入口（渲染层）：

```rust
fn show_running_line(vm: &ToolBlock) -> bool {
    vm.phase == ToolPhase::Running
        && vm
            .started_at
            .map(|t| t.elapsed() >= Duration::from_secs(2))
            .unwrap_or(false)
        && !vm.shell_backgrounded
        && vm.tail.is_empty()
}
```

#### 5.6.2 计时起点与阈值

- 起点是 **TUI 处理 `ToolStart` 的时刻**，不是模型吐出 `tool_use` 的时刻，也不是 `ToolPending`；
- 阈值固定 `2000ms`，与参考实现 `PROGRESS_THRESHOLD_MS`（`BashTool.tsx:90`）一致；
- 2 秒内完成：`Running…` **一次都不会出现**，直接显示结果；
- 恰好 2.0 秒按 `>=` 判定，出现；
- **输出尾部的展示同样受此阈值约束**：`elapsed < 2s` 时即使已有 `ToolOutput` 也只显示 header（见 5.5.6 场景 C）。

#### 5.6.3 刷新节奏

- 每秒刷新一次秒数；
- 渲染线程复用现有 `is_shell_running_status_line()` 的内容定位方式（`spans[0] == "  ⎿  "` 且 `spans[1]` 以 `Running…` 开头；见 `spec/global/domains/tui.md:1219` 记录的「固定下标 1 覆盖命令续行」修复）。新增的 `Waiting…` / `Waiting for permission…` 也必须按内容定位，禁止回到「状态行恒为第 1 行」的假设；
- 秒数格式：`< 60s` → `22s`；`>= 60s` → `1m 5s`（沿用 `humanize_secs`）；
- `timeout` 后缀仅在 `timeout_ms.is_some()` 时拼接，文案 ` · timeout 5m`。

#### 5.6.4 出现后的形态切换

| 触发 | 结果 |
|---|---|
| 首次 `ToolOutput`（`tail` 非空） | `Running…` 行消失，改为最后 5 行输出 + `(22s · timeout 5m)` |
| 后台化 | 改为 `Running in the background (↓ to manage)`，不再计时 |
| `ToolEnd` 且有输出 | 改为结果输出（前 3 行摘要 / 详细模式全量） |
| `ToolEnd` 且无输出 | 改为 `Done` / `(No output)` |
| 取消 / 拒绝 | 改为错误结果，不再出现 `Running…` |
| 子 Agent `ToolEnd` | 同主 Agent，随该工具块结束 |

#### 5.6.5 明确不会出现 `Running…` 的场景

- `phase == Queued`（排队）或 `AwaitingApproval`（审批）；
- 已后台化的命令；
- 历史恢复、`/rewind` 重建的消息；
- `-p` / stdio 非 TUI 模式（不渲染 UI）；
- L3 已收到输出尾部但尚未结束的工具。

### 5.7 与现有设计规范的衔接

| 规范 | 出处 | 本 PRD 的处理 |
|---|---|---|
| `⎿` 前缀统一由 `BranchPrefix` 供给，禁止手写字面量 | `spec/global/domains/tui.md:289-305` | 5.5.1 明确要求 |
| 行首圆点统一 `theme::SYMBOL_DOT`（`●` U+25CF） | `spec/global/domains/tui.md:272-276` | 5.5.1 明确要求 |
| 状态行按内容定位，禁止假设固定下标 | `spec/global/domains/tui.md:1219` | 5.6.3 复用 `is_shell_running_status_line()` |
| VM 新增字段必须参与 `PartialEq`/`Hash`，否则状态变化不触发重渲染 | `spec/global/domains/message-pipeline.md:55-63` | `ToolPhase`、`timeout_ms`、`tail`、`total_lines`、`total_bytes`、`elapsed_ms` 全部参与 |
| Write 的文件快照在审批通过后、`ToolStart` 前采集，经 `_meta.peri.fileSnapshot` 传入 | `spec/global/domains/message-pipeline.md:43` | `ToolPending` **不携带** `file_snapshot`，快照仍走 `ToolStart`，不与 #657 的竞态修复冲突 |
| 新增 `ExecutorEvent` 必须同步 `map_executor_event`，事件不得被静默丢弃 | `CLAUDE.md` TRAP | 5.2.2 已要求同步 TUI 映射与 `PrintEventSink` |
| `collect_tool_results` 延迟写入：执行阶段不写 `state` | `CLAUDE.md` TRAP | `ToolPending` / `ToolOutput` 只发事件，`dispatch_tools` 仍是唯一写入点 |
| 子 Agent 事件必须经 `SubagentEvent` 信封保留来源 | `cc-agent/src/agent/events.rs:77-82` | 新事件均带 `source_agent_id`，不拆包 |
| 测试函数命名全英文 `snake_case` | `CLAUDE.md` 测试规范 | 9 章用例均符合 |

### 5.8 实现触点清单（新增字段的波及面）

新增 `ToolPhase`、`timeout_ms` 填充、`tail` 后，以下位置必须同步处理，否则会出现「状态不刷新」或「视口测量错位」。

**完整字面量构造（必须补新字段）**

| 位置 | 用途 | phase |
|---|---|---|
| `message_pipeline/transform.rs:150` `build_tool_start_vm` | ToolStart | Running |
| `message_pipeline/reconcile.rs:216` | 已完成工具 | Finished |
| `ui/message_view/mod.rs:801` | 历史重建 | Finished |
| `ui/message_view/mod.rs:972` | 工具结果构造（`started_at: None`） | Finished |

**完整解构（需处理新字段）**

| 位置 | 用途 | 处理 |
|---|---|---|
| `ui/render_thread.rs:692/708/740/819` | 运行态增量刷新 | 判定改为 `phase == Running`，并纳入 `tail` |
| `app/message_pipeline/shell_runtime.rs:98` | 后台化状态更新 | 同步更新 phase |
| `ui/message_view/mod.rs:170/182/337` | `PartialEq` / `Hash` | 新字段参与 |
| `ui/render_layout/estimator.rs:69/167` | **高度估算** | Waiting / Running / 输出尾部行数必须计入 |
| `app/agent_ops/lifecycle.rs:343`、`app/subagent_session.rs:187`、`ui/message_view/aggregate.rs:29` | 业务判定 | 确认是否受 phase 影响 |

**特别提醒**

- `estimator.rs` 的高度估算必须与实际渲染行数一致：新增 1 行 `Waiting…` / `Running…`、或最多 5 行输出尾部，都要计入，否则滚动、视口裁剪与全屏判定会偏；
- `render_thread.rs` 的 `needs_refresh` 目前按 `content.is_empty()` 判定运行中，必须改为按 `phase`；
- `ContentBlockView::ToolUse` 不渲染的既有约定不变（`message_render/mod.rs:1535-1537`）。

### 5.9 边界场景设计

#### 5.9.1 执行中断

用户按 Esc / Ctrl+C 中断运行中的命令：

- agent 侧返回 `ToolEnd { is_error: true, output: "interrupted by user" }`（`tool_dispatch.rs:972`）；
- 工具块进入 `Finished(Failed)`，显示 `⎿  interrupted by user`（红色），`Running…`、计时、Ctrl+B 提示全部消失；
- 若同批多个工具，已完成者保留原结果，未完成者各自显示中断结果。

#### 5.9.2 审批被取消

审批弹窗按 Esc → `RequestPermissionOutcome::Cancelled`（`hitl_ops.rs:130`），工具不执行：

- 与拒绝同路径，直接进入 `Finished(Failed)`，**不回退到 `Queued`**；
- 显示拒绝原因（沿用现有文案），不显示 `Waiting…`。

#### 5.9.3 并发多工具

`join_all` 并发执行时：

- 每个工具块独立维护 `phase`、`started_at`、`timeout_ms`、`tail`；
- 每个未完成工具各自显示 Ctrl+B 提示，不做合并；
- 渲染顺序按 tool_use 在消息中的原始顺序，不随完成先后重排；
- 多个工具同时 `Running` 时，计时各自独立。

#### 5.9.4 运行中输出复用装饰器

L3 的 `tail` 必须经过与完成态相同的 `OutputLine` 装饰器：

- ANSI 颜色保留；
- URL 呈现与完成态一致（同一批 OSC 8 链接命中区，见 `ui/osc8.rs`）；
- 单行 JSON 美化与完成态一致；
- 截断、折行口径一致，避免「运行中一种样子、完成后另一种样子」。

#### 5.9.5 详细模式运行中切换

Ctrl+O 切换时：

- 运行中输出立即在「最后 5 行」与「全量」之间切换；
- `phase`、计时不重置；
- 切回非详细模式后恢复最后 5 行 + `+N lines`。

#### 5.9.6 非 Bash 工具边界

`ToolPhase` 对所有工具通用，但差异化渲染：

- 只有 Bash 显示 `Waiting…` / `Running…` 计时 / Ctrl+B / 输出尾部；
- 其他工具（Read / Grep / Glob / Write / Edit 等）只受益于 `Finished` 语义（空输出不再转圈），不新增运行态行；
- Write / Edit 的 diff 预览保持现有逻辑，不在运行中提前渲染。

#### 5.9.7 子 Agent

- 抽屉内的工具块同样按 `phase` 渲染；
- 子 Agent 没有后台化入口，**不显示 Ctrl+B 提示**；
- 事件经 `source_agent_id` 归属，不串到主消息区。

#### 5.9.8 后台任务完成

- `Running in the background (↓ to manage)` 在 `BackgroundTaskCompleted` 后沿用现有更新逻辑；
- 完成通知到达后不再显示 `phase` 运行行，避免与后台完成消息重复。

## 6. 数据流

```
cc-agent/agent/executor/tool_dispatch.rs（collect_tool_results）
  ├─ emit ToolPending × N ───────────┐
  ├─ chain.run_before_tools_batch     │  ← HITL 批量弹窗 / jev 逐个判定
  ├─ emit ToolStart（已批准）          │
  ├─ join_all 并发执行 ───────────────┤
  │    └─ middlewares/terminal.rs → 每秒 ToolOutput
  └─ emit ToolEnd × N ───────────────┘
                                    ↓
                    cc-acp/event/mapper.rs
        ToolCall(Pending) / ToolCall(InProgress)
        ToolCallUpdate(_meta.peri.toolOutput)
        ToolCallUpdate(Completed)
                                    ↓
                    cc-tui/app/agent.rs map_executor_event
                                    ↓
              pending_tools（phase/tail/计时）→ MessagePipeline
                                    ↓
                    message_render::ToolBlock 渲染
```

## 7. 兼容性与降级

| 场景 | 行为 |
|---|---|
| 旧版 ACP server（无 ToolPending/ToolOutput） | TUI 按 ToolStart 出现工具行，行为同现状 |
| 旧版 TUI（忽略新事件） | ACP 只多发事件，不影响协议解析 |
| `-p` / stdio 模式 | ToolPending/ToolOutput 不输出到 stdout；`PrintEventSink` 显式忽略 |
| 子 Agent | 事件携带 `source_agent_id`，沿用现有归属逻辑 |
| 历史恢复 / rewind | VM 一律 `Finished`，无运行态残留 |
| 关闭 streaming | 走 L1 行为 |

## 8. 风险

| 风险 | 影响 | 缓解 |
|---|---|---|
| 提前 emit ToolPending 与「不写 state」契约冲突 | 状态污染、孤儿 tool_use | 只 emit 事件；补发 ToolEnd 的路径与现有 `ready_calls` 逻辑对齐 |
| `ApprovalItem` / `BatchItem` 缺 `tool_call_id` | 审批态无法归属到具体工具块 | L2 前置改动：两个结构补字段，HITL 中间件与 cc-tui 两处构造点透传（5.4.4） |
| 每秒 ToolOutput 触发全量重建 | 长会话卡顿 | 仅失效该条消息的 render key；无变化不重绘 |
| 大输出（`yes`、日志刷屏） | 内存增长 | 执行器侧 64 KiB 环形尾部缓冲；完整输出仍按现有上限截断 |
| 子 Agent 事件顺序 | 抽屉状态错乱 | 复用 `source_agent_id` 与现有冻结 VM 机制，新增事件走同一信封 |
| stdout/stderr 交错顺序 | 尾部展示错乱 | 保留到达顺序合并为一个环形缓冲（不做 stdout/stderr 分栏） |

## 9. 测试计划

### 9.1 L1

- `test_tool_phase_finished_with_empty_output_shows_done`：`true` 命令完成后不再显示运行态，显示 `Done`；
- `test_shell_running_text_includes_timeout_with_dot`：`(22s · timeout 5m)`；
- `test_shell_running_text_without_timeout`：`(22s)`；
- `test_build_tool_start_vm_reads_timeout_from_input`；
- `test_history_rebuild_sets_finished_phase`；
- `test_ctrl_b_hint_follows_running_line`。

### 9.2 L2

- `test_tool_pending_emitted_before_before_tool`（cc-agent）；
- `test_tool_pending_cancelled_emits_tool_end`；
- `test_mapper_tool_pending_status`（cc-acp）；
- `test_tui_renders_waiting_for_permission_during_hitl`（cc-tui，构造 ToolBlock + phase）；
- `test_tui_renders_waiting_for_queued_tools`：3 个工具、逐个审批时，未轮到的两个显示 `Waiting…`；
- `test_tui_renders_waiting_during_jev_judge`：单工具 + jev 判定中显示 `Waiting…`，且不创建审批弹窗；
- `test_jev_allow_transitions_to_running_without_dialog`：judge 返回 Allow 后直接转 `Running…`；
- `test_jev_ask_transitions_to_waiting_for_permission`：judge 返回 Ask 后转 `Waiting for permission…`；
- `test_tool_phase_monotonic_no_flicker`：同一渲染批次内收到 ToolPending + ToolStart 时直接渲染 `Running…`，不闪 `Waiting…`；
- `test_print_sink_ignores_tool_pending`。

### 9.3 L3

- `test_inline_executor_streams_output_tail`（每秒一次、尾部 64 KiB）；
- `test_tool_output_throttled_to_one_per_second`；
- `test_tool_output_does_not_write_agent_state`：连续发送 `ToolOutput` 后 `state.messages()` 长度不变；
- `test_tui_running_shows_last_five_lines_and_plus_n`；
- `test_no_output_streaming_when_disabled`；
- `test_interrupted_tool_shows_error_not_running`：中断后不再渲染运行行；
- `test_concurrent_tools_have_independent_elapsed`：两个 Running 工具各自计时；
- `test_running_tail_reuses_finished_decorator`：运行中 tail 与完成态输出在同一段文本上产出一致的 ANSI/URL/折行结果；
- `test_non_bash_tool_has_no_running_line`：Read/Grep 等不出现 `Running…`；
- 无头 e2e：`seq 1 100000` 运行中 tail 不超 5 行、内存回归（对照 `/gc`）。

### 9.4 回归

- `diff::renderer::tests::` 全量；
- `message_render` 现有工具渲染用例；
- 子 Agent 事件与抽屉测试；
- `cargo clippy --all-targets`。

## 10. 分阶段落地

| 阶段 | 内容 | 触点 | 预计 |
|---|---|---|---|
| L1 | R01-R04、R08 | 仅 cc-tui | 1 天 |
| L2 | R05、R09、R10（排队态与审批态同源；前置：`ApprovalItem`/`BatchItem` 补 `tool_call_id`） | cc-agent + cc-acp + cc-tui | 2-3 天 |
| L3 | R06、R07 | cc-middlewares + cc-agent + cc-acp + cc-tui | 1 周（含性能回归） |

每个阶段独立 PR，L1 可先合入；L2/L3 的 PR 需在描述中注明事件新增点与兼容策略。

## 11. 验收标准

1. 显式 `timeout` 的 Bash 在 2 秒后显示 `Running… (Ns · timeout Nm)`；
2. 未传 `timeout` 时只显示 `Running… (Ns)`；
3. 空输出命令完成后显示 `Done`，header 指示器不再转圈；
4. Ctrl+B 提示位于状态行（`Waiting…` / `Running…`）之后，使用 5 空格悬挂缩进，且仅在未完成且未后台时出现；
5. 审批期间消息流内出现 `● Bash(...)` + `⎿  Waiting for permission…`，批准后转为运行态；
6. 一次回复含多个工具调用时，正在审批的工具显示 `⎿  Waiting for permission…`，尚未轮到的显示 `⎿  Waiting…`，全部批准后进入运行态；
7. jev 自动判定期间显示 `⎿  Waiting…` 且不弹窗；判定 `Allow` 后直接转 `Running…`，判定 `Ask` 后才转 `Waiting for permission…`；
8. 运行中每秒可见最后 5 行输出与 `+N lines`（开关开启时）；
9. 增量输出只更新 UI 缓存，不写入 `AgentState`（`/rewind`、持久化与模型可见结果不受影响）；
10. 执行中断后显示 `interrupted by user`，不再显示运行行、计时与 Ctrl+B 提示；
11. 并发多工具各自独立计时与提示，渲染顺序按 tool_use 原始顺序；
12. 运行中输出尾部与完成态使用同一 OutputLine 装饰器（ANSI / URL / JSON / 折行口径一致）；
13. 非 Bash 工具不新增运行态行，仅修正空输出转圈；
14. 完成且显式传了 `timeout` 时，结果块底部显示独立 `⎿  (timeout Nm)` 行；
15. 关闭 `tool_output_streaming` 后行为回到第 1-8 条，无残留状态行；
16. `-p` 模式 stdout 不出现运行态文本；
17. 子 Agent 的 Bash 运行态在抽屉中表现一致，不显示 Ctrl+B，不串到主消息区；
18. 历史恢复的会话不出现任何运行中指示；
19. 全部新增/回归测试通过，clippy 无告警。

## 12. 不做的事

- 不改模型可见的工具结果文本与截断口径；
- 不引入 stdout/stderr 分栏展示；
- 不做运行中输出的持久化；
- 不调整后台任务（`Running in the background (↓ to manage)`）现有行为。
