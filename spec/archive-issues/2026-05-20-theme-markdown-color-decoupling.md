> 归档于 2026-05-25，原路径 spec/issues/2026-05-20-theme-markdown-color-decoupling.md

# Markdown 与 Theme 颜色体系脱节，存在多处分叉硬编码

**状态**：Fixed
**优先级**：中
**创建日期**：2026-05-20
**修复日期**：2026-05-24

## 问题描述

项目中有三套独立存在、互不联动的颜色定义体系，且存在色值不一致。改动 `DarkTheme` 的色值不会影响 Markdown 渲染、spinner、diff 高亮等组件。

## 已修复

| 项目 | 状态 | 修复方式 |
|------|------|---------|
| MarkdownTheme-Theme 脱节 | ✅ Fixed | 新增 `ThemeMarkdownAdapter<'a>`（`markdown/mod.rs:46`），将 `Theme` trait 方法映射到 `MarkdownTheme` |
| Spinner 硬编码 | ✅ Fixed | 新增 `with_theme(&dyn Theme)` 方法 + `theme_colors()` setter（`spinner/mod.rs`） |
| TUI 常量 vs DarkTheme 不一致 | ✅ Fixed | `DarkTheme::thinking()` 已对齐为 `#A2A9E4`，与 TUI 常量一致 |
| message_render.rs 箭头硬编码 | ✅ Fixed | 改用 `theme::LOADING` 替代硬编码 `Color::Rgb(147, 197, 253)` |
| highlight.rs 代码高亮硬编码 | ✅ Fixed | `highlight_code_line` 及其辅助函数为零调用死代码，已删除并移除 `regex` 依赖 |

## 设计决策

| 文件 | 说明 |
|------|------|
| `peri-widgets/src/markdown/highlight.rs` | syntect `base16-ocean.dark` 语法高亮与项目 Theme 无关，属第三方主题，不影响颜色一致性，保持不变 |

## 涉及文件

- `peri-widgets/src/markdown/mod.rs` — `ThemeMarkdownAdapter` 适配器
- `peri-widgets/src/theme/mod.rs` — `Theme` trait 定义（含 `diff_add`/`diff_remove`/`diff_hunk`）
- `peri-widgets/src/theme/presets.rs` — `DarkTheme` 实现
- `peri-widgets/src/message_block/highlight.rs` — diff 高亮使用 Theme trait，代码高亮死代码已清除
- `peri-widgets/Cargo.toml` — 移除 `regex` 依赖
- `peri-tui/src/ui/theme.rs` — TUI 常量与 DarkTheme 已对齐
- `peri-tui/src/ui/message_render.rs` — 箭头颜色使用 `theme::LOADING`
