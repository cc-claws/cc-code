> 归档于 2026-06-24，原路径 spec/issues/2026-06-24-panel-swallow-ctrl-c.md

# 面板打开时 Ctrl+C 无法退出

**状态**：Fixed
**优先级**：高
**创建日期**：2026-06-24

## 问题描述

任意面板（Session 面板、Global 面板）打开时，按 Ctrl+C 无法退出 TUI。所有键盘事件被面板层无条件吞掉，`handle_ctrl_c` 永远无法执行。

## 症状详情

| 维度 | 描述 |
|------|------|
| 触发条件 | 任意面板处于打开状态 |
| 预期行为 | Ctrl+C 正常退出 TUI |
| 实际行为 | Ctrl+C 被吞，TUI 无法退出，只能 kill 进程 |
| 复现频率 | 100% |
| 影响范围 | 所有面板打开场景下的退出操作 |

## 复现条件

- **复现频率**：必现
- **触发步骤**：
  1. `cargo run -p peri-tui` 启动 TUI
  2. 打开任意面板（如 `Ctrl+B` 打开 Thread Browser）
  3. 按 `Ctrl+C`
  4. 观察：TUI 不退出，面板仍显示

## 根因分析

键盘事件分发链（`keyboard.rs:120-161`）按 Stage 顺序处理按键：

```
Stage 1-2:   Bar focus
Stage 3-6:   Shortcuts (Ctrl+T, Ctrl+B etc)
Stage 7:     Setup wizard
Stage 8-9:   Panels ← BUG: unconditionally swallows all keys
Stage 10-12: Popups
Stage 13:    Normal keys (Ctrl+C quit logic lives here)
```

`panels.rs` 的 `handle_panels` 函数在 Stage 8-9 中，对 Session 面板和 Global 面板都无条件 `return Some(Action::Redraw)`，不检查 `dispatch_key` 的返回值。所有按键事件（包括 Ctrl+C）在此被拦截，永远无法到达 Stage 13 的 `handle_normal_keys` → `handle_ctrl_c`。

```rust
// Session panels（panels.rs:49）
return Some(Action::Redraw);  // ← Ctrl+C 在这里被吃了

// Global panels（panels.rs:81）
return Some(Action::Redraw);  // ← Ctrl+C 在这里被吃了
```

## 涉及文件

- `peri-tui/src/event/keyboard/panels.rs`（lines 49, 81）—— `handle_panels` 函数，无条件返回 `Redraw` 拦截所有按键
- `peri-tui/src/event/keyboard.rs`（lines 120-161）—— 键盘事件分发链，Stage 8-9 面板处理

## 修复内容

两层修复（缺一不可，仅做第 2 层对 10/12 面板无效）：

1. **Ctrl+C 统一拦截**：在 `handle_panels` 开头判断 `Ctrl+C`（`input.ctrl && Key::Char('c')`）直接 `return None`，穿透到 Stage 13 的 `normal_keys → handle_ctrl_c`。原因：12 个面板中仅 AgentPanel/HooksPanel 对 Ctrl+C 返回 `NotConsumed`，其余 10 个（Model/Login/Config/ThreadBrowser/Mcp/Plugin/Cron/Status/Memory/Tasks）用 `_ => Consumed` 兜底吞掉 Ctrl+C，必须在分发到面板前拦截。

2. **NotConsumed 传递机制**：捕获 `with_session_panels!`/`with_global_panels!` 宏返回的 `EventResult`，`NotConsumed` 时不返回 `Redraw` 而 pass through 到下一 Stage。通用机制修复，落实「面板层只在 `Consumed` 时拦截」原则。

## [TRAP] 经验沉淀

**键盘事件分发链中，中间 Stage 不得无条件拦截未消费的按键。**

**Why:** 键盘事件分发链按优先级从高到低分 Stage 处理。如果某个 Stage 无条件 `return Some(Action)` 而不检查事件是否被消费，会导致后续所有 Stage（包括全局快捷键如 Ctrl+C）永远无法触发。面板层只应在 `dispatch_key` 返回 `Consumed` 时拦截事件。

**How to apply:**
- 键盘分发链中每个 Stage 必须检查 `EventResult`：`Consumed` → 返回；`NotConsumed` → 传递给下一 Stage
- 新增分发 Stage 时禁止无条件 `return`，必须先判断事件是否被消费
- 面板、弹窗等 UI 层的 `dispatch_key` 返回值是分发链是否继续的唯一判据
- **全局快捷键（如 Ctrl+C 退出）不应依赖每个面板正确返回 `NotConsumed`**：多数面板用 `_ => Consumed` 兜底吞掉未识别按键，全局键应在分发到面板前（`handle_panels` 开头）统一拦截穿透

## 状态变更记录

| 日期 | 从 | 到 | 操作人 | 说明 |
|------|-----|-----|--------|------|
| 2026-06-24 | — | Fixed | agent | 发现并修复：panels.rs dispatch_key 返回值检查缺失 |

## 修复记录

### 修复 #1（2026-06-24）

- **操作人**：agent（实际代码修复 + 文档校准）
- **修复内容**：
  - `handle_panels` 开头统一拦截 Ctrl+C，直接 `return None` 穿透到 `normal_keys`（解决 10/12 面板 `_ => Consumed` 吞 Ctrl+C 的问题）
  - Session/Global 分支捕获 `dispatch_key` 返回值，`NotConsumed` 时 pass through，不再无条件 `return Some(Action::Redraw)`
- **涉及文件**：`peri-tui/src/event/keyboard/panels.rs`
- **验证状态**：编译通过 + 2 个单元测试通过（`test_ctrl_c_passes_through_when_panel_open` 用会吞 Ctrl+C 的 ModelPanel 验证穿透；`test_consumed_key_still_returns_redraw_when_panel_open` 回归保护 Consumed 按键仍 Redraw）
- **双击退出设计**：`handle_ctrl_c` 全应用既定设计（agent 运行单击中断、空闲双击退出），本次未改其语义
- **UX 瑕疵已修**：`status_bar.rs` 原本面板/详情 hints 覆盖 quit-pending 提示（`status_bar.rs:538`），已将 quit-pending 提示提到最高优先级——面板打开时第一次 Ctrl+C 后状态栏立即显示「Ctrl+C 退出 / 其他键取消」，不再被面板 hints 覆盖
