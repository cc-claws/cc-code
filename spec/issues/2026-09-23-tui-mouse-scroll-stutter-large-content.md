# TUI 在内容量大时长滚动卡顿与不丝滑 (Mouse Scroll Stutter on Large Content)

**状态**：Open
**优先级**：中
**创建日期**：2026-09-23
**GitHub Issue**：#230 (https://github.com/cc-claws/cc-code/issues/230)

## 问题描述

在 TUI 会话内容较多（长对话轮次、包含大段代码或命令长输出）时，用户使用鼠标滚轮进行上下滚动，界面呈现明显的掉帧、顿挫与粘滞感（手已停下但画面还在一格一格跳动下移），缺乏平滑流动的操作体验。而在内容很少（仅一两屏）的简短会话中，该问题并不明显。

## 症状详情

1. **高频滚轮下的输入延迟与顿挫**：快速拨动鼠标滚轮时，画面没有即时跟手移动，而是出现明显的跳帧卡顿，甚至在手指停止滚动后，画面仍有半秒左右的滞后逐帧补移。
2. **长内容下的极度迟滞感**：单次滚轮固定只滚动 3 行，在几千行的长消息或大输出中，滚动反馈微弱，促使用户更加剧烈地快速滑动滚轮，导致界面卡死或掉帧加剧。
3. **滚动条拖拽跳跃（相关观察）**：当内容达到数千行时，鼠标拖拽右侧滚动条滑块（Scrollbar Drag）在纵向上移动 1 个字符行就会导致视口偏移瞬移上百行，视觉上出现大幅度抽搐跳变。

## 出现场景

- 会话经过多轮交互积累了较多历史消息（`total_lines` 达数千行以上）。
- 执行了产生大量输出的终端命令（如 `cargo build`、大文件 `cat`、长日志输出），展开内容极长。
- 平台环境：Windows 终端（Windows Terminal / ConPTY 环境尤其显著）。

## 复现条件

- **复现频率**：长内容场景下 100% 必现
- **触发步骤**：
  1. 在 TUI 中进行多轮问答，或执行产生数百至上千行输出的命令。
  2. 使用鼠标滚轮（特别是平滑滚轮或触控板）快速上下滑动浏览历史消息。
  3. 观察消息区域滚动的流畅度与跟手响应速度。
- **环境**：
  - OS: Windows (ConPTY)
  - Terminal: Windows Terminal / VS Code 内置终端

## 涉及文件

- `peri-tui/src/event/mod.rs` —— 鼠标事件接收与处理入口（原记录指向 `coalesce_drag_events`，该函数已随事件管线重构移除；当前对应实现为 `event/mouse_batch.rs` 的 `collect_mouse_batch` 与 `EventReader`，见「2026-09-23 复核」）
- `peri-tui/src/ui/main_ui/message_area.rs` —— 消息区域渲染与视口裁剪（每帧重绘独占抢占 `render_cache.write()` 锁，且通过 Paragraph 二次 wrap）
- `peri-tui/src/app/thread_ops.rs` —— 滚动步长控制（`scroll_up` / `scroll_down` 步长固定为 3 行，缺乏动态加速度）
- `peri-tui/src/main.rs` —— 主事件循环（`event::Action::Redraw` 缺乏帧率上限与垂直同步限制）

## 2026-09-23 复核

对「涉及文件」逐条核对当前代码：

| 条目 | 复核结果 |
|------|----------|
| `event/mod.rs` 的 `coalesce_drag_events` | ❌ 该函数在当前代码中**已不存在**（全仓库 grep 无命中）。事件管线已重构为 `event/input_pump.rs`（`InputPump`：独立读线程 + 相邻 `Moved` 合并）与 `event/mouse_batch.rs`（`collect_mouse_batch`：`MAX_MOUSE_BATCH = 128`，连续滚轮/拖拽合并） |
| `message_area.rs` 的 `render_cache.write()` 独占锁 + Paragraph 二次 wrap | ✅ 仍成立 |
| `thread_ops.rs` 的 `scroll_up` / `scroll_down` 步长固定 3 行 | ✅ 仍成立（`thread_ops.rs:13,26`） |
| `main.rs` 的 `Action::Redraw` 缺帧率上限与垂直同步 | ✅ 仍成立（未见帧率/垂直同步控制） |

## 关联

- 症状 3「滚动条拖拽跳跃」的交互层缺陷（点击跳转与拖拽换算分母不一致、滑块缩为 1 格、hover 无轨道等）已单独建档：`spec/issues/2026-09-23-tui-scrollbar-drag-and-hover-defects.md`，与本 issue 的滚轮路径分开跟踪。
- 「涉及文件」第 1 条归因（`coalesce_drag_events`）已失效，滚轮重绘路径需按新的 `InputPump` / `collect_mouse_batch` 重新定位。
- 本复核内容已同步为 #230 的评论（https://github.com/cc-claws/cc-code/issues/230）。

## 状态变更记录

| 日期 | 从 | 到 | 操作人 | 说明 |
|------|-----|-----|--------|------|
| 2026-09-23 | — | Open | agent | 创建 issue |
| 2026-09-23 | Open | Open | agent | 复核「涉及文件」：`coalesce_drag_events` 已不存在，其余 3 条仍成立；关联新建的滚动条交互缺陷 issue |

## 修复记录

（由 fix-issue 或 issue-verify skill 追加，创建时留空）
