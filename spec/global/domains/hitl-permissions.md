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
       2) 加载规则；子 Agent 规则失败或不完整且无显式 allow → 拒绝
       3) 确定性层先拦截硬黑名单 / 用户 deny 规则 → Block
       4) 审批记忆命中（用户明确选择本次会话同意）→ 放行
       5) 确定性层其余结果（零成本、不依赖凭据）：用户 allow/safe → Allow
            ｜ 只读命令链 → Allow ｜ gate_scope=matched 且无危险形状 → Allow
       6) 有判定凭据 → Jev 语义门（LLM 按条件集打分，必要时 Ask → 弹窗）
       7) 无判定凭据 → 旧 LLM 分类器兜底；仍不确定则弹窗
       （Ask 但没有确认通道时 → 默认拒绝）
```

审批清单（`default_requires_approval`）：`Bash` / `Agent` / `Write` / `Edit` / `delete_*` / `rm_*` /
`WebFetch` / `WebSearch` / `mcp__*`。

### 审批交互与会话记忆

审批弹窗同时展示「同意本次 / 本次会话同意 / 拒绝」，用上下键选择、Enter 提交；
批量审批用 Tab / Shift+Tab 切换工具，Esc 全部拒绝，快捷键提示固定在底部。

只有 `Approve { source: "session" }` 写入记忆，默认的本次批准及拒绝、编辑参数均不记忆。
Read/Write/Edit 按真实工具名和规范化路径记忆；Bash 按完整命令、实际执行目录和当前分支记忆，
其他工具按真实工具、完整参数和目录记忆。ExecuteExtraTool 使用解包后的目标与参数。
不按 Bash 名称或命令前缀扩大批准范围，UNC 路径保留服务器与共享名称；新会话清空记忆。
确定性禁止规则始终优先于记忆。

### Jev 规则提炼缓存

规则加载器在首次门控时先查进程缓存，再查 `~/.cc-code/jev/peri-<项目目录 SHA-256>/`。
`<来源签名>.index.json` 指向 `<规则 JSON SHA-256>.json`；文件通过临时文件原子发布，
仅保存完整、非空的提炼结果。索引、JSON 或内容校验不通过时重新提炼，读写失败不阻断现有判定。
界面不展示缓存提示。

来源签名包含实际读取并展开引用的项目/个人/全局规则与 hook 内容（来源总量截断前）、
规范化项目目录、提炼模型 provider/id、提炼提示词、来源/分块上限与超时。
这些内容变化后，新会话首次门控会重新提炼；当前会话继续使用建立会话时冻结的来源。
仍沿用现有 Jev 来源选择顺序：项目优先读取首个存在的 `AGENTS.md`、`CLAUDE.md`、
`.claude/AGENTS.md`；缓存不改变指引加载规则。截断、部分失败及未完成的提炼不写磁盘。

缓存只复用规则提炼产物；具体工具调用仍执行现有确定性/语义判定流程。
实现见 `cc-middlewares/src/hitl/jev/rules.rs`、`rules_cache/mod.rs` 与
`cc-acp/src/session/frozen.rs`，TUI 和 Stdio 共用同一路径。

### 业务子 Agent 的权限

父 Agent 在 Auto 中保持 Allow / Block / Ask；业务子 Agent 只接受 Allow / Block。
Jev 判定通过独立 HTTP 请求评分，并不是 Agent 工具启动的子 Agent，也没有审批 broker。

子 Agent 共享父的 `SharedPermissionMode`、Jev 门及规则加载器、兜底分类器和会话审批记忆，
但派生 HITL 不持有 broker。普通、fork、后台、后台 fork 统一注册 `SubAgentPermissionMiddleware`：

- 用户明确的 allow/deny 规则照常生效；确定性层能明确放行的调用仍可执行。
- Jev 未决（包括父配置 `JEV_UNCERTAIN=allow`）、判定请求失败、规则提炼失败或不完整、
  分类器 Unsure 或无可用判定路径均拒绝；子 Agent 不请求用户确认。
- Bypass/YOLO 保持免审批；模式与父共享同一 Arc，父切回 Auto 后缓存或运行中的子工具立即恢复判定。
- `tools`/`disallowedTools` 同时约束直接调用、中间件贡献的工具与 `ExecuteExtraTool` 的真实目标；
  调用必须匹配实际可用的真实工具名称，先规范化大小写再判定，不允许语义/模糊别名、未继承目标
  或嵌套代理跳过权限。代理调用的 Jev 与分类器均查看解包后的真实工具和参数。
- 既有禁递归 Agent 约束覆盖普通、fork、后台及代理路径，委派不扩大工具能力。
- 子 Agent 可指定另一目录读取指引；共享 Bash/文件工具仍在父目录执行，权限判定的 cwd/分支
  因此固定取继承工具的真实执行目录，子 Agent 的对话状态保持原来的指引目录。
- 直接构造子 Agent 未传父权限配置时默认 Auto 且无审批通道，敏感工具 fail-closed；
  宿主可通过 `SubAgentMiddleware::with_permissions` 显式继承父配置。

`-p --permission-mode auto` 也使用同一构建连接，不因非交互或后台运行绕过子权限。
实现见 `hitl/mod.rs::for_subagent`、`hitl/jev/mod.rs::evaluate_semantic_for_subagent`、
`subagent/tool/permission.rs` 与 `cc-acp/src/agent/builder.rs`。

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

最后更新：2026-10-10
