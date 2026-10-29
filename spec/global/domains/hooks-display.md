# Hook 执行展示 PRD

## 背景

当前 hook 执行完全静默：同步 hook 跑完没有任何提示，异步 hook（`async: true`）在
`cc-middlewares/src/hooks/middleware.rs` 里 fire-and-forget，结果直接丢弃。用户配了
hook 却无法确认它是否执行、是否失败，只能去翻日志。

目标：在不制造噪音的前提下，让用户按需看到 hook 执行情况——**默认静默，异常必现，
详情只在详细模式出现**。

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
| hook 执行失败（崩溃、超时、非法输出） | 1 行，红色 |
| 多个失败 | 逐条显示，最多 3 条，其余折叠为 `… +N more` |
| 异步 hook 成功 | 不显示 |
| 异步 hook 失败 | 1 行，红色 |

### 详细模式（Ctrl+O 打开）

在普通模式规则之上追加：

| 情况 | 行为 |
|---|---|
| 每触发一个同步事件 | 1 行：`N <Event> hooks ran · Xms` |
| 同步事件超过 4 个 | 前 4 个各 1 行，其余折叠为 `… +N events` |
| 每个异步 hook 完成 | 1 行：`Async hook <Event> completed` |

**行数恒定**：hook 数量不影响行数（10 个 hook 分属 2 个事件仍然只占 2 行）。

## 渲染规格

| 行类型 | 模板 | 颜色 |
|---|---|---|
| 同步汇总 | `⎿  {n} {event} hooks ran · {ms}ms` | `theme::cur().inactive` |
| 异步开始（占位） | `⎿  Async hook {event} running…` | `theme::cur().inactive` |
| 异步完成 | `⎿  Async hook {event} completed` | `theme::cur().inactive` |
| 阻断 | `⎿  {event} hook blocked: {reason}` | `theme::cur().warning` |
| 失败 | `⎿  {event} hook failed: {reason}` | `theme::cur().error` |
| 折叠 | `⎿  … +{n} events` / `… +{n} more` | `theme::cur().subtle` |

- 行首不加 `⚠` / `✗`：语义由文案里的 `blocked` / `failed` 承担，颜色负责扫视。
  代价是色盲、黑白截图与 ASCII 降级终端只剩文案可辨——这是刻意的取舍。

- 分支前缀复用 `BranchPrefix::PREFIX`（`  ⎿  `，5 列），与工具结果行同列。
- 阻断/失败行的 `⎿` 本身也随状态色（`BranchPrefix::span(is_error)`）。

- **异步占位行的来龙去脉**：异步 hook 完成时，工具块可能早已进入历史前缀，结果行
  就贴不上了。因此异步 hook 在 `tokio::spawn` **之前**先上报 `HooksStarted`，TUI 立刻
  写一条 `Async hook {event} running…` 占位；完成事件到达时按 `(tool_call_id, event)`
  原地替换为 `completed` 或失败行。轮次结束（`MessagePipeline::done()` /
  `interrupt()`）时仍为 Running 的占位被丢弃，不留僵尸 `running…`。

## 挂载位置

- **工具相关事件**（`PreToolUse` / `PostToolUse` / `PostToolUseFailure`）：挂在触发它的
  `ToolBlock` 内部，紧随工具输出行之后，随工具块一起折叠与滚动。
- **会话级事件**（`UserPromptSubmit` / `Stop` / `SessionStart` / `SessionEnd` /
  `PreCompact` / `SubagentStart` …）：**不在 TUI 展示**。它们与工具块无关，也没有
  稳定的展示时机（`SessionEnd` 尤其如此），结果只进日志。

## 数据契约

### 中间件采集（cc-middlewares）

每次 `run_hooks(event, ...)` 结束后产出：

```
HookRunSummary {
    event: HookEvent,        // PreToolUse / PostToolUse / ...
    ran: usize,              // 成功执行的 hook 数
    duration_ms: u64,        // 该事件所有同步 hook 的总耗时
    failures: Vec<HookFailure>,  // 失败/阻断清单（普通模式渲染用）
    is_async: bool,          // 异步完成通知
}
HookFailure { kind: Block|Error, reason: String }
```

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
- 阻断/失败行：两种模式都渲染。
- 不写入 agent history，不参与 compact 统计。

## 分期

**P0（本次）**：事件计数 + 总耗时汇总、异步完成提示、阻断/失败提示。

失败行依赖 `HookAction::Error { reason }`（P0 一并补上）：非预期退出码、超时、被信号
终止、无法启动都归为「hook 失败但**不阻断**调用」，由中间件转成 `kind = "error"` 的
失败项。阻断（exit 2 / decision=block）仍走 `Block { reason }`。

**P1（后续）**：每条 hook 的命令名、逐条耗时与 stderr 明细，并在 `/hooks` 面板展示
最近一次执行记录。

## 验收标准

1. 普通模式下所有 hook 成功 → 屏幕上不出现任何 hook 相关行。
2. 一个 Bash 调用触发 2 个 `PreToolUse` + 2 个 `PostToolUse` hook，详细模式恰好显示 2 行汇总，普通模式 0 行。
3. 异步 hook 完成后，详细模式出现 `Async hook <Event> completed`。
4. hook 阻断时，普通模式出现琥珀色的 `… blocked:` 行，工具未执行。
5. hook 崩溃/超时时，普通模式出现红色的 `… failed:` 行。
6. 触发 8 个事件时，详细模式只显示 4 行 + `… +4 events`。
7. 所有新增行在 ASCII 降级终端下不出现 `?`，行宽不错位。

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

**普通模式**

```
● Bash(cargo test)
  ⎿  test result: ok. 12 passed; 0 failed
  ⎿  PostToolUse hook failed: exit 1                  [error]
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

### C11（已废弃）

TUI 只展示工具级事件，所有 hook 行都挂在各自的 `ToolBlock` 内，不存在「跨事件合并成
一轮预算」的场景，本节不再适用。

### C12 四个 hook 失败（普通模式折叠）

```
● Bash(cargo test)
  ⎿  test result: ok. 12 passed; 0 failed
  ⎿  PreToolUse hook failed: exit 1                   [error]
  ⎿  PostToolUse hook failed: exit 1                  [error]
  ⎿  PostToolUse hook failed: exit 1                  [error]
  ⎿  … +1 more                                        [subtle]
```

### C13 / C14（已废弃）

会话级事件不在 TUI 展示，故「非工具事件成功/失败」的渲染形态不存在。

### C15 ASCII 降级终端（`prefer_ascii_fallback = true`）

详细模式，`⎿` 本身降级为 `+`（兼容层既有映射），列宽与 C4 一致：

```
+ Bash(cargo test)
  + test result: ok. 12 passed; 0 failed
  + 2 PreToolUse hooks ran · 12ms
  + 2 PostToolUse hooks ran · 34ms
```

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
| C6 | PostToolUse 失败 | `PostToolUse: bad.sh` exit 1 | `Bash(cargo test)` | 1 行红 | 1 行红 + 1 行汇总 |
| C7 | hook 超时 | 命令 `sleep 120` | `Bash(ls)` | 1 行红 failed: timeout | 同左 + 汇总 |
| C8 | hook 输出非法 JSON | 打印 `{oops` | `Bash(ls)` | 1 行红 failed: invalid output | 同左 + 汇总 |
| C9 | 异步 hook 成功 | `PostToolUse: async:true` | `Bash(ls)` | — | 1 行 `Async hook … completed` |
| C10 | 异步 hook 失败 | `async:true`，退出码 1 | `Bash(ls)` | 1 行红 | 1 行红 + 1 行完成提示 |
| C11 | ~~事件超过 4 个~~ | — | — | — | 已废弃（仅工具事件，无跨事件折叠） |
| C12 | 多个失败 | 4 个 hook 全部失败 | `Bash(ls)` | 3 行红 + `… +1 more` | 同左 + 汇总 |
| C13 | ~~非工具事件成功~~ | — | — | — | 已废弃（会话级不展示） |
| C14 | ~~非工具事件失败~~ | — | — | — | 已废弃（会话级不展示） |
| C15 | ASCII 降级终端 | 同 C4 | `Bash(cargo test)` | — | 2 行，无 `?`、不错位 |
| C16 | 详情模式来回切 | 同 C4 | `Ctrl+O` ×2 | 0 行 | 2 行 → 0 行 → 2 行 |
| C17 | Bash 被阻断后重试 | `PreToolUse` 按规则阻断 | `Bash(rm -rf x)` → `Bash(ls)` | 阻断行只跟在被阻断的那次 | 同左 |

### 与 Bash 执行结果的组合

hook 行的显示**不依赖工具本身成功与否**，独立叠加：

| Bash 结果 | hook 全成功 | hook 有失败 |
|---|---|---|
| 命令成功（exit 0） | 0 行 / 详细 1 行汇总 | 1 行红 + 汇总 |
| 命令失败（exit ≠ 0） | 0 行 / 详细 1 行汇总 | 1 行红 + 汇总 |
| 命令被 hook 阻断（未执行） | 1 行琥珀，无工具输出 | 1 行琥珀 + 1 行红 |
| 命令产出大量输出（截断） | 0 行 / 汇总 | 1 行红 + 汇总 |
| 命令转入后台（Ctrl+B） | 0 行 / 汇总 | 1 行红 + 汇总 |

### 具体断言

**渲染层（单元测试，`cc-tui`）**

- `test_hook_summary_hidden_in_normal_mode`：构造带 `hook_rows` 的 `ToolBlock`，`detail_mode = false` 时渲染行数与原工具块一致。
- `test_hook_summary_visible_in_detail_mode`：`detail_mode = true` 时多出恰好 N 行，文案匹配 `2 PostToolUse hooks ran · 34ms`。
- `test_hook_block_row_uses_warning_color`：阻断行的 `fg == theme::cur().warning`，且 `⎿` 同为 warning。
- `test_hook_failure_row_uses_error_color`：失败行 `fg == theme::cur().error`。
- `test_hook_rows_capped_at_four_events`：5 个事件 → 4 行 + `… +1 events`。
- `test_hook_rows_capped_at_three_failures`：4 个失败 → 3 行 + `… +1 more`。
- `test_hook_rows_have_no_ascii_fallback_gap`：模拟 `prefer_ascii_fallback`，输出不含 `?` 且每行显示宽度 ≤ 视口宽度。

**中间件层（`cc-middlewares`）**

- `test_hook_summary_counts_and_duration`：2 个 hook 各 sleep 10ms，`ran == 2` 且 `duration_ms >= 20`。
- `test_hook_summary_collects_block_reason`：hook exit 2 且输出 JSON `{"decision":"block","reason":"x"}`，`failures[0].kind == Block` 且 `reason == "x"`。
- `test_async_hook_emits_completed_summary`：`async: true` 的 hook 写 marker 文件后退出，事件通道收到 `is_async = true` 的 summary。
- `test_async_hook_failure_emits_error_summary`：异步 hook 退出码 1 时同样上报，`failures` 非空。

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
- **SessionEnd 不展示**：会话结束（TUI 退出或 `/clear` 切换）没有稳定的渲染时机，
  退出路径推完事件进程即结束，`/clear` 路径也紧接视图重建；其 hook 结果只进日志。
  其余会话级事件（`UserPromptSubmit` / `Stop` / `SessionStart` / `PreCompact`）照常展示。
