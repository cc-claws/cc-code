# 只读简单任务触发过度探索：连续 6+ 次同工具调用（fable 与 opus 均可复现）

**状态**：Open
**优先级**：中
**创建日期**：2026-10-14
**Issue**：[#498](https://github.com/cc-claws/cc-code/issues/498)

---

## 一、问题背景与现场

在 `-p` 非交互模式下，**简单任务会触发明显超出必要的工具调用次数**，且常出现对同一工具的连续多次调用（读不同文件）。该现象在 `claude-fable-5` 与 `claude-opus-4-6` 上**均能复现**，说明不是单一模型的问题。

隔离目录（仅含 `notes.txt` / `data.csv` / `config.json`）实测，`claude-fable-5`：

| 任务 prompt | 实际工具调用序列 | 次数 |
|---|---|---|
| `Print the current working directory using a shell command.` | `Bash, Bash` | 2 |
| `Read notes.txt and tell me how many lines it has.` | `Read, Bash, Bash` | 3 |
| `List all .csv files in the current directory.` | `Glob, Bash` | 2 |
| `Read config.json and show me the value of the key field.` | `Bash, Read` | 2 |
| `Fetch the CSV file from this workspace and look at the local files to understand the context.` | `Bash, Read×7` | **8** |

同一句 "Fetch the CSV…" 重跑三次分别得到 6 / 8 / 10 次调用，其中**连续 6~8 次 `Read`**（读不同文件）。换 `claude-opus-4-6` 跑同一句：`Glob, Read×6`（7 次）。

期望：只读、目标单一的任务应在 1~2 次调用内完成；不应出现连续 6+ 次同工具调用。

## 二、复现条件

- **复现频率**：偶发（同一 prompt 多次运行次数波动，但普遍偏高）
- **触发步骤**：
  1. 建一个只含少量文本文件的目录
  2. `cc-code -p "<只读任务 prompt>" --model fable --output-format stream-json --bare --no-session-persistence --max-turns 12 --permission-mode bypass`
  3. 统计 stdout 中 `"type":"tool_use"` 行数及连续同工具调用
- **环境**：Windows / 网关 `127.0.0.1:20130` / `claude-fable-5` 与 `claude-opus-4-6`

## 三、涉及文件

- `cc-tui/prompts/sections/03_doing_tasks.md` —— 探索/验证行为约束所在段落
- `cc-tui/prompts/sections/05_using_tools.md` —— 工具使用策略段落

## 四、补充观察（熔断器非本问题的解）

同类任务中，三层循环熔断器（`cc-agent/src/agent/executor/tool_dispatch.rs` 的 `ActionLoopDetector` / `ConsecutiveFailureTracker` / `SchemaFailureTracker`）**在全部运行中一次都未触发**——因为上述连续 `Read` 的参数各不相同，不构成"重复动作"。即熔断器按设计工作，本问题不属于其覆盖范围。

## 五、状态变更记录

| 日期 | 从 | 到 | 操作人 | 说明 |
|------|-----|-----|--------|------|
| 2026-10-14 | — | Open | agent | 基于 `-p` 实测创建 Issue #498 |

## 六、修复记录

（由 fix-issue / issue-verify skill 追加，创建时留空）
