> 归档于 2026-05-17，原路径 spec/issues/2026-05-16-model-panel-1m-context-toggle.md
# Model 面板添加 1M 上下文开关

**状态**：Fixed
**优先级**：中
**创建日期**：2026-05-16
**解决日期**：2026-05-16
**Commit**：`346ebd5`

## 问题描述

当前 context_window 由模型自动决定（如 Claude 200K），compact 阈值（auto_compact 0.85、warning 0.70）基于此计算。对于支持 1M 上下文的模型，用户期望手动开启 1M 模式，使压缩等行为以 1M token 为基准窗口，而非模型默认值。

## 期望行为

Model 面板新增一个「1M Context」切换行，开启后 context_window 覆盖为 1,000,000 tokens，compact 阈值相应变化（auto_compact 在 850K 触发、warning 在 700K 触发）。关闭后恢复模型默认 context_window。开关状态持久化到 `~/.peri/settings.json`。

## 涉及文件

- `peri-tui/src/app/model_panel.rs` —— 新增 1M 上下文行的光标/切换逻辑，`apply_to_config` 写入持久化状态
- `peri-tui/src/ui/main_ui/panels/model.rs` —— 渲染 1M 上下文行（复选框样式，类 Effort 行）
- `peri-tui/src/config/types.rs` —— `AppConfig` 新增可选字段存储开关状态
- `peri-tui/src/app/agent.rs` —— 创建 `ContextBudget` 时检查开关，覆盖 `context_window` 为 1,000,000

## 解决方案

在 8 个文件中新增/修改共 109 行：

| 文件 | 变更 |
|------|------|
| `config/types.rs` | `AppConfig` 新增 `context_1m: Option<bool>` 字段 |
| `app/model_panel.rs` | 新增 `ROW_1M_CONTEXT=5`（`ROW_COUNT=6`），`buf_context_1m` 字段，Enter/Space/←/→ 切换逻辑，`apply_to_config` 持久化，`apply_and_close` 同步 `agent.context_window` 到 status line |
| `ui/panels/model.rs` | 渲染 1M Context 行（`●` radio-dot + `1M Context: ON/OFF` 标签，与 Effort/MaxTokens 行风格一致） |
| `app/agent.rs` | `ContextBudget` 创建时若开关开启则覆盖 `context_window = 1_000_000` |
| `app/agent_submit.rs` | 提交前同步 context_window 到 TUI 显示时也遵循 1M 覆盖 |
| `locales/en/main.ftl`, `zh-CN/main.ftl` | 新增 `app-1m-context-enabled` i18n key |
| `ui/panels/model_test.rs` | 测试构造函数新增 `buf_context_1m: false` |

**关键设计点**：`agent.context_window` 在三个路径都会被更新为 1M：
1. 切换开关时（`apply_and_close`）→ 立即反映到 status line 和 `/context` 面板
2. 提交消息时（`agent_submit.rs`）→ 提交前覆盖
3. Agent 运行时（`agent_ops.rs` ← 核心 ContextWarning）→ 从核心层同步

**额外修改**（超出原始涉及文件列表）：
- `app/agent_submit.rs`：status line 显示需在提交时同步
- `locales/*.ftl`：切换提示 i18n
- `model_test.rs`：测试兼容新字段
