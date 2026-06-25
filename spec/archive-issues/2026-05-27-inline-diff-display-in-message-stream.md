> 归档于 2026-05-27，原路径 spec/issues/2026-05-27-inline-diff-display-in-message-stream.md

# 消息流中内嵌 Diff 视图显示

**状态**：Fixed
**优先级**：中
**创建日期**：2026-05-27

## 问题描述

Write/Edit 工具调用执行后，消息流中只显示纯文本结果（如 "Added 3 lines to src/main.rs"），用户无法直观看到具体修改了哪些内容。需要实现内嵌 unified diff 视图，在工具调用结果中展示文件变更的行级 diff，支持单词级高亮、新文件/删除/二进制等场景，提升用户对 Agent 修改文件的可视化感知。

## 症状详情

**当前行为**：

- Edit 工具返回纯文本如 `"Replaced text (same line count) to src/main.rs"`
- Write 工具返回纯文本如 `"Wrote 42 lines to src/main.rs"`
- 消息流中无 diff 视觉表示，用户需要自己打开文件或执行 git diff 才能查看变更

**期望行为**：

- Edit/Write 工具执行后，消息流中显示结构化 diff 视图
- 视图包含：行号、`+`/`-` 前缀、绿色新增/红色删除着色、hunk 头部 (`@@`)
- 支持单词级 diff（在增/删行内高亮具体变化的单词片段）
- 新文件：全部绿色显示；删除文件：全部红色显示；二进制文件：提示无法展示

## 期望改进方向

### 1. Diff 计算（`peri-widgets` 或独立模块）

- 引入 `similar` crate（已在 Cargo.lock 中，作为 langfuse-client 的传递依赖）或类似 Rust diff 库
- 输入：旧文件内容 + 新文件内容
- 输出：结构化 hunks（oldStart、oldLines、newStart、newLines、变更行列表）
- 支持上下文行数配置（默认 3 行）
- 支持单词级 diff（在行内进一步拆分变更词段）

### 2. Diff 渲染组件（`peri-widgets`）

- 新增 diff 渲染组件，产出 `Vec<Line<'static>>` 供 ratatui 消费
- 渲染要素：
  - **Gutter（行号列）**：左侧显示旧/新文件行号，宽度自适应最大行号位数
  - **Hunk 头部**：`@@ -oldStart,oldLines +newStart,newLines @@` 用青色 (`diff_hunk()`)
  - **新增行**：`+` 前缀 + 绿色 (`diff_add()`)
  - **删除行**：`-` 前缀 + 红色 (`diff_remove()`)
  - **上下文行**：默认前景色
  - **单词级高亮**：在新增/删除行内，用更深的背景色标记变化的单词片段
- 主题色已定义：`Theme::diff_add()`、`Theme::diff_remove()`、`Theme::diff_hunk()`
- 现有 `highlight_diff_line()` 只处理纯文本 diff 字符串，需扩展为结构化渲染

### 3. 消息流集成

- 在 `MessageViewModel` 中新增 diff 相关变体（或在 ToolBlock 中扩展）
- Write/Edit 工具的 `after_tool` 中间件（或 executor 层）收集 old/new 内容
- 将 diff 数据传递到 TUI 渲染层
- 多个连续编辑可合并为单个 diff 块显示

### 4. 特殊文件场景

- **新文件**（Write 创建）：全部行标记为新增，绿色显示
- **删除文件**：全部行标记为删除，红色显示
- **二进制文件**：显示提示 "Binary file - cannot display diff"
- **大文件**：设置 diff 大小上限（如 1MB），超出时显示摘要

## 涉及文件

- `peri-widgets/src/message_block/highlight.rs`（34 行）—— 现有 diff 行高亮，需扩展
- `peri-widgets/src/theme/mod.rs` —— diff 颜色主题定义（已就绪）
- `peri-middlewares/src/tools/filesystem/edit.rs`（185 行）—— Edit 工具，需在返回结果中附带 diff 数据
- `peri-middlewares/src/tools/filesystem/write.rs` —— Write 工具，同上
- `peri-tui/src/ui/message_render.rs` / `peri-tui/src/app/message_pipeline/` —— 消息流渲染集成
- `peri-widgets/src/message_block/blocks.rs` —— 消息块渲染，需新增 diff 组件

## Claude Code 参考实现

Claude Code 的 diff 显示实现要点（供设计参考）：

- **Diff 生成**：使用 `diff` npm 包的 `structuredPatch()` 和 `diffWordsWithSpace()`
- **结构化 Hunk 类型**：`{ oldStart, oldLines, newStart, newLines, lines[] }`
- **双层渲染**：原生 NAPI 模块（高性能，支持语法高亮）+ React fallback
- **单词级 diff**：用 `diffArrays()` 在行内做字符级对比，40% 变化阈值回退到行级
- **Gutter 宽度**：`max(oldStart + oldLines, newStart + newLines).toString().length + 3`
- **性能优化**：WeakMap 缓存渲染结果，避免 resize 时重新计算
- **大文件处理**：1MB 上限 + chunked 读取 + `scanForContext()` 局部加载
- **多编辑合并**：连续编辑按文件合并，顺序应用 edits 后生成单一 patch
- **文件路径**：`packages/builtin-tools/src/tools/FileEditTool/`、`src/components/StructuredDiff.tsx`、`src/utils/diff.ts`

## 实现建议

**推荐 Rust crate**：`similar`（已在依赖图中）

```rust
use similar::{ChangeTag, TextDiff};

let diff = TextDiff::from_lines(old_content, new_content);
for hunk in diff.unified_diff().header(&old_path, &new_path).iter_hunks() {
    // 渲染每个 hunk
}
```

`similar` 支持：

- 行级 diff + 内联 word diff（`TextDiff::from_words()`）
- Unified diff 格式
- 上下文行数配置
- 性能优于纯 JS 实现

**实现分层**：

1. **peri-widgets 层**：纯渲染组件，接收 `DiffInput { old_content, new_content, file_path, is_new, is_deleted, is_binary }`，产出渲染行
2. **中间件/工具层**：Write/Edit 工具在执行前后收集文件内容，产出 diff 数据
3. **消息流层**：将 diff 数据包装为新的 ContentBlock 或 MessageViewModel 变体
