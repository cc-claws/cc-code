> 归档于 2026-05-18，原路径 spec/issues/2026-05-18-tui-dot-and-scrollbar-rendering.md

# TUI 指示符号 ⏺ 与 ● 不统一，滚动条在部分终端有空隙

**状态**：Fixed
**优先级**：低
**创建日期**：2026-05-18
**修复日期**：2026-05-18

## 问题描述

TUI 渲染中存在两个字符显示细节问题：

1. **工具/批次指示符用 ⏺ (U+23FA)**，而 AI 消息指示符用 **● (U+25CF)**，两者视觉不一致——⏺ 是录像按钮符号，不是一个纯圆点。
2. **滚动条默认 track 用双线字符 ║ (U+2551)**，在部分 GPU 终端环境下列间有空隙，显示为一段一段不连贯。

## 症状详情

### 问题 1：⏺ 与 ● 不统一

| 位置 | 当前字符 | 码点 | 说明 |
|------|----------|------|------|
| `AssistantBubble` 指示器 | `●` | U+25CF | 流式闪烁 / 固定，正常圆点 |
| `ToolBlock` 指示器 | `⏺` | U+23FA | 运行中/完成，录像按钮 |
| `ToolCallGroup` 汇总行 | `⏺` | U+23FA | 批量工具汇总前缀 |
| `render_batch_summary` 标题行 | `⏺` | U+23FA | Agent 批次标题前缀 |
| `render_ask_user_block` 标题行 | `⏺` | U+23FA | 用户回答标题前缀 |

`⏺` 本质是「黑色圆圈+三角」组合的录制指示符，与纯圆点 `●` 视觉上不匹配。用户期望统一为 `●`。

### 问题 2：滚动条 track 字符有空隙

ratatui `Scrollbar` 默认使用 `DOUBLE_VERTICAL` 符号集：
- track: `║` (U+2551, double vertical line)
- thumb: `█` (U+2588, full block)
- begin: `▲`
- end: `▼`

`║` 是 box-drawing 字符，在某些 GPU 终端（Alacritty、WezTerm、foot 等）中，字体渲染的字符高度不足以完全填满字符格，导致竖排时相邻两行之间出现肉眼可见的空隙，滚动条 track 显示为不连续的分段。

## 涉及文件

- `peri-tui/src/ui/message_render.rs` —— ⏺/● 字符分散在多个渲染函数中
- `peri-widgets/src/scrollable.rs` —— `ScrollableArea::render()` 未自定义滚动条符号，使用 ratatui 默认 `DOUBLE_VERTICAL` 集

## 期望改进方向

1. `message_render.rs` 中所有 `⏺` (5 处) 替换为 `●`
2. `scrollable.rs` 中 `ScrollableArea::render()` 为滚动条显式设置 track 符号为 `█` (FULL BLOCK)，避免 `║` 在部分终端的空隙问题
