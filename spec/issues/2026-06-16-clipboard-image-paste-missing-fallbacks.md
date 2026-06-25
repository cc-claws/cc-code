# 粘贴图片不支持 file_list 与 WSL PowerShell fallback

**状态**：Open
**优先级**：中
**创建日期**：2026-06-16

## 问题描述

Peri TUI 的 Ctrl+V 粘贴图片只走 `arboard::Clipboard::get_image()` 单条路径，覆盖不到两类常见场景：

1. **macOS Finder 复制图片文件**：剪贴板里是文件路径而非图像数据，arboard 给的是 `file_list`，`get_image()` 直接返回错误
2. **WSL 远程会话粘贴截图**：WSL 进程访问不到 Windows 剪贴板，`arboard::Clipboard::new()` 失败

用户在这两个场景下按 Ctrl+V 完全无响应（也不报错），必须手动保存图片到磁盘再走文件附件路径。

## 症状详情

| 场景 | 用户操作 | 当前现象 | 期望行为 |
|------|---------|---------|---------|
| macOS Finder 复制 PNG | 在 Finder 选 .png 文件 Cmd+C，回 Peri Ctrl+V | 静默失败（剪贴板非 image data） | 识别为文件路径 → `image::open` → 转 PNG base64 加入 Attachment Bar |
| WSL + Windows 截图 | Win+Shift+S 截屏到剪贴板，WSL 里 Ctrl+V | `arboard::Clipboard::new()` 报错，回落到 `get_text()` 拿到空字符串 | 通过 `powershell.exe Get-Clipboard -Format Image` 保存到临时 PNG，转 WSL 路径后读取 |
| Linux + 远程 X 转发 | 通过 X11 forwarding 把本地剪贴板传到远端 | 大概率成功，但若 X 转发未启用则失败 | 至少给清晰错误提示 |

## 复现条件

- **复现频率**：必现
- **触发步骤**（Finder 场景）：
  1. macOS Finder 选一张 .png 文件
  2. Cmd+C 复制（注意：是文件不是图像数据）
  3. 启动 `peri`，焦点在 textarea，按 Ctrl+V
- **环境**：macOS Finder / WSL / Windows 截图

## 涉及文件

- `peri-tui/src/event/keyboard/normal_keys.rs:501` —— `handle_ctrl_v`，当前只判 `clipboard.get_image()`
- `peri-tui/src/event/mouse.rs:105` —— `rgba_to_png_base64`，PNG 编码逻辑（可复用）
- `peri-tui/src/app/mod.rs` —— `PendingAttachment` 数据结构（粘贴后落点）

## 期望改进方向

参考 Codex 的 `clipboard_paste.rs`，落地三层 fallback：

```
1. clipboard.get().file_list()              ← Finder 等文件复制场景
   └─ image::open(f) 打开第一个能识别的图片
   └─ 转 DynamicImage → PNG 编码 → base64
2. clipboard.get_image()                    ← Chrome 截图等场景（当前唯一路径）
   └─ RgbaImage::from_raw → DynamicImage → PNG 编码
3. WSL PowerShell fallback（仅 Linux + 检测到 WSL 环境）
   └─ powershell.exe Get-Clipboard -Format Image
   └─ 保存到临时 PNG
   └─ Windows 路径转 WSL 路径（C:\Users → /mnt/c/Users）
   └─ image::image_dimensions() 验证 + 读取
```

WSL 检测方式：`/proc/version` 含 `microsoft` 或 `wsl`，或 `WSL_DISTRO_NAME` / `WSL_INTEROP` 环境变量存在。

错误反馈用 `PasteImageError` 枚举暴露给上层，避免静默失败。

## 状态变更记录

| 日期 | 从 | 到 | 操作人 | 说明 |
|------|-----|-----|--------|------|
| 2026-06-16 | — | Open | agent | 创建，对照 Codex `clipboard_paste.rs` 比对得出 |

## 修复记录

（由 fix-issue 或 issue-verify skill 追加，创建时留空）
