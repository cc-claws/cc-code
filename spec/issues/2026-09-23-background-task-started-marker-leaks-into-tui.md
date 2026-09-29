# TUI 工具结果中 `<background-task-started>` 机器标记原样泄露给用户

**状态**：Open
**优先级**：低
**创建日期**：2026-09-23

## 问题描述

在 TUI 会话中通过 Bash 以 `run_in_background` 启动后台任务后，Bash 工具结果卡片里直接显示了一整行机器可读标记：

```
<background-task-started><task-id>01a0cd03-...</task-id><command>gh pr checks 219 ...</command><output>C:\Users\adim\...\tasks\....output</output></background-task-started>
```

用户在界面上看到的应该是友好的展示行（例如「后台任务已启动 (task-id …)」），而不是给 LLM 解析用的原始 XML 标记。该标记是 agent 侧的任务句柄契约，对用户无意义且造成困惑（用户第一反应是"截图里这是啥"）。

## 症状详情

| 项目 | 现象 |
|------|------|
| 出现位置 | Bash 工具结果卡片的输出区域（`MessageViewModel::ToolBlock` 渲染路径） |
| 显示内容 | `<background-task-started>…</background-task-started>` 完整 XML 原文，单行铺满 |
| 期望显示 | 友好展示行（如 `⎿ 后台任务已启动 (task-id …)`），**具体文案格式由作者定稿** |
| 影响 | 纯展示问题，不影响 agent 对 task-id/output 路径的解析，不阻塞功能 |

截图观察：标记行出现在 `Bash(gh pr checks 219 … --watch --interval 30)` 工具结果的首行，紧接着是工具执行摘要。

### 展示行设计要求（需作者注意）

- 修复只处理 UI 展示层；`format_background_task_started` 生成的机器标记是 agent 解析契约（见关联 issue），**严禁改动其格式**。
- 替换后的友好展示行**文案格式需作者定稿**（是否带 ⎿ 前缀、是否展示 output 路径、折叠态如何显示等由作者设计）。

## 复现条件

- **复现频率**：必现（任何转入后台的 Bash 命令）
- **触发步骤**：
  1. 在 TUI 会话中让 agent 执行 Bash 命令并转入后台（`run_in_background=true`，或长命令超时后 Ctrl+B 转后台）
  2. 查看该 Bash 工具结果卡片
- **环境**：Windows（截图环境），peri TUI；OS/平台无关

## 涉及文件

- `peri-middlewares/src/middleware/terminal.rs` —— `format_background_task_started()` 生成 `<background-task-started>` 标记并写入 Bash 工具结果（LLM 契约侧，不改）
- `peri-tui/src/ui/message_render.rs` —— 工具结果展示层（`ToolBlock` 渲染路径），当前对内容仅做控制字符清洗/截断，未识别该标记
- `peri-tui/src/app/tool_display.rs` —— `sanitize_display_text()` 仅剥离 ANSI/控制字符，不含标记剥离
- 参考先例：`peri-tui/src/app/background_shell.rs` 渲染后台任务通知时会剥离 `<system-reminder>` 包裹，可作为「机器标记 ≠ 展示文本」的处理模式参考

## 关联

- `spec/issues/2026-09-23-background-task-handle-repolling-and-wait-contract.md` —— 定义了 `<background-task-started>` 标记作为 agent 任务句柄契约
- `spec/issues/2026-09-20-bash-timeout-and-background-task-notification-alignment.md` —— 后台任务通知对齐（不同问题，同域）

## 状态变更记录

| 日期 | 从 | 到 | 操作人 | 说明 |
|------|-----|-----|--------|------|
| 2026-09-23 | — | Open | agent | 创建 |

## 修复记录

（由 fix-issue 或 issue-verify skill 追加，创建时留空）
