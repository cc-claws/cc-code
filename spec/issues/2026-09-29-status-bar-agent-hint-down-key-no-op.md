# 状态栏「1 agent ↓ to view」提示但按 ↓ 无反应

**状态**：Open
**优先级**：中
**类型**：Bug
**创建日期**：2026-09-29

## 问题描述

状态栏显示 `1 agent ↓ to view` 提示（后台 SubAgent 运行中），但按下 `↓` 方向键没有任何反应——既没有聚焦后台任务栏，界面也没有任何变化。提示承诺的交互（↓ 查看 agent）不生效，用户无法通过该入口查看运行中的后台 agent。

## 症状详情

| 维度 | 表现 |
|------|------|
| 状态栏提示 | `1 agent ↓ to view`（黄色 pill + MUTED 提示语）正常显示 |
| 按下 ↓ | 无反应（无聚焦、无界面变化） |
| 当前任务构成 | 1 个运行中的后台 **agent**（general-purpose），0 个后台 shell |
| 对比观察 | 提示条件与按键处理条件疑似不一致：有 agent 无 shell 时提示出现但按键不触发（`status_bar.rs` 计入 agent 数显示提示；`normal_keys.rs` 的 Down 处理只认后台 shell 任务） |
| 复现频率 | 必现（本次会话 1 agent / 0 shell 场景下） |

### 用户可见输出

```
Bypass (Shift+Tab to cycle) · 1 agent ↓ to view  Cache 0% · MEM 87MB
main
general-purpose    0calls 8m54s
```

（提示出现，按 `↓` 无任何响应）

## 复现条件

- **复现频率**：必现（agent-only 场景）
- **触发步骤**：
  1. 启动一个后台 SubAgent（`run_in_background: true`），确保**没有**运行中的后台 shell
  2. 状态栏出现 `1 agent ↓ to view`
  3. 按 `↓` 方向键
  4. 预期：进入/聚焦后台任务栏查看 agent；实际：无反应
- **环境**：Windows，chore/test-naming 分支工作会话（2026-09-29），peri TUI

## 涉及文件

- `peri-tui/src/ui/main_ui/status_bar.rs` —— `↓ to view` 提示渲染处（计数含 agent）
- `peri-tui/src/event/keyboard/normal_keys.rs` —— `↓` 按键处理处（判定条件疑似只认后台 shell）

## 期望行为

状态栏出现 `N agent ↓ to view` 提示时，按 `↓` 应聚焦后台任务栏/agent 入口；提示与按键行为一致。

## 状态变更记录

| 日期 | 从 | 到 | 操作人 | 说明 |
|------|-----|-----|--------|------|
| 2026-09-29 | — | Open | agent | 创建 |

## 修复记录

（由 fix-issue 或 issue-verify skill 追加，创建时留空）
