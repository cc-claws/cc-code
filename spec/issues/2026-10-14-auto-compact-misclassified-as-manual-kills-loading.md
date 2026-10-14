# auto-compact 被误判为手动 compact，导致回合中途 `loading=false`（Tip 行/spinner 消失，且 Ctrl+C 变退出）

**状态**：Open
**优先级**：高
**创建日期**：2026-10-14
**GitHub Issue**：#481
**回归引入**：`a24d811e`（2026-05-29）— 间接源于 `bb388ca6`（2026-05-29）删除原 setter
**前身 Issue**：#29（已关闭）「Compact 后 Resubmit 缺少 Loading Spinner」——同一现象，本次为第 2 次回归

## 问题描述

用户在一次长会话中观察到：**TUI 底部的 `⎿ Tip:` 提示行与 spinner 行消失**，但同时在跑的 Bash 工具卡片仍正常显示 `Running… (42s)` 与 `(ctrl+b to run in background)`。同一屏内两组「loading 中才该出现」的内容，一组在、一组不在。

深入调查发现，Tip 行消失只是**表象**。真实缺陷是：**auto-compact（ReAct 循环内自动压缩）被 TUI 误判成手动 `/compact`，导致压缩完成时就调用 `set_loading(false)`，而该回合其实仍在继续运行**。由此产生一组级联后果：

| # | 后果 | 严重度 |
|---|------|--------|
| 1 | 底部 spinner 行 + `⎿ Tip:` 行消失（该回合剩余全程） | 中 |
| 2 | **Ctrl+C 不再中断 agent，变成「双击退出程序」** | **高** |
| 3 | Esc 不再中断 agent，且双击 Esc 会**弹出 rewind 选择器**（本意仅空闲时） | 高 |
| 4 | Enter 的语义反转：改为**直接提交新消息**而非排队（可能在 agent 运行中并发提交） | 中 |
| 5 | 输入框不再进入 loading 态（textarea 可编辑、提示语义错误） | 低 |

> 后果 2/3 是本次调查最重要的发现：**用户想中断 agent，按 Ctrl+C 两次会直接把程序退掉**（`main.rs:997` 无任何 running 保护，`Action::Quit => break 'event_loop`）。

## 根因

### 链路

`ui.loading` 是 TUI 判断「agent 是否在跑」的**唯一代理**，被大量行为门控。而它在 auto-compact 路径上被提前置假：

```text
CompactMiddleware（before_model 钩子，ReAct 循环内）
  └─ compact_middleware.rs:155  send_event(ExecutorEvent::CompactStarted)
       └─ TUI: agent_compact.rs:5  handle_compact_started()
            └─ agent_compact.rs:11  self.session_mgr.current_mut().agent.compact_manual = true;  // ← 无条件！
       └─ 压缩完成: agent_compact.rs:42  let is_manual = ...compact_manual;
            └─ agent_compact.rs:44  if is_manual { self.set_loading(false); }   // ← auto 也命中
```

### 三个关键事实

**① `CompactStarted` 事件不携带来源**
`cc-agent/src/agent/events.rs:184` 是裸变体 `CompactStarted`（无字段）。两条完全不同的路径共用它：

| 路径 | 发射点 |
|---|---|
| 手动 `/compact` | `cc-acp/src/session/command/compact.rs:105` |
| **自动** compact | `cc-middlewares/src/compact_middleware.rs:155` |

TUI 侧仅凭事件**无法区分**两者。

**② `handle_compact_started` 无条件把标志置为「手动」**
`cc-tui/src/app/agent_compact.rs:11` 硬编码 `compact_manual = true`，注释仍写着「标记手动 compact（/compact 命令）」。因为 ①，这个「手动」判断失真——**任何** compact 都会置真。

**③ `is_manual` 判断因此恒真，形同虚设**
`agent_compact.rs:42-46` 的守卫由 ② 永远满足 ⇒ auto-compact 也 `set_loading(false)`。

### 为何 `loading=false` 后本回合不会恢复

`handle_compact_completed` 的注释本身说明了设计意图：

> Auto-compact 在 ReAct 循环内执行，agent 即将 resubmit，spinner 持续到 Done。
> Manual compact 是独立操作，无后续 Done 事件，必须在此结束 loading。

即 auto-compact 后 **agent 在服务端继续 resubmit**，TUI 侧**不会**再走 `set_loading(true)`——生产代码中 `set_loading(true)` 只出现在**用户主动发起**的入口：

| 调用点 | 场景 |
|---|---|
| `agent_compact.rs:14` | compact 开始（本 bug 的起点）|
| `agent_submit.rs:139` | 用户提交消息 |
| `agent_submit.rs:366` | 后台任务 continuation 回流 |
| `shell_command.rs:152` | 用户运行 shell 命令 |

因此该回合剩余全程 `ui.loading == false`，直到下一轮用户手动提交。

### 回归的由来（考古）

**原始正确实现（`725195c7`，2026-05-26）**：标志由 **TUI 侧命令处理器**设置，天然只覆盖手动路径：

```rust
// peri-tui/src/command/session/compact.rs（当时）
// 标记为手动 compact，handle_compact_completed 据此调用 set_loading(false)
app.session_mgr.sessions[app.session_mgr.active].agent.compact_manual = true;
```

**回归第 1 步（`bb388ca6`，2026-05-29，ACP 协议大重构）**：该文件被整体删除（`41 -` 行），**setter 未被迁移**（`git show bb388ca6 | grep compact_manual` 只有删除行 `- .compact_manual = true;`，无新增）。

**回归第 2 步（`a24d811e`，2026-05-29，同日，fix /compact 三个 bug）**：为让手动 compact 恢复正常，把标志「补」到了**两条路径共用的** `handle_compact_started` 里——于是 auto-compact 一并被标记为手动。**修正了手动，却污染了自动**。

一句话：**同一个布尔标志被塞进了无法区分来源的公共入口**，`compact_manual` 从「仅手动路径设置」退化为「任何 compact 都设置」。

### 为何此前未被测试拦住

- `handle_compact_started` / `handle_compact_completed` **没有任何单元测试**（全仓 `grep` 无命中）。
- 仅有的 compact 相关 TUI 测试（`headless_test.rs` 的 `test_compact_done_with_re_inject` / `test_compact_done_without_re_inject`）只断言**渲染消息内容**，通过直接注入 `CompactCompleted` 触发，**绕过了 `handle_compact_started`**，因此完全覆盖不到标志位逻辑。

## 症状详情

| # | 现象 | 观察条件 |
|---|------|----------|
| 1 | 底部 `⎿ Tip:` 行与 spinner 行消失 | auto-compact 后，本回合内必现 |
| 2 | Bash 卡片仍显示 `Running… (Ns)` + `(ctrl+b …)` | 同上（该卡片走 VM 自身状态，不走 `ui.loading`）|
| 3 | Ctrl+C 中断失效，双击直接退出程序 | auto-compact 后必现 |
| 4 | Esc 中断失效，双击弹出 rewind 选择器 | auto-compact 后必现 |
| 5 | Enter 直接提交新消息（跳过排队） | auto-compact 后必现 |

**判定依据对照**（解释现象 1 与 2 为何在同一屏共存）：

| 元素 | 判定依据 | auto-compact 后 |
|---|---|---|
| `⎿ Tip:` 行 | `viewport_loading()`（= `ui.loading`，`message_area.rs:514`）| ❌ 消失 |
| spinner 行 | `viewport_loading()`（`message_area.rs:185`）| ❌ 消失 |
| `Running… (42s)` | Bash VM 自身 `started_at`/`exit_code`（`message_render.rs:1045`）| ✅ 正常 |
| `(ctrl+b to run in background)` | 同上（同 2 秒阈值分支）| ✅ 正常 |
| Ctrl+C / Esc / Enter 语义 | `ui.loading`（`normal_keys.rs:468`/`24`/`202`）| ❌ 全部反转 |

## 复现条件

- **复现频率**：必现
- **触发步骤**：
  1. 长会话持续对话，直至上下文超过 auto-compact 阈值（触发 full compact）
  2. 等待 compact 完成、agent 自动 resubmit 继续执行
  3. 观察底部：`⎿ Tip:` 与 spinner 行消失（Bash 卡片等 VM 内容仍在更新）
  4. 按 Ctrl+C 试图中断 → **不生效**；再按一次 → **程序直接退出**
- **环境**：TUI 模式；auto-compact 启用（默认）

## 涉及文件

- `cc-tui/src/app/agent_compact.rs` — 标志设置点（`:11`）与判断点（`:42-46`），**核心**
- `cc-agent/src/agent/events.rs` — `CompactStarted` 事件定义（`:184`，缺来源字段）
- `cc-middlewares/src/compact_middleware.rs` — auto-compact 发射点（`:155`）
- `cc-acp/src/session/command/compact.rs` — 手动 compact 发射点（`:105`）
- `cc-acp/src/session/command/clear.rs` — `/clear` 复用 `CompactCompleted`（仅发 Completed，不经 Started，**不受本 bug 影响**）
- `cc-tui/src/ui/main_ui/message_area.rs` — 受影响的渲染（`:185`、`:514`、`:653`）
- `cc-tui/src/event/keyboard/normal_keys.rs` — 受影响的按键门控（`:24`、`:202`、`:468`）
- `cc-tui/src/main.rs` — `Action::Quit` 无保护退出（`:997`）

**不受影响**：`/rewind`（`rewind.rs` 只发 `CompactCompleted`，不发 `CompactStarted`，因此不触碰标志）。

## 建议修复方向

**方案 1（推荐）—— 事件携带来源，从源头区分**

```rust
// cc-agent/src/agent/events.rs
CompactStarted { manual: bool },
```

- `CompactCommand`（手动）发 `manual: true`；`CompactMiddleware`（自动）发 `manual: false`
- `handle_compact_started` 改为 `compact_manual = manual`（不再无条件 `true`）
- 优点：区分落在数据本身，不依赖调用顺序或会话状态代理；`is_manual` 判断恢复真实语义

**方案 2 —— TUI 侧自行标记**

在 TUI 提交 `/compact` 时置标志，`handle_compact_started` 不再改它。改动小，但依赖「提交路径能可靠识别 /compact」，且标志跨事件生命周期，较脆（正是本次回归的成因模式）。

**回归测试（无论选哪个方案）**：

1. auto 路径：注入 `CompactStarted { manual: false }` + `CompactCompleted` 后，断言 `ui.loading == true`（回合仍在跑），且 spinner 行/Tip 行仍渲染。
2. manual 路径：`CompactStarted { manual: true }` + `CompactCompleted` 后，断言 `ui.loading == false`（无后续 Done）。
3. 兜底：auto-compact 后 Ctrl+C 仍走 `interrupt()` 而非 `Quit`。

## 关联

- GitHub #29（CLOSED）— 同一现象的前身记录，本次为第 2 次回归
- `spec/archive-issues/2026-05-25-compact-resubmit-missing-loading-spinner.md` — 原始问题（曾 Reopen 一次）
- `spec/archive-issues/2026-05-26-manual-compact-long-loading-skeleton.md` — 相反方向（手动 compact 卡 loading），是引入 `compact_manual` 标志的动机
- 回归引入链：`725195c7`（正确实现）→ `bb388ca6`（删 setter）→ `a24d811e`（错误补位）

## 状态变更记录

| 日期 | 从 | 到 | 操作人 | 说明 |
|------|-----|-----|--------|------|
| 2026-10-14 | — | Open | wismyzhizi2018 | 创建（仅记录，暂不修复） |
