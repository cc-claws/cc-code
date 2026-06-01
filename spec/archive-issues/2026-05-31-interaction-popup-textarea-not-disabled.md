> 归档于 2026-05-31，原路径 spec/issues/2026-05-31-interaction-popup-textarea-not-disabled.md

# 交互弹窗激活时底部常驻输入框未失效

**状态**：Fixed
**优先级**：中
**创建日期**：2026-05-31
**修复日期**：2026-05-31

## 问题描述

当 `/issue-create`（或其他触发 AskUserQuestion 的流程）在 TUI 中显示提问弹窗时，底部的常驻输入框（textarea）仍然处于可用状态：光标可见闪烁、键入/粘贴字符会进入 textarea 而非弹窗。用户期望弹窗激活时底部输入框视觉上失效（光标隐藏、灰色样式），且所有输入都给弹窗。

## 症状详情

| 症状 | 具体表现 | 修复状态 |
|------|----------|----------|
| 粘贴文本泄漏 | Cmd+V 粘贴的文本直接进入底部 textarea | ✅ Fixed |
| IME 中文输入失效 | macOS 终端通过 Bracketed Paste 发送 IME 组合结果，被拦截后弹窗无法接收中文 | ✅ Fixed |
| 鼠标点击 textarea | 弹窗激活后鼠标点击 textarea 区域仍会移动光标 | ✅ Fixed |
| textarea 样式无变化 | 弹窗激活后 textarea 边框无降级提示 | ✅ Fixed（DarkGray 变暗） |
| 光标未隐藏 | 弹窗激活后 textarea 光标仍可见闪烁 | ⚠️ 有意保留——终端 IME 预编辑窗口需要 textarea 光标作为锚点，隐藏后中文输入法完全失效 |
| 键盘输入泄漏 | 键盘打字后字符出现在底部 textarea 中 | ✅ 无需修复——`handle_popups()` 已正确拦截所有 Key 事件，用户最初的报告实际为 Paste 泄漏混淆 |

## 复现条件

- **复现频率**：必现
- **触发步骤**：
  1. 启动 TUI
  2. 输入 `/issue-create` 并回车，触发需要 AskUser 交互的流程
  3. 提问弹窗出现后，在弹窗区域之外键入字符或粘贴文本
- **环境**：macOS，所有模型/配置

## 涉及文件

- `peri-tui/src/event/mod.rs` —— Paste 事件处理（`Event::Paste` 分支，~L306-310）不检查 `interaction_prompt`，直接 fallback 到 textarea；Mouse 点击事件（~L656）也不检查弹窗状态
- `peri-tui/src/ui/main_ui/mod.rs` —— textarea 渲染（~L342-354）仅根据 `app.focused` 和 `is_active` 决定是否隐藏光标，不检查 `interaction_prompt`；textarea block 样式（~L330-340）无弹窗降级逻辑
- `peri-tui/src/event/keyboard/popups.rs` —— 弹窗键盘事件拦截（~L16-68）理论上通过返回 `Some(Action::Redraw)` 阻止 key event 进入 `normal_keys`，但 Paste/Mouse 事件不走此路径

## 技术分析摘要（仅现象级，非诊断）

**键盘事件路径**：`handle_key_event()` → Stage 10-12 `handle_popups()` 在 `interaction_prompt == Some(Questions(_))` 时对所有 key event 返回 `Some(Action::Redraw)`，阻止 Stage 13 `handle_normal_keys()` 向 textarea 写入。此路径从代码层面看应正常工作，用户报告的键盘输入泄漏可能与 Paste 事件混淆，需进一步确认。

**Paste 事件路径**：`Event::Paste` 在 `event/mod.rs:261` 独立处理，检查 setup_wizard → PanelManager 后无弹窗判断，直接 fallback 到 `textarea.insert_str(&text)`。弹窗激活时粘贴的文本会进入底部 textarea。

**Mouse 事件路径**：`Event::Mouse` 的 Scroll 事件在 `event/mod.rs:314` 已检查 `interaction_prompt`，但 Down(Left) 点击事件（~L656）无此检查。

**渲染**：textarea 位于 `chunks[5]`，AskUser 弹窗位于 `chunks[2]`（panel_area），二者不重叠。弹窗激活时 textarea 光标通过 `should_hide_cursor = !app.focused || !is_active` 判断可见性，未纳入 `interaction_prompt` 条件。textarea block 样式在弹窗激活期间无任何变化。

## 修复记录

**Commits**：`34c0f43e` + `d4f67fde` + `be07e141`

### 修复内容

| 文件 | 改动 |
|------|------|
| `app/mod.rs` | 新增 `is_interaction_popup_active()`（统一弹窗检测）+ `paste_to_interaction_popup()`（路由粘贴到 custom_input） |
| `event/mod.rs` | `Event::Paste`：弹窗时路由到 popup custom_input（支持 IME）+ textarea fallback 保留弹窗守卫<br>`MouseEventKind::Down(Left)`：弹窗时跳过 textarea 点击 |
| `ui/main_ui/mod.rs` | textarea 弹窗时 `bar_focused \|\| popup_active` 分支 → DarkGray 边框变暗 |

### 踩坑记录

1. **光标隐藏导致 IME 回归**（`d4f67fde` 回退）：最初将 `should_hide_cursor` 加入 `popup_active` 条件，导致 textarea 光标隐藏。终端 IME 预编辑窗口依赖 textarea 的光标位置作为锚点，隐藏后中文输入法完全不可用。最终仅保留边框变暗作为视觉禁用信号。

2. **Bracketed Paste 携带 IME 文本**：macOS 终端（iTerm2、Terminal.app）将 IME 组合完成后的中文文本通过 `Event::Paste`（Bracketed Paste 协议）发送，而非 `Event::Key(Key::Char)`。第一个修复版本弹窗时直接拦截了所有 Paste 事件（`if !app.is_interaction_popup_active()`），导致中文输入法在弹窗中失效。正确做法是弹窗激活时 Paste 路由到 `custom_input.push_str()`，而非丢弃。

3. **键盘事件未被泄漏**：`handle_popups()` 在 `interaction_prompt` 设值后对所有 Key 事件返回 `Some(Action::Redraw)`，阻止到达 `normal_keys`。用户最初报告的"键盘打字泄漏"经确认实际为 Paste 泄漏混淆——英文键盘在弹窗的 `handle_edit_key` 路径正常工作，进入 `custom_input`。
