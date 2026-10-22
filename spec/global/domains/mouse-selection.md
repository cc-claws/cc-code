# 鼠标选区 领域

## 领域综述

鼠标选区领域负责 TUI 中的鼠标拖拽文字选择和系统剪贴板复制功能，实现字符级精度选区，正确处理 CJK 宽字符。

核心职责：
- TextSelection 模块管理拖拽状态和选区坐标
- WrappedLineInfo 换行映射将屏幕坐标映射到逻辑字符位置
- unicode-width 正确处理 CJK 宽字符选区计算
- Ctrl+C 优先级链：面板打开先关面板 > Loading 中断 > 空闲时 2 秒内双击才退出（链中无选区复制步骤）

## 核心流程

### 鼠标选区流程

```
鼠标左键按下（消息区）
  → TextSelection.start_drag(row, col)
  → 鼠标拖动
  → TextSelection.update_drag(row, col)
  → 鼠标释放
  → extract_selected_text(wrap_map, selection)（virtual 渲染走 RenderEvent::CopySelection 内容锚定导出）
  → 分层写入剪贴板（OSC 52 / arboard / WSL 回落）
  → 复制后不清除选区
```

### Ctrl+C 优先级链

```
Ctrl+C 按下
  → 面板打开？→ 先关闭面板
  → Loading? → 中断当前执行
  → 空闲 → 2 秒内第二次按下才退出（选区复制不经 Ctrl+C）
```

## 技术方案总结

| 维度 | 选型 |
|------|------|
| 选区数据结构 | `TextSelection { start, end, anchor_start, anchor_end, dragging, generation, selected_text }`（起止端点另存锚定快照 anchor_*，generation 防陈旧选区误用；visual_row 用 usize 避免长对话饱和，visual_col 保留 u16 屏幕宽度有限） |
| 面板选区 | PanelTextSelection，Vec<String> 纯文本行直接索引 |
| 换行映射 | `WrappedLineInfo { line_idx, visual_row_start/end: usize, plain_text, char_widths }`（行号 usize，详见 issue_2026-06-14-tui-scroll-overflow-u16-saturation） |
| 坐标映射 | `visual_to_logical(visual_row: usize, visual_col: u16, ...)`：视觉坐标 → 逻辑行+字符偏移 |
| 宽字符 | unicode-width crate，char_widths 累积宽度定位字符 |
| 高亮渲染 | highlight_line_spans()：Span 在字符边界拆分 + Modifier::REVERSED |
| 剪贴板 | 分层策略：SSH/tmux 走 OSC 52、本地 arboard、WSL 回落 powershell.exe，ClipboardLease 托管写入 |

## Feature 附录

### feature_20260429_F002_mouse-text-selection
**摘要:** TUI 鼠标拖拽选中文本并复制到系统剪贴板
**关键决策:**
- 新增 TextSelection 模块管理拖拽状态，存储在 AppCore 中
- 通过 WrappedLineInfo 换行映射表将屏幕坐标映射到逻辑字符位置
- 使用 unicode-width crate 正确处理 CJK 宽字符的选区计算
- Ctrl+C 优先级链：有选区时复制 > Loading 时中断 > 非Loading时退出
- 选区高亮使用 Modifier::REVERSED 反色，只深拷贝涉及的少量行
- 渲染时通过 AppCore.messages_area 存储消息区域 Rect 供事件处理使用
**归档:** [链接](../../archive/feature_20260429_F002_mouse-text-selection/)
**归档日期:** 2026-04-30

---

## 相关 Feature
- → [tui.md](./tui.md) — TUI 事件处理和渲染
