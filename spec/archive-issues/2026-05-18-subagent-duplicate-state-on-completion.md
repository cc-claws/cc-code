> 归档于 2026-05-20，原路径 spec/issues/2026-05-18-subagent-duplicate-state-on-completion.md

# SubAgent 完成后显示重复卡片 + 状态切换闪烁

**状态**：Fixed
**优先级**：高
**创建日期**：2026-05-18
**修复提交**：cd7977d

## 问题描述

父 Agent 调用 Agent 工具后，SubAgent 在运行中→完成的切换过程中出现 UI 闪烁，完成后 `SubAgentGroup` 卡片���现重复——一个显示为已完成（frozen），另一个显示为运行中（running），直到父 Agent Done 或新轮次开始才消失。

## 症状详情

| 阶段 | 用户看到的现象 |
|------|----------------|
| SubAgent 运行中 | 正常显示一个运行中的 SubAgentGroup 卡片 |
| SubAgentEnd 到达 | 卡片瞬间变为已完成，同时旁边出现一个**新的运行中卡片**（内容为空或重复） |
| StateSnapshot 到达 | 两个卡片同时显示，一个有 final_result，一个显示运行中 |
| 父 Agent Done | 重复卡片消失，只保留正确的已完成卡片 |

**视觉表现**：运行中→完成切换时短暂闪烁（重复卡片闪现），多个并发 SubAgent 完成时尤其明显。

## 复现条件

- **复现频率**：必现
- **触发步骤**：
  1. 父 Agent 调用 Agent 工具（单个或多个并发）
  2. SubAgent 执行并完成
  3. 观察完成瞬间的 UI 显示
- **影响版本**：自并发 SubAgent 恢复（6de639b）后至 cd7977d

## 涉及文件

- `peri-tui/src/app/message_pipeline/mod.rs` —— ToolStart/SubAgentStart 事件处理与 SubAgentState 创建逻辑

## 修复说明

ToolStart 事件（name="Agent", source_agent_id=None）和 SubAgentStart 事件都调用了 `tool_start_internal()`，为同一个 SubAgent 创建了两个 `SubAgentState` 条目。SubAgentEnd 只冻结第一个匹配项，第二个残留为 is_running=true。

修复：ToolStart 在 name="Agent" 时不创建 SubAgentState，仅注册 tool_call 和 pending_tool，由 SubAgentStart 事件独占 SubAgentState 创建职责。

## 回归测试

`test_no_duplicate_subagent_state_on_tool_start_plus_subagent_start` — 验证 ToolStart(Agent) + SubAgentStart 序列只产生一个 SubAgentState。
