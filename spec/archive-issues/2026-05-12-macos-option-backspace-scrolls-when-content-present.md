> 归档于 2026-05-20，原路径 spec/issues/2026-05-12-macos-option-backspace-scrolls-when-content-present.md

# Mac 上 Option+Backspace 在有可滚动内容时触发滚动而非删除整行

**状态**：Fixed
**优先级**：中
**创建日期**：2026-05-12
**修复日期**：2026-05-20

## 问题描述

在 Mac 平台使用 VS Code 集成终端时，按 Option+Backspace（应删除整词）会触发消息区域向上滚动，而非删除输入框中的整词。

## 根因

1. **终端层**：VS Code 终端在 Mac 上将 Option+Backspace 映射为 PageUp 转义序列
2. **crossterm 层**：crossterm 将转义序列解释为 `Key::PageUp`
3. **事件处理层**：`PageUp` 被无条件拦截用于消息区域滚动

## 修复方案

`peri-tui/src/event/keyboard.rs:624-643` — PageUp 处理区分 VS Code 终端环境：

```rust
// PageUp: VS Code terminal maps Option+Backspace to PageUp
// Detect VS Code terminal environment; perform word-delete when textarea has content
Input {
    key: Key::PageUp, ..
} if std::env::var("TERM_PROGRAM").as_deref() == Ok("vscode") => {
    if has_content {
        session.ui.textarea.delete_word();  // 在输入框中有内容时删除整词
    } else {
        app.scroll_up();                    // 输入框空时正常滚动
    }
}
Input {
    key: Key::PageDown, ..
} => {
    app.scroll_down();  // PageDown 保持原有滚动行为
}
```

### 修改文件

- `peri-tui/src/event/keyboard.rs:624-650` — PageUp/PageDown 事件处理改为 VS Code 检测 + 词删除

## 症状详情

| 条件 | 修复前 | 修复后 |
|------|--------|--------|
| VS Code 终端 + 输入框有内容 + Option+Backspace | 消息区域滚动 | 删除整词 |
| VS Code 终端 + 输入框为空 + Option+Backspace | 消息区域滚动 | 消息区域滚动 |
| 其他终端 + PageUp 键 | 消息区域滚动 | 消息区域滚动 |

## 外部依赖

- [crossterm-rs/crossterm#575](https://github.com/crossterm-rs/crossterm/issues/575) — macOS, backspace, and modifiers
- [microsoft/vscode#83453](https://github.com/microsoft/vscode/issues/83453) — (Terminal) Option+delete doesn't delete previous word on MacOS
