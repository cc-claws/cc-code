# Feature: 20260513_F001 - streaming-rendering

## 需求背景

当前 TUI 的所有消息更新都通过 `RebuildAll` 路径触发（100ms 节流），每次将完整的尾部 VMs 重建后发送给渲染线程。渲染线程对全量消息做 hash diff 跳过未变化的消息，但存在三个问题：

1. **RebuildAll 的 drain-rebuild 模式**导致渲染线程每次都重新拼接所有消息行（即使大部分消息 hash 未变），前缀消息的 `message_lines[i]` 虽被保留但需要遍历拼接
2. **Reconcile 视觉回退**：`build_streaming_bubble()` 和 `messages_to_view_models()` 两条路径产生视觉不同的 VMs，在 ToolStart/Done 边界处内容"跳变"
3. **段落位置不稳定**：流式追加文本时整个 AssistantBubble 的 markdown 被重新解析，已有段落的渲染结果可能因后续内容而变化（如未闭合的代码块导致前文被重新解释）

## 目标

- 渲染线程 Rebuild 时，hash 前缀完全复用渲染缓存，只重新渲染变化的消息
- 流式追加文本时，已完成段落的渲染结果被缓存，只增量解析新增内容
- Reconcile 边界处不产生视觉回退（内容位置、样式保持一致）

## 方案设计

### 2.1 消息级 Hash 前缀保留

**当前机制**：`RenderTask.rebuild()` 接收完整 VMs 列表，对每条消息计算 hash，跳过 hash 未变的消息渲染，但每次都需要遍历全部 `message_lines` 拼接为 `all_lines`。

**改进**：引入 `prefix_stable_len` 概念——记录前缀中 hash 未变的连续消息数量。

```
rebuild(messages):
  new_hashes = compute_hashes(messages)

  // 找到第一个 hash 变化的消息
  prefix_stable_len = 0
  for i in 0..min(old_hashes.len(), new_hashes.len()):
      if new_hashes[i] == old_hashes[i]:
          prefix_stable_len = i + 1
      else:
          break

  // 只重新渲染 hash 变化的消息及新增消息
  for i in prefix_stable_len..messages.len():
      message_lines[i] = render_one(messages[i])

  // 拼接：前缀直接复用，只拼接变化部分
  all_lines = message_lines[0..prefix_stable_len] + message_lines[prefix_stable_len..]
  offsets = compute_offsets(all_lines)
```

**关键细节**：

- `prefix_stable_len` 之前的 `message_lines[i]` 完全不动，直接拼接
- hash 变化后的所有消息都重新渲染（因为后续消息的位置依赖前面的行数）
- 新增消息（`messages.len() > old_hashes.len()`）自然走渲染路径
- 消息删除（`messages.len() < old_hashes.len()`）时从变化点开始重建

### 2.2 增量 Markdown 解析

**问题**：当前 `ensure_rendered(block, width)` 每次对整个 `raw` 文本做 `parse_markdown`，流式追加时即使只加了几个字符也会重解析全文。

**方案**：在 `ContentBlockView::Text` 中引入 `rendered_prefix_len` 字段，记录已渲染的 `raw` 字符长度。只对新增部分做增量解析。

```rust
// ContentBlockView::Text 新增字段
ContentBlockView::Text {
    raw: String,
    rendered: Text<'static>,
    dirty: bool,
    rendered_prefix_len: usize,  // 新增：已渲染到 raw 的第几个字符
}
```

**增量解析策略——块级边界检测**：

```
ensure_rendered_incremental(block, width):
  if !dirty || raw.len() == rendered_prefix_len:
      return  // 无新内容

  new_text = raw[rendered_prefix_len..]

  // 检查最后一个已渲染块是否可能受新内容影响
  // 例如：未闭合的代码围栏 ``` 需要重解析到最后一个块级边界
  last_stable_boundary = find_last_block_boundary(raw, rendered_prefix_len)

  if last_stable_boundary < rendered_prefix_len:
      // 之前有未稳定的块（如未闭合代码块），需要从该边界重解析
      keep_lines = rendered.lines[..last_stable_boundary对应的行数]
      reparse_text = raw[last_stable_boundary..]
      new_lines = parse_markdown(reparse_text, width)
      rendered = keep_lines + new_lines
  else:
      // 前文全部稳定，只解析新增部分
      new_lines = parse_markdown(new_text, width)
      append_to_rendered(rendered, new_lines)

  rendered_prefix_len = raw.len()
  dirty = false
```

**块级边界**定义为以下位置之一：

- 空行（`\n\n`，段落分隔）
- `#` 标题行起始
- `` ``` `` 代码围栏闭合后的下一行
- `---`/`***` 水平线
- 列表/引用块的起始

**未闭合元素处理**：如果 `raw` 末尾存在未闭合的代码围栏，增量解析会回退到最后一个稳定块级边界。这保证已渲染段落的正确性。

**`find_last_block_boundary` 实现策略**：

在 `raw` 的 `[0..rendered_prefix_len]` 范围内，从后向前扫描空行（`\n\n`），空行即为段落边界。但需排除代码块内的空行。用简单的状态机跟踪"是否在代码围栏内"即可：

1. 从 `rendered_prefix_len` 向前扫描字符
2. 维护 `in_code_fence: bool` 状态
3. 遇到 ``\n`` 后跟 ````` `` 时翻转 `in_code_fence`
4. 遇到 `\n\n`（空行）且不在代码围栏内时，返回该位置
5. 如果扫到 `0` 仍未找到边界，返回 0（全量重解析）

### 2.3 Reconcile 视觉对齐

**问题根源**：`build_streaming_bubble()` 产生的 VM 与 `messages_to_view_models()` 产生的 VM 存在差异：

- markdown 解析时机不同（streaming 用 default width 80，reconcile 用实际终端宽度）
- 折叠状态不同（streaming 的 tool block 默认 collapsed=true，reconcile 从 BaseMessage 重建时可能有不同行为）
- SubAgentGroup 的内容差异（streaming 有 recent_messages，reconcile 从 BaseMessage 重建时 recent_messages 为空）

**方案**：Reconcile 后对比新旧 VMs，对"语义等价"的消息保留旧渲染缓存。

```
rebuild(messages):
  // ... 计算 prefix_stable_len ...

  // 对 hash 变化的消息，进一步检查是否为"语义等价"变更
  for i in prefix_stable_len..messages.len():
      if i < old_messages.len():
          if is_cosmetic_change(old_messages[i], messages[i]):
              // is_streaming: true → false 等纯 UI 标志变化
              // 不影响渲染输出，复用旧缓存
              message_lines[i] = old_message_lines[i]
              continue
      message_lines[i] = render_one(messages[i])
```

**`is_cosmetic_change` 判定规则**：

| 场景 | 是否重渲染 | 理由 |
|------|-----------|------|
| `AssistantBubble` 的 `is_streaming` 从 `true → false`，blocks 内容不变 | 复用 | 文本和样式完全一致 |
| `SubAgentGroup` 的 `is_running` 从 `true → false`，其他字段不变 | 复用 | 渲染输出一致 |
| `ToolBlock` 的 `collapsed` 状态变化 | 重渲染 | 折叠/展开影响行数和显示 |
| `AssistantBubble` 的 `blocks` 内容变化 | 重渲染 | 文本内容改变 |
| `SubAgentGroup` 的 `recent_messages` 变化 | 重渲染 | 内部消息改变 |

### 2.4 数据结构变更汇总

```rust
// ContentBlockView::Text 新增字段
pub enum ContentBlockView {
    Text {
        raw: String,
        rendered: Text<'static>,
        dirty: bool,
        rendered_prefix_len: usize,  // 新增
    },
    // 其他变体不变
}

// RenderTask 新增字段
struct RenderTask {
    // 现有字段保持不变
    last_messages: Vec<MessageViewModel>,
    message_lines: Vec<Vec<Line<'static>>>,
    message_hashes: Vec<u64>,
    // ...
    prefix_stable_len: usize,  // 新增：前缀稳定长度（调试用）
}

// 新增辅助函数
fn find_last_block_boundary(text: &str, prefix_len: usize) -> usize;
fn is_cosmetic_change(old: &MessageViewModel, new: &MessageViewModel) -> bool;
```

### 2.5 改动范围

| 文件 | 改动内容 |
|------|----------|
| `peri-tui/src/ui/render_thread.rs` | `rebuild()` 增加 `prefix_stable_len` 逻辑 + `is_cosmetic_change()` 判定 |
| `peri-tui/src/ui/message_view.rs` | `ContentBlockView::Text` 新增 `rendered_prefix_len` 字段 |
| `peri-tui/src/ui/markdown.rs` | `ensure_rendered()` 改为增量解析，新增 `find_last_block_boundary()` |
| `peri-tui/src/ui/markdown_test.rs` | 增量解析单元测试 |
| `peri-tui/src/ui/render_thread.rs`（测试部分） | 前缀保留和视觉对齐单元测试 |

## 实现要点

### 关键难点 1：增量 Markdown 解析的边界检测

`find_last_block_boundary()` 需要处理多种 Markdown 结构：

- 代码围栏（`` ``` ``）：未闭合时回退到围栏起始位置
- 表格（`|---|` 分隔行）：未闭合时回退到表头
- 嵌套列表：`>` 引用块内的列表项

实现策略：在 `raw` 的 `[0..rendered_prefix_len]` 范围内，从后向前扫描空行（`\n\n`），空行即为段落边界。但需排除代码块内的空行。用简单的状态机跟踪"是否在代码块内"即可。

### 关键难点 2：hash 一致性保证

当前 `MessageViewModel` 的 `Hash` 实现包含 `is_streaming`、`collapsed` 等 UI 字段。这意味着 `is_streaming: true → false` 会导致 hash 变化，触发不必要的重渲染。

`is_cosmetic_change()` 需要覆盖这类场景：虽然 hash 变了，但渲染输出实际不变。具体规则：

- `AssistantBubble`：忽略 `is_streaming` 变化，比较 `blocks` 内容
- `SubAgentGroup`：忽略 `is_running` 变化，比较 `agent_id`/`task_preview`/`recent_messages`/`final_result`

### 关键难点 3：`prefix_stable_len` 与 `message_lines` 的同步

当消息被删除或插入时（如 `aggregate_tool_groups` 合并工具块），`prefix_stable_len` 可能指向错误的消息。处理方式：

- 如果新旧消息数量不同，`prefix_stable_len` 截断到 `min(old, new)`
- 如果 hash 在中间位置不连续变化（如消息 i 和 i+2 变化但 i+1 未变），仍从第一个变化点开始全量重建后续

### 依赖变更

无新 crate 依赖，所有改动在现有代码结构内。

## 约束一致性

与 `spec/global/constraints.md` 和 `spec/global/architecture.md` 的一致性：

- **双线程渲染架构**：保持不变。`RenderTask` 仍在独立线程运行，UI 线程只读 `RenderCache`。
- **事件驱动 TUI 通信**：保持不变。渲染事件仍通过 `mpsc::UnboundedSender<RenderEvent>` 发送。
- **消息管线统一**：保持不变。`MessagePipeline` 仍为消息状态管理唯一入口。
- **Widget 独立 crate**：无影响。`peri-widgets` 不涉及此次改动。
- **编码规范**：遵循 Rust 2021 edition，`parking_lot::RwLock`，`tracing` 日志，测试分离为 `_test.rs` 文件。

无架构偏离，无新增约束。

## 验收标准

- [ ] 渲染线程 `rebuild()` 中，hash 前缀未变的消息直接复用 `message_lines` 缓存，不调用 `render_one()`
- [ ] 增量 markdown 解析：流式追加文本时，已完成段落的渲染结果不被重解析，只解析新增内容
- [ ] `is_cosmetic_change()` 覆盖 `is_streaming: true → false` 场景，不触发重渲染
- [ ] Reconcile 边界（ToolStart/Done）处不产生视觉跳变（已有段落的行位置不变）
- [ ] 现有测试全部通过，新增增量解析和前缀保留的单元测试
- [ ] 无新 crate 依赖，改动范围限于 `render_thread.rs`、`message_view.rs`、`markdown.rs`
