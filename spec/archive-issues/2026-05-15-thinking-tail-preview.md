> 归档于 2026-05-16，原路径 spec/issues/2026-05-15-thinking-tail-preview.md

# Thinking 尾部预览：最后一条 AI 消息无正文时展示思考最后 1 行

**状态**：Fixed
**优先级**：低
**创建日期**：2026-05-15
**解决日期**：2026-05-15

## 问题描述

当前 `ContentBlockView::Reasoning` 只存储字符数，渲染时统一显示 `"Thought for N chars"`（dim 颜色），思考内容本身不可见。用户希望在 **最后一条 AI 消息** 中，如果该消息没有 Text 正文、且最后一个 content block 是 Reasoning，则展示该思考内容的最后 1 行作为尾部预览。

## 症状详情

### 当前行为

```text
● Thought for 188 chars    ← 只显示字数，不知道在想什么
```

### 期望行为

```text
● Thought for 188 chars
 ⎿ xxxxx
```

## 触发条件

1. 是对话中**最后一条 AI 消息**
2. 该消息中**没有任何 `ContentBlockView::Text` block**（纯思考消息）
3. 最后一个 content block 是 `ContentBlockView::Reasoning`

三个条件**同时满足**时，在 `"Thought for N chars"` 行下方追加最后 1 行思考内容。

## 期望实现

### 数据层

- `ContentBlockView::Reasoning` 当前只有 `char_count: usize`，需增加 `text: String` 字段
- `PartialEq` / `Hash` 实现：`text` 不参与比较/哈希（只用于展示，相同的 char_count 意味着相同的 content）
- `message_view.rs:680-681` 转换处传入完整 text

### 控制层（message_pipeline.rs）

- `messages_to_view_models()` 中，因 `ContentBlockView::Reasoning { char_count > 0 }` 当前即被判定为 `has_visible`（行 840），该消息不会被过滤，无需修改跳过逻辑
- 需要标识"最后一条 AI 消息"，建议在 vms 构建完成后扫描最后一个 `AssistantBubble`：如果其 blocks 无 `Text` 且最后一个 block 是 `Reasoning`，则将尾行文本写入该 VM 的 blocks 中

### 渲染层（message_render.rs:265-278）

- 渲染 `ContentBlockView::Reasoning` 时，如果存在 `tail_text`（新增字段），在 `"Thought for N chars"` 行之后追加 `⎿ ` 前缀的行（dim 颜色，纯文本，不经过 markdown 渲染）

### 行提取算法

```rust
fn extract_tail_lines(text: &str, max_lines: usize) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let start = lines.len().saturating_sub(max_lines);
    lines[start..].join("
")
}
```

- 严格按换行符切分
- 单行不截断（允许超长行）
- 最多 1 行

## 实现摘要（2026-05-15）

最终实现展示最后 **1 行**（非原计划的 4 行），原因：减少界面占用，仅提供最小可读上下文。

**改动文件：**

| 文件 | 改动 |
|------|------|
| `peri-tui/src/ui/message_view.rs` | `ContentBlockView::Reasoning` 新增 `text: String`、`tail_lines: Option<String>` 字段 |
| `peri-tui/src/app/message_pipeline.rs` | `build_streaming_bubble()` 传入 text、新增 `extract_tail_lines()` 和 `add_thinking_tail_snapshot()` |
| `peri-tui/src/app/message_pipeline_test.rs` | 3 个 `extract_tail_lines` 单元测试 |
| `peri-tui/src/ui/message_render.rs` | Reasoning 渲染追加 `⎿ ` 前缀的 dim 尾部行 |

**实现要点：**
- `tail_lines` 参与 Hash（变化触发重渲染），`text` 不参与（仅 char_count 决定等价性）
- 后处理在 `build_tail_vms()` 末尾执行，扫描最后一个 `AssistantBubble`
- 条件：无 `ContentBlockView::Text` + 最后一个 block 是 `Reasoning`
- 尾部行不受 `first_text_merged` 限制，独立追加渲染
