> 归档于 2026-05-16，原路径 spec/issues/2026-05-13-cache-hit-rate-chart-y-axis-adaptive.md
# 缓存命中率图表 y 轴应自适应数据范围

**状态**：Fixed
**优先级**：低
**创建日期**：2026-05-13

## 问题描述

缓存命中率图表（`/context` Context tab）的 y 轴固定 0-100%，图表高度仅 4 行（每行 25%）。当实际命中率集中在较高区间（如 85%-99%）时，所有柱子几乎等高，无法区分各次请求的命中率差异。

## 症状详情

### 当前行为

- `build_cache_rate_lines` 中 `y_max` 硬编码为 100（`status.rs:285`）
- 图表高度 `rate_h = 4`（`status.rs:435`），每行对应 25% 区间
- 命中率 80% 和 99% 在 4 行图表中差异仅 1 行（75-100% 区间），柱子看起来几乎一样高

### 期望行为

- y 轴根据实际数据的最大最小值自动分布刻度
- 例如命中率范围 85%-99% 时，y 轴显示约 80%-100%，4 行图表能清晰展示每次请求的差异
- 底部留少量 padding（如 5%），避免最低值的柱子紧贴底部

## 相关代码

- `peri-tui/src/ui/main_ui/panels/status.rs:269-321` —— `build_cache_rate_lines`，y 轴固定 0-100%
- `peri-tui/src/ui/main_ui/panels/status.rs:284-285` —— `y_max: u64 = 100` 硬编码
- `peri-tui/src/ui/main_ui/panels/status.rs:435` —— `rate_h = 4` 图表高度
- `peri-tui/src/ui/main_ui/panels/status.rs:160-200` —— `build_bar_chart_lines`，token 柱状图的 `nice_ceil` 自适应刻度可参考

## 关联 Issue

- `spec/issues/2026-05-13-context-tab-per-request-token-chart.md`（状态：Fixed）—— 此图表的原始 feature issue

## 实现记录

### 改动

`build_cache_rate_lines`（`status.rs:284-318`）y 轴从固定 0-100% 改为自适应数据范围：

- 计算 visible 数据的 `rate_min` / `rate_max`
- 所有值相同时加 ±5 padding；否则加 5% range padding（≥1%），上界用 `nice_ceil` 取整
- `y_min` 钳位 0，`y_max` 钳位 100
- 行标签和底部 x 轴标签改为显示实际范围值

### 测试

- `test_build_cache_rate_lines_adaptive_y_axis` —— 85-99% 数据，验证 y 轴自适应且柱子不全等高
- `test_build_cache_rate_lines_all_same` —— 全部 95%，验证 padding 生效
- `test_build_cache_rate_lines_empty` —— 空数据返回空
