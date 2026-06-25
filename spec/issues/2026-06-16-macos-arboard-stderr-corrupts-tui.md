# macOS arboard 初始化污染 TUI 屏幕

**状态**：Open
**优先级**：中
**创建日期**：2026-06-16

## 问题描述

Peri TUI 在 macOS 上调用 `arboard::Clipboard::new()` 初始化 NSPasteboard 时，底层 `os_log` / `NSLog` 会向 stderr 输出诊断信息。TUI 拥有终端 stdout/stderr，这些杂散输出会破坏终端画面渲染（出现乱码 / 状态栏错位 / 焦点丢失等现象）。

Codex 在 `clipboard_copy.rs` 用 `SuppressStderr` RAII 包装器：dup2 fd2 → /dev/null，调用完恢复原 fd。Peri 当前完全没有这个处理。

## 症状详情

| 场景 | 用户操作 | 当前现象 | 期望行为 |
|------|---------|---------|---------|
| macOS 复制选中文本 | 鼠标双击消息选中文本（触发 `copy_selection_to_clipboard`） | 偶发屏幕闪一下 / 状态栏字符错位 | 完全无副作用 |
| macOS Ctrl+V 粘贴 | 焦点在 textarea，Ctrl+V 触发 `handle_ctrl_v` | 同上 | 同上 |
| macOS 面板复制 | 面板内选中后复制（`copy_panel_selection_to_clipboard`） | 同上 | 同上 |

## 复现条件

- **复现频率**：偶发（取决于 NSPasteboard 是否触发 os_log）
- **触发步骤**：
  1. macOS 启动 `peri`
  2. 等消息出现，鼠标双击选中
  3. 观察终端是否有非预期字符闪过
- **环境**：macOS（特别是较新版本 + M 系列芯片）

## 涉及文件

- `peri-tui/src/event/mouse.rs:121,145` —— 两处 `arboard::Clipboard::new()` 调用点
- `peri-tui/src/event/keyboard/normal_keys.rs:502` —— `handle_ctrl_v` 中 `arboard::Clipboard::new()`

## 期望改进方向

参考 Codex `clipboard_copy.rs:402-455`，实现 `SuppressStderr` RAII：

```rust
#[cfg(target_os = "macos")]
struct SuppressStderr {
    saved_fd: Option<libc::c_int>,
}

#[cfg(target_os = "macos")]
impl SuppressStderr {
    fn new() -> Self {
        unsafe {
            let saved = libc::dup(2);
            // ... open /dev/null, dup2(2), close devnull
        }
    }
}

#[cfg(target_os = "macos")]
impl Drop for SuppressStderr {
    fn drop(&mut self) {
        if let Some(saved) = self.saved_fd {
            unsafe {
                libc::dup2(saved, 2);
                libc::close(saved);
            }
        }
    }
}

// 非 macOS 平台空实现
#[cfg(not(target_os = "macos"))]
struct SuppressStderr;
#[cfg(not(target_os = "macos"))]
impl SuppressStderr { fn new() -> Self { Self } }
```

每个 `arboard::Clipboard::new()` 调用点用 `let _guard = SuppressStderr::new();` 包裹。

## 状态变更记录

| 日期 | 从 | 到 | 操作人 | 说明 |
|------|-----|-----|--------|------|
| 2026-06-16 | — | Open | agent | 创建，对照 Codex `clipboard_copy.rs:402-455` 比对得出 |

## 修复记录

（由 fix-issue 或 issue-verify skill 追加，创建时留空）
