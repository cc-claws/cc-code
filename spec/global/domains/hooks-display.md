# Hook 执行展示 PRD

## 背景

当前 hook 执行完全静默：同步 hook 跑完没有任何提示，异步 hook（`async: true`）在
`cc-middlewares/src/hooks/middleware.rs` 里 fire-and-forget，结果直接丢弃。用户配了
hook 却无法确认它是否执行、是否失败，只能去翻日志。

目标：在不制造噪音的前提下，让用户按需看到 hook 执行情况——**默认静默，阻断必现，
hook 自身失败与执行详情只在详细模式出现**。

## 范围

- 同步 hook 的执行汇总（按事件计数 + 总耗时）
- 异步 hook 的完成提示
- hook 阻断（策略生效）与 hook 失败（脚本异常）的提示

**非目标**：

- hook 配置的增删改（走 `/hooks` 面板与 settings.json）
- 逐条 hook 的完整 stdout/stderr 展示（见「分期 P1」）
- hook 执行历史持久化

## 展示规则

### 普通模式（Ctrl+O 关闭）

| 情况 | 行为 |
|---|---|
| 所有 hook 成功 | **不显示任何行** |
| hook 按策略阻断（exit 2 / decision=block） | 1 行，琥珀色 |
| hook 执行失败（崩溃、超时、非法输出） | **不显示**（仅详细模式） |
| 阻断发生多次 | 逐条显示，最多 3 条，其余折叠为 `… +N more` |
| 同一失败反复发生（同事件、同类型、同 reason） | 只有第一条显示，本次会话内后续同类失败不再新增行 |
| 异步 hook 成功 | 不显示 |
| 异步 hook 失败 | **不显示**（仅详细模式） |

hook 自身失败（脚本不存在、崩溃、超时）只是配置问题，不是工具执行结果；把它放进
普通模式，一旦全局 hooks 配错（如指向不存在的 `clawd-hook.js`），每次工具调用都会
重复刷同一段红字。阻断则不同——工具真的没跑，必须当场提示。

**去重口径**：键为 `(event, kind, reason)`，状态存于会话级 `AgentComm.seen_hook_failures`，
`/clear` 新建 thread 时清空。同键第二次及以后直接丢弃；换了错误内容（reason 变化）仍各
显示一次。本版不做 `×N` 计数展示。

### 详细模式（Ctrl+O 打开）

在普通模式规则之上追加：

| 情况 | 行为 |
|---|---|
| 每触发一个同步事件 | 1 行：`N <Event> hooks ran · Xms` |
| 同步事件超过 4 个 | 前 4 个各 1 行，其余折叠为 `… +N events` |
| 每个异步 hook 完成 | 1 行：`Async hook <Event> completed` |
| hook 执行失败 | 1 行，红色（同样受同会话去重约束，最多 3 条，其余 `… +N more`） |

**行数恒定**：hook 数量不影响行数（10 个 hook 分属 2 个事件仍然只占 2 行）。

## 渲染规格

| 行类型 | 模板 | 颜色 |
|---|---|---|
| 同步汇总 | `⎿  1 {event} hook ran · {ms}ms` / `⎿  {n} {event} hooks ran · {ms}ms` | `theme::cur().inactive` |
| 异步开始（占位） | `⎿  Async hook {event} running…` | `theme::cur().inactive` |
| 异步完成（同事件） | `⎿  {n} async {event} hook completed` / `hooks completed` | `theme::cur().inactive` |
| 异步完成（跨事件合并） | `⎿  Async hook A completed, Async hook B completed` | `theme::cur().inactive` |
| 阻断 | `⎿  {event} hook blocked: {reason}` | `theme::cur().warning` |
| 失败 | `⎿  {event} hook failed: {reason}` | `theme::cur().error` |
| 折叠 | `⎿  … +{n} events` / `… +{n} more` | `theme::cur().subtle` |

- 行首不加 `⚠` / `✗`：语义由文案里的 `blocked` / `failed` 承担，颜色负责扫视。
  代价是色盲、黑白截图与 ASCII 降级终端只剩文案可辨——这是刻意的取舍。
- **强制单行**：hook 行超宽时按列宽截断并以 `…` 收尾，**不折行**。hook 行是状态提示，
  折成多行会把正文挤出视口；长 reason 的完整内容只进日志。

- 分支前缀复用 `BranchPrefix::PREFIX`（`  ⎿  `，5 列），与工具结果行同列。
- 阻断/失败行的 `⎿` 与正文同色：`push_hook_line` 自建 Span
  （`Span::styled(BranchPrefix::PREFIX, fg(color))`，续行的 `BranchPrefix::HANG` 同色），
  不走 `BranchPrefix::span(is_error)`——那个构造器只能表达 inactive / error 两种颜色。

- **异步行的来龙去脉**：异步 hook 完成时，工具块可能早已进入历史前缀，结果行就贴不上了。
  因此异步 hook 在 `tokio::spawn` **之前**先上报 `HooksStarted`，TUI 立刻写一条
  `Async hook {event} running…` 占位；完成事件到达时**移除占位**，结果写入视图尾部的
  合并行（见下一条），不挂在工具块上。轮次结束（`MessagePipeline::done()` /
  `interrupt()`）时仍为 Running 的占位被丢弃，不留僵尸 `running…`。

- **异步完成按窗口合并**：同一事件的多条异步完成累加成 `{n} async {event} hooks completed`；
  不同事件的异步完成若在 **1 秒窗口**内到达，合并成一行、用 `, ` 分隔。合并后就地改写
  已渲染的那一行（而非追加），因此 20 个异步 hook 也只占 1-2 行。

## 挂载位置

- **工具相关事件**（`PreToolUse` / `PostToolUse` / `PostToolUseFailure`）：挂在触发它的
  `ToolBlock` 内部，紧随工具输出行之后，随工具块一起折叠与滚动。
- **只读工具不展示**：`Read` / `Grep` / `Glob` 这类调用不产生 hook 行——只读操作对用户
  没有决策价值，叠一行只是噪音。`Bash` / `Agent` / `Edit` / `Write` 照常展示
  （`Edit` / `Write` 上的 PostToolUse hook 常用来做格式化/lint，用户需要确认它跑没跑）。
- **被折叠/聚合的块不展示**：只读工具聚合进 `ToolCallGroup` 后，**展开也不显示 hook 行**。
  主动折叠代表完全信任该工具的细节，不需要 hook 明细。
- **会话级事件**（`UserPromptSubmit` / `Stop` / `SessionStart` / `PreCompact` …）：展示为
  视图尾部可改写的 `⎿` 行，仅详细模式（Ctrl+O）可见；同一事件跨来源合并成一行、计数累加。
- **`SessionEnd`**：不展示（会话结束没有稳定渲染时机），结果只进日志。

## 数据契约

### 中间件采集（cc-middlewares）

每次 `run_hooks(event, ...)` 结束后产出：

```
HookRunSummary {
    event: HookEvent,        // PreToolUse / PostToolUse / ...
    ran: usize,              // 执行过的 hook 数，两条路径口径不同（见下）
    duration_ms: u64,        // 该事件所有同步 hook 的总耗时
    failures: Vec<HookFailure>,  // 失败/阻断清单（普通模式渲染用）
    is_async: bool,          // 异步完成通知
}
HookFailure { kind: Block|Error, reason: String }
```

- `ran` 的口径：**同步路径**每执行一条就 +1，阻断 / 执行失败同样计入（失败另收进
  `failures`，互不抵消）；**异步路径**每次上报只对应一条 hook，成功计 1、失败计 0。
  被 matcher / `once` / 无 LLM factory 跳过的一律不计。

- 耗时由中间件在调用前后计时，不需要改 `executor`（现在只返回 `HookAction`）。
- 异步 hook 现在 `tokio::spawn` 后丢弃结果，需要保留句柄或在 spawn 内部完成后
  回发一个 `is_async = true` 的 summary。

### 事件链路

```
cc-middlewares → ExecutorEvent::HooksCompleted(HookRunSummary)
              → cc-acp mapper
              → TUI AgentEvent → 写入对应 VM（ToolBlock.hook_rows / SystemNote）
```

### TUI 渲染

- 汇总行与异步完成行：仅在 `detail_mode` 渲染。
- 阻断行：两种模式都渲染。
- hook 自身失败行：仅 `detail_mode` 渲染（配置问题不该在普通模式刷屏）。
- 同 `(event, kind, reason)` 的失败本次会话只渲染第一条。
- 不写入 agent history，不参与 compact 统计。

## 分期

**P0（本次）**：事件计数 + 总耗时汇总、异步完成提示、阻断/失败提示。

失败行依赖 `HookAction::Error { reason }`（P0 一并补上）：非预期退出码、超时、被信号
终止、无法启动都归为「hook 失败但**不阻断**调用」，由中间件转成 `HookFailureKind::Error` 的
失败项。阻断（exit 2 / decision=block）仍走 `Block { reason }`。

**P1（后续）**：每条 hook 的命令名、逐条耗时与 stderr 明细，并在 `/hooks` 面板展示
最近一次执行记录。

## 验收标准

1. 普通模式下所有 hook 成功 → 屏幕上不出现任何 hook 相关行。
2. 一个 Bash 调用触发 2 个 `PreToolUse` + 2 个 `PostToolUse` hook，详细模式恰好显示 2 行汇总，普通模式 0 行。
3. 异步 hook 完成后，详细模式出现 `Async hook <Event> completed`。
4. hook 阻断时，普通模式出现琥珀色的 `… blocked:` 行，工具未执行。
5. hook 崩溃/超时时，普通模式**不出现任何行**；详细模式出现红色的 `… failed:` 行。
6. 触发 8 个事件时，详细模式只显示 4 行 + `… +4 events`。
7. 所有新增行在 ASCII 降级终端下不出现 `?`，行宽不错位。
8. 同一个 hook 反复以相同原因失败（如全局 hooks 指向不存在的脚本）：普通模式 0 行，
   详细模式只有第一条失败行。

## 渲染快照

以下为逐行 ASCII 基准，验收时以这些快照为准。颜色用行尾标注，`⎿` 前缀固定
`BranchPrefix::PREFIX`（2 空格 + `⎿` + 2 空格，共 5 列）。

### C1 无 hook

普通模式 / 详细模式一致，hook 不产生任何行：

```
● Bash(cargo test)
  ⎿  test result: ok. 12 passed; 0 failed
```

### C4 两个 PreToolUse + 两个 PostToolUse，全部成功

**普通模式**（hook 行全隐藏）

```
● Bash(cargo test)
  ⎿  test result: ok. 12 passed; 0 failed
```

**详细模式**

```
● Bash(cargo test)
  ⎿  test result: ok. 12 passed; 0 failed
  ⎿  2 PreToolUse hooks ran · 12ms          [inactive]
  ⎿  2 PostToolUse hooks ran · 34ms         [inactive]
```

### C5 PreToolUse 阻断，工具未执行

**普通模式**

```
● Bash(rm -rf build)
  ⎿  PreToolUse hook blocked: matched "rm -rf" rule   [warning]
```

**详细模式**（阻断行 + 汇总行）

```
● Bash(rm -rf build)
  ⎿  PreToolUse hook blocked: matched "rm -rf" rule   [warning]
  ⎿  1 PreToolUse hooks ran · 8ms                     [inactive]
```

### C6 PostToolUse 失败

**普通模式**（失败行只在详细模式出现）

```
● Bash(cargo test)
  ⎿  test result: ok. 12 passed; 0 failed
```

**详细模式**

```
● Bash(cargo test)
  ⎿  test result: ok. 12 passed; 0 failed
  ⎿  PostToolUse hook failed: exit 1                  [error]
  ⎿  2 PostToolUse hooks ran · 34ms                   [inactive]
```

### C9 异步 hook 完成

**普通模式**

```
● Bash(cargo test)
  ⎿  test result: ok. 12 passed; 0 failed
```

**详细模式**

```
● Bash(cargo test)
  ⎿  test result: ok. 12 passed; 0 failed
  ⎿  1 PostToolUse hooks ran · 6ms                    [inactive]
  ⎿  Async hook UserPromptSubmit completed            [inactive]
```

### C13 会话级事件（UserPromptSubmit）

**普通模式**：不显示（仅详细模式）
**详细模式**：视图尾部一行，跨来源计数累加

```
  ⎿  1 UserPromptSubmit hook ran · 4ms
```

### C14 会话级事件失败

**普通模式**：不显示（`failed` 仅详细模式；`blocked` 两模式都显示）
**详细模式**：`⎿  Stop hook failed: <reason>`

### C11（已废弃）

TUI 只展示工具级事件，所有 hook 行都挂在各自的 `ToolBlock` 内，不存在「跨事件合并成
一轮预算」的场景，本节不再适用。

### C12 四个 hook 失败（详细模式折叠）

```
● Bash(cargo test)
  ⎿  test result: ok. 12 passed; 0 failed
  ⎿  PreToolUse hook failed: exit 1                   [error]
  ⎿  PostToolUse hook failed: exit 1                  [error]
  ⎿  PostToolUse hook failed: exit 1                  [error]
  ⎿  … +1 more                                        [subtle]
```

普通模式下这 4 行全部隐藏（hook 自身失败只进详细模式）。

### C15 ASCII 降级终端

详细模式，列宽与 C4 一致：

```
+ Bash(cargo test)
  + test result: ok. 12 passed; 0 failed
  + 2 PreToolUse hooks ran · 12ms
  + 2 PostToolUse hooks ran · 34ms
```

渲染层不感知 ASCII 降级：hook 行照常输出 `⎿`，是否变成 `+` 由 `terminal_backend`
的宽度探针决定（`fallback_symbol` 判到 `⎿` 列宽与预期不符时替换为 `+`；
`ascii_fallback` 定向表不含 `⎿`）。上面的快照即降级生效后的样子。

断言要点：不含 `?`，每行显示宽度 ≤ 视口宽度，续行不抖动。

### C16 Ctrl+O 来回切换

切到详细模式多出 2 行，切回普通模式后这 2 行**完全消失**，不残留空行、不打乱工具块位置：

```
[普通]                      [详细]                      [切回普通]
● Bash(cargo test)          ● Bash(cargo test)          ● Bash(cargo test)
  ⎿  ok. 12 passed            ⎿  ok. 12 passed              ⎿  ok. 12 passed
                             ⎿  2 PreToolUse …
                             ⎿  2 PostToolUse …
```

## 测试用例

### 用例矩阵

`普` = 普通模式行数，`详` = 详细模式行数，`—` 表示无 hook 相关行。

| ID | 场景 | hook 配置 | 触发 | 普 | 详 |
|----|------|-----------|------|---|----|
| C1 | 无 hook | 无 | `Bash(cargo test)` | — | — |
| C2 | 单个 PreToolUse 成功 | `PreToolUse: guard.sh` | `Bash(ls)` | — | 1 行汇总 |
| C3 | 两个 PostToolUse 成功 | `PostToolUse: fmt.sh, lint.sh` | `Bash(cargo test)` | — | 1 行汇总 |
| C4 | Pre + Post 混合 | `PreToolUse ×2`、`PostToolUse ×2` | `Bash(cargo test)` | — | 2 行汇总 |
| C5 | PreToolUse 阻断 | `PreToolUse: guard.sh` exit 2 | `Bash(rm -rf build)` | 1 行琥珀 | 1 行琥珀 + 1 行汇总 |
| C6 | PostToolUse 失败 | `PostToolUse: bad.sh` exit 1 | `Bash(cargo test)` | — | 1 行红 + 1 行汇总 |
| C7 | hook 超时 | 命令 `sleep 120` | `Bash(ls)` | — | 1 行红 failed: timeout + 汇总 |
| C8 | hook 输出非法 JSON | 打印 `{oops` | `Bash(ls)` | — | 1 行红 failed: invalid output + 汇总 |
| C9 | 异步 hook 成功 | `PostToolUse: async:true` | `Bash(ls)` | — | 1 行 `Async hook … completed` |
| C10 | 异步 hook 失败 | `async:true`，退出码 1 | `Bash(ls)` | — | 1 行红 + 1 行完成提示 |
| C11 | ~~事件超过 4 个~~ | — | — | — | 已废弃（仅工具事件，无跨事件折叠） |
| C12 | 多个失败 | 4 个 hook 全部失败 | `Bash(ls)` | — | 3 行红 + `… +1 more` + 汇总 |
| C13 | 会话级事件成功 | `UserPromptSubmit: log.sh` | 发送任意消息 | — | 1 行汇总（尾部） |
| C14 | 会话级事件失败 | `Stop: bad.sh` exit 1 | 一轮结束 | — | 1 行红 |
| C15 | ASCII 降级终端 | 同 C4 | `Bash(cargo test)` | — | 2 行，无 `?`、不错位 |
| C16 | 详情模式来回切 | 同 C4 | `Ctrl+O` ×2 | 0 行 | 2 行 → 0 行 → 2 行 |
| C17 | Bash 被阻断后重试 | `PreToolUse` 按规则阻断 | `Bash(rm -rf x)` → `Bash(ls)` | 阻断行只跟在被阻断的那次 | 同左 |
| C18 | 同一失败反复发生 | `PreToolUse` 指向不存在的脚本 | 连续 3 次 `Bash(ls)` | — | 只有第一条失败行 |

### 与 Bash 执行结果的组合

hook 行的显示**不依赖工具本身成功与否**，独立叠加：

| Bash 结果 | hook 全成功 | hook 有失败 |
|---|---|---|
| 命令成功（exit 0） | 0 行 / 详细 1 行汇总 | 普通 0 行 / 详细 1 行红 + 汇总 |
| 命令失败（exit ≠ 0） | 0 行 / 详细 1 行汇总 | 普通 0 行 / 详细 1 行红 + 汇总 |
| 命令被 hook 阻断（未执行） | 1 行琥珀，无工具输出 | 1 行琥珀（普通）+ 详细 1 行红 |
| 命令产出大量输出（截断） | 0 行 / 汇总 | 普通 0 行 / 详细 1 行红 + 汇总 |
| 命令转入后台（Ctrl+B） | 0 行 / 汇总 | 普通 0 行 / 详细 1 行红 + 汇总 |

### 具体断言

**渲染层（单元测试，`cc-tui`）**

- `test_hook_summary_hidden_in_normal_mode`：构造带 `hook_rows` 的 `ToolBlock`，`detail_mode = false` 时渲染行数与原工具块一致。
- `test_hook_summary_visible_in_detail_mode`：`detail_mode = true` 时多出恰好 N 行，文案匹配 `2 PostToolUse hooks ran · 34ms`。
- `test_hook_block_row_uses_warning_color`：阻断行的 `fg == theme::cur().warning`，且 `⎿` 同为 warning。
- `test_hook_failure_row_uses_error_color`：失败行 `fg == theme::cur().error`。
- `test_hook_block_row_visible_in_normal_mode`：阻断行在 `detail_mode = false` 下照常渲染。
- `test_hook_failure_row_hidden_in_normal_mode`：失败行 `detail_mode = false` 下 0 行、`true` 下 1 行。
- `test_hook_rows_capped_at_four_events`：5 个事件 → 4 行 + `… +1 events`。
- `test_hook_rows_capped_at_three_failures`：4 个失败 → 3 行 + `… +1 more`（详细模式）。
- `test_hook_rows_have_no_ascii_fallback_gap`：模拟 `prefer_ascii_fallback`，输出不含 `?` 且每行显示宽度 ≤ 视口宽度。
- `test_repeated_hook_failure_renders_single_row`（`agent_ops`）：同一 `(event, kind, reason)` 连发 3 次，工具块上只有 1 条失败行。
- `test_changed_hook_failure_reason_renders_new_row`（`agent_ops`）：reason 变化仍各占一行。

**中间件层（`cc-middlewares`）**

- `test_hook_summary_counts_and_duration`：2 个 hook 各 sleep 10ms，`ran == 2` 且 `duration_ms >= 20`。
- `test_hook_summary_collects_block_reason`：hook exit 2 且输出 JSON `{"decision":"block","reason":"x"}`，`failures[0].kind == Block` 且 `reason == "x"`。
- `test_async_hook_emits_completed_summary`：`async: true` 的 hook 写 marker 文件后退出，事件通道收到 `is_async = true` 的 summary。
- `test_async_hook_failure_emits_error_summary`：异步 hook 退出码 1 时同样上报，`failures` 非空。
- `test_hook_error_reason_picks_last_error_line_from_stack`：Node 崩溃堆栈只取含 `Error:` 的那一行。
- `test_hook_error_reason_falls_back_to_last_non_empty_line`：没有 `Error:` 时取最后一条非空行。
- `test_hook_error_reason_truncates_long_line_to_100_chars`：关键行按字符截断到 100。

**端到端（headless TUI）**

- 用真实 `.claude/settings.json` 配 2 个 `PostToolUse` hook 执行 `echo hook-ok`，跑一次 Bash 工具，断言普通模式快照不含 `hooks ran`、详细模式快照含 `2 PostToolUse hooks ran`。
- 配一个 `PreToolUse` 返回 exit 2 的 hook，执行 `Bash(echo hi)`，断言工具输出不存在、快照含 `blocked` 且颜色为 warning。

## 决策记录

- **只用颜色，不加符号**：文案里已经带 `blocked` / `failed` 语义词，行首再叠 `⚠` / `✗`
  属于重复；颜色负责快速扫视。代价是色盲、黑白截图与 ASCII 降级终端下只能靠文案区分，
  已知并接受。
- **阻断用 warning、失败用 error**：策略性阻断是用户配置正常生效，不是故障，长期标红
  会让用户对红色脱敏。
- **不逐条展开**：hook 成功细节绝大多数时候无人查看，逐条展开会让 TUI 行数随 hook 数量
  线性增长；需要时走 `/hooks` 面板。
- **汇总行只在详细模式**：与参考实现一致，普通模式保持简洁。
- **失败行也不进普通模式 + 同会话去重**：hook 自身失败（脚本不存在、崩溃、超时）是配置
  问题，不是工具执行结果。实测把全局 hooks 指向不存在的 `clawd-hook.js` 时，PreToolUse /
  Notification / PostToolUse 每次工具调用各刷一条、每条 reason 还是整段 Node 堆栈，普通
  模式完全无法阅读。因此失败行只留详细模式，并在会话内按 `(event, kind, reason)` 去重：
  同一失败只报一次，改了错误内容仍会再报；`/clear` 新建 thread 时去重集合随会话清空，
  换了话题后同一条错误会重新提示一次。阻断不受影响——工具真的没跑，必须当场可见。
- **reason 只取最后一条有意义的信息**：优先最后一行含 `Error:` 的文本，否则最后一行
  非空文本，剥 ANSI、折叠空白后截断到 100 字符。整段堆栈折成一行只会把关键行挤到视口外。
- **SessionEnd 不展示**：会话结束（TUI 退出或 `/clear` 切换）没有稳定的渲染时机，
  退出路径推完事件进程即结束，`/clear` 路径也紧接视图重建；其 hook 结果只进日志。
  其余会话级事件（`UserPromptSubmit` / `Stop` / `SessionStart` / `PreCompact`）**展示**，
  以视图尾部的可改写 `⎿` 行呈现、仅详细模式可见，与参考实现一致。
- **异步完成按事件计数、按窗口合并**：同事件的异步完成累加计数，跨事件在 1 秒窗口内
  合并成一行。否则 20 个异步 hook 会刷出 20 行。
- **只读工具与折叠块不展示 hook 行**：`Read` / `Grep` / `Glob` 对用户没有决策价值；
  折叠等于用户主动放弃细节，展开也不再补 hook 行。
- **hook 行强制单行**：超宽截断（`…`）而非折行，保证窄终端下行数可预期。
