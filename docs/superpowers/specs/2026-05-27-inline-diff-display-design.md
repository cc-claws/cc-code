# Inline Diff Display in Message Stream — Design

**日期**：2026-05-27
**状态**：Approved
**关联 Issue**：`spec/issues/2026-05-27-inline-diff-display-in-message-stream.md`

## 概述

在消息流中为 Write/Edit 工具调用结果添加内嵌 unified diff 视图。利用已有的 `ToolStart`/`ToolEnd` 事件流中的工具入参，在 TUI 渲染层按需生成 diff，零侵入不改工具、不改事件类型、不加中间件。

## 架构方案

**方案 A：TUI 层配对 ToolStart/ToolEnd + peri-widgets 纯渲染**

数据流：
```
ToolStart(name="Edit", input={file_path, old_string, new_string})
ToolEnd(name="Edit", output="Added 3 lines...", is_error=false)
     ↓ TUI 配对事件，构造 DiffInput
     ↓ peri-widgets diff 模块
similar::TextDiff::from_lines(old_string, new_string)
     ↓
DiffHunk[] → render_diff() → Vec<Line<'static>>
     ↓ 内嵌到 ToolBlock 渲染
```

不选择方案 B（改 ToolEnd 事件携带 diff payload）的原因：影响面大，改 peri-agent 核心事件类型 + executor + TUI 事件映射。
不选择方案 C（中间件拦截）的原因：diff 是显示关注点不是中间件关注点，且中间件需额外文件 I/O。

## 1. Diff 计算层（peri-widgets）

### 新增依赖

`similar` crate 加在 `peri-widgets/Cargo.toml`（已在项目 Cargo.lock 中）。

### 新增文件

- `peri-widgets/src/diff/mod.rs` — 类型定义 + 计算入口
- `peri-widgets/src/diff/renderer.rs` — 渲染逻辑
- `peri-widgets/src/diff/mod_test.rs` — 计算测试
- `peri-widgets/src/diff/renderer_test.rs` — 渲染测试

### 核心类型

```rust
// diff/mod.rs

/// Diff 输入（TUI 从 ToolStart.input 构造）
pub struct DiffInput {
    pub file_path: String,
    pub old_content: String,     // Edit: old_string | Write(is_new): ""
    pub new_content: String,     // Edit: new_string | Write: content
    pub is_new_file: bool,
    pub is_deleted_file: bool,   // 预留
    pub is_binary: bool,         // 预留
}

/// 结构化 Hunk
pub struct DiffHunk {
    pub old_start: u32,
    pub old_lines: u32,
    pub new_start: u32,
    pub new_lines: u32,
    pub lines: Vec<DiffLine>,
}

pub enum DiffLine {
    Context { text: String, line_num: u32 },
    Add { text: String, line_num: u32 },
    Remove { text: String, line_num: u32 },
    HunkHeader { text: String },
}

/// 单词级 diff 片段
pub struct WordDiff {
    pub segments: Vec<(String, DiffWordType)>,
}
```

### 计算逻辑

- `similar::TextDiff::from_lines(old, new)` 生成行级 diff
- 对 Add/Remove 行对，用 `similar::TextDiff::from_words(old_line, new_line)` 生成单词级 diff
- 上下文行数：3 行
- 大文件保护：`old_content.len() + new_content.len() > 1MB` 或变更行超过 400 行时，返回截断摘要

## 2. Diff 渲染层（peri-widgets）

### 渲染格式

内嵌在消息流中，无边框，连续行：

```
  42 │ fn main() {                          ← 上下文，默认前景色
  43 │     println!("old");                 ← 上下文
@@ -42,3 +42,3 @@                           ← hunk 头部，cyan (diff_hunk)
- 43 │     return 0;                        ← 删除，红色 (diff_remove)
+ 43 │     return 1;                        ← 新增，绿色 (diff_add)
  44 │ }                                    ← 上下文
```

### Gutter（行号列）

- 格式：`{marker}{old_line:>N} {new_line:>N} │ `，N = 最大行号位数 + 1
- marker：` `（上下文）、`-`（删除）、`+`（新增）
- 删除行只显示旧行号，新增行只显示新行号，上下文行两列都显示

### 单词级高亮

- 在 Add/Remove 行内，用更深的颜色变体（加 BOLD modifier）标记变化的单词片段
- 单词拆分按 whitespace + punctuation 边界
- 40% 变化阈值：单行内变化词超过 40% 时回退到整行着色

### 特殊场景渲染

| 场景 | 渲染 |
|------|------|
| 新文件 | 标题 `+ {file_path}` + 全部行绿色 `+` 前缀 |
| 二进制 | 标题 + "Binary file - cannot display diff" |
| 大文件截断 | 标题 + "+N/-M lines changed (diff too large)" |

### 与现有代码的关系

- `highlight_diff_line()` 和 `is_diff_content()` 保留（处理 LLM 输出的纯文本 diff，与结构化 diff 是独立路径）
- 新的 `render_diff()` 直接从 `DiffInput` → `Vec<Line>`，不经过 markdown 解析

## 3. TUI 消息流集成

### ToolBlock 视图模型扩展

`MessageViewModel::ToolBlock` 新增字段：

```rust
diff_view: Option<DiffInput>,
```

`DiffInput` 实现 `Clone + PartialEq`（用于 rebuild 判断）。

### 事件处理中的 diff 缓存

在 `agent_ops` 层新增缓存：

```rust
pending_diff_inputs: HashMap<String, DiffInput>,
```

处理逻辑：
- `ToolStart(name="Write"/"Edit")` → 提取 `file_path`/`old_string`/`new_string`/`content`，构造 `DiffInput` 并缓存
- `ToolEnd` is_error=false → 取出 `DiffInput`，注入 `ToolBlock.diff_view`，触发 RebuildAll
- `ToolEnd` is_error=true → 丢弃缓存
- 非 Write/Edit 工具 → 不缓存
- 会话中断/轮次切换 → `begin_round()` 清空缓存

### ToolBlock 渲染逻辑变更

`BlockRenderStrategy::ToolCall` 渲染路径中，如果 `diff_view` 存在，在结果文本之后追加 diff 渲染块。diff 块默认展开。

### Write 工具特殊处理

Write 的 ToolStart.input 只有 `file_path` + `content`（新文件内容），没有旧内容：

- `is_new_file = true` → 全部绿色（可渲染）
- `is_new_file = false`（覆盖已有文件）→ TUI 无法读取旧内容，降级为纯文本结果

### 不变更的部分

- `ContentBlockView` 枚举不变
- `MessagePipeline` 的 reconcile/rebuild 逻辑不变
- `AgentEvent` 枚举不变
- `highlight_diff_line()` / `is_diff_content()` 不变

## 4. 边界处理与性能

### 边界场景

| 场景 | 行为 |
|------|------|
| Edit 返回错误（old_string 未找到） | is_error=true → 丢弃缓存，不显示 diff |
| old_string == new_string | 不生成 diff，只显示结果文本 |
| replace_all=true | diff 显示 old→new 的单次替换语义 |
| ToolStart 无匹配 ToolEnd | 缓存留在 pending，下一轮清空 |
| SubAgent 内的 Write/Edit | source_agent_id 区分，diff 在对应 SubAgentGroup 内显示 |
| 入参 JSON 缺失字段 | 构造 DiffInput 失败 → 降级纯文本，不崩溃 |

### 性能

- `similar` TextDiff 计算 O(N)，典型编辑（几十行变更）开销可忽略
- 大文件保护：>1MB 跳过 diff
- diff 渲染结果随 ToolBlock 缓存在 MessageViewModel 中，不重复计算
- 单词级 diff 只在 Add/Remove 对之间执行

### 测试策略

| 层级 | 内容 |
|------|------|
| peri-widgets diff 单元测试 | DiffInput→DiffHunk 正确性；单词级 diff；大文件截断；新文件全绿；空内容 |
| peri-widgets 渲染测试 | DiffInput→Vec\<Line\> span 颜色/文本；gutter 宽度；hunk 头部格式 |
| TUI 集成测试 | ToolStart+ToolEnd → diff_view 注入；非 Write/Edit 不受影响；错误路径降级 |

## 涉及文件

| 文件 | 变更类型 | 说明 |
|------|----------|------|
| `peri-widgets/Cargo.toml` | 修改 | 添加 `similar` 依赖 |
| `peri-widgets/src/diff/mod.rs` | 新增 | Diff 类型 + 计算逻辑 |
| `peri-widgets/src/diff/renderer.rs` | 新增 | 渲染逻辑 |
| `peri-widgets/src/lib.rs` | 修改 | 导出 `diff` 模块 |
| `peri-tui/src/ui/message_view/mod.rs` | 修改 | ToolBlock 新增 `diff_view` 字段 |
| `peri-tui/src/app/agent_ops/` | 修改 | ToolStart/ToolEnd 配对 + diff 缓存 |
| `peri-tui/src/ui/message_render.rs` | 修改 | ToolBlock 渲染时调用 diff 组件 |
| `peri-widgets/src/message_block/blocks.rs` | 修改 | BlockRenderStrategy 支持 diff 渲染 |
