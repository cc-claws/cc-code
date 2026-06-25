> 归档于 2026-05-27，原路径 spec/issues/2026-05-25-compact-resubmit-missing-loading-spinner.md

# Compact 后 Resubmit 缺少 Loading Spinner

**状态**：Reopen
**优先级**：中
**创建日期**：2026-05-25
**Reopen 日期**：2026-05-30

## 问题描述

自动 compact（上下文超阈值触发）完成后，agent 自动 resubmit 继续执行，但 TUI 的 loading spinner 没有显示。界面上的 agent 输出（工具调用、文本回复）正常更新，用户能通过内容变化判断 agent 在工作，但缺少 spinner 导致无法从状态栏直观感知"agent 正在执行"。

## 症状详情

| 维度 | 表现 |
|------|------|
| 触发条件 | 自动 compact（上下文超阈值） |
| 复现频率 | 必现 |
| loading spinner | 不显示 |
| agent 输出 | 正常更新（工具调用、文本等可见） |
| agent 执行 | 正常继续，未中断 |

对比正常场景：非 compact 的 agent 执行期间，status bar 始终显示 loading spinner。

## 复现条件

- **复现频率**：必现
- **触发步骤**：
  1. 开始一个会话，持续对话直到上下文超过 compact 阈值
  2. 触发自动 compact（full compact）
  3. compact 完成后 agent 自动 resubmit
  4. 观察 resubmit 期间的 UI 状态——spinner 缺失，但内容在更新
- **环境**：TUI 模式

## 涉及文件

- `peri-tui/src/app/agent_compact.rs` —— compact 生命周期处理（`handle_compact_started`/`handle_compact_completed`），compact 完成时调用 `set_loading(false)`
- `peri-tui/src/app/agent_ops/lifecycle.rs` —— agent 生命周期处理，cleanup 时 `set_loading(false)`
- `peri-acp/src/session/executor.rs` —— auto-compact 循环：执行后检查阈值 → compact → resubmit

## 现象 2（Reopen 2026-05-30）

用户确认该问题为回归——之前已修复，近期又出现。症状与原始报告完全一致：

- 触发条件：自动 compact（非手动 `/compact`）
- 表现：compact 后 resubmit 期间 status bar spinner 消失，agent 输出正常可见
- 复现频率：必现
- 之前修复状态：已归档（Closed），本次 Reopen

## 关联 Issue

- `spec/archive-issues/2026-05-26-manual-compact-long-loading-skeleton.md` — 手动 compact 的相反问题（loading 不消失），涉及相同的 loading 状态管理逻辑
