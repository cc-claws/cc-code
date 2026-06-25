> 归档于 2026-05-16，原路径 spec/issues/2026-05-13-context-tab-per-request-token-chart.md
# /context Tab 增加每次请求的 token 柱状图和缓存命中率柱状图

**状态**：Fixed
**优先级**：低
**创建日期**：2026-05-13
**类型**：Feature

## 问题描述

当前 `/context` 的 Context tab 仅展示累计统计值（上下文窗口大小、已用 token、使用率百分比、消息数等），无法观察 token 消耗和缓存效率随对话轮次的变化趋势。希望在 Context tab 中增加基于每次 LLM 请求的可视化图表，帮助用户直观感知 token 增长和缓存命中情况。

## 需求详情

### 数据收集

- 在 `TokenTracker`（`peri-agent/src/agent/token.rs`）中新增每次请求的历史记录数组
- 每次 `accumulate()` 调用时，将当次 `TokenUsage` 追加到历史记录中（仅内存，不持久化）
- 当前会话 `/new` 或 compact 时可重置

### 图表内容

1. **Input token 堆叠柱状图**：每根柱子代表一次 LLM 调用的 input token 数量，堆叠区域分为：
   - cache_read（绿色 SAGE）
   - cache_creation（橙色 WARNING）
   - 非缓存部分（ACCENT）
2. **缓存命中率柱状图**：独立图表区域，y 轴 0-100% 刻度，█ 填充，与 token 柱状图风格统一

### 展示方式

- **替换 Context tab 内容**，原有统计信息（上下文窗口、已用 token、使用率、消息数、工具调用次数）作为图表上方的摘要行保留
- 面板高度从 14 行增大到 20 行
- Tab 样式仿照 plugin 面板（手动渲染，激活态 `bg(THINKING)` + `BOLD`）

## 实现记录

### 数据层

- `TokenTracker` 新增 `request_history: Vec<RequestRecord>`（`#[serde(skip)]` 不持久化）
- `RequestRecord` 包含 `input_tokens`、`output_tokens`、`cache_creation_input_tokens`、`cache_read_input_tokens`，提供 `cache_hit_rate()` 方法
- `accumulate()` 开头调用 `RequestRecord::from_usage(usage)` 追加历史

### 渲染层

- `build_bar_chart_lines`：文本柱状图，`█` 字符堆叠，y 轴自动刻度（`nice_ceil` 上取整到 1/2/5×10^n）
- `build_cache_rate_lines`：缓存命中率柱状图，y 轴固定 0-100%
- `build_x_axis_labels`：底部请求编号，自动间隔（≤10 每条、≤20 每 5 条、≤50 每 10 条）
- `build_context_summary`：摘要行（上下文窗口、已用、百分比、消息数、工具数）
- Tab 栏：去掉 ratatui `TabBar` widget，改为手动 `Span` 渲染

## 涉及文件

- `peri-agent/src/agent/token.rs`（`TokenTracker`）—— 新增 per-request 历史记录
- `peri-tui/src/ui/main_ui/panels/status.rs`（`build_context_lines`）—— 替换为图表渲染
- `peri-tui/src/app/status_panel.rs`（`desired_height`）—— 面板高度从 14 增大到 20
