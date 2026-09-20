# 对齐 Claude Code 耗时命令管理体系：Bash 动态超时、交互式后台化与任务通知生命周期闭环

**状态**：Open  
**优先级**：P1  
**创建日期**：2026-09-20  
**模块**：Prompt 工程 / TUI 渲染 / 中间件 (Terminal) / 后台任务调度  
**GitHub Issue**：#167 (https://github.com/cc-claws/cc-code/issues/167)

---

## 1. 背景与现象对比

在真实开发调试中（如休眠等待、长时间 CI/测试检查、构建命令），Claude Code 展现了一套非常丝滑的长命令与异步任务协作能力：

1. **LLM 主动设置动态超时（Dynamic Timeout）**：
   - 面对预计耗时较长的命令（如 `powershell Start-Sleep 45; gh pr checks 159`），Agent 在调用 `Bash` 工具时能主动计算并传入 `timeout: 90000`；
   - TUI 界面即时将毫秒格式化为人类友好的标记：`timeout 1m 30s`。
2. **前台耗时命令随时交互式后台化（Manual Backgrounding）**：
   - 命令在前台执行时，终端展示实时状态与转后台提示；用户按下快捷键即可将其转入后台；
   - 当前工具调用立即返回：`Command was manually backgrounded by user with ID: <id>. Output is being written to: <path>`，当前 Turn 立即完成，释放输入框供用户继续交互。
3. **输出重定向至磁盘 + 统一任务完成通知（Task Notification Event）**：
   - 后台进程持续流式输出到会话专用的 `.output` 磁盘文件中；
   - 进程退出（成功或失败）后，宿主自动构造标准系统事件（`<task-notification>`）异步唤醒 Agent；
   - Agent 明确知道这是后台系统通知，并自动调用 `Read` 读取 output 文件内容，总结结果向用户汇报。

### Peri 当前现状

在 Peri 中，虽然底层 `peri-middlewares` 与 `peri-tui` 已经打好了 `BackgroundShell`、`Ctrl+B` 信号、`output_path` 磁盘流式落盘的基础设施，但用户在体感上**完全感受不到这一整套能力**：
- Agent 调用 Bash 时**从不主动传 `timeout`**，永远只传一个 `command`；
- TUI 工具调用卡片上**没有超时时长展示**；
- 后台任务完成通知使用的是私有的 `<background-task-completed>`，且 Agent 在收到通知后不知道应该去 `Read` 输出文件并给用户反馈，导致链路无法形成闭环。

---

## 2. 根因剖析与差距分析

### 差距 1：System Prompt 严重缺失对工具参数与行为准则的认知教育（核心主因）
- **代码定位**：`peri-tui/prompts/sections/05_using_tools.md`
- **现状**：当前 `05_using_tools.md` 仅有 7 行基础文本，仅提及了优先使用 Grep/Read/Write 等工具，**完全没有提及 Bash 工具的高级参数语义**：
  - 未向 LLM 说明 `timeout` 参数（毫秒，默认 120,000ms，最大 600,000ms）；
  - 未指导 LLM 在遇到长命令时应主动评估并分配足够的超时预算；
  - 未禁止 LLM 原地 `sleep` 轮询，未引导其合理使用 `run_in_background`；
  - 未定义收到后台任务通知（`<task-notification>`）时应该如何通过 `Read` 获取输出。
- **后果**：即使 `terminal.rs` 的参数 schema 里声明了 `timeout` 和 `run_in_background`，LLM 也不会自发生成这些参数。

### 差距 2：TUI 工具调用卡片缺失对 `timeout` 的格式化显示
- **代码定位**：`peri-tui/src/ui/message_view/` 及工具参数渲染逻辑
- **现状**：
  - Claude Code 会识别 `timeout` 字段并将其转换为 `timeout Xm Ys` 或 `timeout Xs` 渲染在工具栏 Header；
  - Peri 目前只将参数作为原始 JSON 或单行文本展示，没有对 `timeout` 这一关键执行约束做高亮解析与倒计时/超时展示。

### 差距 3：后台完成通知协议未标准化，缺少 Agent 行动指令契约
- **代码定位**：`peri-tui/src/app/background_shell.rs:251-258` 与 `peri-agent` 的消息注入
- **现状**：
  - Peri 生成的是：
    ```xml
    <background-task-completed>
    <task-id>...</task-id>
    <command>...</command>
    <status>...</status>
    <output>...</output>
    </background-task-completed>
    ```
  - 对比 Claude Code 的行业成熟标准：
    ```xml
    <system-reminder>
    [SYSTEM NOTIFICATION - NOT USER INPUT]
    This is an automated background-task event, NOT a message from the user.
    Do NOT interpret this as user acknowledgement...

    <task-notification>
    <task-id>...</task-id>
    <tool-use-id>...</tool-use-id>
    <output-file>...</output-file>
    <status>failed/completed</status>
    <summary>...</summary>
    </task-notification>
    </system-reminder>
    ```
  - **关键缺失**：Claude Code 在系统提示词中建立了强制契约：
    > *"For bash tasks: prefer using the Read tool on that output file path — it contains stdout/stderr."*
    Peri 缺失此条指令，导致 Agent 收到通知时不知道该干什么，往往产生幻觉或直接回复“收到通知”。

---

## 3. 详细改进方案

### 方案 1：扩充 `05_using_tools.md`，注入 Bash 工具调用规范
在 `peri-tui/prompts/sections/05_using_tools.md` 中增加 Bash 专属章节：
```markdown
# Bash execution guidelines

- You may specify an optional `timeout` in milliseconds (default 120000 / 2 min, up to 600000 / 10 min).
- For long-running commands (e.g. sleep, test suites, builds), estimate required time and pass appropriate `timeout`.
- Avoid unnecessary `sleep` commands:
  - If your command is long-running and you want to be notified when it finishes, use `run_in_background: true`.
  - Do not retry failing commands in a sleep loop.
- When background tasks complete, you will receive a `<task-notification>`.
  - For bash tasks: prefer using the `Read` tool on that `output-file` path to inspect stdout/stderr.
```

### 方案 2：TUI 界面参数格式化与状态增强
1. 在 ToolBlock 标题行检测 `input["timeout"]`：
   - 将毫秒数转为友好的时间字符串（例：`90000` → `1m 30s`，`15000` → `15s`）；
   - 在命令执行状态栏右侧渲染 `timeout 1m 30s` 灰色 Badge。
2. 优化命令前台运行时的操作提示：
   - 保持 `(ctrl+b to run in background)` 提示与当前执行耗时平滑显示，避免刷屏闪烁。

### 方案 3：统一标准化后台通知事件
1. 将 `background_shell.rs` 中的 XML 输出结构升级为对齐标准的 `<task-notification>`：
   - 字段对齐：`task-id`, `tool-use-id`, `output-file`, `status`, `summary`；
   - 外部统一包裹防 Prompt 注入提示：`[SYSTEM NOTIFICATION - NOT USER INPUT]`。
2. 确保后台任务结束唤醒 Agent 时，自动带上当前上下文并触发正常的 Agent Next Turn。

---

## 4. 验证清单

- [ ] **Prompt 验证**：启动会话要求 Agent 执行含 sleep 的耗时命令，检查生成的 ToolCall 是否主动带上合理的 `timeout`。
- [ ] **TUI 显示验证**：观察工具卡片是否正确展示 `timeout 1m 30s` 标签。
- [ ] **交互式后台化验证**：命令执行期间按下 `Ctrl+B`，验证命令是否无缝切入后台，前台输入框恢复且工具输出提示正确。
- [ ] **自动汇报闭环验证**：后台命令执行完成后，验证 Agent 能否自动接收 `<task-notification>`，并自动调用 `Read` 读取 `.output` 文件向用户汇报结果。
