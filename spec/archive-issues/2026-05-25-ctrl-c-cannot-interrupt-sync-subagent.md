> 归档于 2026-05-27，原路径 spec/issues/2026-05-25-ctrl-c-cannot-interrupt-sync-subagent.md

# Ctrl+C 无法中断同步 SubAgent，需等待其自然结束后父 Agent 才被中断

**状态**：Fixed
**优先级**：高
**创建日期**：2026-05-25

## 问题描述

当父 Agent 通过 Agent 工具启动同步（非 background）SubAgent 时，用户按 Ctrl+C 无法中断正在执行的 SubAgent。UI 卡住不动，直到 SubAgent 自然执行完毕后，父 Agent 才被中断。中断后会话状态正常（可继续输入）。

期望行为：Ctrl+C 应能中断同步 SubAgent 的执行，立即返回控制权给用户。

## 症状详情

| 维度 | 表现 |
|------|------|
| 触发操作 | 父 Agent 执行同步 SubAgent 期间按 Ctrl+C |
| UI 表现 | 卡住不动，无任何响应 |
| SubAgent 行为 | 继续执行直到自然结束 |
| 中断时机 | SubAgent 结束后，父 Agent 才被中断 |
| 中断后状态 | 会话状态正常，可继续输入 |

## 复现条件

- **复现频率**：必现
- **触发步骤**：
  1. 父 Agent 启动同步 SubAgent（非 background 模式）
  2. SubAgent 开始执行
  3. 按 Ctrl+C
- **期望行为**：Ctrl+C 立即中断 SubAgent 并取消父 Agent 执行

## 涉及文件

- `peri-middlewares/src/subagent/tool/define.rs` — SubAgent 工具定义，包含同步执行（`is_background: false`）和取消逻辑
