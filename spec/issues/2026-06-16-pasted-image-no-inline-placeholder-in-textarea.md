# 粘贴图片在 textarea 中无内嵌原子占位符

**状态**：Open
**优先级**：中
**创建日期**：2026-06-16

## 问题描述

Peri TUI 粘贴图片后，图片只挂到输入框上方的独立 Attachment Bar，**textarea 内部无任何可见标记**。用户无法：

1. 在文本中混排图片：`look at this [Image #1] and compare with [Image #2]`
2. 控制图片在 prompt 中的位置（图片总是统一发到末尾）
3. 在 textarea 中直接 Backspace 删除最后一张图片（必须用 Del 键走 Attachment Bar 删除路径）

Codex 的做法：粘贴时在 textarea **当前光标位置** 插入原子 placeholder `[Image #1]`，作为 textarea 的 atomic text element（不可编辑中间字符、光标整体跳过、可整体 Backspace、可重新编号）。

## 症状详情

| 场景 | 当前 Peri 行为 | Codex 行为 |
|------|---------------|-----------|
| Ctrl+V 粘贴一张图片 | textarea 无变化，Attachment Bar 多一个 `[img clipboard_1.png 12KB]` | textarea 光标处插入 `[Image #1]`（cyan + reversed 高亮），Attachment Bar 同步显示 |
| 文本中混排多张图片 | 不支持——所有图片统一附加在 prompt 末尾 | 完全支持：`look [Image #1] vs [Image #2]` |
| textarea 中删除图片 | 只能按 Del 删除 Attachment Bar 末尾，不能在文本中删除 | textarea 内 Backspace 直接删整个 `[Image #N]`，编号自动重排 |
| 图片编号随增删动态调整 | 不存在编号概念 | Codex `relabel_local_images` 在增删时自动重新编号 |

## 复现条件

- **复现频率**：必现
- **触发步骤**：
  1. 启动 `peri`
  2. 在 textarea 输入 `look at this `
  3. Ctrl+V 粘贴一张图片（剪贴板有截图）
  4. 继续输入 ` and compare with `
  5. Ctrl+V 粘贴第二张
  6. 观察 textarea：图片无任何标记；Attachment Bar 只显示两个 `[img ...]`，位置信息完全丢失
- **环境**：任意 OS

## 涉及文件

- `peri-tui/src/event/keyboard/normal_keys.rs:501` —— `handle_ctrl_v`，当前直接 `add_pending_attachment`，未触碰 textarea
- `peri-tui/src/ui/main_ui/attachment.rs` —— Attachment Bar 渲染，是当前唯一可见反馈
- `peri-tui/src/app/session_metadata.rs:6` —— `pending_attachments: Vec<PendingAttachment>`，无 textarea 关联
- `peri-tui/src/app/paste_ops.rs` —— 现有的 `PastedTextBlock` placeholder 模型（文本粘贴已有类似设计，但没有 atomic element 语义）

## 期望改进方向

参考 Codex 的 textarea atomic element 实现：

**Codex 关键设计**（`bottom_pane/textarea.rs` + `attachment_state.rs`）：
- textarea 维护 `elements: Vec<TextElement { id, range }>`，每个 element 标记一段不可编辑的 range
- 光标移动时整体跳过 element（`clamp_pos_to_nearest_boundary`）
- Backspace 命中 element 边界时整体删除
- `insert_element(text)` 在光标处插入并注册 atomic range
- `replace_element_payload(old, new)` 用于 relabel（编号重排）
- `remove_deleted_local_placeholders` 在 textarea 编辑后清理被删的 element 对应附件

**Peri 落地路径**：

1. **textarea 层**：`tui_textarea` 不直接支持 atomic element，需要自己包一层维护 `Vec<TextRange>`，或换用 Codex 的 textarea 实现
2. **粘贴时**：在光标处插入 `[Image #N]` 字符串 + 注册为 atomic range
3. **Attachment Bar 与 textarea 同步**：删除任何一个时联动清理
4. **提交时**：`prune_local_images_for_submission` 仅提交 textarea 中仍存在的 placeholder 对应的图片
5. **重编号**：删/增图片时 `relabel_local_images` 把 `[Image #3]` 改成 `[Image #1]`

**注意**：这是较大的设计改动，涉及 textarea 数据模型。建议作为单独的工作项排期，不与 OSC 52 / WSL fallback 等 fallback 类改动捆绑。

## 状态变更记录

| 日期 | 从 | 到 | 操作人 | 说明 |
|------|-----|-----|--------|------|
| 2026-06-16 | — | Open | agent | 创建，对照 Codex `bottom_pane/textarea.rs:1461` + `attachment_state.rs` 比对得出 |

## 修复记录

（由 fix-issue 或 issue-verify skill 追加，创建时留空）
