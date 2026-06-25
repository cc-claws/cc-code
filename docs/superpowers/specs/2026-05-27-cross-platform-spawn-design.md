# Cross-Platform Shell Spawn Wrapper

**Date**: 2026-05-27
**Status**: Approved

## Problem

Windows 上 `tokio::process::Command::new("npx")` 失败——`npx` 是 `.cmd` 批处理脚本，不能直接执行，必须通过 `cmd /C` 包裹。项目中 3 处 spawn 调用点各自处理跨平台，无统一封装。

## Current State

| File | Line | Pattern | Windows |
|------|------|---------|---------|
| `mcp/client.rs` | 345-382 | `Command::new(command)` 直接执行 | Broken |
| `middleware/terminal.rs` | 160-174 | `cfg!` 切换 `cmd /C` / `bash -c` | OK |
| `hooks/executor.rs` | 66-84 | `Command::new(shell)` + `-c`，shell 默认 bash | Broken |

## Design

### New Module: `peri-middlewares/src/process.rs`

Two-layer API:

**Layer 1 — Builder**: Returns `tokio::process::Command` for caller customization.

```rust
/// Wraps command in platform shell.
/// Windows → cmd /C <command> <args...>
/// Unix    → bash -c <command> <args...>
pub fn shell_command(command: &str, args: &[&str]) -> tokio::process::Command
```

Implementation note: On Unix, `bash -c` receives the command as a single string argument (the command and args are joined). On Windows, `cmd /C` receives the command and args as separate arguments.

**Layer 2 — Spawn shortcut**: Pre-configures common settings.

```rust
/// spawn_shell: piped stdio + kill_on_drop + process_group(Unix)
pub fn spawn_shell(command: &str, args: &[&str]) -> io::Result<tokio::process::Child>

/// spawn_shell_with_env: same + env injection (for MCP)
pub fn spawn_shell_with_env(
    command: &str,
    args: &[&str],
    env: &HashMap<String, String>,
) -> io::Result<tokio::process::Child>
```

### Call Site Changes

**1. MCP `spawn_stdio_transport` (`mcp/client.rs:345`)**

Replace `Command::new(command)` with `shell_command(command, &args_strs)`. Keep existing TokioChildProcess builder + stderr logging logic unchanged.

```rust
// Before
let mut cmd = tokio::process::Command::new(command);
cmd.args(args).envs(env);

// After
let mut cmd = crate::process::shell_command(command, &args.iter().map(|s| s.as_str()).collect::<Vec<_>>());
cmd.envs(env);
```

**2. Bash Tool (`middleware/terminal.rs:160`)**

Replace inline `cfg!` block with `shell_command()`.

```rust
// Before
let (shell, flag) = if cfg!(target_os = "windows") {
    ("cmd", "/C")
} else {
    ("bash", "-c")
};
let mut cmd = Command::new(shell);
cmd.arg(flag).arg(command)...

// After
let mut cmd = crate::process::shell_command(&command, &[]);
cmd.current_dir(&self.cwd)...
```

**3. Hook Executor (`hooks/executor.rs:66`)**

Replace `Command::new(shell).arg("-c")` with `shell_command()`. Remove `shell` config field from `HookType::Command` handling — all command hooks now use unified shell wrapping.

```rust
// Before
let shell = shell.clone().unwrap_or_else(|| "bash".to_string());
let mut cmd = tokio::process::Command::new(&shell);
cmd.arg("-c").arg(&command)...

// After
let mut cmd = crate::process::shell_command(&command, &[]);
```

### Module Registration

Add `pub mod process;` to `peri-middlewares/src/lib.rs`.

### Out of Scope

- No MCP config schema changes — shell wrapping is internal, `"command": "npx"` works as-is
- No changes to `peri-lsp` or `peri-tui` `Command::new` usages (different scenarios: LSP transport, browser open, editor launch)
- No new workspace crate
- No shell auto-detection or `.cmd` sniffing — always wrap in shell

## Testing

- Unit test `shell_command()` on both platforms: verify `cmd /C` on Windows, `bash -c` on Unix
- Unit test arg passing: multi-arg commands produce correct shell invocation string
- Integration: existing MCP/Bash/Hook tests should pass unchanged
