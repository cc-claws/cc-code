# SSH/远程会话下复制文本失效

**状态**：Open
**优先级**：高
**创建日期**：2026-06-16

## 问题描述

Peri TUI 在 SSH 会话、tmux 嵌套、WSL 等远程/嵌套环境下，复制选中文本到系统剪贴板失败。用户在本地终端通过 SSH 连到远端服务器跑 Peri 时，选中消息执行复制，文本被写到远端服务器的剪贴板（X11/Wayland），本地终端拿不到内容，等同于"复制无效"。

## 症状详情

| 环境 | 用户期望 | 实际现象 |
|------|---------|---------|
| SSH 远程会话 | 选中 → 复制 → 本地终端可粘贴 | 本地粘贴无内容 / 粘贴出上次复制的内容 |
| SSH + tmux 嵌套 | 同上 | 同上，tmux 包装层未处理 |
| 本地 WSL（无 X server） | 选中 → 复制 → Windows 可粘贴 | arboard 创建失败或写到不存在的 Linux 剪贴板 |
| 本地 Linux X11 | 选中 → 复制 → 粘贴 | 复制后能粘，但 TUI 退出后内容消失（无 ClipboardLease） |

## 复现条件

- **复现频率**：必现
- **触发步骤**：
  1. SSH 登录到远端 Linux 服务器，启动 `peri`
  2. 等待消息出现，鼠标双击选中消息文本
  3. 在本地终端按 Ctrl+V / Cmd+V 粘贴
- **环境**：SSH + 远端 Linux / WSL / 嵌套 tmux

## 涉及文件

- `peri-tui/src/event/mouse.rs:121,145` —— `copy_selection_to_clipboard` / `copy_panel_selection_to_clipboard`，当前仅 `arboard::Clipboard::new().set_text()`
- `peri-tui/src/event/keyboard/normal_keys.rs:501` —— `handle_ctrl_v` 同样问题

## 期望改进方向

参考 Codex 的 `clipboard_copy.rs`，落地多层 fallback：
1. SSH 会话优先 OSC 52 转义（`\x1b]52;c;{base64}\x07`，100KB 上限）
2. tmux 嵌套用 `\x1bPtmux;\x1b\x1b]52;c;...\x07\x1b\\` 包裹
3. WSL 本地用 `powershell.exe Set-Clipboard` 兜底
4. Linux X11/Wayland 用 `ClipboardLease` 持有 `arboard::Clipboard` 直到 TUI 退出
5. macOS arboard 初始化 NSPasteboard 时抑制 stderr 污染（dup2 fd2 → /dev/null）

## 状态变更记录

| 日期 | 从 | 到 | 操作人 | 说明 |
|------|-----|-----|--------|------|
| 2026-06-16 | — | Open | agent | 创建，对照 Codex `clipboard_copy.rs` 比对得出 |

## 修复记录

（由 fix-issue 或 issue-verify skill 追加，创建时留空）
