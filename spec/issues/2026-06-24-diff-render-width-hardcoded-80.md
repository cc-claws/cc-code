# [BUG] Write/Edit diff 视图宽度硬编码 80，长行被截断

**状态**：Pending
**优先级**：中
**模块**：tui
**创建日期**：2026-06-24
**发现方式**：用户报告

## 现象

Write/Edit 工具的内联 diff 视图（Ctrl+O 切换）中，长行在第 80 列被截断，内容不完整。

## 根因

两处调用 `render_diff` 时硬编码宽度为 80：

1. `peri-tui/src/ui/message_view/mod.rs:70` — `build_diff_lines`
   ```rust
   let mut lines = peri_widgets::diff::render_diff(&diff_input, 80, &peri_widgets::DarkTheme);
   ```

2. `peri-tui/src/app/message_pipeline/reconcile.rs:66` — `try_build_diff_lines`
   ```rust
   let lines = peri_widgets::diff::render_diff(&diff_input, 80, &peri_widgets::DarkTheme);
   ```

`render_diff` 本身接受任意 `width` 参数，函数签名：
```rust
pub fn render_diff(input: &DiffInput, width: usize, theme: &dyn Theme) -> Vec<Line<'static>>
```

## 影响

- 终端宽度 > 80 列时（绝大多数现代终端），diff 行在第 80 列被截断
- 影响 Write 和 Edit 两个工具的 diff 渲染
- 用户无法看到完整的代码变更内容

## 修复方向

将硬编码 `80` 替换为终端实际宽度，需要从调用链透传：

1. `build_diff_lines` — 调用方 `tool_block_with_id()`（mod.rs:748）需传入宽度
2. `try_build_diff_lines` — 调用方 reconcile pipeline（reconcile.rs:241）需传入宽度

注意：diff 渲染结果缓存在 `ToolBlock.diff_lines` 中，宽度变化时需要重新计算。
渲染线程的 `RenderEvent::Resize` 已有宽度变化通知机制，diff_lines 重建可复用此路径。

## 相关文件

- `peri-tui/src/ui/message_view/mod.rs` — `build_diff_lines` + `tool_block_with_id` 调用
- `peri-tui/src/app/message_pipeline/reconcile.rs` — `try_build_diff_lines` + reconcile 调用
- `peri-widgets/src/diff/renderer.rs` — `render_diff_impl` 实现（已支持任意宽度）
- `peri-widgets/src/diff/mod.rs` — `render_diff` 公开入口

## 关联 Issue

- `spec/issues/2026-06-06-edit-diff-view-rendering-not-elegant.md` — 现象 2 记录

## 验证标准

终端宽度变化后，diff 视图自动重新渲染适配新宽度——长行不再被截断在第 80 列。

## 状态变更记录

| 日期 | 从 | 到 | 操作人 | 说明 |
|------|-----|-----|--------|------|
| 2026-06-24 | — | Open | 用户 | 创建 |
| 2026-06-24 | Open | Pending | agent | 修复 push 到 PR #27，等待用户在真实环境验证 |

## 修复记录

### 修复 #1（2026-06-24）

- **操作人**：agent（Claude glm-5.2）
- **用户原意**：终端变宽时 Write/Edit 工具的内联 diff（Ctrl+O 切换）应自动适配宽度，长行不再被截断在第 80 列
- **修复内容**：
  - `MessageViewModel::ToolBlock` 字段 `diff_lines: Option<Vec<Line>>` → `diff_input: Option<DiffInput>`（VM 持源数据而非预渲染缓存）
  - `build_diff_lines` → `build_diff_input`，`try_build_diff_lines` → `try_build_diff_input`（去掉硬编码 80 的 render_diff 调用，只构造 DiffInput）
  - `ui/message_render.rs` detail_mode 分支按 `width.saturating_sub(4)` 实时调 `render_diff`，复用既有 `RenderEvent::Resize` 路径自动响应宽度变化
  - `renderer.rs` 内 `RENDER_CACHE` 按 (input, width) 做 LRU 兜底，避免重复计算
- **涉及 commit**：`74814104`（分支 `hotfix#wismyzhizi2018#6月份#T00011_diff-width-hardcoded`）
- **PR**：https://github.com/wismyzhizi2018/peri/pull/27
- **验证状态**：待验证（用户需在真实环境按 Ctrl+O 切换 detail_mode 后确认 diff 长行不再截断、resize 时自动重新渲染）
