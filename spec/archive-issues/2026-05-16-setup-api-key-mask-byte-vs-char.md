> 归档于 2026-05-16，原路径 spec/issues/2026-05-16-setup-api-key-mask-byte-vs-char.md

# API Key 遮罩使用字节长度而非字符数

**状态**：Fixed
**优先级**：中
**创建日期**：2026-05-16
**修复日期**：2026-05-16

## 问题描述

`render_form_edit` 在非聚焦状态下显示 API Key 遮罩时使用 `"•".repeat(mp.api_key.len())` （字节长度），而非 `chars().count()`。对于多字节字符（如中文 API key），显示异常：2 个中文字符（6 字节）显示 6 个 • 而非 2 个。

## 症状详情

| 现象 | 详情 |
|------|------|
| 遮罩数量不对 | CJK API key "你好" 显示 6 个 •，但实际只有 2 个字符 |
| 泄露长度信息 | 遮罩数 = 字节数，可能泄露 key 格式信息 |
| 编辑模式正常 | 聚焦时使用 `edit_display_parts`（字符级），无此问题 |

## 根因

`peri-tui/src/ui/main_ui/popups/setup_wizard.rs:321`

```rust
} else if mp.api_key.is_empty() {
    String::new()
} else {
    "•".repeat(mp.api_key.len())  // ← 字节长度
}
```

## 期望

```rust
"•".repeat(mp.api_key.chars().count())
```

## 涉及文件

- `peri-tui/src/ui/main_ui/popups/setup_wizard.rs` —— `render_form_edit()` line 321
