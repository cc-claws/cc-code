# PermissionRequest 钩子在 bypass 模式下不应触发

**状态**：Fixed
**优先级**：中
**创建日期**：2026-06-01

## 问题描述

PermissionRequest 钩子在 YOLO/bypass 权限模式下仍然触发，但 Claude Code 官方行为是仅在权限对话框即将展示给用户时才触发。bypass 模式下不会展示权限对话框，因此不应触发。

## 当前行为

```rust
// middleware.rs:412-414
// 不检查 permission_mode（YOLO/审批）：hook 始终触发以便观察/日志，HITL 弹窗是否显示
// 由 HITL 中间件独立决定。
let is_sensitive = (self.requires_approval)(&tool_call.name);
if is_sensitive {
    // PermissionRequest 始终触发
}
```

无论权限模式是 `Bypass`、`Default` 还是 `DontAsk`，只要工具是敏感工具，PermissionRequest 就会触发。

## 预期行为

| 权限模式 | 是否展示权限对话框 | PermissionRequest 是否触发 |
|---------|-------------------|--------------------------|
| bypass / auto-mode | 否 | **不触发** |
| default（审批模式） | 是 | 触发 |
| dont-ask | 否 | **不触发** |

## 影响范围

用户配置的 PermissionRequest 钩子（如 `herdr-agent-state.sh blocked`）在 YOLO 模式下也会被调用，可能执行不必要的副作用（如状态栏显示为 blocked 但实际上工具直接执行了）。

## 修复方向

1. `HookMiddleware` 持有当前 `permission_mode` 信息（已通过构造函数传入 `self.permission_mode`）
2. 在 `before_tool` 中判断：仅当 `permission_mode != "bypass"` 且工具需要审批时，才触发 PermissionRequest
3. 注意：PreToolUse 仍应始终触发（它独立于权限系统）

## 涉及文件

- `peri-middlewares/src/hooks/middleware.rs` — `before_tool` 中 PermissionRequest 门控逻辑（约 line 407-460）

## 状态变更记录

| 日期 | 从 | 到 | 操作人 | 说明 |
|------|-----|-----|--------|------|
| 2026-06-01 | — | Open | agent | 创建 |
| 2026-06-24 | Open | Fixed | agent | 发现 `b1563b29`（2026-06-02）已修复，补状态记录 |

## 修复记录

### 修复 #1（2026-06-02，2026-06-24 补录）

- **操作人**：KonghaYao（上游）
- **用户原意**：PermissionRequest 钩子在 YOLO/bypass 权限模式下不应触发，避免状态指示器误显示为 blocked
- **修复内容**：
  - `HookMiddleware` 的 `permission_mode` 从 `String` 改为 `Arc<SharedPermissionMode>`，运行时可变
  - 新增 `needs_permission_dialog()` 门控：`Bypass`/`DontAsk` 返回 false（无对话框），`AcceptEdit` 仅非编辑工具，`AutoMode`/`Default` 始终触发
  - `before_tool` 中 PermissionRequest 触发条件改为 `is_sensitive && needs_dialog`
  - `builder.rs` 修复传真实 permission_mode（之前传空字符串）
- **涉及 commit**：`b1563b29`（2026-06-02，main 分支）
- **验证状态**：待用户验证（修复已合并 22 天，无回归报告，但用户未明确确认）
