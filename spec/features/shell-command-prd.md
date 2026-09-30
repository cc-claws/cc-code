# PRD: `!` 前缀系统命令执行

## 1. 背景

当前 peri TUI 中，所有用户输入要么是 `/slash` 命令，要么发送给 Agent 处理。用户无法直接执行系统命令（如 `git status`、`cargo build`）而不经过 LLM。

参考 Codex CLI 的实现，增加 `!` 前缀支持，让用户在 TUI 中直接执行系统命令，结果展示在聊天记录中。

## 2. 用户体验

### 2.1 触发方式

在输入框输入 `!` 开头的文本，按 Enter 直接执行：

```
> !git status
  On branch main
  Your branch is up to date with 'origin/main'.
  ...

> !cargo build -p peri-tui
   Compiling peri-tui v0.1.0
   Finished dev [unoptimized + debuginfo] target(s) in 12.34s
```

### 2.2 视觉反馈

- 输入 `!` 后，输入框 placeholder 变为 `Enter shell command...`
- 执行中显示 loading spinner
- 结果以特殊样式（灰色背景/边框）展示在聊天记录中
- 命令本身显示为 `> !git status`（带 `!` 前缀）

### 2.3 与 Agent 对话的区别

| 维度 | `!` 命令 | 普通输入 |
|------|----------|----------|
| 执行者 | 用户 shell（cmd/bash） | Agent (LLM) |
| 输出 | stdout/stderr 原始输出 | Agent 流式回复 |
| 历史 | 记录在聊天流；**摘要片段回流至 Agent history**（见 §3.1.1） | 进入 Agent history |
| 耗时 | 即时 | 取决于 LLM 响应 |

> **2026-09-30 修订**：原表述为「记录在聊天流，**不进入** Agent history」。
> 经产品决策调整 —— `!` 命令结果需让模型在后续轮次可见（对齐 Codex CLI 与 claude-code-best 行为），
> 故新增「上下文回流」机制，详见 §3.1.1。安全前提：回流内容**必须先经脱敏**（§4.3）。

## 3. 技术方案

### 3.1 架构决策：TUI 层拦截

**选择**: 在 TUI 层拦截 `!` 命令的执行本身，**不将命令发送给 Agent 去执行**。

**理由**:
1. 系统命令是用户 shell 操作，执行由 TUI 直接完成（无 Agent 构建开销）
2. 执行速度快
3. 参考 Codex 的 `AppCommand::RunUserShellCommand` 设计

**对比 `/slash` 命令**:
- `/clear` 等 UI 命令在 TUI 层拦截（操作 App 状态）
- `/compact` 等 Agent 命令在 ACP 层拦截（操作 Agent history）
- `!` 命令在 TUI 层**执行**；执行结果另经 §3.1.1 的回流通道进入 Agent history

> **2026-09-30 修订**：原决策为「`!` 命令不发送到 ACP Server」，理由含「避免污染 Agent 的消息历史」。
> 该理由已不再成立 —— 现明确要求结果回流以便模型感知（§3.1.1）。
> **执行的拦截位置不变**（仍在 TUI 层），变化的只是「结果是否进入 Agent 上下文」。

#### 3.1.1 上下文回流（新增）

`!` 命令执行完成后，TUI 构造以下片段经 `session/append_history` 写入会话 history：

```
<local-command-caveat>Caveat: …DO NOT respond to these messages…</local-command-caveat>
<bash-input>{命令}</bash-input>
<bash-stdout>{stdout + 退出码}</bash-stdout><bash-stderr>{stderr}</bash-stderr>
```

**设计要点**：

| 要点 | 说明 |
|------|------|
| 不触发推理轮次 | 仅写 history，由**下一次** `session/prompt` 携带给模型；不产生额外 Agent 回复 |
| role = user | 与 Codex / claude-code-best 一致，模型归因为「用户执行」而非「自己执行」 |
| caveat 前置 | 阻止模型把命令回放误当需要响应的用户请求 |
| 不重复展示 | 片段在展示层跳过（`message_pipeline::transform`）；UI 展示仍由 `ShellCommand` VM 负责 |
| 输出格式 | 复用 agent 自身执行 Bash 的 `format_command_output`，保持模型侧表示一致 |
| 超长输出 | 复用 `truncate_shell_output` 截断 + 落盘 |
| 并发 | 与 `session/prompt` 共用 `prompt_locks` 串行，避免 prompt 回写时覆盖 |

> **未纳入回流**：Ctrl+B 后台化的 `!` 命令走既有后台通知链路（会额外起一轮），行为与前台不同。

### 3.2 数据流

```
用户输入 "!git status"
  │
  ▼
normal_keys.rs: handle_enter()
  │ 检测 text.starts_with('!')
  │
  ├─ 剥离 ! 前缀 → "git status"
  │
  └─ Action::RunShellCommand("git status")
           │
           ▼
main.rs: action handler
  │
  ├─ app.push_user_message("!git status")  // 显示用户输入
  ├─ app.set_loading(true)
  │
  └─ tokio::spawn {
       │
       ├─ shell_command("git status", &[])  // 复用现有模块
       ├─ .output().await                    // 捕获 stdout+stderr
       │
       └─ app.push_command_result(output)   // 发回 TUI
     }
           │
           ▼
TUI 渲染: exec_result 组件
  ├─ 命令标题: "> !git status"
  ├─ 输出内容: stdout/stderr
  └─ 状态码: exit code (非0 红色高亮)
```

**2026-09-30 新增（§3.1.1 上下文回流）**：`push_command_result` 之后追加一条支路 ——

```
       └─ app.push_command_result(output)   // 发回 TUI（展示）
                │
                ▼
        inject_shell_context(record)
          ├─ shell_context_messages()        // caveat + bash-input + bash-stdout/stderr
          │    └─ redact_secrets()           // ★ 脱敏（离开本机前）
          │    └─ truncate_shell_output()    // 超长截断 + 落盘
          └─ AcpTuiClient::append_history(session_id, messages)
                   │  （session/append_history，不触发推理）
                   ▼
        ACP Server: state.history.extend(...) + thread_store.append_messages(...)
                   │
                   ▼
        下一次 session/prompt 时随 history 送入模型
```

> 注意：该支路**不改变**上方主链路的展示行为 —— UI 仍由 `ShellCommand` VM 渲染。

### 3.3 核心组件

#### 3.3.1 Action 枚举扩展

```rust
// peri-tui/src/event/mod.rs
pub enum Action {
    // ... 现有变体
    RunShellCommand(String),  // 新增
}
```

#### 3.3.2 输入拦截（normal_keys.rs）

```rust
// peri-tui/src/event/keyboard/normal_keys.rs
fn handle_enter(app: &mut App, text: String) -> Option<Action> {
    // ... 现有 loading 缓冲逻辑

    if text.starts_with('/') {
        // ... 现有 slash 命令逻辑
    }

    if text.starts_with('!') {
        let command = text[1..].trim().to_string();
        if !command.is_empty() {
            return Some(Action::RunShellCommand(command));
        }
    }

    // ... 现有普通提交逻辑
}
```

#### 3.3.3 命令执行器

```rust
// peri-tui/src/shell_exec.rs (新文件)
use std::process::Output;
use tokio::process::Command;
use crate::process::shell_command;  // 复用 peri-middlewares

pub async fn execute_shell_command(command: &str) -> Result<CommandOutput> {
    let output = shell_command(command, &[])
        .output()
        .await?;

    Ok(CommandOutput {
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        exit_code: output.status.code().unwrap_or(-1),
    })
}

pub struct CommandOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}
```

#### 3.3.4 UI 渲染组件

```rust
// peri-tui/src/widgets/exec_result.rs (新文件)
use ratatui::widgets::{Block, Borders, Paragraph};

pub fn render_command_result(f: &mut Frame, area: Rect, cmd: &str, output: &CommandOutput) {
    let title = format!("> !{}", cmd);
    let content = if output.exit_code == 0 {
        &output.stdout
    } else {
        &format!("{}\n[Exit code: {}]", output.stderr, output.exit_code)
    };

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .style(Style::default().fg(Color::Gray));

    let paragraph = Paragraph::new(content.as_ref())
        .block(block)
        .wrap(Wrap { trim: false });

    f.render_widget(paragraph, area);
}
```

### 3.4 会话持久化

命令执行结果需要持久化到会话历史，以便恢复会话时能看到：

```rust
// 存储格式（在 SessionState 或消息列表中）
pub struct ShellCommandRecord {
    pub command: String,
    pub output: CommandOutput,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}
```

> **2026-09-30 修订**：现有实现为**双持久化**，职责不同：
> 1. `ShellCommandRecord` → `shell-commands.jsonl`（`ShellCommandStore`）：供 **UI 恢复展示**（§3.4 原文所述目的）
> 2. `BaseMessage` 片段 → ThreadStore（SQLite）`messages` 表：供 **模型上下文恢复**（§3.1.1 回流）
>
> 二者数据源独立，故 `merge_shell_records_into_view` 的锚点计数必须与展示层跳过口径一致，
> 否则恢复会话时 shell 卡片位置漂移。

### 3.4.1 安全：回流内容必须脱敏（新增）

`!` 命令输出进入会话 history 后，会随每一次 `session/prompt` 发送至 **LLM 提供方 API（离开本机）**。
这改变了原有的数据边界 —— 此前 `!` 输出只写入本地文件。

**要求**：

- 所有回流字段（命令文本、stdout、stderr）在**入库与入 history 之前**经
  `peri_middlewares::hitl::jev::redact::redact_secrets` 处理
- 脱敏须**先于**截断，避免凭据被截断规则切成半截而漏过模式匹配
- 与审批弹窗（HITL）保持同一安全口径 —— 该函数的设计目标即「在状态离开本机之前替换明显凭据」

**已知局限**（不阻塞，但需知悉）：

- 脱敏为**模式匹配**（正则），无法覆盖所有凭据形态（如自研格式、base64 编码后的密钥）
- 命令输出以 role=user 进入上下文，理论上存在**提示注入**面（如 `!curl <恶意地址>` 的回显）；
  `<local-command-caveat>` 只提供软约束，非隔离机制

### 3.5 工作目录

命令在 App 的当前工作目录（`app.cwd`）下执行，与 Agent 工具执行保持一致。

## 4. 安全考虑

### 4.1 权限控制

- `!` 命令**不经过 HITL 审批**（与 Codex 一致）
- 理由：用户主动输入的命令，等同于在终端手动执行
- 如果需要审批，用户应使用 Agent 的 Bash 工具

### 4.2 危险命令

- 不做命令黑名单（太难维护且容易绕过）
- 依赖用户的常识和操作系统的权限控制
- 在文档中提示用户注意安全

### 4.3 数据外发（新增，2026-09-30）

`!` 命令的输出**不再只留在本机** —— 经 §3.1.1 回流后会随会话 history 发送至 LLM 提供方 API。

| 项 | 说明 |
|----|------|
| 外发内容 | 命令文本 + stdout + stderr（脱敏后） |
| 外发时机 | 下一次 `session/prompt` 时 |
| 强制措施 | 所有回流字段经 `redact_secrets`（§3.4.1） |
| 用户预期管理 | 用户应知悉：`!` 输出可能被模型看到，等同「把该输出贴进对话」 |

**与 §4.1 的关系**：`!` 命令**执行**仍不经 HITL 审批；但**结果外发**这一新增行为需在文档/CHANGELOG 中明示，
避免用户误以为 `!` 输出始终不出本机。

## 5. 实现步骤

### Phase 1: 核心功能

1. **扩展 Action 枚举** — 添加 `RunShellCommand(String)`
2. **修改输入拦截** — `normal_keys.rs` 中检测 `!` 前缀
3. **实现命令执行器** — `shell_exec.rs`，复用 `shell_command()`
4. **添加 Action handler** — `main.rs` 中处理 `RunShellCommand`
5. **实现结果渲染** — `exec_result.rs` 组件

### Phase 2: 体验优化

6. **Loading 状态** — 执行中显示 spinner
7. **输出截断** — 超长输出截断 + "Show more" 按钮
8. **历史记录** — 持久化到会话
9. **输入提示** — placeholder 变化

### Phase 3: 高级特性

10. **交互式命令** — 支持 stdin 输入（如 `grep` 的交互模式）
11. **ANSI 颜色** — 保留命令输出的 ANSI 颜色码
12. **多行输出分页** — 长输出支持翻页

## 6. 测试用例

```rust
#[tokio::test]
async fn test_shell_command_basic() {
    let output = execute_shell_command("echo hello").await.unwrap();
    assert_eq!(output.stdout.trim(), "hello");
    assert_eq!(output.exit_code, 0);
}

#[tokio::test]
async fn test_shell_command_error() {
    let output = execute_shell_command("ls /nonexistent").await.unwrap();
    assert_ne!(output.exit_code, 0);
    assert!(!output.stderr.is_empty());
}

#[tokio::test]
async fn test_shell_command_strip_prefix() {
    // 验证 ! 前缀被正确剥离
    let text = "!git status";
    assert!(text.starts_with('!'));
    let command = &text[1..];
    assert_eq!(command, "git status");
}
```

## 7. 参考实现

- **Codex CLI**: `codex-rs/tui/src/app_command.rs` — `AppCommand::RunUserShellCommand`
- **Codex CLI**: `codex-rs/tui/src/bottom_pane/chat_composer.rs` — `is_bash_mode` 检测
- **peri 现有模块**: `peri-middlewares/src/process/mod.rs` — `shell_command()` 跨平台封装

## 8. 文件清单

| 操作 | 文件 | 说明 |
|------|------|------|
| 新增 | `peri-tui/src/shell_exec.rs` | 命令执行器 |
| 新增 | `peri-tui/src/widgets/exec_result.rs` | 结果渲染组件 |
| 修改 | `peri-tui/src/event/mod.rs` | 添加 `RunShellCommand` Action |
| 修改 | `peri-tui/src/event/keyboard/normal_keys.rs` | 输入拦截逻辑 |
| 修改 | `peri-tui/src/main.rs` 或 `app/mod.rs` | Action handler |
| 修改 | `peri-tui/src/widgets/mod.rs` | 导出新组件 |

**2026-09-30 新增（§3.1.1 上下文回流）**：

| 操作 | 文件 | 说明 |
|------|------|------|
| 修改 | `peri-tui/src/app/shell_command.rs` | `shell_context_messages()`、`inject_shell_context()`、`is_shell_context_fragment()`、`SHELL_CAVEAT` |
| 修改 | `peri-tui/src/app/message_pipeline/transform.rs` | 展示层跳过回流片段 |
| 修改 | `peri-tui/src/acp_client/client.rs` | `append_history()` |
| 修改 | `peri-tui/src/acp_server/mod.rs` | `session/append_history` 拦截 + `append_history_to_session()` |
| 修改 | `peri-middlewares/src/middleware/terminal.rs` / `mod.rs` | `format_command_output` 提 `pub` 复用 |
| 修改 | `peri-tui/src/app/background_shell.rs` | `xml_escape` 提 `pub(crate)` |
| 复用 | `peri-middlewares/src/hitl/jev/redact.rs` | `redact_secrets()` —— 脱敏（§3.4.1） |
| 复用 | `peri-middlewares/src/tools/output_persist.rs` | `truncate_shell_output()` —— 超长截断 |
