# 长上下文会话恢复时 CPU 短暂暴涨

**状态**：Partial（P0-1/P0-2/P1 已完成，P2-P4 待定）
**优先级**：低
**类型**：性能
**创建日期**：2026-05-30
**更新日期**：2026-05-30

## 问题描述

使用 `-c`/`-r` 参数恢复一个上下文较长的会话（50 轮+）时，加载瞬间 CPU 会短暂飙升至 100%+，持续数秒后恢复正常。不影响正常使用，但体验上有明显卡顿。

## 症状详情

| 维度 | 观察 |
|------|------|
| 触发场景 | `-c` 继续最近会话 / `-r <id>` 恢复指定会话 |
| 上下文长度 | 50 轮以上对话，含大量工具调用/文件读取 |
| CPU 表现 | 加载瞬间飙升，几秒后恢复 |
| 用户体验 | 可接受，但能优化最好 |
| 复现频率 | 必现（长上下文会话） |

## 复现条件

- **复现频率**：必现
- **触发步骤**：
  1. 进行 50 轮以上的对话（含工具调用）
  2. 退出 TUI
  3. 使用 `peri -c` 或 `peri -r <session-id>` 恢复该会话
  4. 观察加载期间 CPU 占用
- **环境**：macOS，任意模型

## 涉及文件

- `peri-agent/src/thread/sqlite_store.rs` —— `load_context()`：SQLite 读取 + 逐条 JSON 反序列化
- `peri-tui/src/ui/message_view/mod.rs` —— `from_base_message_with_cwd()`：首次 Markdown 解析（宽度80）
- `peri-tui/src/ui/render_thread.rs` —— `render_one()` + `build_wrap_map()`：二次 Markdown 解析 + wrap 计算
- `peri-tui/src/app/message_pipeline/transform.rs` —— `messages_to_view_models()`：消息到 VM 的全量转换
- `peri-tui/src/app/thread_ops.rs` —— `open_thread()`：恢复会话的主流程（TUI + ACP 双重加载）
- `peri-acp/src/dispatch/session_load.rs` —— `load_session_messages()`：ACP 侧加载

## 根因分析（2026-05-30 系统性调试）

会话恢复的 CPU 热点是一个 **串行管线**，50 轮对话（约 150+ 条消息，500+ 个 ContentBlock）时累积开销大。

### 串行管线

| 阶段 | 操作 | 文件 | 热点 |
|------|------|------|------|
| ① SQLite 读取 | `load_context()` 逐条 `serde_json::from_str` | `sqlite_store.rs:455-519` | O(n) 反序列化，cached_context 缓存有效时减少 DB 查询 |
| ② VM 转换 | `from_base_message_with_cwd()` → `parse_markdown_default()` | `message_view/mod.rs:436-620` | **每条文本消息/ContentBlock 调用一次** pulldown-cmark 解析（宽度=80） |
| ③ 渲染重建 | `render_one()` → `parse_markdown(content, width)` | `render_thread.rs:166-190` | **UserBubble 再次用实际宽度重新解析** Markdown |
| ④ wrap 计算 | `build_wrap_map()` → `Paragraph::line_count()` | `render_thread.rs:127-163` | **每个逻辑行创建 Paragraph 并计算 wrap** |
| ⑤ ACP 同步 | `load_session()` → `load_context()` | `thread_ops.rs:197-208` | TUI 端加载完成后，ACP 侧再走一遍 load_context |

### 关键发现

1. **Markdown 被解析了两次**：`from_base_message_with_cwd()` 用 `parse_markdown_default()`（宽度80）创建 UserBubble/TextBlock，`render_one()` 又用 `parse_markdown(content, width)`（实际终端宽度）重新解析 UserBubble
2. **wrap 计算极昂贵**：`build_wrap_map()` 中每个逻辑行都创建 `Paragraph` 对象并调用 `line_count()`，复杂度与总行数线性相关
3. **双重 load_context**：TUI 侧 `open_thread()` 调用 `store.load_messages()`，然后 ACP 侧通过 `client.load_session()` 再走一遍 `load_context()`（含 cached_context 读取 + 增量检查）。两条路径职责不同（TUI 显示 vs ACP 执行上下文），但串行执行增加了延迟
4. **cached_context 有效**：`load_context()` 先查 `cached_context` 列，命中时直接反序列化一个大的 JSON 字符串（50 轮约 500KB-1MB），跳过多条 SQL 查询

### 数据规模估算（50 轮对话）

| 指标 | 估算值 |
|------|--------|
| 消息数 | ~150-200 条 |
| ContentBlock 总数 | ~500-800 个 |
| Markdown 解析次数 | ~1000-1600 次（双重解析 × 消息数） |
| wrap 计算行数 | ~2000-5000 行 |
| cached_context JSON 大小 | ~500KB-1MB |

### 优化方向

| 优先级 | 方向 | 预期收益 |
|--------|------|----------|
| P0 | 消除 Markdown 双重解析 | 减少 50% markdown 解析开销 |
| P0 | 消除 wrap 重复计算 | 减少 50% wrap 计算开销 |
| P1 | TUI 侧改用 `load_context()` | 省一次 SQLite 读取 |
| P2 | 手动 wrap 算法替代 `Paragraph::line_count` | 减少 30% 额外开销 |
| P3 | wrap 结果缓存 | 恢复时 -100% wrap 开销 |
| P4 | 可见区域优先渲染（虚拟滚动） | 首屏 O(n) → O(visible) |

## 审阅与方案（2026-05-30 三 agent 并行审阅）

### 审阅结论

三个 agent（Markdown 解析 / wrap 计算 / 架构级优化）一致确认根因分析准确，并补充了一个遗漏：

- **wrap 重复计算**：`rebuild()` 第 354-355 行对同一批 `lines` 先调用 `compute_wrapped_height()`（对所有行调用 `Paragraph::line_count`），再调用 `build_wrap_map()`（对每行单独调用 `Paragraph::line_count`），两者遍历完全重复

### 方案详情

#### P0-1：消除 wrap 重复计算

**文件**：`peri-tui/src/ui/render_thread.rs`

**改动**：
- 删除 `compute_wrapped_height()` 函数（第 73-81 行）
- 修改 `build_wrap_map()` 返回 `(usize, Vec<WrappedLineInfo>)` 元组，内部累加 `total_lines`
- `rebuild()` 中拆解元组赋值

**风险**：无，纯重构

#### P0-2：Markdown 延迟解析

**文件**：`peri-tui/src/ui/message_view/mod.rs`、`peri-tui/src/ui/render_thread.rs`

**改动**：
- `from_base_message_with_cwd()` 中 `UserBubble` 不调用 `parse_markdown_default()`，`rendered` 字段存空 `Text`
- `AssistantBubble` 的 `ContentBlockView::Text` 不调用 `parse_markdown_default()`，`rendered` 存空，`dirty` 标记为 `true`
- `render_one()` 中已有的 `parse_markdown(content, width)` 和 `ensure_rendered_incremental(block, width)` 负责实际解析
- 流式路径不变：流式事件仍然通过 `ensure_rendered_incremental()` 增量解析

**风险**：低。`messages_to_view_models()` 产出 VM 后不消费 `rendered` 字段，只有 `render_one()` 消费

#### P1：TUI 侧改用 `load_context()`

**文件**：`peri-tui/src/app/thread_ops.rs`

**改动**：
- `open_thread()` 第 155-159 行：`store.load_messages(&tid)` 改为 `store.load_context(&tid)`
- ACP 侧 `load_session()` 保留（确保 server 端状态同步），但第二次加载会命中 `cached_context` 缓存

**风险**：低。`load_context()` 包含祖先链逻辑，功能上是 `load_messages()` 的超集

#### P2：手动 wrap 算法

**文件**：新增 `peri-tui/src/ui/render_thread/wrap.rs`

**改动**：
- 实现轻量级 `fast_wrap_line(text, char_widths, width) -> u16`，直接用已有的 `char_widths` 累加计算
- 替换 `build_wrap_map()` 中每行的 `Paragraph::new(text).wrap(Wrap{trim:false}).line_count(width)` 调用
- 必须与 ratatui `WordWrapper` 算法结果完全一致（CJK、超长单词、空行边界）

**风险**：中。需充分测试一致性

#### P3：wrap 结果缓存

**文件**：`peri-tui/src/ui/render_thread.rs`

**改动**：
- `RenderCache` 增加 `wrap_width: u16` 字段
- `RenderTask` 增加 `last_wrap_cache: Option<(u16, usize, Vec<WrappedLineInfo>)>`
- `rebuild()` 中如果宽度未变且 lines 内容相同（通过 hash 验证），直接复用上次 wrap_map
- `Resize` 事件清空缓存

**风险**：低。额外内存约 200-500KB（可接受）

#### P4：可见区域优先渲染

**文件**：`peri-tui/src/ui/render_thread.rs`、`peri-tui/src/app/message_pipeline/`

**改动**：
- 新增 `RenderEvent::RebuildVisible { all_messages, visible_range }`
- 首次恢复只渲染前 N 条消息（约 40-60 行），wrap_map 仅覆盖已渲染区域
- 滚动接近边界时触发增量渲染（加载下 N 条）
- 需重新设计 `prefix_stable_len` 机制以支持部分渲染

**风险**：高。需大幅修改渲染线程和滚动逻辑，可能破坏 hash diff 优化

### 推荐实施顺序

| 阶段 | 方案 | 工期 |
|------|------|------|
| 立即 | P0-1（wrap 去重）+ P0-2（Markdown 延迟）+ P1（load_context 统一） | 1 天 |
| 短期 | P2（手动 wrap 算法） | 2-3 天（含测试） |
| 中期 | P3（wrap 缓存） | 1-2 天 |
| 长期 | P4（虚拟滚动） | 2-4 周（架构重构） |

## 关联 Issue

- `spec/issues/2026-05-22-memory-linear-growth-no-compact.md` —— 内存持续增长问题，同属长上下文性能范畴
