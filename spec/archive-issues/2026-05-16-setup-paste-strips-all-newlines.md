> 归档于 2026-05-17，原路径 spec/issues/2026-05-16-setup-paste-strips-all-newlines.md
# 粘贴文本去除所有换行符（信息丢失）

**状态**：Closed
**优先级**：中
**创建日期**：2026-05-16
**关闭日期**：2026-05-17

## 问题描述

`paste_text` 方法将 `text.replace('\n', "")` 应用于粘贴文本——移除**所有**换行符，导致多行内容被拼接为一行。对于从 `.env` 文件或配置文件中复制的多行文本（如包含换行的值），所有行直接连接在一起，信息丢失。

## 症状详情

| 现象 | 详情 |
|------|------|
| 多行拼接 | "line1\nline2\nline3" 变为 "line1line2line3" |
| 不可逆 | 用户无法从拼接结果中恢复原始多行结构 |

## 根因

`peri-tui/src/app/setup_wizard.rs:281`

```rust
pub fn paste_text(&mut self, text: &str) {
    let text = text.replace('\n', "");  // 移除所有换行
    // ...
}
```

## 期望

只保留第一行（`text.lines().next().unwrap_or("")`），或根据字段类型智能处理。API Key 字段通常是单行，URL 可能含查询参数。

## 涉及文件

- `peri-tui/src/app/setup_wizard.rs` —— `paste_text()` (line 280-322)
