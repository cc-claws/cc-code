> 归档于 2026-05-24，原路径 spec/issues/2026-05-23-ask-user-overflow-and-description-missing.md

# AskUser 弹窗内容溢出不可滚动且选项描述丢失

**状态**：Fixed
**优先级**：中
**创建日期**：2026-05-23
**修复日期**：2026-05-23

## 问题描述

AskUser 批量弹窗在选项多或文字长时，计算出的面板高度不足以显示全部内容，出现滚动条但无法交互（键盘/鼠标均无响应）。同时，通过 ACP Elicitation 协议传入的选项 `description` 字段在 TUI 解析时被丢弃，始终为 `None`，导致选项说明文字不显示。

## 症状详情

| 问题 | 现象 | 期望 |
|------|------|------|
| 面板高度不足 | 选项多/文字长时内容被截断，出现滚动条 | 弹窗高度能完整显示所有内容，或滚动条可交互 |
| 滚动条不可交互 | 出现滚动条但键盘滚动/鼠标点击均无效 | 可通过键盘/鼠标操作滚动条查看完整内容 |
| 选项描述丢失 | 每个选项的��明文字不显示 | 选项下方显示灰色说明文字 |

### 复现条件

- **复现频率**：必现
- **触发步骤**：
  1. Agent 调用 `AskUserQuestion` 工具，传入 4 个选项且每个选项带 `description`
  2. TUI 弹出 AskUser 弹窗
  3. 观察选项说明文字不显示、面板内容溢出
- **环境**：TUI 模式，通过 ACP Elicitation 路径触发

## 涉及文件

- `peri-tui/src/ui/main_ui/mod.rs:310-376` —— `active_panel_height` 函数：面板高度计算逻辑，AskUser 最大 60% 屏幕高度，选项多时不够
- `peri-tui/src/ui/main_ui/popups/ask_user.rs:176-181` —— `ScrollableArea` 渲染滚动条但未绑定交互事件（仅渲染 `ScrollState`，无键盘/鼠标处理）
- `peri-tui/src/app/agent_ops_interaction.rs:86-88,102-104` —— `handle_acp_elicitation` 中构建 `AskUserOption` 时 `description: None` 硬编码，未从 Elicitation JSON 的 `oneOf`/`anyOf` 数组中读取注入的 `description` 字段
- `peri-acp/src/broker/transport_broker.rs:258-299` —— `inject_option_descriptions` 函数：已在 JSON 层面注入 `description`，但 TUI 反序列化时未读取

## 修复记录

| 子问题 | 修复 | Commit |
|--------|------|--------|
| 选项 description 丢失 | 添加 `extract_option_descriptions` 函数，从原始 JSON 提取被 `EnumOption` 丢弃的 description；修复 JSON 路径（`requestedSchema` 在顶层而非 `mode` 下） | `4687d1e` `1518ccb` |
| 面板高度不足 | AskUser 弹窗最大高度从 60% 提升到 75% | `e8ab325` |
| 滚动条不可交互（键盘） | 添加 `ask_user_scroll` 方法 + Ctrl+U/Ctrl+D 绑定，存储 `ScrollbarMetrics` 到 prompt 状态 | `419f1aa` |
| 滚动条不可交互（鼠标） | 滚轮 ±3 行滚动 + 点击滚动条按比例跳转 | `8ef4c8a` |
| 滚动仍不生效（二次修复） | `panel_area` 未设置导致鼠标事件无法路由；`option_cursor`（选项索引）被当作渲染行号使用导致滚动偏移错误；新增 `option_row_map` 在渲染时追踪真实行号 | `129428f` |
