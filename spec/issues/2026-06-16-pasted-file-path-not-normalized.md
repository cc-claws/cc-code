# 粘贴文件路径未规范化识别

**状态**：Open
**优先级**：低
**创建日期**：2026-06-16

## 问题描述

用户把图片或任意文件的路径粘贴到 Peri TUI 输入框时，Peri 不识别其为"文件引用"，统一作为普通文本塞进 textarea。常见 4 种路径形态都被原样保留，无法触发后续附件处理或文件加载：

1. `file://` URL
2. Windows drive 路径（`C:\Users\...`）
3. UNC 路径（`\\server\share\...`）
4. shell 转义的单路径（`/path/to/My\ File.png`）

Codex 在 `clipboard_paste.rs:251` 提供 `normalize_pasted_path()`，统一把 4 种形态归一化为 `PathBuf`，下游可决定是否作为图片附件处理。

## 症状详情

| 粘贴内容 | 当前 Peri 行为 | Codex 行为 |
|---------|---------------|-----------|
| `file:///tmp/example.png` | 原样字符串塞入 textarea | 转换为 `/tmp/example.png`，按需作图片附件 |
| `C:\Temp\example.png`（含 WSL） | 原样字符串 | 转 `/mnt/c/Temp/example.png` |
| `\\server\share\file.jpg` | 原样字符串 | 识别为 UNC 路径 |
| `/home/user/My\ File.png` | 原样字符串（含反斜杠） | shlex 解转义为 `/home/user/My File.png` |

## 复现条件

- **复现频率**：必现
- **触发步骤**：
  1. 文件管理器复制一张图片文件
  2. 粘贴路径（注意：是路径字符串不是图片数据）到 Peri textarea
  3. 提交，观察 Peri 是否把路径作为文件附件处理
- **环境**：任意 OS

## 涉及文件

- `peri-tui/src/event/mod.rs:280` —— `Event::Paste(text)` 入口，需要加路径识别分支
- `peri-tui/src/app/paste_ops.rs` —— `paste_text_into_textarea`，单行文本可直接检查是否路径
- `peri-tui/src/app/mod.rs` —— `PendingAttachment` / `add_pending_attachment`，作为路径附件的落点

## 期望改进方向

参考 Codex `clipboard_paste.rs:251-287`，新增 `normalize_pasted_path()` 工具函数：

```rust
pub fn normalize_pasted_path(pasted: &str) -> Option<PathBuf> {
    let pasted = pasted.trim();
    // 1. strip 简单引号包裹
    // 2. file:// URL → filesystem path（url::Url::parse）
    // 3. Windows drive / UNC 路径（含 WSL 路径转换）
    // 4. shell-escaped 单路径（shlex::Shlex 解析）
}
```

集成点：
- 单行粘贴且 `normalize_pasted_path` 返回 `Some(path)` 时，弹询问："识别为文件路径：xxx，作为附件添加？"
- 或者直接走当前 `add_pending_attachment` 流程，label 用文件名

依赖：`url` crate（已在 Codex 用到）+ `shlex` crate。

## 状态变更记录

| 日期 | 从 | 到 | 操作人 | 说明 |
|------|-----|-----|--------|------|
| 2026-06-16 | — | Open | agent | 创建，对照 Codex `clipboard_paste.rs:251` 比对得出 |

## 修复记录

（由 fix-issue 或 issue-verify skill 追加，创建时留空）
