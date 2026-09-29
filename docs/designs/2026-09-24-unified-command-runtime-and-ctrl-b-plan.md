# 统一命令运行时、计时与 Ctrl+B 设计及实施计划

日期：2026-09-24
范围：Bash、用户 `!` 命令、子 Agent 命令、Hook 等执行阶段的可观测性
状态：设计确定，分阶段实施；下文目标架构不代表已经实现。实施记录见末尾。

## 1. 结论与设计边界

采用“一个命令任务、一份生命周期、多个观察者”的设计：**启动前确定解释器，任务由服务端持有，界面和工具调用只负责观察、等待和请求控制。**

统一的是任务身份、状态、计时、输出与控制协议，不是强迫所有子进程使用同一种 shell，也不是给所有等待都贴 Ctrl+B。

- Bash 工具真正执行 Bash：Windows 使用 Git Bash，Unix 使用 Bash；禁止 CMD 执行失败后自动重跑整条命令。
- 主 Agent、子 Agent、直接后台和手动后台共用任务入口。复合命令、管道及其后代进程属于同一个任务，不按每个子进程创建工具任务。
- 从请求受理开始记录总耗时；审批、Hook、准备、实际执行、输出收尾分别计时，不把等待审批显示为进程运行。
- Ctrl+B 只改变当前等待关系，不重新启动进程、不取消命令、不绕过审批和同步 Hook。
- 提示是否出现由运行时提供的可执行操作决定，不由工具名、命令文本、stderr 或“猜测运行很久”决定。
- 提示词解释真实契约；代码保证生命周期和不重复执行。提示词不是防绕过机制。

不建设新的通用工作流框架，不新增 crate，不在本次顺手统一 Git 探测、MCP、LSP 等所有基础设施的业务语义。

## 2. 证据与现有缺陷

### 2.1 本次命令为什么没有 Ctrl+B

先前现场取证的命令是 `cd /d/code/peri-recap && cargo fmt ... && cargo clippy ... 2>&1 | tail -5`，没有指定 `run_in_background`。它在约 109 秒后返回，而初次 CMD 的任务输出只有“命令语法不正确”。

原链路为：CMD 快速失败 → 不满足 2 秒 UI 注册阈值 → BashTool 根据短 stderr 触发 Git Bash fallback → fallback 直接调用进程 `.output()` → 长时间执行没有进入注入的 ShellExecutor。

现场运行程序来自 **`D:\code\peri-recap`**；本设计和当前修改位于 **`D:\code\peri`**。修改本仓库不会自动替换另一个 worktree 中正在运行的程序。

### 2.2 代码核对入口

下表描述本轮修改前的基线；部分问题将在实施阶段消除。

| 位置 | 已确认的问题/约束 |
| --- | --- |
| [terminal.rs](../../peri-middlewares/src/middleware/terminal.rs) | fallback 位于前台完成分支，直接 spawn；直接后台、Ctrl+B、超时返回走不到同一策略；剩余超时最少补 10 秒会延长预算 |
| [process/mod.rs](../../peri-middlewares/src/process/mod.rs) | 短 stderr 不证明命令没有产生副作用；通用默认 shell 还服务 Hook，不能全局改成 Bash |
| [shell/mod.rs](../../peri-agent/src/shell/mod.rs) | ShellRequest 缺少稳定调用身份、明确解释器；退出信号只提供 bool |
| [agent_shell_executor.rs](../../peri-tui/src/app/agent_shell_executor.rs) | 每次 execute 新建任务；前台延迟注册；UI 拥有控制通道；注册数据未携带所属会话 |
| [shell_command.rs](../../peri-tui/src/app/shell_command.rs) | 注册到当前会话；按 command 文本关联计时；`!` 和 Agent 各有执行池；Ctrl+B 会批量处理前台 Agent 命令 |
| [message_pipeline/mod.rs](../../peri-tui/src/app/message_pipeline/mod.rs) | 通过 Bash 名称和原始 command 精确匹配，改写命令、相同命令并发均不可靠 |
| [shell_exec.rs](../../peri-tui/src/shell_exec.rs) | 流式输出要求消费者持续 drain；已有输出上限和管道收尾超时，应复用而非丢弃 |
| [terminal_inline_executor.rs](../../peri-middlewares/src/middleware/terminal_inline_executor.rs) | 默认宿主没有后台输出服务；不能返回不存在的可用后台任务；stdout/stderr 顺序读取有阻塞风险 |
| [08_windows.md](../../peri-tui/prompts/sections/08_windows.md) | 同时要求 CMD 执行和 POSIX 语法；工具描述还宣称 cwd 持久化，实际每次使用配置 cwd |
| [tool_dispatch.rs](../../peri-agent/src/agent/executor/tool_dispatch.rs) | 已有 tool_call_id；调用上下文应该从这里传入，而不是让模型生成 |
| [event_sink.rs](../../peri-acp/src/session/event_sink.rs) | 两种 transport 的事件映射不同，扩展需明确协商和降级 |

相关问题：[Hook shell](../../spec/issues/2026-09-24-windows-hook-executor-cmd-fallback-and-shell-abstraction.md)、[后台等待协议](../../spec/issues/2026-09-23-background-task-handle-repolling-and-wait-contract.md)、[后台标记泄露](../../spec/issues/2026-09-23-background-task-started-marker-leaks-into-tui.md)。这些文档提供问题背景，不直接采用其中未经验证的候选实现。

### 2.3 明确否决的修补方式

1. **把 fallback 再调用一次 executor**：仍然可能重放 `修改数据 && 不存在的命令`，仍有两个 task_id，后台分支仍不一致。
2. **正则识别所有 POSIX/CMD 语法**：无法可靠判定脚本、引号、变量展开和嵌套解释器的语义；不建立语法猜测路由器。
3. **所有工具一开始就显示 Ctrl+B**：权限审批和同步 Hook 并不支持独立后台化，会成为假按钮。
4. **仅扩大计时范围或仅加提示词**：不能修复运行时绕路、输出丢失、错误会话归属。
5. **按 command/cwd 哈希禁止重复**：用户可能有意并行执行相同命令；幂等键必须是调用身份，不是命令内容。
6. **后台完成后重写原 tool_result**：会破坏消息前缀稳定性，甚至形成重复 tool_result；不能采用。

## 3. “所有命令”的准确范围

| 执行来源 | 统一记录/计时 | Ctrl+B | 归属及限制 |
| --- | --- | --- | --- |
| 主 Agent Bash | 是 | Running 且可 detach 时 | session + agent + tool_call |
| 普通/fork/后台子 Agent Bash | 是 | 同上，需展示所属子 Agent | 子 Agent 身份不能折叠成主 Agent |
| 用户 `!command` | 是 | 可 detach 且 stdin 策略允许时 | session + 用户请求 ID，无伪造 tool_call |
| `run_in_background=true` | 是 | 已在后台，不再提示 | 同一任务从一开始就是 Detached |
| PreToolUse 等同步 Hook | 作为父请求阶段记录，保留自身执行详情 | 不单独 detach | 完成/阻止决定父命令能否启动 |
| 明确配置的异步 Hook | 记录 | 无需再后台化 | Hook 服务管理，不伪装为 LLM 后台 Bash |
| 等待审批、排队、解释器探测/RTK 准备 | 记录阶段耗时 | 不作为进程任务 detach | 显示“等待审批/准备中”等真实原因 |
| MCP/LSP 常驻服务、元数据探测 | 独立诊断 span，可汇总 | 不使用此快捷键 | 已有服务生命周期，禁止混入命令任务列表 |

“统一入口”是受管理的用户命令必须统一，不是拦截操作系统中所有进程。父命令内部启动的程序由进程树管理；恶意程序主动脱离控制域属于沙箱/权限问题，不承诺仅靠任务登记解决。

## 4. 目标架构与所有权

```text
Bash / 子 Agent Bash / 用户 ! 命令
             │  请求 + 可信调用上下文
             ▼
ACP session 持有 CommandRuntime（任务登记、状态、控制、通知）
             │
             ▼
middlewares::process（解释器构造、进程树、并发读写、输出存储）
             │
             ├── 工具等待者：最终结果 或 已后台任务句柄
             └── ACP 状态事件/快照 → TUI、IDE、stdio 客户端
```

分层安排：

- `peri-agent` 定义请求、身份、状态、能力、控制结果等契约，不依赖 TUI。
- `peri-middlewares` 提供执行实现，复用进程构造与输出基础设施；BashTool 不得自行 spawn 用户命令。
- `peri-acp` 在 session 级持有运行时，跨 execute_prompt 重建仍保留任务；正常/后台/fork 子 Agent 使用相同服务但不同 origin。
- `peri-tui` 只投影状态和发送 ACP 控制请求，不通过新的共享 KillHandle/channel 直连后台运行时。现有注入 ShellExecutor 作为迁移桥，不继续扩张。

不同时保留两套任务池作为长期真相源；迁移期间必须明确每个生产入口的唯一 owner。

### 4.1 稳定身份

请求关联键：`session_id + agent_instance_id + tool_call_id`；用户命令使用独立 `request_id`。运行时分配唯一 `task_id`，整个生命周期不变。

- dispatch 显式传递 `ToolInvocationContext`，通过兼容默认方法逐步迁移 BaseTool、ToolWrapper、deferred tool 和子 Agent 包装层。
- 上下文不是 JSON 参数，模型不能指定另一个会话/task 身份。
- 保存 `original_command`（用户/模型原文）和 `effective_command`（真正执行内容），以及选定 shell、cwd；RTK 等改写不能改变关联键。
- 同一调用的重复 submit 返回原句柄；不同调用允许相同命令并发。进程崩溃后不凭此自动重新执行未知结果的命令。
- 同一任务的 start/progress/end 使用递增 revision；UI 用 ID 更新，包括子 Agent 嵌套行，不再匹配显示字符串。

### 4.2 两个正交状态

执行状态：`Preparing → Starting → Running → Draining → Succeeded/Failed/Cancelled/TimedOut/StartFailed`。

等待关系：`Foreground → Detached`，不改变进程状态。初始后台任务直接 Detached；暂不增加不完整的“恢复交互前台”功能。

任务必须在启动前登记。spawn 失败也有 task_id、阶段耗时和终态，不是没有记录的错误。取消发生在 Starting 时必须锁存，禁止随后偷偷启动。

控制请求串行化处理：

- `detach(task_id, expected_revision)` 返回 Applied / AlreadyDetached / AlreadyFinished / Unsupported / NotFound。
- 完成先发生：返回 AlreadyFinished，不生成虚假的后台启动结果。
- detach 先发生：工具返回一个后台句柄，之后生成一次逻辑完成通知；底层命令继续执行。
- cancel 必须等清理确认再进入终态；用户重复操作幂等。

## 5. Shell 与兼容策略

### 5.1 Bash 的确定性契约

**BashTool 固定 Bash 语义**。Windows 必须找到可用 Git Bash，否则在运行用户命令之前清晰报错；不偷偷使用 WSL Bash 或 CMD 解释同一文本。Unix 执行 `bash -c`。

初期不新增模型侧 `shell` 参数，以免把 Bash 变成语义不明确的通用 Shell 工具。确需 CMD/PowerShell 的命令，显式调用相应解释器并正确引用其脚本；外层仍是同一受管理 Bash 任务。今后需要 native-only 工具时单独设计 schema 和权限判断。

内部请求使用有类型的 shell 选择，不能通过字符串拼接 `bash -c ...` 把 quoting 留给另一层 CMD。Hook 的配置 shell、默认 platform shell 保留独立兼容语义。当前 `!` 入口先保留 platform default；目标阶段明确显示解释器标签，不静默改变用户手写命令的语法。

不启用隐式 `set -e`/`pipefail`，避免未经说明改变脚本退出语义。验证命令使用管道时，提示模型显式设置 `set -o pipefail`；`cargo clippy | tail` 默认可能只返回 tail 的退出码。

Windows 现有 git commit `-m → -F` 改写仅为 CMD workaround，Bash 路径应移除，避免损坏原生 Bash 的引号、变量展开和多段消息。为中文、多段消息、路径含空格补隔离测试，不能仅删除旧测试。

### 5.2 准备与重试

允许在用户命令尚未启动时探测可执行文件位置，但探测必须有时间边界并单独计入 Preparing。不得执行用户命令来探测语法。

一旦启动过用户命令，无论短 stderr、exit 1/2/127、RTK 失败还是输出为空，都返回真实结果，不自动换 shell、不自动撤销改写重跑。需要重试必须成为明确的新调用。

RTK 只改写支持的 Bash 输入，保留原文与改写文本；依然不能依据其结果推断命令是否产生过副作用。准备进程不计为另一次用户命令。

## 6. 计时、等待期限与取消

记录 `accepted_at`、阶段切换、`spawned_at`、`process_exited_at`、`completed_at`。本机时长使用单调时钟，协议传已累计毫秒数和必要墙钟时间，不序列化 Instant、不用墙钟回拨计算时长。

- 总耗时 = 请求受理到终态，包括实际等待；运行耗时 = 成功 spawn 到进程退出；Draining 单独显示。
- 审批可显示等待时长，但默认不消耗进程执行预算；同步 Hook 使用自己的 timeout，并纳入父请求总时间。
- 所有任务立即登记，2 秒只控制运行提示的防抖显示，不控制任务是否存在、能否响应按键。
- 完成后计时冻结；切换会话、刷新页面、转后台都不能重置。
- 前台等待期限与硬执行期限分离：`foreground_wait_ms` 决定何时返回后台句柄，`execution_deadline` 决定何时取消进程树。两者都不能在 detach 后重置。
- 迁移期保留现有 `timeout` 映射并如实描述：可后台宿主达到期限时 detach，inline 宿主取消。后续增加明确硬期限之前，不把前台 timeout 宣称为执行上限。
- 任何 deadline 用同一任务的绝对时限；禁止 fallback 的“至少额外 10 秒”。

Ctrl+C 取消当前前台工作及其从属任务；已显式 Detached 的任务保留到用户取消或 session 真正关闭。关闭/删除 session 与临时断开客户端不同：前者清理所有所属进程树，后者在服务仍存活时允许任务继续并可恢复观察。进程退出时不承诺后台任务跨程序重启继续运行。

无后台宿主必须在 spawn 前拒绝 `run_in_background=true`；不能返回虚构的 task_id/output 路径。print/stdio 能力由持有任务的服务生命周期决定，而不是“没有 TUI 就永远不支持”。

## 7. 用户交互规则

统一命令行信息：`标题/命令摘要 · 来源 · shell · 状态 · 耗时 · 可用操作`。命令使用显示列宽省略，中文不能按字节截断。

示例：

```text
Bash(cargo clippy …) · 主 Agent · Git Bash · 运行 1m 12s   Ctrl+B 后台运行
Bash(cargo test …)   · 子 Agent A · Git Bash · 后台运行 8s
Bash(git commit …)  · 等待 PreToolUse Hook 4s（完成前不能后台化）
```

Ctrl+B 目标规则：有明确选中的可后台命令，操作该任务；无选择且只有一个候选，直接操作；多个候选，打开任务选择器，由用户选一个或显式“全部后台”。不再悄悄把所有并发命令转后台。

工具行提示与全局快捷键使用同一个 capability/target resolver。多任务时全局写“Ctrl+B 选择后台任务”，不能每行暗示按键只处理自己。已经退出/已后台/不支持 detach 的任务没有后台化提示；Preparing/Hook 显示原因，不能表现为卡死。

`!` 的 stdin 策略必须显式：普通无交互命令可 detach；需要前台 stdin 的任务，在实现后台 stdin 所有权之前标记 Unsupported，不得靠丢弃 sender 把后台化变成 EOF。PTY/交互会话不在首批迁移范围。

计时刷新只更新受影响行/区域，沿用防闪烁的 diff 渲染，不每秒强制全屏重建。

## 8. ACP、后台通知与任务等待

拟新增 Peri 私有 task snapshot/event 和 detach/cancel/read/wait 方法，名称以实现时协议模块为准；**不是宣称 ACP 标准已经定义这些方法**。

- initialize 协商任务展示/控制能力。普通 ACP 客户端仅接收兼容的工具状态和文本结果；不支持控制就不承诺快捷键。
- TUI 与 stdio 两条 event sink 同步实现或明确降级，不仅更新一侧映射。
- 事件携带 session、origin、task、revision；注册到事件所属会话，不是当前聚焦会话。
- 提供活跃任务快照，解决断线、乱序、漏过 Started、先收到 Completed 的情况；旧 revision 不覆盖新终态。
- 服务端保存真实 exit_code、取消/超时原因与输出状态，不用 `-1` 猜完成结果。

每个原始 tool_use 只产生一个 tool_result：前台完成返回最终结果；detach 返回结构化任务句柄及文本兼容表示。后续完成事件作为独立通知追加，不篡改历史、不再给同一 tool_call 追加第二份 tool_result。

传输允许重复，使用 `task_id + terminal_revision` 做逻辑去重；不承诺网络层 exactly-once。完成/后台化的竞态由 runtime 决定通知资格。子 Agent 已结束时，完成通知交给所属 session 的父协调者，不注入另一个会话。

通过已有 deferred tool 机制提供任务 read/wait/cancel 操作，避免增加核心工具数量。wait 读取已有句柄，使用有界 long-poll/cursor；没有变化返回 pending，不新开 Bash、不频繁触发模型推理。输出刷新只去 UI/存储，不每一行写入上下文。

不以“命令内容相同”硬拦新的合法工具调用；对已有 watch 再查相同状态的行为可提示已有任务，但不能靠无法证明语义等价的黑名单保证模型不重复操作。

## 9. 输出与进程生命周期

运行时独立 drain stdout/stderr，持有磁盘输出和有限内存预览；不能因 UI 未消费、切换会话或 result receiver drop 而阻塞子进程。后台化前后同一个输出文件/游标，不复制或重建任务。

复用已有输出截断和 pipe drain timeout，补齐清晰状态：进程退出、输出读完、磁盘 flush 完成不是同一时刻。完成通知应在可读输出落盘后发出；收尾超时/写盘失败要披露 output_incomplete，不能伪称完整输出。

取消 Windows 进程树使用 Job，Unix 使用独立进程组及升级终止；不能把 `kill_on_drop(true)` 当作全平台整树保证。Hook 已有 capture 辅助器可复用，但流式执行须单独验证，不能仅凭 Hook 测试宣称覆盖。

明确输出配额与磁盘失败策略：停止继续积累，记录 output error，取消任务并收敛终态，保留已落盘数据。任务列表清理不等于立即删除仍被面板/read 游标使用的文件；输出保留策略与活跃任务生命周期分开。

服务重启时，历史 running 标为 interrupted/unknown，不按 PID 复用进程、不自动重跑。需要跨重启托管时应另立后台 daemon 设计。

## 10. 提示词与会话兼容

工具描述的第一原则是与执行事实一致。首阶段可直接落地以下契约：

```text
Runs one command using Bash (Git Bash on Windows). The interpreter is chosen
before execution. Failed commands are never automatically rerun in another shell.
Each invocation starts in the configured working directory; cd and shell state
do not persist across calls. Use an explicit interpreter for native CMD or
PowerShell scripts. Background execution requires host support. When a task
handle is returned, reuse that task; do not rerun the command to retrieve output.
```

Windows 段落取消“CMD + fallback”承诺，说明 Bash/POSIX quoting、Git Bash 必需、原生脚本显式解释器。禁止裸写 PowerShell cmdlet，但不禁止有意执行 `powershell -File`。

任务 wait 工具未交付前，不在提示词中宣称可以调用它；保留现有输出路径/完成通知的真实使用方法。任务控制能力不能通过每轮重写系统提示词动态注入。

系统提示词、工具 schema 与能力模板在 session/new 冻结。行为版本升级只用于新建或明确迁移的会话；恢复旧 session 时保留其契约版本或提示建立新会话，不能静默在一个活动会话中更换模板。首阶段部署要求重启并新建会话，恢复版本迁移机制在协议阶段补齐。

## 11. 防绕过措施

1. BashTool 只能构造请求和处理结果，移除直接 Command/output fallback；测试断言每次调用只提交一次。
2. 内部 shell 枚举贯穿请求和执行器；TUI 与 inline 都走同一解释器构造函数。Hook 保留明确入口与政策，不能借“统一”更改 exit 2 阻止语义。
3. 运行时保存稳定 ID/版本，所有生产入口提交到服务；UI 没有绕开服务控制进程的第二条路径。
4. CI 检查生产源中的 spawn/output/Command 创建点，维护小型理由明确的 allowlist（进程模块、基础设施服务、探测等）。先可用代码扫描，后升级语法树检查；注释/测试不计入违规。
5. 为主 Agent、三类子 Agent、直接后台、`!`、两个 ACP transport 各写契约测试，不能只测一个示例命令。

Rust trait 不能禁止同一 crate 中开发者重新写 Command；真正约束来自最小可见性、入口清单、CI 和集成测试。静态扫描是回归护栏，不是安全沙箱。

## 12. 分阶段实施与验收门槛

### P1：移除本次绕路与重复执行风险

范围：`peri-agent/src/shell`、`middlewares/terminal*`、`process`、TUI shell adapter、Windows 提示词及相应测试。

- 明确 Bash 请求的解释器；inline/TUI 都从请求选择 shell。
- 删除执行后 CMD→Bash fallback 和 Bash 路径的 CMD quoting workaround。
- 保留显示原文和执行文本，避免 RTK 改写造成现有 UI 完全匹配不到命令。稳定 ID 替代留在 P2，不把此过渡修复当最终架构。
- 修正错误的 cwd/profile/timeout 描述；无后台宿主在启动前拒绝直接后台。
- 验证短 stderr、先写副作用再失败、单行 POSIX、引号/中文、直接后台和 Ctrl+B 的执行器契约。

交付门槛：相关单测及 clippy 通过；真实长 Bash 能进入 TUI 登记链路；测试不能通过“Git Bash 不可用就跳过”伪造成功。此阶段不宣称解决全部跨会话/子 Agent 计时问题。

### P2：服务端任务身份与生命周期

先加调用上下文并覆盖所有工具 wrapper，再把执行状态/输出所有权迁到 session 服务。先实现有界队列、状态机、cancel/detach 竞态和真实结果，再迁移客户端。

交付门槛：启动前登记；同调用幂等、不同调用同命令可并行；取消无泄漏；输出与退出顺序稳定；prompt 结束不销毁 Detached 任务；session 关闭整树清理。

### P3：ACP 与统一界面

新增协商事件/快照/control，切换主 Agent、子 Agent、用户 `!` 的数据源；删去旧字符串匹配和双池 owner。stdin 未满足能力门槛的 `!` 命令先明确禁用 detach。

交付门槛：跨会话事件不串线；多任务选择规则可预测；快速命令也有时长；2 秒阈值只防抖；提示与实际控制同源；TUI/stdio 降级一致。

### P4：等待协议、提示词闭环与回归护栏

提供 deferred read/wait/cancel，完成通知去重和子 Agent 归属；更新新会话提示词/工具说明；CI 阻止新的生产旁路。

交付门槛：watch 场景可复用已有任务等待；输出刷新不刷模型上下文；不制造第二个 tool_result；真实验收以下矩阵。

### 验收矩阵

| 场景 | 必须验证 |
| --- | --- |
| Windows 单行 POSIX + 长等待 + `2>&1`/管道 | 从一开始选 Git Bash，一次执行，真实计时，Ctrl+B 后继续同任务 |
| 先写 count 再短 stderr/exit 2/127 | count 只增加一次，保留原错误 |
| Git Bash 缺失/路径无效 | spawn 用户命令之前报错，无副作用，无静默 CMD/WSL |
| RTK 改写、相同命令并发 | 原文展示不丢；最终按调用 ID 关联，不串计时 |
| 直接后台、手动后台、到期自动后台 | 同任务 ID/输出/进程，通知一次，真实退出码 |
| 瞬间退出与 detach/cancel 竞争 | 一个终态、一个原始 tool_result，不出现幽灵任务 |
| 审批拒绝、同步 Hook exit 2 | 命令零次启动，阶段耗时可见，没有绕过按钮 |
| 子 Agent normal/fork/background | 正确 owner、取消域和通知路由 |
| 执行期间切换会话/断开重连 | 原会话归属与累计耗时不变，快照恢复 |
| print/无后台服务 + 直接后台请求 | 启动前拒绝，不返回不存在的 output 文件 |
| 大量 stdout/stderr、管道继承、磁盘错误 | 不死锁、不 OOM、收尾有界、输出不完整明确标记 |
| 中文/空格路径/git 多段消息 | 原生 Bash quoting 正常，不触发旧 CMD 改写 |
| Windows Job / Unix process group 取消 | 后代不再写入隔离目录，无残留工作进程 |
| frozen prompt / 恢复旧会话 | 活动会话缓存前缀稳定，契约升级不静默发生 |

各平台测试只代表实际执行的平台；Windows 本地通过不能写成 Linux/macOS 已验证。自动化执行器测试也不能代替终端中按键、选择器、显示宽度的人工验收。

## 13. 发布与回滚

先交付 P1 的闭环，再推进 P2→P3→P4，不把半成品状态协议暴露给 UI。既有 Hook 修复保留独立测试和影响说明。

首次发布明确 Windows Bash 语义变化和 Git Bash 前置要求；验证用户当前启动的是哪个 worktree/二进制。已有运行进程不热替换。新运行时启用开关只能在 session 创建时选择，不能在运行中把活跃任务切换 owner。

回滚优先撤销新 runtime 接入，保留“不重放已执行命令”的安全约束；不要把危险 fallback 当作兼容开关重新打开。旧 session/活跃任务先完成或显式取消，再切版本。

## 14. 实施记录

- 2026-09-24：完成代码路径审计和此方案；P1 代码、局部回归与静态检查完成。当时 P2–P4 尚未实现；2026-09-28 的增量修复见下节，不代表整套服务迁移完成。
- `ShellDialect` 从 BashTool 透传到 inline/TUI，共用解释器构造；删除执行后 fallback、其错误识别启发式和 CMD git commit 改写。Windows 无原生 Git/MSYS Bash 时明确拒绝。
- P1 请求分开保存原文与执行命令，避免 RTK 改写破坏显示关联；当时仍使用字符串关联。2026-09-28 已替换为调度器提供的调用身份。
- inline 宿主启动前拒绝直接后台，并发 drain stdout/stderr；TUI spawn 失败直接返回错误，不注册虚假后台任务。
- 根据用户后补截图修复后台 XML 展示泄露：在共用渲染入口投影为“已转入后台（任务 ID）”，详情显示输出位置；原始工具结果不变，普通/错误/不完整文本不被过滤。标记使用中性色，不将后台移交误报为进程执行成功。
- 兼容期 XML 投影只能识别旧格式，不能区分“程序恰好输出完全相同 envelope”；它不是安全过滤器。P3 的可信结构化元数据应替代该识别方式。
- 本文中的目标协议、状态与 UI 行为均需按阶段验证，不代表现有代码已经支持。

### P1 历史验证记录（Windows 本地，2026-09-24）

| 命令/检查 | 结果 |
| --- | --- |
| `cargo test -p peri-middlewares --lib middleware::terminal` | 32 通过 |
| `cargo test -p peri-middlewares --lib process::` | 22 通过 |
| `cargo test -p peri-middlewares --lib hooks::executor::` | 29 通过，包含原 Hook 修复回归 |
| `cargo test -p peri-tui --lib ui::message_render::` | 54 通过，包含 4 个新增后台展示测试 |
| `cargo test -p peri-tui --lib agent_shell` | 21 通过，包含真实 Bash 进程、后台化信号、同任务输出及启动失败测试 |
| `cargo test -p peri-tui --lib shell_exec::` | 8 通过 |
| `cargo test -p peri-acp --lib prompt::` | 29 通过 |
| `cargo clippy -p peri-agent -p peri-middlewares -p peri-acp -p peri-tui --lib --tests -- -D warnings` | 通过 |
| 改动 Rust 文件的 `rustfmt --check`、`git diff --check` | 通过 |
| 本文 14 个本地链接存在性检查 | 通过 |

共 195 项局部测试通过，不等于全 workspace 测试通过。真实进程测试使用隔离目录内的 `printf/sleep` 复合命令，不重跑用户完整构建；git 多段消息测试使用 Bash 同名函数检查真实 argv，不创建仓库提交。未执行 Linux/macOS、实际 TUI 人工按键验收或旧会话迁移验收；修改仅位于 `D:\code\peri`，未替换其他 worktree 或正在运行的程序。

旧“100ms 内一定完成 echo”的测试在 Git Bash 冷启动时失败，已改为直接断言传给执行器的最小 timeout=1ms；没有通过放宽生产超时掩盖错误。旧 fallback/CMD 改写策略测试随废弃实现删除，并用真实单次副作用、Bash 引用和执行器调用次数回归替代。

### 2026-09-28：现有 TUI 执行链路的生命周期补强

此次先修可复现缺陷，不以提示词替代执行层保证，也不把 P2–P4 的完整服务迁移混入修复。

1. **启动立即登记，显示延后两秒**：短 `timeout` 不再落入延迟登记窗口；后台句柄对应真实登记任务，登记通道关闭时停止进程并报错。
2. **可信调用身份**：`tool_dispatch` 通过 task-local `ToolInvocationContext` 传递 `tool_call_id`，normal/fork/background 子 Agent 的 state 提供 `source_agent_id`。模型参数不能伪造这两个值。UI 按 `(source_agent_id, tool_call_id)` 关联，支持登记先于工具事件和相同命令并发；不再按命令文本关联。
3. **真实终态**：`ExitSignal` 一次性保存退出码、超时、取消或执行错误。工具返回后台句柄后即使释放结果接收器，UI 仍可得到真实终态。自动后台请求先于退出收口，快速结束任务不会漏通知；写盘任务结束后才发布完成信号。
4. **独立硬期限**：`timeout` 保持旧的前台等待语义；新增 `execution_timeout` 控制总运行时间，前台、手动后台、自动后台、直接后台均受约束。起点为进程启动，后台化不重置。非法硬期限在启动前拒绝。
5. **Windows 进程树与取消**：TUI streaming/capture 共用 `ManagedChild`，Windows 通过隐藏、挂起启动及 Job 管理后代。取消、硬超时和根进程退出都会清理 Job 内残留后代；stdout/stderr/stdin 辅助任务也随 owner 取消，管道超时不再 detach reader。前台工具 future 被取消时会停止尚未移交后台的进程。
6. **可预测的 Ctrl+B**：一个前台任务直接后台化；多个前台任务打开任务面板，方向键选择后 Ctrl+B 仅操作选中项。面板是 global scope，快捷键不得先被全局分支消费。嵌套工具计时由 render ticker 更新，并显式失效旧组缓存；无需等待新 stdout 才出现提示。
7. **展示与模型协议分离**：后台开始的 XML 仍用于模型/历史，TUI 投影为任务提示；完成通知准确区分成功、失败、取消和超时，不把取消显示为“已完成”。
8. **控制面消失时拒绝启动**：登记通道已关闭时在执行命令前报错；若启动与通道关闭发生竞争，则停止已启动的命令并等待清理，不返回幽灵句柄。

新增运行元数据使 `MessageViewModel` 增大，因此 `PipelineAction::AddMessage` 改为 boxed payload，并提供统一构造函数。消息提交、通知、压缩及中断路径只调整构造/解包，展示语义不变；相应扩大到 TUI 全库回归，不通过禁用 `large_enum_variant` 警告处理。

| 参数 / 行为 | 当前修复后的含义 |
| --- | --- |
| `timeout` | 默认 120000 ms，上限 600000 ms；TUI 到期移交后台，inline 宿主到期取消 |
| `execution_timeout` | 默认 600000 ms，允许整数 1–600000 ms；到期停止任务，不是再后台化 |
| `Running… (Xs) (timeout 10m)` | 实际已运行时间及硬执行上限；并非审批/Hook 等待时间 |
| Ctrl+B / `run_in_background` | 继续同一次执行，保持任务、输出路径和截止时间 |
| PowerShell / CMD / git / Git Bash | 从 Bash 工具调用的整条命令统一登记；原生脚本需显式解释器，内部子命令不是独立工具任务 |

#### 范围与剩余边界

- 已接入的主/子 Agent Bash 调用共用此链路；新提示词只作用于新会话构建，不修改活跃会话的 frozen prompt。
- 这是生命周期管理，不是阻止任意恶意代码逃逸的安全沙箱。外部 MCP 服务、独立系统服务/计划任务和同步审批 Hook 不属于可任意 Ctrl+B 的命令任务；不能承诺“系统上一切进程都被接管”。
- `!command` 保留既有前后台交互，仅共享本次进程树/I/O 清理；没有借此加入 Agent 专用的硬期限参数。
- 完整 ACP 任务协议、跨会话 owner/重连、对子 Agent 定向投递后台完成事件、Unix 进程组强制清理、deferred read/wait/cancel 及生产 spawn CI 护栏仍是后续阶段。现有完成通知仍由 TUI 会话轮询注入，不能宣称上述迁移已完成。
- 输出仍沿用既有磁盘大小上限和写失败策略；等待 writer 退出保证正常写盘路径的可读顺序，不代表新增了磁盘错误恢复或无限日志能力。

并行处理的 UI 路径（`shell_command`、任务面板、调用身份映射）和进程路径（`process/capture`、`shell_exec`）均由主任务补齐接线和回归，不以局部实现替代整体按键链路测试。

| 子任务 | 合并结果与主要文件 |
| --- | --- |
| `fix_shell_ui` | 身份映射、嵌套计时、单任务后台化及真实完成通知；`app/message_pipeline/shell_runtime.rs`、`app/shell_command.rs`、`app/background_tasks_panel.rs`、`ui/render_thread.rs` |
| `fix_process_tree` | Windows Job 所有权及 I/O 取消回收；`peri-middlewares/src/process/capture.rs`、`peri-tui/src/shell_exec.rs`、`peri-tui/src/shell_exec_lifecycle_test.rs` |

测试过程说明：一次默认并发运行中，两个旧的 Bash 语义测试超过各自设定的 5 秒等待期限；保持生产代码和测试期限不变，串行全组及随后三轮默认并发全组均通过。记录为本机时序波动，不以放大超时掩盖。构建曾因 D 盘耗尽失败，仅清理本仓库两个 TUI 增量构建缓存目录后重跑，后续验证使用 `CARGO_INCREMENTAL=0`；未清理源码或替换运行中的程序。

TUI 首轮全库回归为 956 通过、1 失败、8 跳过；失败的面板输出测试固定等待 100ms 读取异步结果，独立重跑两轮均通过。现已改为等待真实输出 channel 就绪，并经实际 global 面板入口打开；未改生产输出渲染时序。其后一次重编译再次遇到磁盘耗尽，空间恢复后重新验证；未停止其他工作区的构建。

#### 最终验证记录（Windows 本地，2026-09-28）

以下 Cargo 验证使用 `CARGO_INCREMENTAL=0`，不修改仓库构建配置。

| 命令 / 检查 | 最终结果 |
| --- | --- |
| `cargo test -p peri-tui --lib -- --test-threads=4 --quiet` | 961 通过，0 失败，8 跳过（含交接竞态补测） |
| `cargo test -p peri-agent --lib tools::invocation::` | 2 通过 |
| `cargo test -p peri-agent --lib tool_dispatch` | 36 通过，含并发身份隔离及不丢 tool_result 回归 |
| `cargo test -p peri-middlewares --lib middleware::terminal::tests -- --test-threads=4` | 40 通过（此前 36 项及新增 4 项交接测试） |
| `cargo test -p peri-middlewares --lib process:: -- --test-threads=1` | 22 通过 |
| `cargo test -p peri-middlewares --lib hooks::executor:: -- --test-threads=1` | 29 通过 |
| `cargo test -p peri-middlewares --lib subagent::tool::` | 33 通过 |
| `cargo test -p peri-acp --lib prompt::` | 29 通过 |
| `cargo clippy -p peri-agent -p peri-middlewares -p peri-acp -p peri-tui --lib --tests -- -D warnings` | 最终通过 |
| 触达 Rust 模块的 `rustfmt --check`、`git diff --check` | 通过；`include!` 测试保留父模块原有格式，未扩散重排无关代码 |

以上去重共 1152 项通过、8 项跳过，不代表全 workspace 测试通过。交接竞态修复后重跑了 TUI 全库、终端模块和严格 Clippy；其他模块记录来自本轮之前已完成的验证。TUI 全库结果已经包含前期各过滤测试组，不重复累加。

真实进程验收包含：CMD / Git Bash 调起 PowerShell 子孙后取消、根进程退出后清理后代、短等待自动后台、四种前后台模式下硬期限不重置、退出码 0/7 及完成时可读最终磁盘输出。按键与显示采用 headless TUI 测试；未启动真实交互终端人工验收，未验证 Linux/macOS。

### 2026-09-28：Ctrl+B 与命令完成的交接竞态

继续复查发现：旧 `mark_backgrounded()` 忽略 oneshot 发送失败，仍将 UI 标成后台；工具等待端又优先选择结果，因此后台请求和结果同时就绪时，可能既返回前台结果，又注入后台完成通知。

- 工具等待端选择完成或超时时，先关闭手动交接入口，再消费关闭前已经成功发送的请求。已接受交接则只返回后台句柄；否则保留前台结果或超时策略。单纯调整 `select!` 优先级无法封住跨线程窗口。
- UI 只有手动信号成功发送才改变归属；已退出、通道关闭或缺少发送端时不标后台、不启动 watchdog，也不丢弃并发到来的自动后台请求。
- 自动后台信号代表工具已完成移交，单独确认归属，不复用手动请求方法。保留短任务在 UI 首次轮询前已结束时恰好一次通知的语义。
- 新增同时就绪、跨线程竞争（128 次）、关闭入口、超时交接和 UI 拒绝场景测试；原键盘测试改为持有真实接收端并断言只有选中任务收到信号。

修复前新增工具等待测试为 1 通过、2 失败，其中确定性的“同时就绪”和跨线程竞争均复现；修复后终端模块 40 项全部通过。TUI 第一次回归 959 通过、1 失败、8 跳过：失败键盘测试提前丢弃了后台 receiver 却期望成功，依赖了发送失败仍标记后台的旧错误行为。修正 fixture 并增加交接拒绝后的通知断言后，最终 TUI 961 通过、0 失败、8 跳过；严格 Clippy、触达模块格式检查及 `git diff --check` 通过。

### 2026-09-29：本地可执行文件交付

- `cargo build -p peri-tui --bin cc-code` 构建成功；Windows PowerShell 下通过 `$env:CARGO_INCREMENTAL = '0'` 关闭本次增量缓存。
- 产物：`D:\code\peri\target\debug\cc-code.exe`，66,398,208 字节；`--version` 与 `--help` 均退出 0，版本输出为 `cc-code 0.2.0`（沿用 workspace 版本，未修改 npm 发版号）。
- SHA-256：`F18E042E31854A99072E7EACEF1B00DBE5B005F173056F31B0B6369A4C98C683`。
- 当前发现的运行实例来自 npm 安装目录以及 `D:\code\cc-code\target\debug\cc-code.exe`，并非本工作区；没有覆盖、停止或重启它们。
- 构建期间 D 盘空间紧张，检查绝对路径、reparse point 及编译器占用后，仅删除本仓库 `target\debug\incremental\cc_code-12tb9yw7r464e` 旧缓存（约 464 MiB）。缓存可由 Cargo 重新生成，未删除源码或可执行文件。
- 仅完成启动参数冒烟检查和上述自动化验证，未连接真实模型进行交互终端人工验收。

在 Windows PowerShell 中明确启动本次产物：

```powershell
& 'D:\code\peri\target\debug\cc-code.exe'
```

代码位于 `D:\code\peri` 的 `fix/windows-hook-shell-routing` 分支；未提交、未推送，也未同步到其他 worktree。已构建本工作区版本，现有运行实例不会自动获得这些修复。
