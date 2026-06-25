> 归档于 2026-05-27，原路径 spec/issues/2026-05-27-message-area-scrollbar-thumb-misaligned.md

# 消息区域滚动条滑块位置与鼠标可拖拽位置不对齐

**状态**：Fixed
**优先级**：中
**创建日期**：2026-05-27

## 问题描述

消息区域（主聊天列表）的滚动条，在内容较长时，滑块的视觉渲染位置与鼠标实际能抓住并拖拽的位置不一致。用户需要点击滑块旁边的区域才能开始拖拽，而非直接点击滑块本身。整体不对齐，没有固定的偏移方向。

## 症状详情

| 条件 | 表现 |
|------|------|
| 内容较短 | 未观察到问题 |
| 内容较长（滑块变小） | 滑块视觉位置和鼠标可交互位置明显错位 |

**期望行为**：鼠标点击滑块视觉渲染位置应能直接开始拖拽。
**实际行为**：鼠标点击滑块位置无法触发拖拽，需要偏移到其他位置才能抓住。

## 复现条件

- **复现频率**：必现（内容较长时）
- **触发步骤**：
  1. 进行较长的对话，使消息列表超出可视区域
  2. 观察右侧滚动条的滑块位置
  3. 尝试用鼠标点击滑块进行拖拽
- **环境**：macOS

## 涉及文件

- `peri-tui/src/ui/main_ui/message_area.rs` —— 滚动条渲染（`ScrollbarState` 配置与 widget 渲染）
- `peri-tui/src/event/mod.rs` —— 鼠标点击/拖拽事件处理（scrollbar_col 判断与 offset 计算）
- `peri-widgets/src/scrollable.rs` —— `unified_vertical_scrollbar()` 滚动条构造器

## 根因

鼠标事件处理器使用简单线性公式 `offset = rel_y × max_scroll / (height - 1)`，与 ratatui `Scrollbar::part_lengths()` 的 thumb 定位公式 `thumb_start = position × track / (content_length - 1 + viewport)` 不一致。此外，鼠标处理器未考虑 thumb 可变长度——点击 thumb 中部/底部时 thumb 会跳动。

## 修复

1. 新增 `scrollbar_thumb_geometry()` 复刻 ratatui 的 thumb 位置/长度计算
2. 新增 `scrollbar_thumb_start_to_offset()` 实现 ratatui 公式的逆运算
3. 点击处理区分「点击在 thumb 上」和「点击在 track 上」：
   - thumb 上 → 记录 `drag_y_offset`（鼠标相对于 thumb 顶部的偏移），拖拽时 thumb 不跳动
   - track 上 → thumb 中心跳到点击位置
4. 拖拽处理使用 `drag_y_offset` 计算 `desired_thumb_start`，通过逆公式得到 scroll offset
5. `UiState` 新增 `scrollbar_drag_y_offset` 字段
