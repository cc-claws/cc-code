> 归档于 2026-05-16，原路径 spec/issues/2026-05-12-askuserquestion-display-format.md

# AskUserQuestion 问答显示格式优化

**状态**：Closed
**优先级**：低
**创建日期**：2026-05-12

## 修复记录

1. **换行符替换**（`message_render.rs`）：`render_ask_user_block()` 和 `ToolCallGroup::AskUser` 两处解析逻辑中，对 `header`/`answer` 调用 `.replace('
', " ").replace('
', " ")`，单问题模式的 block 用 `.lines().collect::<Vec<_>>().join(" ")` 处理。
2. **移除重复 SystemNote**（`agent_ops.rs`）：删除 `InteractionRequest::Questions` 处理中额外创建的 `SystemNote`（显示问题列表），因为 `ToolCallGroup::AskUser` 渲染已包含问答摘要。
3. **去掉 `·` 符号 + 修复 `[问:` 截断**（`message_render.rs`）：两处渲染移除 `"· "` Span；`strip_suffix(']')` → `trim_end_matches(']')`，解决文本截断时 `]` 缺失导致匹配失败。
4. **统一单/多问题返回格式**（`ask_user_tool.rs`）：删除 `if single` 分支（旧逻辑单问题直接返回裸文本），统一使用 `[问: header]
回答: val` 格式；更新 6 个单问题测试用例期望值。

## 问题描述

AskUserQuestion 工具的问答结果显示时，问题和答案内部的换行符导致显示不美观。用户期望将换行符替换为空格，使每个问答对在单行内完整显示。

## 症状详情

### 当前显示

```
⏺ User answered Peri's questions:
  ⎿ · 仅摘要行

· 显示范围 [单选]
·   > final_result "只显示一行" 是指...
```

问题和答案内部的换行符导致内容被拆分到多行，影响可读性。

### 期望显示

```
⏺ User answered Peri's questions:
  ⎿ · 仅摘要行 · 显示范围 [单选] · > final_result "只显示一行" 是指只保留第一行摘要...
```

每个问答对保持独立一行，但内部的换行符被替换为空格，内容紧凑显示。

## 相关代码

- `peri-tui/src/ui/message_render.rs:24-73` — `render_ask_user_block()` 函数
- `peri-tui/src/ui/message_render.rs:473-524` — `ToolCallGroup` 中的 AskUser 渲染逻辑

当前解析逻辑：
```rust
for line in block.lines() {
    if let Some(h) = line.strip_prefix("[问: ").and_then(|s| s.strip_suffix(']')) {
        header = h.to_string();
    } else if let Some(a) = line.strip_prefix("回答: ") {
        answer = a.to_string();
    }
}
let text = if !header.is_empty() {
    format!("{} → {}", header, answer)
    // ...
};
```

## 期望改进方向

1. 对解析后的 `header` 和 `answer` 分别调用换行符替换
2. 使用 `.replace('
', " ").replace('
', " ")` 或 `.lines().collect::<Vec<_>>().join(" ")`
3. 保持现有的 `→` 分隔符和每行一个问答对的格式
