# HITL 权限 领域

## 领域综述

HITL 权限领域负责工具调用的审批策略：**只有两档模式**（`Auto` / `Bypass`），默认 `Auto`。

> **历史**：早期是 5 档（`Default` / `AcceptEdits` / `Auto` / `BypassPermissions` / `DontAsk`）。
> 实测用户不愿意「天天确认」，其余几档要么每次弹窗、要么半自动，体验都差且不智能，
> 故收敛为两档（见 CHANGELOG 的 #246/#247 条目）。

核心职责：
- 2 种 PermissionMode：**`AutoMode`（默认）** / **`Bypass`**
- `Arc<AtomicU8>` 无锁原子共享当前模式，TUI 与 Agent task 间零锁竞争
- `Auto` 模式：先跑**确定性层**（人写的规则 / 内置黑名单，零成本、不依赖凭据），
  再走 Jev 语义门；**只有判不准时才弹一次窗**；没有判定凭据时回落旧 LLM 分类器
- `ask_user_question` 不在审批清单里，不受权限模式影响，始终弹窗

## 核心流程

### 模式解析（启动时）

优先级（`resolve_initial_permission_mode`，TUI）：

```
--dangerously-skip-permissions  → Bypass
--permission-mode bypass        → Bypass   （其余取值一律回退 Auto）
-a / --approve                  → Auto
-y / --yolo 或 YOLO_MODE 真值   → Bypass
（都没有）                      → Auto      ← 默认档，fail-closed
```

`-p/--print` 非交互模式**默认 Bypass**（没有人工确认通道）；`--permission-mode auto` 可显式打开判定。
`Shift+Tab` 在 `Auto ↔ Bypass` 间循环；`PermissionMode::from(u8)` 的**未知取值一律回退 `Auto`**
（绝不因陈旧/异常值意外滑进 `Bypass`）。权限模式不落盘，只存在于内存与 ACP 字符串映射。

### 权限判断流程（`before_tool`）

```
工具调用 → HITL middleware.before_tool()
  → 计算「有效调用」（Bash 会先经 RTK 前缀改写 X → rtk X，见下节）
  → Bypass:            全部放行
  → Auto（默认）:
       1) 非审批清单工具（Read/Glob/Grep/TodoWrite/AskUserQuestion…）→ 直接放行
       2) 审批记忆命中（同「工具 + 路径」本会话已批准）→ 直接放行
       3) 确定性层（JevGate::deterministic，零成本、不依赖凭据）：
            硬黑名单 → Block ｜ 用户 deny 规则 → Block ｜ 用户 allow/safe → Allow
            ｜ 只读命令链 → Allow ｜ gate_scope=matched 且无危险形状 → Allow
       4) 有判定凭据 → Jev 语义门（LLM 按条件集打分，必要时 Ask → 弹窗）
       5) 无判定凭据 → 旧 LLM 分类器兜底；仍不确定则弹窗
       （Ask 但没有确认通道时 → 默认拒绝）
```

审批清单（`default_requires_approval`）：`Bash` / `Agent` / `Write` / `Edit` / `delete_*` / `rm_*` /
`WebFetch` / `WebSearch` / `mcp__*`。

### Bash 命令的判定口径（RTK 改写 × 显式规则）

Bash 工具在执行前会经 RTK 前缀改写（`X` → `rtk X`，仅对 git/cargo/npm/docker/kubectl… 等白名单命令族），
因此同一件事有两个命令文本：**原始命令**（用户敲的）与**有效命令**（实际会执行的）。门控的取值口径：

| 判定项 | 看哪些命令 | 方向 |
|---|---|---|
| 硬黑名单 / 危险形状 | 原始 **+** 有效 | 任一命中即拦（fail-closed）|
| 用户 `disallowed_commands` | 原始 **+** 有效 | 任一命中即拦 |
| 用户 `allowed_commands` / `safe_commands` | 原始 **+** 有效 | 任一命中即生效（白名单表达的是意图，改写只是前缀包装）|
| 内置只读白名单（快车道） | 透明包装时看原始，否则看有效 | 进快车道（零成本放行）|

**为什么两边都要看**：

- 只看有效命令（#288 的原始动机：避免「批准 X、实际执行 X′」）→ 用户写的 `kubectl delete*` 在装了 `rtk`
  的机器上被 `rtk kubectl delete pod x` 绕过，**规则静默失效**（#358，同一份代码在 CI 上还测不出来）；
- 只看原始命令 → 重新引入 #288 的语义漏洞。

**只读快车道为什么不「两条都要」**：`rtk` 是**输出过滤型透明包装**，`rtk git status` 与 `git status`
的只读性质完全一致。若一律按有效命令判定，装了 rtk 的机器上**每条只读命令都会失去快车道**，
升级成一次 Jev 判定、没配 key 时甚至一次 LLM 分类调用——那是假阴性带来的真实延迟/费用代价。
因此仅当「有效命令恰好是 `rtk <原始命令>`」时按原始命令判定；包装不透明（或没有原始命令）时仍按有效命令判定。

实现见 `cc-middlewares/src/hitl/jev/mod.rs` 的 `Commands`（有效命令 + 原始命令的集合，`GateCall.original_command` 携带原始命令）。

## 技术方案总结

| 维度 | 选型 |
|------|------|
| 模式枚举 | `PermissionMode`: `AutoMode`（默认）/ `Bypass`，`#[repr(u8)]` |
| 共享状态 | `SharedPermissionMode`: `Arc<AtomicU8>`，`cycle()` 用 CAS 循环切换 |
| 切换方式 | `Shift+Tab` 在 Auto ↔ Bypass 间循环，状态栏实时显示 + 1.5s 高亮 |
| 初始模式 | `resolve_initial_permission_mode()`（见上）；`-p` 默认 Bypass；未显式开启一律 Auto |
| 确定性层 | 硬黑名单 / 人写的 allow·deny·safe 规则 / 危险形状 / 只读白名单——零成本、不依赖判定凭据 |
| 语义层 | Jev 语义门（条件集 + 阈值），CLAUDE.md 提炼出的规则只进语义层，不做硬 Block |
| 兜底 | 旧 LLM 分类器（`LlmAutoClassifier`）→ 不确定时弹窗；无确认通道则拒绝 |
| 兼容性 | `YOLO_MODE` 环境变量只决定**初始**模式（未设置 = Auto，不再 fail-open）|

## Feature 附录

### feature_20260427_F002_permission-mode

> ⚠️ 该 Feature 为**历史形态**：当时是 5 档模式（Default/AcceptEdits/Auto/BypassPermissions/DontAsk），
> 后续已精简为 Auto / Bypass 两档（见上「模式解析」）。此处按记录当时状态保留。

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
- → [agent-instructions.md](./agent-instructions.md) — 指引注入（`@import` 范围）与门控的 `CLAUDE.md` 提炼来源
