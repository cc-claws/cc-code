# HITL 权限 领域

## 领域综述

HITL 权限领域负责工具调用的审批策略管理，支持 5 级权限模式从全放行到全拦截的精细控制。

核心职责：
- 5 种 PermissionMode：Default / AcceptEdits / Auto / BypassPermissions / DontAsk
- Arc<AtomicU8> 无锁原子共享当前模式，TUI 与 Agent task 间零锁竞争
- Auto 模式通过 LLM 分类器判断工具调用放行/拒绝
- ask_user_question 不受权限模式影响，始终弹窗

## 核心流程

### 权限判断流程

```
工具调用 → HITL middleware.before_tool()
  → 读取 SharedPermissionMode
  → Default: 按默认拦截清单（bash/write/edit/delete/rm/folder）弹窗
  → AcceptEdits: 自动放行 write_*/edit_*/folder，bash/launch_agent 仍弹窗
  → Auto: LlmAutoClassifier.classify(tool, input) → Approve/Reject/Unsure(弹窗)
  → BypassPermissions: 全部放行（不含 ask_user）
  → DontAsk: 全部放行
  → ask_user_question: 始终弹窗，不受权限模式影响
```

### Bash 命令的判定口径（RTK 改写 × 显式规则）

Bash 工具在执行前会经 RTK 前缀改写（`X` → `rtk X`，仅对 git/cargo/npm/docker/kubectl… 等白名单命令族），
因此同一件事有两个命令文本：**原始命令**（用户敲的）与**有效命令**（实际会执行的）。门控的取值口径：

| 判定项 | 看哪些命令 | 方向 |
|---|---|---|
| 硬黑名单 / 危险形状 | 原始 **+** 有效 | 任一命中即拦（fail-closed）|
| 用户 `disallowed_commands` | 原始 **+** 有效 | 任一命中即拦 |
| 用户 `allowed_commands` / `safe_commands` | 原始 **+** 有效 | 任一命中即生效（白名单表达的是意图，改写只是前缀包装）|
| 内置只读白名单（快车道） | 原始 **+** 有效 | **两条都**是只读链才进快车道（保守）|

**为什么两边都要看**：

- 只看有效命令（#288 的原始动机：避免「批准 X、实际执行 X′」）→ 用户写的 `kubectl delete*` 在装了 `rtk`
  的机器上被 `rtk kubectl delete pod x` 绕过，**规则静默失效**（#358，同一份代码在 CI 上还测不出来）；
- 只看原始命令 → 重新引入 #288 的语义漏洞。

实现见 `cc-middlewares/src/hitl/jev/mod.rs` 的 `Commands`（有效命令 + 原始命令的集合，`GateCall.original_command` 携带原始命令）。

## 技术方案总结
| 维度 | 选型 |
|------|------|
| 模式枚举 | PermissionMode: Default/AcceptEdits/Auto/BypassPermissions/DontAsk |
| 共享状态 | SharedPermissionMode: Arc<AtomicU8>，CAS 循环切换 |
| 切换方式 | Shift+Tab 循环，状态栏实时显示 + 1.5s 高亮 |
| Auto 分类器 | LlmAutoClassifier trait，缓存机制避免重复调用 |
| 兼容性 | YOLO_MODE 环境变量决定初始模式 |

## Feature 附录

### feature_20260427_F002_permission-mode
**摘要:** 支持 5 级权限模式，Shift+Tab 循环切换 HITL 审批策略
**关键决策:**
- 定义 5 种 PermissionMode：Default / AcceptEdits / Auto / BypassPermissions / DontAsk
- 使用 Arc<AtomicU8> 无锁原子共享当前模式，TUI 与 Agent task 间零锁竞争
- Auto 模式通过 LLM 分类器（AutoClassifier trait）判断工具调用放行/拒绝/Unsure
- acceptEdits 模式自动放行 write_*/edit_*，bash/launch_agent 仍弹窗
- ask_user_question 不受权限模式影响，始终弹窗问答
- 保留 YOLO_MODE 环境变量兼容性，仅决定初始模式
**归档:** [链接](../../archive/feature_20260427_F002_permission-mode/)
**归档日期:** 2026-04-30

---

## 相关 Feature
- → [tui.md](./tui.md) — TUI 状态栏权限模式显示
- → [agent.md](./agent.md) — HITL middleware 集成
