> 归档于 2026-06-24，原路径 spec/issues/2026-06-14-tui-scroll-overflow-u16-saturation.md

# 长对话滚动溢出 — 视觉行号 u16 饱和导致 follow-bottom 失效

**状态**：Fixed
**优先级**：高
**创建日期**：2026-06-14

## 问题描述

多轮长对话后，消息区全局视觉行号和滚动 offset 使用 `u16` 保存，累计渲染行超过 65535 后会饱和，导致：

1. follow-bottom 模式停在旧位置，新回复无法继续顶到最新底部
2. 鼠标滚轮滚动被错误边界卡住
3. 文本选择坐标在大行号场景下截断，选区错位

## 症状详情

| 受影响字段 | 旧类型 | 触发阈值 |
|-----------|--------|---------|
| `WrappedLineInfo.visual_row_start` | `u16` | 65535 行 |
| `WrappedLineInfo.visual_row_end` | `u16` | 65535 行 |
| `UiState.scroll_offset` | `u16` | 65535 行 |
| `UiState.scrollbar_min_offset` | `u16` | 65535 行 |
| `UiState.scrollbar_max_offset` | `u16` | 65535 行 |
| `TextSelection.start.0` (visual_row) | `u16` | 65535 行 |
| `TextSelection.end.0` (visual_row) | `u16` | 65535 行 |

辅助函数 `to_u16_saturated()` 在 `message_area.rs` 显式做饱和转换，掩盖了类型设计错误。

## 复现条件

- **复现频率**：必现（达到阈值后）
- **触发步骤**：
  1. 在窄终端（80 列）开启长对话
  2. 累计渲染行超过 65535（典型约 50-100 轮对话）
  3. 发送新消息，观察 follow-bottom 是否能跟到最新底部
- **环境**：所有 OS

## 涉及文件

- `peri-tui/src/ui/render_thread.rs` —— `WrappedLineInfo` 字段类型 + `RenderTask::build_wrap_map_full` / `incremental`
- `peri-tui/src/app/ui_state.rs` —— `UiState` 字段类型
- `peri-tui/src/app/thread_ops.rs` —— `scroll_to_bottom()` 哨兵值
- `peri-tui/src/app/agent_submit.rs` —— submit 哨兵值
- `peri-tui/src/app/text_selection.rs` —— 选区坐标 + `visual_to_logical` / `extract_selected_text`
- `peri-tui/src/event/mod.rs` —— 鼠标坐标转换
- `peri-tui/src/ui/main_ui/message_area.rs` —— 删除 `to_u16_saturated`，消费 `scroll_anchor`
- `peri-tui/src/app/agent_render.rs` —— 类型 cast 清理
- `peri-tui/src/main.rs` —— scrollback history 处理

## 修复内容

PR #15（commit `64f99132`）：

- 所有消息区视觉行号字段统一改为 `usize`
- `usize::MAX` + clamp 替代 `u16::MAX` 哨兵值
- 删除冗余 `to_u16_saturated()` 辅助函数
- 新增 `RebuildWithAnchor` 事件消费链路：`render_thread` 计算锚点消息视觉行号 → 写入 `cache.scroll_anchor` → `message_area` 渲染时消费并 clamp
- `panel_scroll_offset` 保留 `u16`（面板内容不会超 65535 行，合理范围控制）
- `viewport_clip` 中 `local_offset` 仍 `as u16`（单行内偏移不会超 `u16::MAX`，合理）
- 补充大行号场景回归测试（`message_area_test.rs` / `render_thread_test.rs`）

## [TRAP] 经验沉淀

**消息区视觉行号 / scroll offset 必须 `usize`，禁止 `u16`**。

**Why:** TUI 长对话场景下，wrap 后的视觉行号会快速累积（窄终端 + 多轮长回复），65535 是个会被实际触达的阈值。饱和后所有依赖行号的逻辑（follow-bottom、scrollbar、鼠标坐标映射、文本选区）全部失效。

**How to apply:**
- 任何承接 `Paragraph::line_count()` 累加结果的字段必须是 `usize`
- 任何承接用户操作累计（滚动、选区、锚点）的字段必须是 `usize`
- 单行内偏移（`local_offset`）和面板内容滚动可以保留 `u16`（单行 wrap 不会超 65535，面板内容也不会）
- 哨兵值用 `usize::MAX` + 后续 `clamp(min, max)` 模式，**禁止**显式 `to_u16_saturated()` 掩盖类型错误

## 状态变更记录

| 日期 | 从 | 到 | 操作人 | 说明 |
|------|-----|-----|--------|------|
| 2026-06-14 | — | Fixed | agent | PR #15（commit 64f99132）合并到 main，3 平台 CI 全绿 |

## 修复记录

### 修复 #1（2026-06-14）

- **操作人**：wismyzhizi2018（原始修复）+ agent（issue 文档化）
- **用户原意**：长对话场景下 TUI 滚动应该正常工作，不被 65535 行阈值卡死
- **修复内容**：u16 → usize 类型升级 + scroll_anchor 机制 + 删除 `to_u16_saturated`
- **涉及 commit**：`64f99132`（PR #15）
- **验证状态**：已验证（CI 3 平台全绿，回归测试通过）
