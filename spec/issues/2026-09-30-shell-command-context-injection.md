# 前台 `!` Shell 命令结果回流 Agent 上下文（静默注入）

**状态**：Draft（待确认后开工）
**优先级**：中
**创建日期**：2026-09-30
**方案代号**：C（静默注入，对齐 Codex CLI / claude-code-best）

---

## 一、问题描述

用户在 TUI 输入框执行前台 `!cmd`（如 `!git push -u origin fix/x`）后，**Agent 完全无感知**：

- 命令与输出只写入 TUI 侧 shell 历史（`ShellCommandStore`）并在聊天区渲染，**不进入 Agent 上下文**
- Agent 想确认结果只能自己猜测或另起命令去查（例如 `git ls-remote`）
- 实际后果：本次会话中用户 `!git push` 后，Agent 误判为「推送未成功」，需用户再口头告知

这与主流实现（Codex CLI、claude-code-best）的默认行为不一致 —— 它们都会把 `!` 命令及其输出作为**上下文的一部分**保留，使模型在后续轮次自然可见。

### 现状证据

| 环节 | 位置 | 行为 |
|------|------|------|
| 完成处理 | `peri-tui/src/app/shell_command.rs:925` `handle_shell_command_completed()` | 仅 `persist_shell_record()` + 渲染 `ShellCommand` VM，无 `submit_message` / `push_recall` / ACP 调用 |
| 事件定义 | `peri-tui/src/app/events.rs:7` | 注释明示：`/// TUI 本地 shell 命令执行完成（不进入 Agent history）` |
| 数据边界 | `peri-tui/src/shell_history.rs:16-18` | `ShellCommandRecord` 注释：`intentionally separate from BaseMessage so shell output is restored in the UI without entering Agent history` |
| 注入通道 | `peri-tui/src/app/shell_command.rs:335`、`:663` | 仅**后台**命令走 `submit_message()` 注入（`<background-task-completed>`） |

> **历史澄清**：该行为自 `812bbf1b`（2026-06-03，`!` 前缀功能首版）即为设计选择，**非缺陷回归**。2026-06-28 的 `71685599`/`df6c56ff`（Ctrl+B 后台 shell）才引入注入，且只服务后台路径；`CHANGELOG v0.6.32 (#100)` 明确记载「前台小命令快速结束不再打断对话流，仅后台化（Ctrl+B）命令注入通知」。

---

## 二、目标

1. 前台 `!` 命令（成功/失败/中断）的**命令文本 + 输出 + 退出码**进入 Agent 上下文
2. **不触发新一轮推理**（不打断对话流）
3. **不改变** TUI 侧已有的 shell 历史与渲染行为（回归风险可控）
4. 大输出不撑爆上下文窗口

## 三、非目标

- 不引入「Agent 主动感知后自动回话」的行为（那属于方案 B，会打断）
- 不改后台命令现有通知链路（`<background-task-completed>`）
- 不改 `!` 命令的沙箱/权限语义（保持现有 `ShellDialect::PlatformDefault` 路由）

---

## 四、参考实现

### 4.1 Codex CLI（OpenAI）

`!cmd` → `Op::RunUserShellCommand`（`codex-rs/protocol/src/protocol.rs:760`，注释 `The raw command string after '!'`）：

- `run_user_shell_command()`（`codex-rs/core/src/session/handlers.rs:96`）分两模式：
  - 有 turn 在跑 → `ActiveTurnAuxiliary`（**不新开 turn**）
  - 空闲 → `StandaloneTurn`
- 执行完 `persist_user_shell_output()` → 以 `UserShellCommand` 上下文片段写入历史，role=`user`：

```xml
<user_shell_command>
<command>git push origin fix/x</command>
<result>
Exit code: 0
Duration: 3.2041 seconds
Output:
...
</result>
</user_shell_command>
```

- 输出经 `format_exec_output_str(truncation_policy)` **按模型截断策略裁剪**后才入库

### 4.2 claude-code-best（本机 `/d/code/cc-best-src`）

`!cmd` → `processBashCommand()`（`src/utils/processUserInput/processBashCommand.tsx`）：

```tsx
const userMessage = createUserMessage({
  content: prepareUserContent({ inputString: `<bash-input>${inputString}</bash-input>` }),
});
// 复用 BashTool 执行（dangerouslyDisableSandbox: true）
const response = await BashTool.call({ command: inputString, dangerouslyDisableSandbox: true }, ...);
const mapped = await processToolResultBlock(shellTool, { ...data, stderr: '' }, randomUUID());
return {
  messages: [
    createSyntheticUserCaveatMessage(),
    userMessage,                                        // <bash-input>…</bash-input>
    ...attachmentMessages,
    createUserMessage({                                 // 执行结果
      content: `<bash-stdout>${stdout}</bash-stdout><bash-stderr>${escaped}</bash-stderr>`,
    }),
  ],
  shouldQuery: false,     // ← 关键：写入历史但不发起推理
};
```

要点：

| 要点 | claude-code-best 做法 |
|------|----------------------|
| 命令与结果 | **两条独立 user 消息**：`<bash-input>` + `<bash-stdout>/<bash-stderr>` |
| 执行引擎 | **复用 BashTool**，连同大输出落盘（`<persisted-output>`）一起复用 |
| 是否起轮次 | `shouldQuery: false` —— 不发起推理 |
| 历史可见性 | 消息已进 `messages`，模型下一轮可见 |

**结论**：两者都是「写入上下文 + 不打断」，即本方案。标签风格沿用 claude-code-best（`bash-*`），因其为本项目的对齐目标。

---

## 五、我们的落点设计

### 5.1 通道选择：复用 `recall` 机制

peri 已有「会话级通知缓冲 → 包成 `<system-reminder>` 追加到下一条用户消息」的通用通路：

| 环节 | 位置 |
|------|------|
| 缓冲区 | `peri-agent/src/agent/state.rs:73-76`（`recall_buffer`，`push_recall` / `drain_recall`） |
| 注入 | `peri-acp/src/session/executor.rs:257-269`（包 `<system-reminder>` 追加到本次用户消息 content blocks） |
| 回传 | `peri-acp/src/session/executor.rs:59` → `peri-tui/src/acp_server/prompt.rs:94`、`:221` |
| 现有调用点 | `peri-middlewares/src/tool_search/middleware.rs:73`（延迟工具数量变更，唯一使用者） |

#### ⚠️ 关键约束（已核实，决定实现方式）

1. **`AgentState` 每轮重建**：`peri-acp/src/session/executor.rs:610` `AgentState::with_messages(...)` 在每次 `execute_prompt` 内新建；`drain_recall()` 在同函数 `:652` 调用。
   → **实例本身不跨轮存活**，`recall` 的跨轮传递靠**值搬运**：`recall_items = agent_state.drain_recall()`（`:652`）→ `PromptResult.recall_items`（`:59`）→ `prompt.rs:221` `state.recall_items = result.recall_items` → 下一轮 `prompt.rs:94` `std::mem::take(&mut state.recall_items)` 作为 `incoming_recalls` 传入。

2. **当前唯一的写入方在 agent 执行内部**：`tool_search/middleware.rs:73` 持有 `&mut State`（即当轮的 `agent_state`）；外部（TUI / 服务器）**没有写入点** —— `requests.rs` 的 `SessionState` 构造处只写 `recall_items: Vec::new()`，无追加 API。

3. **因此新增 `session/append_context` 必须落在「注入窗口」内才不丢**：
   - ✅ **安全窗口**：`prompt.rs:94` take 之后、`:221` 回写之前 —— 即 `execute_prompt` 执行期间（含 agent 推理中，因为 `incoming_recalls` 已在函数开头消费）
   - ❌ **危险窗口**：`prompt.rs` 未在跑时写入 `state.recall_items`，会被**下一条用户消息 take 掉** → `!` 输出错位到那条消息上
   - ❌ **丢失窗口**：`:221` 回写之后、下次 prompt 之前的写入会被 `state.recall_items = result.recall_items` **整体覆盖**

> 注：后台命令当前走的是 `submit_message` 主动起轮次（`shell_command.rs:335`），因此天然处于安全窗口。前台 `!` 若走本方案，必须显式解决窗口问题。

#### 结论

`recall` 通道**可用于「agent 正在跑」时**的注入；**空闲时**的注入会被错位或覆盖。故需二选一：

- **改进型 C1**：TUI 侧维护独立待投递队列（如 `pending_shell_context`），在**下一次 `submit_message` 时**随请求携带，服务器在安全窗口内 `push_recall` —— 语义正确，但 `!` 后若不发消息，Agent 仍看不到（时序同 §7-1 选项①）
- **改进型 C2**：服务器侧新增 `SessionState.pending_context: Vec<String>`，`prompt.rs` 读取时与 `incoming_recalls` **合并**（且 `:221` 只做 `extend` 而非覆盖）—— 容量与顺序可控，但需新增字段与合并逻辑

**（本方案暂定 C2，因其不依赖 TUI 侧时序配合，且能修正 `:221` 覆盖语义的隐患。）**


### 5.2 新增 TUI → 服务端通道

**已核实：ACP server 与 TUI 同进程**（`peri-tui/src/main.rs:863` `run_acp_server(...)` 由 `main.rs:862` 的 `tokio::spawn` 启动，经 `mpsc_transport_pair()` 与 `AcpTuiClient` 相连），因此新增方法只需扩展现有 JSON-RPC 方法表，无跨进程成本。

`sessions`（`SharedSessions`）在服务器主循环 `run_acp_server` 中可用，但请求处理器 `handle_request` 当前**不含 sessions 访问**。二选一：

- **方案 a（推荐）**：在 `run_acp_server` 主循环内**内联拦截** `session/append_context`（与现有 `session/prompt` 拦截同层，`peri-tui/src/acp_server/mod.rs:100`），直接操作 `sessions`
- **方案 b**：给 `handle_request` 传 `&SharedSessions`，新增分支（改动面更大，需同步调整既有 req_id 处理）

### 5.3 服务端处理

收到 `session/append_context` 后写入 **`SessionState.pending_context`**（§5.1 结论，新增字段），由 `prompt.rs` 在安全窗口内合并进 `incoming_recalls`。

- 写入时机不受限（TUI 随时可发），因为读取方是「下次 prompt 时合并」而非「覆盖」
- 需为 `pending_context` 设容量上限（避免高频 `!` 累积），超出时保留最近 N 条

> 若改用「直接 `push_recall` 到常驻 State」的写法，**必须**先解决 §5.1 的窗口问题，当前不推荐。


### 5.4 载荷格式（claude-code-best 风格）

```xml
<bash-input>git push -u origin fix/x</bash-input>
```

```xml
<bash-stdout>To https://github.com/...  * [new branch] ...</bash-stdout><bash-stderr></bash-stderr>
```

- 命令与结果**分两条 recall 条目**（对齐参考实现的「两条消息」语义）
- 退出码：claude-code-best 未显式携带（依赖 stdout 内容）；**本方案建议补 `<bash-exit-code>N</bash-exit-code>`**，因为 `!` 失败时 stdout 常为空，仅靠 stderr 无法判断成败
- 内容需 XML 转义（参考 `peri-middlewares` 既有 `xml_escape`）

### 5.5 大输出处理

**直接复用现成能力**，不新增阈值：

`peri-middlewares/src/tools/output_persist.rs`

| 函数 | 用途 |
|------|------|
| `truncate_shell_output()`（`:77`） | shell 输出专用：超 `MAX_SHELL_OUTPUT_CHARS` 时 head/tail 截断 + **落盘 + 路径提示** |
| `truncate_tool_output()`（`:92`） | 通用：行数或字节任一超阈触发 head/tail + 落盘（issue #47 引入） |

即：`!` 命令的 stdout/stderr 在写入 recall 前先过 `truncate_shell_output()`，超限部分自动落盘并在片段中给出路径，Agent 可按需读取 —— 与 BashTool 行为一致。

### 5.6 时序与边界

| 场景 | 期望行为 |
|------|---------|
| Agent 空闲（`loading=false`） | 写入 `pending_context`，下一条用户消息携带 |
| Agent 推理中 | 同样写入（不打断）；因 `incoming_recalls` 已在 `execute_prompt` 开头消费，本次轮次看不到，下轮携带 |
| 命令中断（`ShellError.interrupted`） | 记录中断标记，不写正常 stdout（对齐参考实现） |
| 命令失败（exit≠0） | 正常写入，携带非零退出码 |
| 多 session | 按 `session_id` 精确投递，不串话 |
| ACP server 不可用 | 静默降级（只写 TUI 侧），不 panic |
| 写入后立即发消息 | 应与该条消息一同送达（`pending_context` 与 `incoming_recalls` 合并） |

---

## 六、涉及文件（预估）

| 文件 | 改动 |
|------|------|
| `peri-tui/src/app/shell_command.rs` | `handle_shell_command_completed()` 末尾构造片段并发送；新增片段构造函数 |
| `peri-tui/src/acp_client/client.rs` | 新增 `append_context()`（发 `session/append_context`） |
| `peri-tui/src/acp_server/mod.rs` | 主循环内联拦截 `session/append_context` → 写 `SessionState.pending_context`；`SessionState` 新增字段 |
| `peri-tui/src/acp_server/prompt.rs` | 读取 `pending_context` 并与 `incoming_recalls` 合并；修正 `:221` 覆盖语义 |
| `peri-tui/src/app/shell_command_test.rs` | 单测：片段格式、转义、退出码、截断 |
| `peri-middlewares/src/tools/output_persist.rs` | （复用，无改动）`truncate_shell_output()` 供片段构造调用 |
| `TUI-STYLE.md` / `CHANGELOG.md` | 行为变更记录（发版时） |

---

## 七、待决项（开工前需确认）

1. **时序语义**：`recall` 是「随下一条用户消息送达」，而 Codex 是「立即入 history」。
   - 选项 ①：接受差异（实现最简，复用现有通路）
   - 选项 ②：`!` 完成后主动触发一次「空轮次」提交，使 recall 立即被消费，但**仍不产生 Agent 回复**（需确认是否可行 —— recall 在 `execute_prompt` 内被 drain，若无 prompt 调用则不会消费）
   - 选项 ③：改走 `history` 注入（类似 `inject_bg_result_messages`，`peri-acp/src/session/executor.rs:693`），由 TUI 发 `bgResults` 同款结构化载荷 —— 更接近 Codex 语义，但需扩展现有 `prompt_with_bg_results` 契约

2. **是否携带退出码**：建议携带（见 §5.4），需确认不违背「对齐 claude-code-best」的约束

3. **`pending_context` 与 compact 的交互**：注入内容会占用上下文预算，需明确是否纳入 token 告警/compact 触发阈值（见 §5.3）

4. **`!` 命令高频场景**：连续多条 `!`（如脚本化调试）时的合并策略 —— 是逐条追加还是批量合并为一条，影响上下文体积与可读性

---

## 八、验收标准

- [ ] `!echo hello` 后，Agent 在**下一条回复**中能准确复述该命令及其输出
- [ ] `!git push`（成功/失败）后，Agent 能说出是否成功、推到哪个分支
- [ ] `!cmd` **不产生**额外的 Agent 轮次（对话流不被插入回复）
- [ ] 超过 `MAX_SHELL_OUTPUT_CHARS` 的输出被截断且附落盘路径
- [ ] TUI 侧 shell 历史与渲染**无回归**（现有 `shell_command_test.rs` 全绿）
- [ ] 多 session 并发下不串话
- [ ] ACP 不可用时静默降级，不 panic

---

## 九、关联

- `spec/issues/2026-06-05-shell-output-truncation-inconsistency.md` —— 输出截断口径
- `spec/archive-issues/2026-06-27-ctrl-b-background-shell.md` —— 后台通知链路设计（本方案不改动）
- claude-code-best：`src/utils/processUserInput/processBashCommand.tsx`（本机 `/d/code/cc-best-src`）
- Codex CLI：`codex-rs/core/src/tasks/user_shell.rs`、`codex-rs/core/src/context/user_shell_command.rs`
