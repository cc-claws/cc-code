# 非详细模式移除思考尾部预览（消除流式闪烁）

**状态**：Fixed
**优先级**：中（影响非详细模式默认观感 + 流式可读性）
**创建日期**：2026-10-07
**分支**：`investigate/thought-collapse`

---

## 一、问题描述

用户在非详细模式（默认，未按 `Ctrl+O`）下，看到思考摘要行
`∴ Thought for 6382 chars (ctrl+o to expand)` **下方还多出几行思考内容**
（`  ⎿ ` 前缀、DIM 灰色、尾部最多 3 行），反馈「闪的眼睛疼」。

用户的设计意图是：**非详细模式下只保留摘要行**，正文区只暴露
`Thought for N chars` / `Thought for Ns` 这一个数字的变化，
思考内容本身**必须**按 `Ctrl+O` 展开才可见。

### 症状详情

**当前行为（非详细模式）**：

```text
  Thought for 6382 chars (ctrl+o to expand)
  ⎿ 我来分析一下这个问题……
    首先要确认渲染链路……
    再看看 reconcile 的条件……
```

**期望行为（非详细模式）**：

```text
  Thought for 6382 chars (ctrl+o to expand)
```

（摘要行下方**没有任何思考内容**；`Ctrl+O` 后显示完整推理。）

### 闪烁成因

思考尾部预览由 `build_tail_vms()` **末尾**的后处理注入
（`add_thinking_tail_snapshot`），而**流式进行中的 bubble 正是 `tail_vms`
里的最后一个 `AssistantBubble`**，因此流式期间：

- 每来一个 reasoning chunk → 末尾 3 行预览整体重算 → 行数/内容跳动 → 视觉闪烁；
- 摘要行的 `char_count` 与预览内容**同时**变化，重绘面积被放大。

## 二、根因

这不是 2026-09-29 那份设计稿（`docs/designs/2026-09-29-thinking-status-line-and-tool-summary.md`）
的产物——该设计稿**从未规定**非详细模式要输出思考内容预览
（`grep "尾部预览\|tail"` 在该文档零命中，其 §3.2 效果稿摘要行下方亦为空）。

真正的来源是一份更早的独立特性 **`2026-05-15-thinking-tail-preview`**
（`spec/archive-issues/2026-05-15-thinking-tail-preview.md`，状态 Fixed）：
在「最后一条 AI 消息、无正文、末 block 为 Reasoning」时展示思考**尾部 1 行**
（后由 `2787eca6` 改为 3 行以「消除单行超宽导致的 1↔2 行布局抖动」）。

两个特性在时间上叠加后产生了**设计冲突**：2026-09-29 的新规格要求
「非详细模式只留摘要行」，而 05-15 的旧预览无条件在摘要行下方追加内容。
本 issue 依据 2026-09-29 规格裁决：**非详细模式不再渲染思考内容预览**。

## 三、改动

| 文件 | 改动 |
|------|------|
| `cc-tui/src/ui/message_view/mod.rs` | `ContentBlockView::Reasoning` 删除 `tail_lines` 字段（及 `Hash` 中的引用、构造点的 `tail_lines: None`） |
| `cc-tui/src/ui/message_render.rs` | 非详细模式 `content` 恒为 `None`，不再取 `tail_lines`；更新注释说明历史与裁决 |
| `cc-tui/src/app/message_pipeline/reconcile.rs` | 删除 `extract_tail_lines()` 与 `add_thinking_tail_snapshot()`；移除 `build_tail_vms()` 末尾的调用；清理不再使用的 `ContentBlockView` 导入 |
| `cc-tui/src/app/message_pipeline/mod.rs` | 删除 `extract_tail_lines` 的测试用导入 |
| `cc-tui/src/app/message_pipeline/transform.rs` | `build_streaming_bubble()` 构造点去掉 `tail_lines: None` |
| `cc-tui/src/app/message_pipeline/message_pipeline_test.rs` | 删除 `extract_tail_lines` 的 3 个单元测试（函数已移除） |

**保留**：`ContentBlockView::Reasoning.text` 字段（详细模式 `Ctrl+O` 展开时仍需展示完整推理）。

**未改动**（确认无需动）：`PartialEq` 本就未比较 `tail_lines`；详细模式渲染路径不受影响。

## 四、验证

- `cargo build -p cc-tui`：通过，无 warning。
- `cargo clippy -p cc-tui`：无 warning/error。
- `cargo test -p cc-tui --lib -- message_pipeline message_render render_thread reasoning`：
  **201 passed / 0 failed**。
- 全量 `cargo test -p cc-tui --lib`：1054 passed / 4 failed，
  4 个失败**全部**为 shell/后台任务生命周期用例
  （`agent_shell_*`、`shell_exec_lifecycle`，Windows 路径/子进程问题），
  与本改动无关。

## 五、人工验证要点

1. 非详细模式（默认）触发一次长思考：摘要行下方**不应**出现任何 `⎿` 内容行。
2. `Ctrl+O` 切详细模式：应能看到完整思考内容。
3. 再 `Ctrl+O` 切回非详细：思考内容应重新折叠为仅摘要行。
4. 流式思考期间观察：摘要行 `chars` 数字更新，但**无**多行内容滚动/跳动。

## 六、关联

- 冲突来源：`spec/archive-issues/2026-05-15-thinking-tail-preview.md`（旧特性，本 issue 在非详细模式予以推翻）
- 裁决依据：`docs/designs/2026-09-29-thinking-status-line-and-tool-summary.md`（§2.3 / §3.2）
- 相关历史提交：`846deb89`（引入尾部预览）、`2787eca6`（1 行 → 3 行）
