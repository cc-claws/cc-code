> 归档于 2026-05-27，原路径 spec/issues/2026-05-26-windows-paste-multiline-truncated.md

# Windows 输入框粘贴多行内容被截断为单行发送

**状态**：Fixed
**优先级**：中
**创建日期**：2026-05-26

## 问题描述

在 Windows 平台（cmd.exe / PowerShell 终端）中，向 TUI 输入框粘贴多行文本时，内容不会完整插入 textarea，而是只发送了第一行��触发了 submit。用户无法通过粘贴方式输入多行内容，只能手动逐行输入。

## 症状详情

| 维度 | 表现 |
|------|------|
| 期望行为 | 粘贴多行文本后，所有内容完整插入 textarea，用户可编辑后再手动 Enter 提交 |
| 实际行为 | 粘贴后只有第一行被发送，其余行丢失 |
| 平台 | Windows（cmd.exe / PowerShell / Windows Terminal） |
| 触发方式 | 复制多行文本 → 在输入框中 Ctrl+V 或右键粘贴 |
| Workaround | 手动 Shift+Enter 逐行输入 |

### 现象分析

1. TUI 启用时执行了 `EnableBracketedPaste`（`main.rs`），macOS/Linux 终端支持此协议，粘贴多行内容产生 `Event::Paste` 事件，`event/mod.rs:161-211` 正确将其插入 textarea
2. Windows 终端**不支持 bracketed paste 协议**，粘贴的多行内容被终端模拟为独立的 key event 序列（每个字符一个 Key event，换行处产生 `Key::Enter` event）
3. `keyboard.rs` 中 `Key::Enter` event 直接匹配到 submit 逻辑（`keyboard.rs:830-833`），导致第一行末尾的换行触发了提交
4. 后续行的 key event 在 submit 后被丢弃

## 复现条件

- **复现频率**：必现
- **触发步骤**：
  1. 在 Windows 上启动 TUI（cmd.exe / PowerShell / Windows Terminal）
  2. 复制一段包含换行的多行文本（如从记事本复制）
  3. 在输入框中粘贴（Ctrl+V 或右键粘贴）
  4. 观察：只有第一行被发送，后续行丢失
- **环境**：Windows 平台，任意终端模拟器

## 涉及文件

- `peri-tui/src/event/mod.rs` —— `Event::Paste` 处理分支（macOS/Linux 正常路径），`Event::Key` 分支（Windows 走此路径）
- `peri-tui/src/event/keyboard.rs` —— Enter key 匹配 submit 逻辑（`keyboard.rs:830-833`）
- `peri-tui/src/main.rs` —— `EnableBracketedPaste` 启用位置
