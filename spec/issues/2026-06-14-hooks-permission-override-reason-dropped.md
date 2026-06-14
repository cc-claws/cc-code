# Hooks PermissionOverride 透传时 reason 字段被丢弃

**状态**：Fixed
**优先级**：中
**创建日期**：2026-06-14

## 问题描述

Hooks 系统中，当 command hook 通过 `hookSpecificOutput.permissionDecision` 返回 `deny` 决定时，配套的 `permissionDecisionReason` 字段在解析层被硬编码丢弃，导致下游 HITL 弹窗和遥测拿不到 hook 给出的拒绝理由，用户只能看到泛化的 "Blocked by hook"。

## 症状详情

| 阶段 | 实际行为 | 预期行为 |
|------|---------|---------|
| Hook 返回 | `{"hook_specific_output":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":"blocked by policy"}}` | — |
| `output_parser.rs` 解析 | `HookAction::PermissionOverride { decision, reason: None }` | `reason: Some("blocked by policy")` |
| HITL 弹窗显示 | "Blocked by hook"（泛化） | "blocked by policy"（hook 给出） |
| 遥测/日志 | 缺失拒绝原因 | 记录具体原因 |

## 复现条件

- **复现频率**：必现
- **触发步骤**：
  1. 配置一个 PreToolUse command hook，stdout 输出上述 JSON
  2. 在 HITL（非 bypass）模式下触发对应工具
  3. 观察弹窗是否显示 `permissionDecisionReason` 内容
- **环境**：所有 OS，default 权限模式

## 涉及文件

- `peri-middlewares/src/hooks/output_parser.rs` —— `hook_specific_to_action()` 中 `PermissionOverride` 分支硬编码 `reason: None`
- `peri-middlewares/src/hooks/middleware_test.rs:245` —— 测试 JSON 多了一个 `}`（三闭花括号应为两闭花括号），导致 JSON 解析失败回退到 Allow，CI 上 test_fire_event_preserves_permission_override 在 ubuntu/macos 失败

## 修复内容

PR #13（commit `0493e77c`）：

1. `hook_specific_to_action` 的 `PreToolUse { permission_decision, .. }` 模式改为绑定 `permission_decision_reason`，透传到 `HookAction::PermissionOverride.reason`
2. `middleware_test.rs:245` 修复测试 JSON typo（`}}}'` → `}}'`），CI 全绿
3. 本地 833 个 hooks 测试全部通过

## [TRAP] 经验沉淀

**`hook_specific_to_action` 必须透传所有 hook 返回字段，禁止 hardcode 默认值**。

**Why:** Hook 系统的 `hookSpecificOutput` 设计是给上游（用户/插件作者）一个表达决策 + 理由的渠道。如果解析层把理由丢弃，用户在 HITL 弹窗看不到具体原因，遥测也拿不到结构化数据，整个 hook 生态的可观测性下降。

**How to apply:**
- 任何 `hook_specific_to_action` 中的字段映射，必须用 `pattern { field_name, .. }` 绑定然后透传，禁止 `_` 通配后 hardcode 默认值
- 添加新的 `HookSpecificOutput` 变体或字段时，必须同步检查所有 match arm 是否透传新字段
- Hook 测试 JSON 必须用 `serde_json::json!` 宏构造，避免手写字符串易错（typo 会让 JSON 解析失败静默回退 Allow）

## 状态变更记录

| 日期 | 从 | 到 | 操作人 | 说明 |
|------|-----|-----|--------|------|
| 2026-06-14 | — | Fixed | agent | PR #13（commit 0493e77c）合并到 main，CI 全绿 |

## 修复记录

### 修复 #1（2026-06-14）

- **操作人**：agent
- **用户原意**：Hook 返回的 `permissionDecisionReason` 应该完整透传到 HITL 弹窗和遥测
- **修复内容**：`hook_specific_to_action` 绑定 `permission_decision_reason` 透传；测试 JSON typo 修复
- **涉及 commit**：`0493e77c`（PR #13）
- **验证状态**：已验证（CI 3 平台全绿，833 hooks 测试通过）
