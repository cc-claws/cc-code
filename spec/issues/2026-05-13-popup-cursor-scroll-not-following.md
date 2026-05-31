# 弹窗光标移动时滚动不跟随

**状态**：Partial — 症状三/四（hints.rs panic）已修复，症状一/二（AskUser/HITL 滚动）仍 Open
**优先级**：中（panic 已修复，剩余为体验问题）
**创建日期**：2026-05-13

## 问题描述

三个包含光标选项的弹窗（AskUser、HITL、Hint 浮层）都存在光标移动后滚动未跟随的问题。在选项较多的情况下，按下 Up/Down 移动光标会导致光标移出可见区域，用户看不到当前选中的项。严重影响使用体验。

## 症状详情

### 症状一：AskUser 弹窗 — 光标行号计算不准确

| 操作 | 预期 | 实际 |
|------|------|------|
| 选项超过可视高度后按 Down | 滚动跟随，光标可见 | 光标移出可视区，滚动未跟随 |

**根因**：`ask_user_ops.rs:35-36` 中 `cursor_row` 直接使用 `option_cursor`（选项索引），未考虑渲染时选项前的问文本（2-3 行）、选项间的空行、选项的 `description` 额外行等。同时 `visible_height` 硬编码为 10，与实际弹窗内容区域高度不符。

```rust
// ask_user_ops.rs:34-36
p.current().move_option_cursor(delta);
let cursor_row = p.current().option_cursor.max(0) as u16;  // 只是索引，不是实际行号
p.scroll_offset = ensure_cursor_visible(cursor_row, p.scroll_offset, 10); // 10 是写死的
```

渲染实际布局（`ask_user.rs`）：
- 问文本：N 行
- 空行：1 行
- 每个选项：label（1 行）+ description 若有（1 行）+ 空行（除非是最后一个）

所以当 `option_cursor = 7` 时，实际渲染行号可能是 14+，但 `ensure_cursor_visible(7, ...)` 认为光标仍在可见范围内。

### 症状二：HITL 审批弹窗 — 完全无滚动机制

HITL 弹窗使用 `Paragraph` 直接渲染，没有任何滚动支持。

**根因**：
- `hitl.rs:99-100`：直接用 `Paragraph::new(Text::from(lines))` 渲染，未使用 `ScrollableArea`
- `hitl_prompt.rs:20-29`：数据结构无 `scroll_offset` 字段
- `hitl_ops.rs:5-13`：`hitl_move` 只移动光标，未调整任何滚动偏移

当批处理项超过弹窗可视区域时（默认高度 `items.len() * 2 + 5` 但被 `max_h` 截断），光标可以移动到可视区外。

### 症状三：命令提示浮层 — 过滤后光标越界 / 渲染切片越界

`hints.rs:78-89` 中 `scroll_offset` 基于 `cursor` 实时计算，渲染逻辑本身正确。但存在边界问题：

1. **光标未随列表缩小而钳位**：用户输入过滤字符时列表缩小，`hint_cursor` 未 clamp 到 `items.len() - 1`
2. **渲染切片越界风险**：当 `cursor >= items.len()` 时，`scroll_offset` 计算可能导致 `items[scroll_offset..scroll_offset + viewport]` panic（见 `hints.rs:89`）

```rust
// hints.rs:78-89
let scroll_offset = if let Some(cur) = cursor {
    if cur < viewport { 0 }
    else { cur - viewport + 1 }
    // 若 cursor=5, total=3, viewport=3, 则 scroll_offset=3, items[3..6] panic
} else { 0 };
let visible_items = &items[scroll_offset..scroll_offset + viewport];
```

## 复现条件

### AskUser 弹窗
- **复现频率**：必现（当选项足够多时）
- **触发步骤**：
  1. 触发 AskUser 批量弹窗
  2. 问题包含 6+ 个选项
  3. 按 Down 键移动光标到底部选项
  4. 光标移出可视区
- **环境**：任意终端尺寸

### HITL 审批弹窗
- **复现频率**：必现（当工具调用项超过弹窗高度时）
- **触发步骤**：
  1. 触发批量工具审批
  2. 5+ 个工具调用
  3. 按 Down 键移动光标
  4. 光标移出可视区

### 命令提示浮层
- **复现频率**：偶发（当输入过滤使候选项减少时）
- **触发步骤**：
  1. 输入 `/` 触发提示浮层
  2. 向下移动光标到后半段
  3. 继续输入字符使候选项变少
  4. 光标位置异常或渲染异常

## 相关代码

- `peri-tui/src/app/ask_user_ops.rs:26-37` —— ask_user 光标移动与滚动跟随（`cursor_row` 计算错误）
- `peri-tui/src/app/ask_user_prompt.rs:32-38` —— `QuestionState::move_option_cursor` 只改光标，不自知行号
- `peri-tui/src/ui/main_ui/popups/ask_user.rs:173-176` —— `ScrollState::with_offset` 使用 `prompt.scroll_offset`
- `peri-tui/src/app/hitl_ops.rs:5-13` —— HITL 光标移动，无滚动处理
- `peri-tui/src/app/hitl_prompt.rs:20-29` —— `HitlBatchPrompt` 无 `scroll_offset` 字段
- `peri-tui/src/ui/main_ui/popups/hitl.rs:99-100` —— HITL 渲染使用 `Paragraph`，无 `ScrollableArea`
- `peri-tui/src/ui/main_ui/popups/hints.rs:78-89` —— hint 浮层 `scroll_offset` 计算，存在越界风险
- `peri-tui/src/event.rs:545-595` —— hint Up/Down 事件处理，未在列表缩小后 clamp cursor
- `peri-tui/src/app/mod.rs:572-583` —— `ensure_cursor_visible` 工具函数本身正确，但调用方传递错误参数

## 影响范围

- 所有使用 AskUser 弹窗（多选项场景）的用户
- 所有使用批量 HITL 审批的用户
- 所有使用命令提示浮层的用户
- 特别是在选项/项目较多时，问题严重

### 现象四（2026-05-31）：Windows 平台 hints.rs 切片越界 panic

在 Windows 平台上输入 `/` 后按 Up/Down 移动光标即可触发 panic（无需输入过滤字符）：

```
thread 'main' (40176) panicked at peri-tui\src\ui\main_ui\popups\hints.rs:91:31:
range end index 29 out of range for slice of length 28
```

- **触发步骤**：
  1. 输入 `/` 触发提示浮层
  2. 按 Up/Down 移动光标
  3. Panic 崩溃
- **环境**：Windows 平台（非 Windows 平台也可能触发，取决于 agent_commands 数量）
- **关键观察**：`hint_ops.rs::build_hint_items()` 包含三类候选项（Cmd + Skill + **AgentCmd**），而 `hints.rs::render_unified_hint()` 只构建了两类（Cmd + Skill，**缺少 AgentCmd**）。cursor 的 Up/Down 循环范围基于 `hint_candidates_count()`（含 AgentCmd），但渲染切片 `&items[scroll_offset..scroll_offset + viewport]` 用的 `items` 不含 AgentCmd，两者长度不一致。当 agent_commands 数量 ≥ 1 时，cursor 可被设到超出渲染列表长度的位置，触发 slice panic。

## 修复方向

### AskUser 弹窗
1. **计算实际光标行号**：根据选项索引、问文本行数、description、空行等计算 `option_cursor` 对应的实际渲染行号
2. **使用实际可见高度**：从渲染上下文获取实际 `content_area.height` 替代硬编码 `10`

### HITL 弹窗
1. **添加 `scroll_offset` 字段**：在 `HitlBatchPrompt` 中添加 `scroll_offset: u16`
2. **`hitl_move` 中更新 scroll_offset**：类似 `ask_user_move`，使用 `ensure_cursor_visible`
3. **渲染使用 `ScrollableArea`**：替换 `Paragraph` 为 `ScrollableArea` + `ScrollState`

### 命令提示浮层
1. **过滤时 clamp cursor**：输入过滤字符导致 `hint_candidates_count()` 变化时，clamp `hint_cursor` 到新长度内
2. **渲染保护**：在 `items[scroll_offset..]` 前检查 `scroll_offset` 是否越界

## 修复记录

### 症状三/四修复（2026-05-31）

**根因**：`hints.rs::render_unified_hint()` 只收集 Cmd + Skill 候选项，但 `hint_ops.rs::build_hint_items()` 还包含 AgentCmd。cursor 的 Up/Down 循环范围基于 `hint_candidates_count()`（含 AgentCmd，更长），渲染切片 `&items[scroll_offset..scroll_offset + viewport]` 基于不含 AgentCmd 的更短列表，导致 slice 越界 panic。

**修复**：`hints.rs` 中补齐 `agent_cmd_candidates` 收集逻辑，`HintItem` 枚举新增 `AgentCmd` 变体，排序优先级（`rank()`）与 `hint_ops.rs` 完全对齐。AgentCmd 无 description，渲染时显示空字符串。

- 文件：`peri-tui/src/ui/main_ui/popups/hints.rs`
- commit：待提交
