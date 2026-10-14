# 后台任务完成通知强制唤醒 agent，但 agent 侧缺少管理/合并/静默能力

**状态**：Open
**优先级**：中
**创建日期**：2026-10-14
**Issue**：[#491](https://github.com/cc-claws/cc-code/issues/491)

---

## 一、问题背景与现场

在会话中通过 `Bash(run_in_background=true)` 启动的后台轮询命令（如 `sleep 300; gh pr checks 487`），完成后会**强制唤醒 agent 开启新一轮 turn**。当该通知不含任何可行动项时，agent 只能产出填充话术：

```
· Background shell completed: sleep 300; gh pr checks 487 ... (completed (exit 0))
  ⏺ 收到，那是我早前启动的 CI 轮询后台任务，现已结束。其信息已过时……
· Background shell completed: sleep 120; cat ".../xxx.output" ... (completed (exit 0))
  ⏺ 收到，这是我早前启动的第二个 CI 轮询后台任务，同样已过时。
```

两个任务先后完成 → **2 条通知 → 2 轮独立 turn → 2 句无信息量的「收到」**。当时用户已明确表示「好了」（即这些轮询的用途已终结），但 agent 没有任何手段止损。

## 二、已被否定并撤回的方向（重要，避免重走岔路）

> **曾提出的错误方案**：按「纯展示类 vs 可行动类」区分通知，对纯展示类**不唤醒 agent**（只渲染、不注入）。
>
> **该方案已撤回，不可采纳。** 理由：
> 1. **launch 时无法预知用途**——`Bash(run_in_background=true)` 是模型自己发起的异步调用，启动那一刻不存在任何可判定「这个结果稍后要不要用」的信息；
> 2. **通知是 agent 唯一的获知渠道**——`cc-middlewares/src/tool_search/core_tools.rs:18-54` 的核心工具清单为
>    `Bash / Write / Edit / Read / Glob / Grep / WebFetch / WebSearch / Agent / AskUserQuestion / TodoWrite`，
>    **不存在 `KillShell` / `BashOutput` / `ListShells`**（全仓 grep 无命中）。agent 既查不了状态、也取消不了，只能被动等推送。此时抑制通知等于把结果彻底吞掉，严格劣于现状。

**结论：唤醒必须保留。** 缺陷不在「该不该通知」，而在下述三处能力缺口。

## 三、代码实证与缺陷清单

| # | 缺陷 | 位置 | 与「必须唤醒」冲突 |
|---|---|---|---|
| 1 | **agent 缺后台任务管理工具**：人类侧有面板（`background_tasks_panel.rs` 的 Shells/Local agents 分组）且实现了 `kill_background_shell()`（`:677-690`，abort 进程 + watchdog、标记 `Killed`），agent 侧零工具 | 需新增工具 | 否——补齐后「通知」才更可放心保留 |
| 2 | **空闲态不合并**：忙时通知会排队（`pending_bg_shell_notifications: VecDeque`），空闲时来一条就提一条 | `cc-tui/src/app/agent_ops/mod.rs:82-96`（空闲分支 `self.submit_message(notif)`） | 否——合并注入不丢信息 |
| 3 | **排队路径同样逐条消费**：`pending.remove(index)` 一次只取 1 条，Done 后 n 条通知 = n 轮 turn（每轮 Done 再触发下一条） | `cc-tui/src/app/agent_ops/lifecycle.rs:184` | 否 |
| 4 | **应答协议缺失**：通知以 **user 角色**消息注入对话流，模型必须应答，却没有「若无需行动则可不产出用户可见回复」的约定，于是退化为填充话术 | 提示词层 | 否 |

补充事实（供修复时参考）：通知本体是结构化 XML，设计意图明确要求 agent 消费结果。

```
cc-tui/src/app/background_shell.rs:284-291
<background-task-completed>
<task-id>…</task-id><command>…</command>
<status>completed (exit 0)</status><output>…输出文件路径…</output>
</background-task-completed>
```
注释原文：「agent 用 Read 工具读取获取完整 stdout/stderr」。

## 四、涉及文件

- `cc-tui/src/app/agent_ops/mod.rs:82-96` —— 后台 shell 完成事件的分流（空闲直提 / 忙时排队）
- `cc-tui/src/app/agent_ops/lifecycle.rs:170-185` —— Done 后消费 `pending_bg_shell_notifications`
- `cc-tui/src/app/background_shell.rs:270-312` —— 完成通知 XML 构建（`shell_completion_notification` / `agent_shell_completion_notification`）
- `cc-tui/src/app/background_tasks_panel.rs:677-690` —— 人类侧 kill 能力（agent 侧缺失的对照组）
- `cc-middlewares/src/tool_search/core_tools.rs:18-54` —— 核心工具清单（佐证无 shell 管理工具）
- `cc-tui/src/app/chat_session.rs:51-57` —— `background_shells` / `agent_shells` / `pending_bg_shell_notifications` 数据结构

## 五、修复建议

1. **补齐 agent 侧后台任务管理能力**（本质缺口，优先级最高）：
   新增 `ListShells` / `BashOutput`（读取指定 task 的 output 文件）/ `KillShell` 三类工具，与既有人类面板能力对齐。有了自省与撤销手段，「目的失效后被动挨唤醒」的场景才真正可解。
2. **空闲态按短窗口合并注入**：
   在 `agent_ops/mod.rs:95` 与 `lifecycle.rs:184` 引入合并（如累积若干毫秒或一次性 drain 全部匹配同 owner 的通知），把 n 条通知压成 1 轮 turn，信息不丢失。
3. **明确应答协议**：
   在系统提示词中注明「后台完成通知为系统注入，若无需行动则不必产出用户可见回复」，避免填充话术。

## 六、与既有 issue 的关系

本条与以下既有 issue **关注点不同，不重复**：

- `spec/issues/2026-09-20-bash-timeout-and-background-task-notification-alignment.md` —— 关注通知**格式**对齐 `<task-notification>`
- `spec/issues/2026-10-07-background-shell-notification-hardcoded-i18n.md` —— 关注展示**文案 i18n**
- `spec/issues/2026-09-30-shell-command-context-injection-plan.md` —— 关注**上下文注入**设计

**本条关注的是「空闲态通知的唤醒与合并语义」以及「agent 侧管理能力的缺失」。**

## 七、状态变更记录

| 日期 | 从 | 到 | 操作人 | 说明 |
|------|-----|-----|--------|------|
| 2026-10-14 | — | Open | agent | 创建 Issue #489 及本地 Spec 文档；记录并撤回「按可行动性抑制唤醒」的错误方案 |
| 2026-10-14 | Open | In Progress | agent | 落地「确定合理且立即修」部分（下方 §八）：通知注入全量合并 + 队列一次性 drain |

## 八、已落地修复（2026-10-14）

评审后确认：Issue 正文「五、修复建议」中的**空闲态防抖**（方案 2 的短窗口）
对本条现场无效——现场两个轮询任务相隔 **3 分钟**（`sleep 120` / `sleep 300`）自然结束，
任何毫秒级防抖窗口都合并不了。结合代码实证，先落地两处**确定合理、无副作用**的修复：

### 8.1 排队态（busy）一次性全量 drain + 合并注入（缺陷 #3 根治）

**现状 bug**：`lifecycle.rs` 在上一轮 Done 后 **每次只从 `pending` 队列弹 1 条通知**
（`pending.remove(index)`），n 条积压通知 → n 轮 turn 级联雪崩（每轮 Done 再触发下一条）。

**修复**：
- 新增 `App::inject_bg_shell_notifications()`（`cc-tui/src/app/shell_command.rs`）统一注入入口：
  **空闲态**把「本批通知 + 队列中同会话待注入通知」**一次性 drain 并拼接为单条**消息提交，
  把 n 条压成 1 轮 turn；**推理中**全部入队，待 Done 后一次性 drain。
- `lifecycle.rs` 的 Done 分支改为**一次性 drain 当前会话全部通知**并合并提交（原逐条逻辑删除）。
- `agent_ops/mod.rs` 的 stall 警告、`shell_command.rs` 的 `!command` / agent shell 两条
  完成路径统一改为调用 `inject_bg_shell_notifications`，消除三处重复的「首个 submit + 其余入队」分支。
- 合并仅在消息层拼接（保留每条 `<background-task-completed>` XML），**信息不丢失**；
  其它会话（owner 不匹配）的通知仍留在队列，交由其会话消费。

### 8.2 合并消息的展示解析兼容多块（前端不丢行）

`background_shell.rs::shell_notification_display_text_with` 原先只识别**单个**通知块且
「完成块」优先。合并后单条消息可能含多个块（甚至**混合类型**完成 + 等待输入），
重构为按文档顺序逐块扫描，每块产出 1 行，避免多块合并时丢行/串序。

### 8.3 测试

- `shell_command_test.rs`：空闲态合并为 1 轮、队列同会话 drain、跨会话隔离、Done 全量 drain 合并（4 项）。
- `background_shell_test.rs`：多条完成合并展示、混合类型合并保序（2 项）。

### 8.4 未采纳 / 待决（后续单独研究，不在本次范围）

1. **空闲态短窗口防抖**——对相隔分钟级的多任务合并不了，本现场无效，**不做**。
2. **agent 侧管理工具**（`KillShell`/`ListShells`）——本质缺口，需独立设计（工具形态争议：
   扩展 `Bash(action=...)` vs 独立 `Task(...)`），后续单独立项。
3. **`Cancelled` 任务静默不唤醒**——切断「主动取消 → 二次唤醒 → 又一句收到」死循环，
   需先确认与人类面板 kill 的语义边界，后续单独研究。
4. **System Prompt 应答契约**——「无需行动可不产出用户可见回复」，属提示词层，后续单独研究。

