# Word diff 渲染 old/new 值拼接在同一行

**状态**：Fixed
**优先级**：中
**创建日期**：2026-06-13

## 问题描述

Edit 工具的内联 diff 视图中，单行替换场景下 word diff 渲染器把 old 值和 new 值拼在同一行显示，例如 `256;1024;`，而不是分别显示 `256;` 和 `1024;`。

## 根因

`render_word_diff_spans`（`peri-widgets/src/diff/renderer.rs`）渲染了 word diff 的**所有** segments（Added + Removed + Unchanged），但：

- Remove 行应只显示 Removed + Unchanged 段
- Add 行应只显示 Added + Unchanged 段

原代码没有过滤，导致 Remove 行和 Add 行都显示了完整的新旧内容。

## 复现条件

- **复现频率**：必现
- **触发步骤**：
  1. 使用 Edit 工具修改单行内容（如 `256` → `1024`）
  2. 查看 diff 视图
  3. Remove 行和 Add 行都显示 `256;1024;`
- **环境**：所有 OS

## 涉及文件

- `peri-widgets/src/diff/renderer.rs` — `render_word_diff_spans` 函数

## 状态变更记录

| 日期 | 从 | 到 | 操作人 | 说明 |
|------|-----|-----|--------|------|
| 2026-06-13 | — | Open | agent | 创建 |
| 2026-06-13 | Open | Fixed | agent | 修复：给 `render_word_diff_spans` 加 `is_add` 参数过滤段类型 |

## 修复记录

- **改动**：`render_word_diff_spans` 新增 `is_add: bool` 参数，Add 行跳过 `Removed` 段，Remove 行跳过 `Added` 段
- **验证**：新增 `test_render_diff_single_line_edit_no_concat` 测试，peri-widgets 全量 130 个测试通过
