# Login 面板切换 provider 后当前 session 仍使用旧模型

**状态**：Open
**优先级**：中
**创建日期**：2026-05-31

## 问题描述

在 TUI 中通过 `/login` 打开 Login 面板并选中（激活）一个不同的 provider 后，界面状态栏显示已切换到新 provider，但当前 session 继续对话时，agent 实际使用的仍是切换前的旧模型。

## 症状详情

| 现象 | 详情 |
|------|------|
| 界面显示已切换 | 状态栏中 provider name / model name 已更新为新选中的 provider |
| 实际对话未切换 | 当前 session 继续发送消息，agent 回复的风格/能力仍是旧模型的特征 |
| 无错误提示 | 切换过程没有弹出配置保存失败或 provider 无效的错误 |
| agent idle 时切换 | 切换时当前 session 没有在执行 agent（无 pending 的 LLM 调用或工具执行） |

## 复现条件

- **复现频率**：稳定复现
- **触发步骤**：
  1. 在当前 session 中与 agent 对话若干轮
  2. 输入 `/login` 打开 Login 面板
  3. 用方向键选中另一个 provider，按 Enter 激活
  4. 面板关闭，状态栏显示新 provider 名称
  5. 在当前 session 中继续发送消息
  6. 观察：agent 回复仍是旧模型的风格/能力

## 涉及文件

- `peri-tui/src/app/panel_login.rs` — `login_panel_select_provider()`，处理 provider 选中、本地 provider 更新、异步通知 ACP Server `update_config`
