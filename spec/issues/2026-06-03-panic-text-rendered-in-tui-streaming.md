# 流式输出过程中 panic 文本被渲染到 TUI 界面

**状态**：Open
**优先级**：中
**创建日期**：2026-06-03

## 问题描述

在 TUI 模式下的流式输出过程中，偶发性地将 panic 相关文本渲染到了 TUI 界面上。panic 文本出现后会被后续的正常输出覆盖，不影响实际执行流程，TUI 也不会崩溃退出。日志文件 `.tmp/agent-tui.log` 中无对应的 panic trace 记录，仅包含正常的 INFO/WARN 级别日志。

## 症状详情

| 维度 | 表现 |
|------|------|
| **出现位置** | TUI 界面的流式输出区域 |
| **出现时机** | 流式输出过程中，随机出现 |
| **显示效果** | panic 文本短暂出现在界面上，随后被后续输出覆盖 |
| **对执行的影响** | 无影响，程序继续正常运行 |
| **TUI 稳定性** | TUI 未崩溃、未退出 |
| **日志记录** | `.tmp/agent-tui.log` 中无 panic trace |
| **panic 位置** | `tokio::runtime::task::harness.rs:523:18`（tokio 的 catch_unwind re-panic 点） |

## 根因机制

### 已确认

1. **`RenderTask::run` 通过 `tokio::spawn` 执行**（`render_thread.rs:548`）—— tokio task panic 时默认 panic handler 写 stderr
2. **TUI 处于 raw mode** —— stderr 输出直接渲染到终端屏幕上
3. **无自定义 panic hook** —— `spec/feature_20260514_F001_panic-hook-tui/` 规划了但从未实现
4. **日志无记录** —— tracing subscriber 不捕获 stderr，只捕获 log 事件

### 推测（待诊断基础设施确认）

- 用户输入包含换行符的长文本并提交后，在 agent 流式输出期间触发
- panic 发生在 render thread（`tokio::spawn`）或 ACP server 的 agent 执行任务中
- 具体的 panic 原因在 panic 消息文本中（用户仅记得 harness.rs:523:18 这个 re-panic 位置）
- 用户怀疑是 unicode-width 处理不当，但渲染管道代码（markdown 解析、wrap map、cell wrapping）均使用了 `char_indices()` 确保安全 UTF-8 边界，未发现明显的不安全字符串切片

## 已实施的诊断基础设施

为下次复现时自动捕获完整信息，已实施以下改动：

| 改动 | 文件 | 作用 |
|------|------|------|
| 自定义 panic hook | `main.rs` | 替换 Rust 默认 hook，panic 信息写 `tracing::error!`（进日志文件）而非 stderr（不污染 TUI 画面） |
| 自动 backtrace | `main.rs` | panic hook 中自动调用 `Backtrace::capture()`，无需手动设 `RUST_BACKTRACE=1` |
| TUI 通知通道 | `main.rs` + `polling.rs` + `service_registry.rs` | panic 信息通过 channel 推送到 TUI，以 system note 形式显示给用户 |
| `catch_unwind` 保护 | `render_thread.rs` | `rebuild()` 调用包裹在 `catch_unwind` 中，panic 不再杀死渲染线程 |

## 复现条件

- **复现频率**：偶发（随机出现）
- **触发步骤**：
  1. 启动 TUI 模式
  2. 在 textarea 中输入包含换行符的长文本（含 CJK）
  3. 按 Enter 提交
  4. Agent 开始流式输出时，panic 文本随机出现
- **环境**：TUI 模式

## 涉及文件

- `peri-tui/src/main.rs` —— panic hook 安装与通知初始化
- `peri-tui/src/ui/render_thread.rs` —— RenderTask::run（tokio::spawn），rebuild_safe 保护
- `peri-tui/src/app/agent_ops/polling.rs` —— poll_panic_notifications
- `peri-tui/src/app/service_registry.rs` —— panic_notify_rx 字段
- `spec/feature_20260514_F001_panic-hook-tui/` —— 原始设计方案（已实施）

## 下一步

下次复现时检查 `.tmp/agent-tui.log` 中的 `ERROR` 级别日志，将包含完整的 panic 消息、原始位置和 backtrace。

## 状态变更记录

| 日期 | 从 | 到 | 操作人 | 说明 |
|------|-----|-----|--------|------|
| 2026-06-03 | — | Open | agent | 创建 |
| 2026-06-03 | Open | Open | agent | 添加诊断基础设施（panic hook + catch_unwind） |

## 修复记录

（由 fix-issue 或 issue-verify skill 追加，创建时留空）
