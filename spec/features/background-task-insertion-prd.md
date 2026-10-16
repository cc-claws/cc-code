# PRD: 后台任务完成通知的运行中插入

**状态**：实现完成，待评审（v3）
**作者**：agent（与 adim 讨论产出）
**日期**：2026-10-16
**修订说明**：v2 修正了 v1 在实现层的两处硬伤（异步回收、失败分类），并补入行为风险与既有同类缺陷。v3 按实现期核查修订 §4.4——消费回执的确认时机前移到「真正写入 state 之前」，`confirm` 失败因此同样代表未送达，两类失败的处置统一为**回队重试**，并新增「交付结果未知」一档；同时回填 §6 的实际落点。

---

## 1. 背景

### 1.1 现象

agent 正在执行任务时，用户按 `Ctrl+B` 后台化（或前台命令超时被自动转后台）的 Bash 命令**早就跑完了**，但 agent 长时间看不到这个完成事件——实测中被积压了 **十几分钟**，直到 agent 那一轮结束（`Done`）才被注入。期间 agent 只能靠主动 `Read` 输出文件才知道任务已完成。

实测时间线（2026-10-16）：

| 时刻 | 事件 |
|---|---|
| T+0 | agent 启动 `cargo check -p cc-tui --all-targets`，因前台超时被转后台 |
| T+14s | 命令实际已完成（exit 0） |
| T+0 ~ T+15min | agent 连续作业，未收到任何完成通知 |
| T+15min | 用户发下一条消息（让出轮次）→ 通知才送达 |

### 1.2 两条后台路径的待遇不一致

| 路径 | 注入时机 | 证据（已逐条核对） |
|---|---|---|
| `Agent` 工具起的**后台 SubAgent** | **运行中插入**：每轮工具执行后 drain 通知队列 → `state.add_message(human(通知))` → 当轮下一次 LLM 调用即可见 | `cc-agent/src/agent/executor/final_answer.rs:28-36`；调用点 `cc-agent/src/agent/executor/mod.rs:355` |
| **Bash 命令后台化**（Ctrl+B / 超时转后台） | **等 `Done` 才注入**：运行中一律入 `pending_bg_shell_notifications` 队列，轮次结束后由 `lifecycle` 一次性 drain 合并成新 prompt | `cc-tui/src/app/shell_command.rs:646-680`、`cc-tui/src/app/agent_ops/lifecycle.rs:165-198` |

本 PRD 只处理第二条。

### 1.3 为什么"等 Done"在长任务下不可接受

Bash 后台化的典型用途是"**起一条慢命令，然后继续干别的**"。若 agent 的任务本身要跑十分钟以上，完成通知的延迟上限 = **该任务剩余时长**，而不是"一次 LLM 来回"。极端情况下用户已经等到任务结束，后台结果才被处理。§2.2.1/§2.2.2 还会说明：现状甚至有"根本送达不了"的路径。

---

## 2. 现状机制（改动前）

### 2.1 通知的产生

后台 shell 结束时构造通知（`cc-tui/src/app/shell_command.rs:318-327`），正文由 `shell_completion_notification` 生成（`cc-tui/src/app/background_shell.rs:270-291`）：

```xml
<background-task-completed>
<task-id>xxx</task-id>
<command>cargo check --all-targets</command>
<status>completed (exit 0)</status>
<output>C:\...\tasks\xxx.output</output>
</background-task-completed>
```

**正文已是"短句 + 结果句柄"形态**：只说状态，完整 stdout/stderr 走文件路径，agent 需要时用 `Read` 取。本 PRD **不改文案**。

要素（行号已核对）：

- 输出文件路径规则 `{temp_dir}/peri-{user}/{sanitize(cwd)}/{session_id}/tasks/{id}.output`（`cc-agent/src/task_output.rs:33-41`）
- 路径在任务登记时生成（`cc-tui/src/app/agent_shell_executor.rs:221-226`）
- 结束信号由 executor 在进程退出后落定：`exit_signal_clone.finish(outcome)`（`cc-tui/src/app/agent_shell_executor.rs:284`）
- TUI 每帧读取：`poll_agent_shells()`（`cc-tui/src/app/shell_command.rs:576-605`）

### 2.2 现有的注入策略（`inject_bg_shell_notifications`）

`cc-tui/src/app/shell_command.rs:646-649` 的注释即策略说明：

- **idle**（`!loading`）：当前会话的通知 + 队列中同会话待注入通知**一次性 drain 合并**为一条 `submit_message`，把 n 条压成 1 轮 turn，避免逐条级联唤醒
- **推理中**（`loading`）：**全部入队**，待 `Done` 后由 `lifecycle` 一次性 drain 合并注入
- 跨会话：`owner_session_id` 不匹配的通知留在队列，交由对应会话消费
- 该函数还有第二个调用方：stall 警告（`cc-tui/src/app/agent_ops/mod.rs:91-95`，注释"与完成通知共用合并注入逻辑"），改动同样受益

### 2.2.1 Done 路径的前置条件：用户排队消息会把它挤掉

`cc-tui/src/app/agent_ops/lifecycle.rs:157-165` 在轮次结束时**优先**处理用户排队消息：

```rust
if !pending_messages.is_empty() {
    self.flush_pending_messages();   // ← 走这条
} else {
    /* 后台通知的 drain 只在这里 */   // ← 被整段跳过
}
```

即：**只要轮次结束时队列里有用户排队消息，后台完成通知就不会被注入**，继续滞留到下一次 `Done`。若每轮结束都恰好有排队消息，通知可能长期无法送达。

### 2.2.2 Error / Interrupted 不消费该队列

`handle_interrupted`（`lifecycle.rs:203`）与 `handle_error`（`lifecycle.rs:345`）两条分支**都没有消费** `pending_bg_shell_notifications`（该队列全仓仅在 `lifecycle.rs:178` 一处被 drain）。轮次以中断或报错收尾时，积压通知不会在本次收尾注入，只能等下一次 `Done`——若用户此后不再发起轮次，它们就停在内存队列里。

**这两条是本 PRD 要一并消除的隐患**：方案把注入时机从"依赖 Done"前移到"通知产生时"，上述分支将不再影响通知送达。

### 2.3 已存在的插入通道（steering）

```
TUI: acp_client.steer(session_id, content)     cc-tui/src/acp_client/client.rs:433-449（async）
  → ACP: "peri/session/steer"                  cc-tui/src/acp_server/mod.rs:213-226
  → 入队: steering::enqueue → Option<receipt>  cc-acp/src/session/steering.rs:8-23
  → 等待: steering::confirm(receipt).await     cc-acp/src/session/steering.rs:27-31
  → 队列: SteeringQueue（每轮 prompt 创建，轮末 close_on_drop）
                                               cc-tui/src/acp_server/prompt.rs:71-79
  → 消费: executor 检查点                      cc-agent/src/agent/executor/mod.rs:316-318（轮首）、
                                               365-376（最终答复前 drain_or_close）
  → 注入: state.add_message(human(content))    cc-agent/src/agent/executor/mod.rs:432-457
```

**必须注意的两条语义**（v1 漏掉了，直接导致 v1 的伪代码不可实现）：

1. `steer` 是 `async` 且**要等到 agent 真正消费才返回**（ACP 侧 `confirm(receipt).await`）。它不是"发出去就完"，耗时取决于 agent 何时到下一个检查点。
2. 两类失败的语义**不同**，但当前**错误码相同**（都是 `-32000`）：
   - `enqueue` 失败 → 队列已关闭，**消息没进 agent 队列**（文案："当前执行已结束，消息仍保留在队列中"）
   - `confirm` 失败 → **消息已入队但未被消费**（文案："本轮未接收补充信息，消息仍保留在队列中"），agent 侧最终答复前会 `drain_or_close` 再消费一次，即**消息其实已送达**

其它关键性质（与需求吻合）：注入走 `add_message`（尾部追加，不破坏 prompt cache 前缀）；不打断 in-flight 的 LLM 请求；注入后立即 `emit(StateSnapshot)`，TUI 同步可见。

---

## 3. 目标与非目标

### 3.1 目标

1. agent 运行中，Bash 后台任务完成通知在**当轮的下一次 LLM 调用前**就可见（延迟从"任务剩余时长"降到"一次 LLM 来回"）
2. 不打断正在飞的 LLM 请求；不破坏 prompt cache 前缀
3. **一条通知只被处理一次**：不得因兜底逻辑造成重复投递（§4.4）
4. 结果不丢：未入队时才退回现有排队路径，行为与今天一致
5. 跨会话路由语义不变
6. 顺带消除 §2.2.1 / §2.2.2 的滞留路径

### 3.2 非目标

- **不改** SubAgent 后台任务路径（已是插入语义）
- **不改**通知正文格式（已是短句 + 句柄）
- **不改**闲时（`!loading`）的合并策略
- 不新增"是否立即插入"的配置开关（见 §9）

---

## 4. 方案设计

### 4.1 时机分流（异步实现）

`inject_bg_shell_notifications`（`cc-tui/src/app/shell_command.rs:652`）是**同步函数、每帧被调用**，因此 steer 必须异步发起、结果异步回收，**不能在函数内同步判定成败**：

```
# 在 inject_bg_shell_notifications 内（同步）
for notification in notifications:
    if !loading and matches_current(owner):
        to_inject.push(...)                       # 闲时合并（现状不变）
    else if loading and matches_current(owner):
        self.spawn_steer_notification(notification)   # tokio::spawn，不阻塞帧循环
    else:
        queue.push_back(notification)             # 跨会话：留给对应会话

# 在某个 poll_* 里回收结果（与 poll_steering_results 同位置）
for (notification, result) in steer_result_rx.try_recv():
    match result:
        Ok(())                        -> 完成（agent 已消费）
        Err(NotEnqueued)              -> queue.push_back(notification)   # 兜底，唯一允许入队的失败
        Err(NotConsumed)              -> 丢弃（agent 侧 drain 保证送达，见 §4.4）
```

异步发起的写法参照现有先例：`cc-tui/src/app/queued_messages.rs:163-167`（`tokio::spawn` + `steering_result_rx` 回执通道）。

### 4.2 注入身份

复用 steering 现状：`BaseMessage::human(content)`（`cc-agent/src/agent/executor/mod.rs:443`）。

理由：这是项目既有惯例——SubAgent 的完成通知同样以 human 注入（`final_answer.rs:32`）；steering 通道本身只承载 human 内容。正文自带 `<background-task-completed>` XML 包裹，agent 可精确解析，降低与真实用户消息的混淆（残留风险见 §5.4）。

### 4.3 文案

**不改**（§2.1）。后续若需压缩，可另开一版去掉 `<command>` 等字段。

### 4.4 失败分类与竞态（v3 修订）

**v3 的核心修正**：v2 假设「`confirm` 失败 = 消息已入队、agent 最终答复前会 `drain_or_close` 消费 = 其实已送达」，因此规定**不得重试**。实现期核查推翻了这一假设——消费回执的确认时机已前移到**真正写入 state 之前**（`cc-agent/src/agent/executor/mod.rs`：steering 延后到 `before_model` 之后、compact 之后写入；回执在写入前保持未确认，提前退出时由 `Drop` 拒绝）。于是 `confirm` 失败同样意味着**未写入历史 = 未送达**，与「未入队」在处置上等价。

| 情形 | 语义 | 处置（v3） |
|---|---|---|
| `enqueue` 返回 `-32001`（队列已关闭） | 消息没进 agent 队列 | 回 `pending_bg_shell_notifications`，待下一轮或空闲重试 ✅ |
| `confirm` 返回 `-32002`（已入队但未写入历史） | 未送达（写入前回执保持未确认） | **同样回队重试** ✅——确认时机前移后，重试不会造成重复投递 |
| 连接中断等无法判定的错误 | 交付结果未知 | 保留原文进 `unconfirmed`，**不自动重投**（宁可不送也不重复）⚠️ |
| 无 `acp_client` / 无 `session_id` | 无法发起 steer | 回队兜底 ✅ |
| 判定 `loading` 后 agent 立刻 `Done` | 队列已关闭 → 走第一行 | 回队兜底 ✅ |

**错误码仍然分开**（`cc-acp/src/session/steering.rs`）：`-32001` = 未入队、`-32002` = 已入队未消费。v3 下两者处置相同，分开保留是为了可观测性（日志能区分卡在哪一环），也避免 TUI 依赖错误文案匹配（脆）。

这会让改动面扩到 `cc-acp/src/session/steering.rs`（§6）。

### 4.5 在途回执的回收状态

`spawn_steer_notification` 需要把"待确认的通知 + 结果通道"挂在 App 上（新字段，例如 `pending_steer_notifications: Vec<(PendingBgShellNotification, oneshot::Receiver<...>)>`，或复用一个统一的通道）。**v1 的 §6 遗漏了这项**，它是 P0-1 的直接后果。

---

## 5. 边界与风险

### 5.1 compact 期间

`loading == true` 且正在 compact 时，`SteeringQueue` 在 prompt 生命周期内仍存活，理论上可入队并被消费。但 compact 会**重写消息数组**（摘要 + `re_inject`），插入的 human 消息可能被压缩掉。

**归属说明**：这不是本方案引入的问题——SubAgent 路径同样以 `human` 注入（`final_answer.rs:32`），面临相同压缩风险。本 PRD 只要求"不比现状更差"，并在验收时确认：

- 若发现插入的消息被 compact 吞掉 → 降级为该场景入队（仍在 §4.1 分支内可调）

### 5.2 合并语义的变化

现状 idle 路径把 n 条通知合并成 1 轮 turn（`lifecycle.rs:166-168` 注释说明原因：避免 n 条触发 n 轮级联唤醒）。运行中改逐条插入后：

- 运行中：n 条变成同一轮内的 n 条 human 消息——**不额外产生轮次**，优于现状的 n 轮级联
- idle：保持合并，不变

### 5.3 prompt cache

插入走 `add_message`（尾部追加），不触碰前缀；但任何新消息都会让缓存前缀失效到此前的长度，这是内容变更的必然代价，非本方案引入。

### 5.4 打断当前计划 / 被误当用户要求

插入会让 agent 在处理长任务的中途看到"某事完成了"，可能：

1. 改变其既定计划（这是"插入"的固有代价，也是需求目的）
2. **把它当成新的用户要求去响应**，甚至放弃手头任务——因为身份是 `human`

第 2 点的缓解：正文自带 `<background-task-completed>` XML 包裹（§2.1），SubAgent 路径已在用同一形态。**验收时需观察**：agent 收到插入通知后是否仍能继续原任务，而不是掉头去处理通知。

### 5.5 旧路径的滞留隐患（改后自动消除）

§2.2.1（Done 时被用户排队消息挤掉）与 §2.2.2（Error/Interrupted 不消费）在改动后不再影响通知送达——通知在产生时就尝试插入，不再依赖 `Done` 这个唯一出口。

队列本身**保留**（作为 steer 失败/跨会话/闲时合并的载体），上述分支从"唯一出口"降级为"兜底出口"，代码不删。

### 5.6 既有同类缺陷：用户排队消息的 steer 失败也会重复投递

审计时发现的**既有问题**（非本方案引入）：`cc-tui/src/app/queued_messages.rs:202-206` 在 steer 失败时把消息保留在 `pending_messages`（`sending = false`），随后若 `!loading` 会 `flush_pending_messages()` 重发（`:215-217`）。若失败原因是"已入队未消费"，用户消息同样会被处理两次。

**处置建议**：本 PRD 不修，但应在 §4.4 的错误码方案落地后**顺手复用**（判断逻辑完全相同），或单独记 issue 跟踪。

---

## 6. 涉及文件（预期改动清单）

> **实际落点（v3 回填）**：通知的注入与回执回收整体抽成新模块 `cc-tui/src/app/bg_shell_notifications.rs`——`inject_bg_shell_notifications` 是同步入口，`dispatch_pending_bg_shell_notifications` 按 `loading` 分流（运行中 steer / 空闲合并），`spawn_bg_shell_notification` 用 `tokio::spawn` 异步发起，`poll_bg_shell_notifications` 每帧收口回执；在途回执挂在 `ChatSession.bg_shell_steering`。`cc-agent/src/agent/executor/mod.rs` 的 steering 消费点从轮首移到 `before_model` 之后（compact 之后写入），`lifecycle.rs` 的 Done 出口不再自行 drain 而是调用 `poll_bg_shell_notifications()`。

| 文件 | 改动 | 备注 |
|---|---|---|
| `cc-tui/src/app/bg_shell_notifications.rs`（新） | 注入入口 + 每帧回执回收 + 分流 + 异步发起 | 核心（内容自 `shell_command.rs` 迁出） |
| `cc-tui/src/app/shell_command.rs` | 删除旧同步实现（-68 行） | 迁移 |
| `cc-tui/src/app/chat_session.rs`（或 App 同级） | 新增"在途 steer 回执"字段（§4.5） | 由 P0-1 派生 |
| `cc-acp/src/session/steering.rs` | 两类失败改用不同错误码（-32001 未入队 / -32002 已入队未消费） | 由 P0-2 派生 |
| `cc-tui/src/app/shell_command_test.rs` | 单测（见 §7） | |
| （可能）`cc-tui/src/app/queued_messages.rs` | 若决定顺手修 §5.6 | 可选 |

**不改动**：`cc-agent`（消费逻辑已具备）、通知正文生成、SubAgent 路径。

---

## 7. 验收标准

### 7.1 功能验收（真机 TUI）

1. **运行中插入**（核心）：agent 运行中后台化 `sleep 20 && echo done`。硬断言：命令结束后 **30 秒内、且期间没有任何用户输入**，agent 的输出或工具调用中出现了该 `task-id`（或读取了 `<output>` 路径）。不得依赖"用户再发一条消息"才发生。
2. **无额外轮次**：同一轮内后台化 3 条命令 → 3 条通知都在当轮可被引用，且**不额外产生 3 轮 turn**。
3. **闲时行为不变**：idle 时后台命令结束 → 合并为一条、立即触发一轮（回归 §2.2）。
4. **不重复投递**：构造 `confirm` 超时/未消费场景，确认该通知最终只被 agent 处理一次——v3 允许这类回执回队重试，重试不得造成重复投递（关键回归，对应 §4.4）。
5. **不丢**：agent 恰好在通知到达瞬间结束 → 通知在下一轮可见，不消失。
6. **跨会话不串**：后台任务属于其它会话 → 仍留给对应会话消费。
7. **中断/报错后不滞留**：轮次以 Ctrl+C 中断或报错收尾时，通知仍能被送达（对应 §2.2.2）。

### 7.2 单测

- `test_inject_bg_shell_notification_steers_while_loading`（运行中走 steer，不入队）
- `test_inject_bg_shell_notification_queues_when_not_enqueued`（错误码 -32001 → 入队兜底）
- `test_inject_bg_shell_notification_drops_when_not_consumed`（错误码 -32002 → **不**入队，防重复）
- `test_inject_bg_shell_notification_keeps_other_session_in_queue`
- `test_inject_bg_shell_notifications_merge_when_idle`（闲时合并回归）
- `test_queued_notification_survives_interrupt_and_error`（兜底不丢）
- `test_done_injects_queued_notification_when_no_pending_user_message`（Done 兜底出口回归 §2.2.1）
- compact 场景：`test_steered_notification_survives_compaction`（若 §5.1 验证发现问题则改为该场景降级入队）

---

## 8. 实施约束

当前开发机 D 盘可用空间约 4G，而 peri 全量 `target` 需 5–8G（此前已实测 `cargo test` codegen 阶段写满盘 `no space on device`）。因此本次改动**本机只能做 `cargo check --all-targets` 级别验证**（若空间允许），真实 TUI 用例与单测交给 CI 三平台构建。评审需接受"合并前不在本地跑真实 TUI 用例"这一限制。

---

## 9. 未决问题（需评审确认）

1. **是否给用户开关**：默认"立即插入"，还是提供"后台完成是否立即插入"的配置项？本 PRD 按"**默认插入、无开关**"撰写。
2. **SubAgent 路径是否一并收敛**：它已是插入语义，但注入**全文**且没有结果文件句柄（`cc-agent/src/agent/events.rs:18`）。改短需先给 SubAgent 结果落地文件。建议**本 PRD 不动**，另开一版。
3. ~~**错误码方案是否采纳**~~（已定）：已新增 `-32001`/`-32002`；v3 下两类处置相同，错误码保留用于可观测性与日志区分，TUI 不再依赖错误文案匹配。
4. **§5.6 的既有缺陷**：顺手修（复用同一判定）还是单独记 issue？

### 已自查、不再作为未决项

- **`ui.loading` 是否等价于"轮次活跃"**：置位点为 `agent_submit.rs:216/443`（提交消息、后台续跑）与 `agent_compact.rs:15`（compact 开始）；清除点为 `lifecycle.rs:42`（Done）、`agent_compact.rs:45/91`、`agent_recap.rs:10/21`。结论：它 ≈"轮次活跃窗口（含 compact/recap）"，**可作分流判据**；但由于最终成败由 steer 回执决定（§4.4），不应对它形成强依赖。
