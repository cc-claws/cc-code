> 归档于 2026-05-18，原路径 spec/issues/2026-05-17-pipeline-render-heavy-files.md

# Pipeline/渲染层大文件拆分：message_pipeline.rs + message_view.rs

**状态**：Fixed
**优先级**：高
**创建日期**：2026-05-17
**解决日期**：2026-05-17
**修复 commit**：`3673fdb`

## 问题描述

`message_pipeline.rs`（1067 行）和 `message_view.rs`（1061 行）是消息系统的核心，两者都超过 1000 行严重线。Pipeline 混合了流式事件处理、reconcile、tail 构建、视图模型转换；message_view 混合了渲染逻辑和布局计算。

## 现状数据

| 文件 | 行数 | 大小 | 主要问题 |
|------|------|------|---------|
| `peri-tui/src/app/message_pipeline.rs` | 1067 | 44KB | 23 个 pub 函数，Pipeline 核心逻辑 + reconcile + tail 构建 + view model 转换 |
| `peri-tui/src/ui/message_view.rs` | 1061 | 38KB | 26 个 pub 函数，渲染 + 布局计算 + 滚动逻辑 |

两个文件是消息显示链路的上下游，修改时跨文件认知负担大。

## 期望改进方向

- `message_pipeline.rs` → `message_pipeline/transform.rs`（消息转换） + `message_pipeline/reconcile.rs`（reconcile 逻辑） + `message_pipeline/view_model.rs`（VM 构建）
- `message_view.rs` → `message_view/render.rs`（渲染逻辑） + `message_view/layout.rs`（布局计算） + `message_view/scroll.rs`（滚动逻辑）

注意修改涉及 [TRAP] 约束：Ephemeral VM 锚点机制、`prefix_len` vs `round_start_vm_idx` 维度区分、`RebuildAll` 只能在非 Pipeline 层触发。

## 涉及文件

- `peri-tui/src/app/message_pipeline.rs`（1067 行，44KB）
- `peri-tui/src/ui/message_view.rs`（1061 行，38KB）
