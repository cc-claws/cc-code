# 后台 SubAgent 状态栏运行期间 calls 计数恒为 0

**状态**：Open
**优先级**：低
**类型**：Bug
**创建日期**：2026-09-29

## 问题描述

后台子代理运行期间，状态栏（bg_agent_bar）的 calls 计数一直显示 `0 calls`，而旁边的耗时计时正常走动。子代理结束后计数显示正确。用户在等待两个并行子代理完成时观察到此现象（如 `● general-purpose  0 calls  6m12s`），无法从状态栏感知子代理的实际工作进度。

## 症状详情

| 维度 | 表现 |
|------|------|
| 运行中 | `calls` 恒显示 `0 calls`，持续整个运行周期（观察样本 6m12s+） |
| 耗时 | `elapsed` 实时走动，正常 |
| 结束后 | calls 显示正确步数（用户确认） |
| 影响 | 状态栏本应展示实时进度（工具调用次数），运行期间信息缺失，用户无法感知子代理是否在工作 |
| 复现频率 | 必现（本次两个并行 general-purpose 子代理均如此） |

### 用户可见输出

```
● general-purpose  0 calls  6m12s
● general-purpose  0 calls  6m12s
```

（实际为运行中的两个后台子代理条目，calls 全程为 0）

## 复现条件

- **复现频率**：必现
- **触发步骤**：
  1. 启动一个后台 SubAgent（如 `Agent(general-purpose, run_in_background: true)`）
  2. 观察状态栏 bg_agent_bar 中该 agent 条目
  3. 运行全程 calls 显示 0，耗时正常递增
  4. 子代理完成后 calls 显示正确值
- **环境**：Windows，chore/test-naming 分支工作会话（2026-09-29），peri TUI

## 涉及文件

- `peri-tui/src/ui/main_ui/bg_agent_bar.rs` —— 状态栏子代理条目渲染处（calls 字段展示位置）
- `spec/issues/2026-05-26-bg-agent-message-flow-broken.md` —— 关联 issue（同模块 bg_agent_bar 显示问题，含 total_steps 过时现象，Open）

## 期望行为

子代理运行期间，状态栏 calls 应随工具调用实时（或准实时）增长，让用户能感知执行进度；结束后为最终步数。

## 状态变更记录

| 日期 | 从 | 到 | 操作人 | 说明 |
|------|-----|-----|--------|------|
| 2026-09-29 | — | Open | agent | 创建 |

## 修复记录

（由 fix-issue 或 issue-verify skill 追加，创建时留空）
