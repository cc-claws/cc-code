# Issue: Windows 环境下 Hook 执行器缺失 Git Bash 路由与回退导致高频报错

**状态**：Shell 路由与 Windows 进程树回收已修复，47 项相关回归通过；完整退出日志待独立验证
**优先级**：P1
**创建日期**：2026-09-24
**模块**：中间件执行器 (`peri-middlewares` / `hooks::executor` / `process`)
**GitHub Issue**：[#260](https://github.com/cc-claws/cc-code/issues/260)

---

## 1. 问题现象与日志

在 Windows 环境下运行 Agent（例如 CLI print 模式或交互模式），当项目中配置了生命周期 Hook（如 `.claude/settings.local.json`）时，每次工具调用（`PreToolUse`、`PostToolUse`）都会高频触发如下警告刷屏：

```text
WARN agent.execute{max_iterations=500}: peri_middlewares::hooks::executor: Command hook exited with code 1: 'bash' 不是内部或外部命令，也不是可运行的程序
或批处理文件。
```

伴随现象：
1. 每调用一次工具（读写文件、运行命令、搜索等），该告警就会成对输出 2 次；
2. 会话结束或 Agent 退出时，产生异步任务取消告警：`Command hook execution failed: task was cancelled`。

---

## 2. 根因分析与代码审计

经代码与真实子进程验证，问题涉及**默认值注释过期**、**前缀识别缺失**与**进程 I/O/生命周期不完整**：

### 2.1 默认值注释与既有 Windows 行为不一致
在 `peri-middlewares/src/hooks/executor.rs:23-42` 中：
```rust
/// Execute a command hook (shell script).
///
/// - shell default "bash", timeout default 600s
...
pub async fn execute_command_hook(...) -> HookAction {
    let (command, shell, timeout_secs) = match hook {
        HookType::Command {
            command,
            shell,
            timeout,
            ..
        } => (command.clone(), shell.clone(), timeout.unwrap_or(600)),
```
- **旧注释**：executor 仍声称默认 bash。
- **历史决策**：2026-05-27 跨平台修复已将 Windows Hook 统一交给 CMD。不能仅凭旧注释强制恢复 bash，否则会改变现有 CMD Hook 的语法。最终修复保留平台默认值，并补齐明确 POSIX 命令的执行前路由。

### 2.2 底层封装未识别 `bash`/`sh` 前缀
在 `peri-middlewares/src/process/mod.rs:98` 中：
```rust
_ => {
    if cfg!(target_os = "windows") {
        if command.contains('\n') {
            if let Some(bash_exe) = git_bash_path() {
                return git_bash_command(&bash_exe, command, args);
            }
        }
        shell_command_cmd(command, args)
    }
}
```
底层封装仅考虑了多行命令（`contains('\n')`）需自动交由 Git Bash，**遗漏了单行以 `bash ` 或 `sh ` 开头的命令**。当命令形如 `bash .claude/hooks/guard.sh` 且 `shell: None` 时，仍然原样扔给 Windows 原生 `cmd /C`。而 Windows 下默认安装的 Git 通常只将 `C:\Program Files\Git\cmd` 加入 PATH（只有 `git.exe`，没有 `bash.exe`），导致 CMD 报错“'bash' 不是内部或外部命令”。

### 2.3 I/O 与进程回收缺口，不能直接复用终端 Fallback
在 `peri-middlewares/src/process/mod.rs` 中已封装有：
- `git_bash_path()`（探测 Git Bash 路径）
- `should_fallback_to_bash()`（检测“不是内部或外部命令”）

但目前的 `process` 仅停留在“构造 `tokio::process::Command` 对象”，未封装完整的执行生命周期：
- 终端（`terminal.rs:518`）单独手写了捕获 CMD 报错并通过 `should_fallback_to_bash` 拉起 Git Bash 重试的逻辑；
- Hook 执行器顺序写 stdin、再读输出，大输入/输出可能互相阻塞；未设置子进程 cwd；Windows `kill_on_drop` 只杀直接启动器，可能留下 Git Bash 的实际脚本进程。
- Hook 不能接入终端现有的“短 stderr + 非零退出码”重试：exit 2 本身就是拦截决策，失败前也可能已经发生副作用。应复用一次性 I/O/回收机制，不复用业务重试策略。

---

## 3. 重构方案（手术刀式抽象与修复）

> 以下 A/B/C 为初始讨论，最终实现以第 6 节为准。尤其不采用默认强制 bash 或执行失败后重跑 Hook。

### 方案 A：底层构造智能路由（前置阻断）
在 `peri-middlewares/src/process/mod.rs` 的 `shell_command_with_shell` 中，当 `shell == None` 时增加对 `bash `/`sh ` 命令前缀的识别：

```rust
_ => {
    if cfg!(target_os = "windows") {
        let trimmed = command.trim_start();
        let needs_bash = command.contains('\n')
            || trimmed.starts_with("bash ")
            || trimmed.starts_with("sh ")
            || trimmed == "bash"
            || trimmed == "sh";

        if needs_bash {
            if let Some(bash_exe) = git_bash_path() {
                return git_bash_command(&bash_exe, command, args);
            }
        }
        shell_command_cmd(command, args)
    } else { ... }
}
```

### 方案 B：对齐 Hook 契约默认值
在 `peri-middlewares/src/hooks/executor.rs` 中，对齐函数约定的默认值：
```rust
let shell_name = shell.as_deref().or(Some("bash"));
let mut cmd = crate::process::shell_command_with_shell(&command, &[], shell_name);
```

### 方案 C：下沉通用批处理命令执行器 `run_shell_command`
在 `peri-middlewares/src/process/mod.rs` 中提供统一的非流式进程执行接口，闭环管理 `spawn`、`stdin` 写入、`wait_with_output` 超时控制和 Windows CMD 失败时的自动 `git_bash_path` 重试，供 `hooks/executor.rs` 等模块统一复用，杜绝各处手写进程操作。

---

## 4. 涉及核心文件

- `peri-middlewares/src/process/mod.rs`
- `peri-middlewares/src/process/process_test.rs`
- `peri-middlewares/src/hooks/executor.rs`
- `peri-middlewares/src/hooks/executor_test.rs`（如有）

---

## 5. 影响面评估与验证建议

1. **边界安全**：
   - 识别 `bash ` 前缀时，直接传原始 `command` 给 `git_bash_command`（在 bash 环境内执行 `bash xxx.sh` 完全合法），避免手动剥离前缀误伤类似 `bash -c "..."` 的参数；
   - 原生 Windows 命令（`dir`、`tasklist`、`python script.py` 等）不受任何影响，继续走 CMD 分支。
2. **测试验证项**：
   - 增加单元测试：Windows 下 `shell_command_with_shell("bash test.sh", &[], None)` 自动路由至 Git Bash；
   - 本地验证：在未将 `bash.exe` 置于 PATH 的 Windows 机器上，执行配置了 `bash xxx.sh` 的 Hook，确认不再产生 Exit code 1 警告。

---

## 6. 最终实现与兼容边界

### 执行前选择解释器，只执行一次

- `process::auto_git_bash_path` 统一决定 Windows 自动路由：已有多行命令逻辑保留，增加完整的 `bash`/`sh` 首命令识别（含 `.exe`、前导空白、Tab 和带引号名称）。不修改原始命令、参数或路径。
- 普通 Windows 命令继续使用 CMD，显式 `shell: cmd` 优先于自动路由。显式 bash/sh 缺失时返回启动错误，不改用 CMD；pwsh 使用 pwsh 可执行文件，其他显式 shell 按支持 `-c` 的可执行文件处理。
- 不把 executor 的旧注释当作默认值迁移依据：2026-05-27 的跨平台修复已经明确选择 Windows CMD 默认值，详见 `spec/archive-issues/2026-05-27-cross-platform-spawn-wrapper.md`。
- Hook 不接入 `should_fallback_to_bash`。终端仍保留既有 CMD fallback，但已经路由到 Git Bash 的命令禁止再次 fallback，防止重复副作用。

### 收敛进程 I/O，保留 Hook 业务语义

- `process::output_with_input` 负责一次启动、同时写 stdin 与读取 stdout/stderr。调用方持有总超时，不新增后台 I/O task。
- Windows 的 Git `bin/bash.exe` 是启动器，实际脚本由 `usr/bin/bash.exe` 子进程执行。只杀启动器会留下脚本和管道。执行器使用 Windows Job Object，在进程恢复前完成归组，超时/取消/drop 时显式终止该 Job，同时保留 `CREATE_NO_WINDOW`。
- 复用已在依赖树中的 `process-wrap`，但不依赖其 9.1 版本的 `KillOnDrop`/`CreationFlags` 交叉查询：该版本 `spawn_with` 暂时取走 wrapper 列表，导致这两项策略无法被 JobObject 查询到。使用局部 `HiddenJob` 设置启动标志，`JobChild` 显式持有并回收 Job，无需修改第三方源码或新增 unsafe。
- Hook 显式使用 `input.cwd` 作为子进程工作目录，保留 stdin JSON、插件环境变量、输出编码转换以及退出码 0/1/2 的既有处理。
- 脚本提前退出产生的 BrokenPipe 不覆盖原始退出码，尤其不能丢失 exit 2 拦截。

### 验证范围

- process 回归：子进程 PATH 不含 bash 时仍能执行；首命令边界、引号、Tab；显式 CMD 优先；缺失解释器不隐式执行其他 shell。
- Hook 回归：中文及空格工作目录、相对脚本路径、完整 stdin 与环境变量；exit 1/2 只执行一次；大输入输出不死锁；stdin 阻塞也受总超时控制；超时与取消后脚本不能继续写文件；CMD/PowerShell 兼容。
- 终端回归：已路由 Bash 的失败脚本只写入一次计数文件。
- 测试只写隔离临时目录，不执行项目现有生命周期 Hook，不修改全局配置。

本机 Windows 验证结果：

- `cargo test -p peri-middlewares --lib test_command_hook -- --nocapture`：11 通过。
- `cargo test -p peri-middlewares --lib -- process:: test_bash_routed_command_is_not_replayed`：36 通过。
- 本次修改的 Rust 文件 `rustfmt --check`、`git diff --check` 通过。
- `cargo clippy -p peri-middlewares --lib --tests -- -W clippy::all`：通过，无告警。
- `cargo fmt --all -- --check` 发现基线已有格式差异，位于 `peri-agent/src/agent/executor/tool_dispatch*.rs`、`peri-middlewares/src/tools/output_filter_test.rs`、`peri-tui/src/terminal_title_test.rs`；未顺手修改。
- 测试结束后查询本机进程，未发现残留测试进程或 Bash 进程。未运行完整 workspace 测试及真实 TUI 会话。

### 尚未归因的问题

`task was cancelled` 不能仅凭伴随日志认定与 PATH 问题同源。本次覆盖 Windows 捕获式命令的进程树回收，没有修改会话级异步 Hook 的 fire-and-forget 策略，也不改变终端流式执行器的生命周期。Unix 仍沿用直接子进程 `kill_on_drop`，未宣称跨平台任意后代进程回收。完整 TUI 退出日志仍需独立场景验证。
