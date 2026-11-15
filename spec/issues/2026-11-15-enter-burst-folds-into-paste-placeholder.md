# 连按 Enter 后输入框偶发出现 `[Pasted text #N +X lines]` 占位符

**状态**：Open
**优先级**：中
**创建日期**：2026-11-15

## 问题描述

在 Windows Terminal 上，Agent 正在运行（loading/转圈）时连续按 Enter，输入框里会**偶发**出现一行粘贴占位符 `[Pasted text #N +X lines]`。用户期望的是一次连按 Enter 只等同于重复提交/无动作，输入框保持空白，不应凭空出现任何文本。

## 症状详情

| 观察项 | 表现 |
| :--- | :--- |
| 出现位置 | 输入框（textarea）内，作为一行普通文本 |
| 出现内容 | `[Pasted text #N +X lines]` 占位符，`N` 为自增编号，`X` 与连按次数相关 |
| 出现时机 | Agent 正在运行（loading）期间连续按 Enter 之后 |
| 复现频率 | 偶发，非必现 |
| 与真实粘贴的区别 | 用户并未执行任何粘贴操作 |

## 复现条件

- **复现频率**：偶发；仅在后续 Enter 已经排入输入队列时出现，因此与当时的渲染/流式处理耗时相关
- **触发步骤**：
  1. 在 Windows Terminal 中启动 cc-code TUI，提交一条会长时间运行的消息，使 Agent 进入 loading 状态
  2. 在 Agent 运行期间连续按 Enter 若干次
  3. 观察输入框：偶发出现 `[Pasted text #N +X lines]` 占位符
- **环境**：Windows + Windows Terminal；Agent 运行中（loading）

## 涉及文件

- `cc-tui/src/event/mod.rs` —— 输入事件入口与 `detect_simulated_paste`（模拟粘贴检测）所在
- `cc-tui/src/event/input_pump.rs` —— 终端输入泵：独立线程读取终端事件并入队
- `cc-tui/src/app/paste_ops.rs` —— 粘贴文本落进输入框、多行折叠为占位符的逻辑

## 状态变更记录

| 日期 | 从 | 到 | 操作人 | 说明 |
|------|-----|-----|--------|------|
| 2026-11-15 | — | Open | agent | 创建 |

## 修复记录

### 修复 #1（2026-11-15）

- **操作人**：agent
- **用户原意**：连按 Enter 不该在输入框里凭空出现粘贴占位符；按「事件真实到达时刻」根治
- **修复内容**：事件带上被读到瞬间的到达时刻（`StampedEvent`），模拟粘贴检测的两个时间窗口改为按真实到达间隔判定；已缓存但间隔超窗的事件退回输入队列，按独立按键逐轮处理
- **涉及 commit**：`bf08249`（分支 `fix/simulated-paste-arrival-timestamp`）
- **验证状态**：待验证（单测与反转验证通过，真机端到端未验证）
