# SessionEnd 钩子缺少 reason 字段和部分触发场景

**状态**：Partial
**优先级**：高
**创建日期**：2026-06-01

## 问题描述

SessionEnd 钩子缺少 Claude Code 官方的 `reason` 字段，且未覆盖所有应触发场景。

## 当前行为 vs 预期行为

| 场景 | Claude Code (reason) | Peri | 差异 |
|------|---------------------|------|------|
| `/clear` 新建 thread | `clear` | 触发（无 reason） | ✗ 缺 reason |
| 恢复其他会话（导致当前会话结束） | `resume` | 不触发 | ✗ 缺场景 |
| TUI 退出（Ctrl+C 双击） | `prompt_input_exit` | 触发（无 reason） | ✗ 缺 reason |
| `/quit` 退出 | `other` | 触发（无 reason） | ✗ 缺 reason |

## 影响范围

钩子脚本无法根据退出原因执行不同的清理逻辑。例如用户在 `herdr-agent-state.sh` 中需要区分 `clear`（状态变为 blocked）和 `prompt_input_exit`（状态变为 idle）。

## 根因分析

- `fire_standalone_lifecycle_hooks` 中 SessionEnd 的 `HookInput` 不包含 `source` 字段（设为 `None`）
- `thread_ops.rs:new_thread()` 和 `main.rs` 退出路径调用时未传递 reason
- 缺少 resume 场景的触发点

## 修复方向

1. `HookInput` 中增加 `source` 字段用于 SessionEnd reason
2. `new_thread()` 调用时传 `reason = "clear"`
3. TUI 退出时传 `reason = "prompt_input_exit"` 或 `"other"`
4. 检查 resume 会话时是否需要触发当前会话的 SessionEnd（`reason = "resume"`）
5. Claude Code 默认超时 1.5s，peri 当前无超时控制

## 涉及文件

- `peri-middlewares/src/hooks/middleware.rs` — `fire_standalone_lifecycle_hooks` SessionEnd 分支
- `peri-tui/src/app/thread_ops.rs` — `new_thread()` 中 SessionEnd 触发
- `peri-tui/src/main.rs` — TUI 退出时 SessionEnd 触发

## 状态变更记录

| 日期 | 从 | 到 | 操作人 | 说明 |
|------|-----|-----|--------|------|
| 2026-06-01 | — | Open | agent | 创建 |
| 2026-06-24 | Open | Partial | agent | Phase 2B 修复 reason 字段透传，resume 场景仍未覆盖 |

## 修复记录

### 修复 #1（2026-06-24）

- **操作人**：agent（Claude glm-5.2）
- **用户原意**：SessionEnd 应携带 reason 字段（clear/prompt_input_exit/other），让钩子脚本能区分退出原因
- **修复内容**：
  - `fire_standalone_lifecycle_hooks` 加 `source: Option<&str>` 参数，SessionEnd 分支透传到 `HookInput.source`
  - `thread_ops.rs:new_thread()` 调用时传 `Some("clear")`（/clear 场景）
  - `main.rs` TUI 退出路径传 `Some("prompt_input_exit")`
  - `compact_middleware.rs` 传 `None`（compact 路径不触发 SessionEnd）
  - 加 `#[allow(clippy::too_many_arguments)]` 应对参数增长
- **涉及 commit**：本 PR（Phase 2）
- **验证状态**：待验证（reason 字段已透传，用户需在真实环境用钩子脚本验证 stdin JSON 中 source 字段值）

### 残留问题

- **resume 场景未覆盖**：恢复其他会话导致当前会话结束时，应触发 SessionEnd with `source = "resume"`。当前 TUI 层在 resume 时未调用 `fire_standalone_lifecycle_hooks`，需要单独排查 resume 路径并补触发点。
- **/quit 与 Ctrl+C 双击区分**：当前统一传 `prompt_input_exit`，未区分 `/quit`（应为 `other`）和双击 Ctrl+C（应为 `prompt_input_exit`）。
- **超时控制**：Claude Code 默认 1.5s 超时，Peri 当前无超时控制。
