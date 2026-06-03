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
| 历史 | 记录在聊天流，不进入 Agent history | 进入 Agent history |
| 耗时 | 即时 | 取决于 LLM 响应 |

## 3. 技术方案

### 3.1 架构决策：TUI 层拦截

**选择**: 在 TUI 层拦截 `!` 命令，不发送到 ACP Server。

**理由**:
1. 系统命令是用户 shell 操作，与 Agent 无关
2. 避免污染 Agent 的消息历史
3. 执行速度更快（无 Agent 构建开销）
4. 参考 Codex 的 `AppCommand::RunUserShellCommand` 设计

**对比 `/slash` 命令**:
- `/clear` 等 UI 命令在 TUI 层拦截（操作 App 状态）
- `/compact` 等 Agent 命令在 ACP 层拦截（操作 Agent history）
- `!` 命令在 TUI 层拦截（执行系统命令，结果展示在 UI）

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
