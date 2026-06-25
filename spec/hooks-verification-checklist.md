# Hooks 实现验证清单

> 验证日期：2026-05-07（含共性问题修复后更新）
> 对照源：`.claude/settings.local.json` 定义的 28 个 Hook 事件
> 验证方式：多 SubAgent Explorer 逐事件代码审查 + 单元测试验证

---

## Phase 1 — 已实现且可触发

| # | Hook 事件 | 状态 | 触发位置 | 备注 |
|---|-----------|------|----------|------|
| 1 | `PreToolUse` | ✅ 已实现 | `middleware.rs:307-313` `before_tool()` | 支持 Block/PreventContinuation/ModifyInput |
| 2 | `PermissionRequest` | ✅ 已实现 | `middleware.rs:343-387` `before_tool()` | 在 HITL 之前触发，HookMiddleware 排序正确 |
| 3 | `PostToolUse` | ✅ 已实现 | `middleware.rs:389-419` `after_tool()` | `is_error=false` 时触发，传递完整 tool output |
| 4 | `PostToolUseFailure` | ✅ 已实现 | `middleware.rs:389-419` `after_tool()` | `is_error=true` 时触发，与 PostToolUse 共享代码路径 |
| 5 | `UserPromptSubmit` | ✅ 已实现 | `middleware.rs:264-291` `before_agent()` | 每次 prompt 都触发，支持 Block/PreventContinuation |
| 6 | `SessionStart` | ⚠️ 部分实现 | `middleware.rs:240-258` `before_agent()` | 缺少 `PreventContinuation` 处理（仅处理 Block） |
| 7 | `SessionEnd` | ✅ 已实现 | `thread_ops.rs:152-206` `new_thread()` | `/clear` 时 tokio::spawn 后台触发，合并 plugin + settings.local hooks |
| 8 | `Stop` | ✅ 已实现 | `middleware.rs:424-451` `after_agent()` | 缺少 stop reason 传递（成功/失败/中断） |
| 9 | `SubagentStart` | ✅ 已实现 | `subagent/tool.rs:169-208` `fire_subagent_lifecycle_hook()` | 三条路径全覆盖：Fork/Normal/Background |
| 10 | `SubagentStop` | ⚠️ 部分实现 | `subagent/tool.rs:329,783,528-548` | Fork/Normal 用统一方法；Background 路径手动内联实现，代码不一致 |
| 11 | `Notification` | ✅ 已实现 | `middleware.rs:352-358` `before_tool()` + `middleware.rs:448-449` `after_agent()` | 跟随 PermissionRequest 和 Stop 自动触发；`fire_standalone_lifecycle_hooks` 也支持 |
| 12 | `PreCompact` | ✅ 已实现 | `agent.rs` `compact_task()` | `full_compact` 调用前触发；通过 `fire_standalone_lifecycle_hooks` dispatch |
| 13 | `PostCompact` | ✅ 已实现 | `agent.rs` `compact_task()` | compact 所有退出路径（成功/失败/取消）均触发 |

## Phase 2 — 枚举已定义但未触发

| # | Hook 事件 | 状态 | 枚举定义 | 触发代码 | 缺失内容 |
|---|-----------|------|----------|----------|----------|
| 14 | `StopFailure` | ⚠️ 部分实现 | `types.rs:19` | ❌ 无 | 枚举+序列化已有，需在 `on_error()` 回调中 dispatch |

## Phase 2 — 未实现（Phase 2 规划）

| # | Hook 事件 | 状态 | 枚举定义 | 备注 |
|---|-----------|------|----------|------|
| 15 | `Setup` | ❌ 未实现 | ❌ 无（走 Unknown 兜底） | spec Phase 2；与项目初始化向导相关 |
| 16 | `TeammateIdle` | ❌ 未实现 | ❌ 无 | spec Phase 2 |
| 17 | `TaskCreated` | ❌ 未实现 | ❌ 无 | spec Phase 2 |
| 18 | `TaskCompleted` | ❌ 未实现 | ❌ 无 | spec Phase 2 |
| 19 | `ConfigChange` | ❌ 未实现 | ❌ 无 | spec Phase 2 |
| 20 | `WorktreeCreate` | ❌ 未实现 | ❌ 无 | spec Phase 2 |
| 21 | `WorktreeRemove` | ❌ 未实现 | ❌ 无 | spec Phase 2 |
| 22 | `InstructionsLoaded` | ❌ 未实现 | ❌ 无 | spec Phase 2 |
| 23 | `Elicitation` | ❌ 未实现 | ❌ 无 | spec Phase 2 |
| 24 | `ElicitationResult` | ❌ 未实现 | ❌ 无 | spec Phase 2 |
| 25 | `CwdChanged` | ❌ 未实现 | ❌ 无 | spec Phase 2 |
| 26 | `FileChanged` | ❌ 未实现 | ❌ 无 | spec Phase 2；已有 matcher 字段解析 + 测试数据脚手架 |
| 27 | `PermissionDenied` | ❌ 未实现 | ❌ 无 | spec Phase 2 |

---

## 变更历史

### 共性问题修复（2026-05-07）

修复跨事件层面的 3 个共性问题，影响所有已实现的 hook 事件。

#### Fix #1：`async: true` 支持

**问题**：所有 Command 类型 hooks 同步执行，`async_run` 字段存在但从未生效，长时间运行的 hook 阻塞主流程。

**修复**：`fire_event` 和 `fire_standalone_lifecycle_hooks` 中检测 `is_async()`，异步 hook 通过 `tokio::spawn` 后台执行，立即返回 `Allow`，结果被忽略。

**代码变更**：

| 文件 | 变更 |
|------|------|
| `middleware.rs` `fire_event()` | 新增 async 分支：`is_async()` 为 true 时 clone hook/input/registered → `tokio::spawn` 后台执行 |
| `middleware.rs` `fire_standalone_lifecycle_hooks()` | 新增 async 检测，匹配的 hook 若为 async 则 spawn 后 continue |

#### Fix #2：`statusMessage` 日志

**问题**：`status_message` 字段在所有 4 种 HookType 变体上已定义，但从未被读取或展示。

**修复**：新增 `HookType::get_status_message()` getter 方法，`fire_event` 和 `fire_standalone_lifecycle_hooks` 在执行前通过 `tracing::info!` 记录。后续可通过 AgentEvent 集成到 TUI 状态栏。

**代码变更**：

| 文件 | 变更 |
|------|------|
| `types.rs` | 新增 `HookType::get_status_message()` getter |
| `middleware.rs` `fire_event()` | 执行前 `tracing::info!` 记录 status_message |
| `middleware.rs` `fire_standalone_lifecycle_hooks()` | 同上 |

#### Fix #3：`${VAR}` 命令字符串展开

**问题**：`variables.rs` 中的 `resolve_hook_variables` 函数处理 `${CLAUDE_PLUGIN_ROOT}`/`${CLAUDE_PLUGIN_DATA}`/`${ARGUMENTS}` 替换，但从未被 `execute_command_hook` 调用。虽然 bash shell 会展开通过 `.env()` 传入的环境变量，但文本级替换提供了跨平台一致性（Windows PowerShell 等）。

**修复**：`execute_command_hook` 在构建子进程前调用 `resolve_hook_variables` 展开命令字符串。

**代码变更**：

| 文件 | 变更 |
|------|------|
| `executor.rs` | import `resolve_hook_variables`，在命令执行前展开变量 |

### PreCompact / PostCompact（新增实现）

**触发路径**：`start_compact()` → 合并 plugin + settings.local hooks → `compact_task()`

| 事件 | 触发时机 | 代码位置 |
|------|----------|----------|
| `PreCompact` | `full_compact` 调用前 | `agent.rs` `compact_task()` |
| `PostCompact` | compact 所有退出路径（成功/失败/取消） | `agent.rs` `compact_task()` |

**新增/修改文件**：

| 文件 | 变更 |
|------|------|
| `peri-middlewares/src/hooks/types.rs` | 新增 `message_count` 字段 + `HookInput::compact()` 构造器 |
| `peri-middlewares/src/hooks/middleware.rs` | `fire_session_lifecycle_hooks` → `fire_standalone_lifecycle_hooks`（支持 SessionEnd/PreCompact/PostCompact/Notification） |
| `peri-tui/src/app/agent.rs` | `compact_task` 新增 hooks 参数，在 compact 前后触发 |
| `peri-tui/src/app/thread_ops.rs` | `start_compact` 合并 hooks 并传入 `compact_task` |

### Notification（新增实现）

**触发时机**：agent 需要用户关注时自动触发

| 触发点 | 说明 |
|--------|------|
| `PermissionRequest` 之后 | 工具审批前，agent 等待用户操作 |
| `Stop` 之后 | agent 完成，等待用户输入下一条消息 |

**新增/修改文件**：

| 文件 | 变更 |
|------|------|
| `peri-middlewares/src/hooks/types.rs` | 新增 `HookEvent::Notification` 枚举变体（从 Unknown 提升为一级变体） |
| `peri-middlewares/src/hooks/middleware.rs` | `before_tool()` 和 `after_agent()` 中 PermissionRequest/Stop 后触发 Notification |
| `peri-middlewares/src/hooks/loader.rs` | 测试断言更新 |
| `peri-tui/src/app/hooks_panel.rs` | hooks 面板新增 Notification 条目 |

---

## 跨事件共性问题

| 问题 | 严重度 | 影响范围 | 状态 |
|------|--------|----------|------|
| `async: true` 标志被忽略 | 🔴 高 | 所有 Command 类型 hooks | ✅ 已修复 — `tokio::spawn` 后台执行 |
| `statusMessage` 未使用 | 🟡 中 | 所有 hooks | ✅ 已修复 — `tracing::info!` 日志记录（UI 集成待定） |
| `${VAR}` 命令字符串展开缺失 | 🟡 中 | Command 类型 hooks | ✅ 已修复 — `resolve_hook_variables` 文本替换 |

## 统计

| 分类 | 数量 |
|------|------|
| ✅ 完全实现 | **10** (PreToolUse, PermissionRequest, PostToolUse, PostToolUseFailure, UserPromptSubmit, SessionEnd, Stop, SubagentStart, Notification, PreCompact, PostCompact) |
| ⚠️ 部分实现 | **3** (SessionStart, SubagentStop, StopFailure) |
| ❌ 未实现 | **14** (Setup, +13 个 Phase 2 事件) |
| **总计** | **28** |
