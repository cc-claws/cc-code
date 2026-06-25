# Cross-Platform Shell Spawn Wrapper Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a unified `process` module that wraps shell command spawning for Windows (`cmd /C`) and Unix (`bash -c`), then refactor 3 existing call sites to use it.

**Architecture:** New `peri-middlewares/src/process.rs` exposes two functions: `shell_command()` (returns `Command` for custom config) and `spawn_shell()` (pre-configured shortcut). Three call sites (MCP, Bash tool, Hook executor) are refactored to use these functions, replacing their inline platform-specific code.

**Tech Stack:** `tokio::process::Command`, `cfg!(target_os = "windows")`, `std::process::Stdio`

---

## File Structure

| Action | File | Responsibility |
|--------|------|---------------|
| Create | `peri-middlewares/src/process.rs` | `shell_command()` + `spawn_shell()` + `spawn_shell_with_env()` |
| Create | `peri-middlewares/src/process_test.rs` | Unit tests for the process module |
| Modify | `peri-middlewares/src/lib.rs` | Add `pub mod process;` |
| Modify | `peri-middlewares/src/mcp/client.rs:345-382` | Replace `Command::new(command)` with `shell_command()` |
| Modify | `peri-middlewares/src/middleware/terminal.rs:160-178` | Replace inline `cfg!` with `shell_command()` |
| Modify | `peri-middlewares/src/hooks/executor.rs:29-76` | Replace `Command::new(&shell)` with `shell_command()` |

---

### Task 1: Create `process` module with tests

**Files:**
- Create: `peri-middlewares/src/process.rs`
- Create: `peri-middlewares/src/process_test.rs`
- Modify: `peri-middlewares/src/lib.rs`

- [ ] **Step 1: Write failing tests for `shell_command`**

Create `peri-middlewares/src/process_test.rs`:

```rust
use crate::process::shell_command;

#[test]
fn test_shell_command_unix_bash_c() {
    // Unix: shell_command("echo", &["hello"]) should produce:
    // Command::new("bash").arg("-c").arg("echo hello")
    let cmd = shell_command("echo", &["hello"]);
    // We can't inspect Command args directly, so we test via format
    let formatted = format!("{cmd:?}");
    #[cfg(unix)]
    {
        assert!(formatted.contains("bash"), "expected bash, got: {formatted}");
        assert!(formatted.contains("-c"), "expected -c flag, got: {formatted}");
    }
    #[cfg(windows)]
    {
        assert!(formatted.contains("cmd"), "expected cmd, got: {formatted}");
        assert!(formatted.contains("/C"), "expected /C flag, got: {formatted}");
    }
}

#[test]
fn test_shell_command_no_args() {
    // shell_command("ls", &[]) — command without args
    let cmd = shell_command("ls", &[]);
    let formatted = format!("{cmd:?}");
    #[cfg(unix)]
    {
        assert!(formatted.contains("bash"), "expected bash, got: {formatted}");
        assert!(formatted.contains("ls"), "expected 'ls' in command, got: {formatted}");
    }
    #[cfg(windows)]
    {
        assert!(formatted.contains("cmd"), "expected cmd, got: {formatted}");
        assert!(formatted.contains("ls"), "expected 'ls' in command, got: {formatted}");
    }
}

#[test]
fn test_shell_command_multi_args() {
    // shell_command("npx", &["-y", "@anthropic/mcp-server"]) — typical MCP pattern
    let cmd = shell_command("npx", &["-y", "@anthropic/mcp-server"]);
    let formatted = format!("{cmd:?}");
    #[cfg(unix)]
    {
        assert!(formatted.contains("bash"), "expected bash, got: {formatted}");
        // On Unix, args are joined into the -c string: "npx -y @anthropic/mcp-server"
        assert!(formatted.contains("npx"), "expected 'npx', got: {formatted}");
    }
    #[cfg(windows)]
    {
        assert!(formatted.contains("cmd"), "expected cmd, got: {formatted}");
        assert!(formatted.contains("npx"), "expected 'npx', got: {formatted}");
    }
}
```

- [ ] **Step 2: Register module in lib.rs and run tests to verify they fail**

Add to `peri-middlewares/src/lib.rs` (after `pub mod plugin;` line, ~line 36):

```rust
pub mod process;
```

Run: `cargo test -p peri-middlewares --lib -- process 2>&1 | head -30`
Expected: Compilation error — `process` module not found

- [ ] **Step 3: Implement `process.rs`**

Create `peri-middlewares/src/process.rs`:

```rust
pub fn shell_command(command: &str, args: &[&str]) -> tokio::process::Command {
    if cfg!(target_os = "windows") {
        let mut cmd = tokio::process::Command::new("cmd");
        cmd.arg("/C").arg(command);
        for arg in args {
            cmd.arg(arg);
        }
        cmd
    } else {
        let mut parts = vec![command.to_string()];
        for arg in args {
            if arg.contains(' ') || arg.contains('"') || arg.contains('\'') || arg.contains('\\') {
                parts.push(format!("'{}'", arg.replace('\'', "'\\''")));
            } else {
                parts.push(arg.to_string());
            }
        }
        let shell_cmd = parts.join(" ");
        let mut cmd = tokio::process::Command::new("bash");
        cmd.arg("-c").arg(&shell_cmd);
        cmd
    }
}
```

This is the final implementation — use this code, not the `shell_words` version above.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p peri-middlewares --lib -- process 2>&1`
Expected: 3 tests PASS

- [ ] **Step 5: Add `spawn_shell` and `spawn_shell_with_env` functions**

Add to `peri-middlewares/src/process.rs` after `shell_command`:

```rust
/// Spawn a command through the platform shell with common defaults:
/// - piped stdin/stdout/stderr
/// - `kill_on_drop(true)`
/// - Unix: `process_group(0)` for clean process tree termination
pub fn spawn_shell(command: &str, args: &[&str]) -> io::Result<tokio::process::Child> {
    let mut cmd = shell_command(command, args);
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(unix)]
    cmd.process_group(0);
    cmd.spawn()
}

/// Same as `spawn_shell` but with additional environment variables.
pub fn spawn_shell_with_env(
    command: &str,
    args: &[&str],
    env: &HashMap<String, String>,
) -> io::Result<tokio::process::Child> {
    let mut cmd = shell_command(command, args);
    cmd.envs(env)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(unix)]
    cmd.process_group(0);
    cmd.spawn()
}
```

- [ ] **Step 6: Commit**

```bash
git add peri-middlewares/src/process.rs peri-middlewares/src/process_test.rs peri-middlewares/src/lib.rs
git commit -m "feat(middlewares): add cross-platform process spawn module

Co-Authored-By: glm-5.1 <zai-org@claude-code-best.win>"
```

---

### Task 2: Refactor MCP `spawn_stdio_transport`

**Files:**
- Modify: `peri-middlewares/src/mcp/client.rs:345-382`

- [ ] **Step 1: Refactor `spawn_stdio_transport` to use `shell_command`**

In `peri-middlewares/src/mcp/client.rs`, replace the function body at line 345-382:

**Before:**
```rust
pub(crate) fn spawn_stdio_transport(
    command: &str,
    args: &[String],
    env: &HashMap<String, String>,
) -> std::io::Result<rmcp::transport::child_process::TokioChildProcess> {
    use std::process::Stdio;

    // 使用 builder 模式以获取 stderr 句柄
    let mut cmd = tokio::process::Command::new(command);
    cmd.args(args).envs(env);
    // ...
```

**After:**
```rust
pub(crate) fn spawn_stdio_transport(
    command: &str,
    args: &[String],
    env: &HashMap<String, String>,
) -> std::io::Result<rmcp::transport::child_process::TokioChildProcess> {
    use std::process::Stdio;

    let arg_strs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    let mut cmd = crate::process::shell_command(command, &arg_strs);
    cmd.envs(env);
    // ... rest unchanged (TokioChildProcess builder + stderr logging)
```

Only the first 2 lines inside the function change. Lines 356-382 (TokioChildProcess builder + stderr logging) remain untouched.

- [ ] **Step 2: Build to verify compilation**

Run: `cargo build -p peri-middlewares 2>&1 | tail -5`
Expected: BUILD SUCCEEDED

- [ ] **Step 3: Commit**

```bash
git add peri-middlewares/src/mcp/client.rs
git commit -m "refactor(mcp): use cross-platform shell_command for spawn_stdio_transport

Co-Authored-By: glm-5.1 <zai-org@claude-code-best.win>"
```

---

### Task 3: Refactor Bash tool (TerminalMiddleware)

**Files:**
- Modify: `peri-middlewares/src/middleware/terminal.rs:160-178`

- [ ] **Step 1: Replace inline `cfg!` with `shell_command`**

In `peri-middlewares/src/middleware/terminal.rs`, replace lines 160-178:

**Before:**
```rust
        let (shell, flag) = if cfg!(target_os = "windows") {
            ("cmd", "/C")
        } else {
            ("bash", "-c")
        };

        let result = timeout(Duration::from_millis(timeout_ms), {
            let mut cmd = Command::new(shell);
            cmd.arg(flag)
                .arg(command)
                .current_dir(&self.cwd)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .kill_on_drop(true);
            #[cfg(unix)]
            cmd.process_group(0);
            cmd.output()
        })
        .await;
```

**After:**
```rust
        let result = timeout(Duration::from_millis(timeout_ms), {
            let mut cmd = crate::process::shell_command(&command, &[]);
            cmd.current_dir(&self.cwd)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .kill_on_drop(true);
            #[cfg(unix)]
            cmd.process_group(0);
            cmd.output()
        })
        .await;
```

- [ ] **Step 2: Build to verify compilation**

Run: `cargo build -p peri-middlewares 2>&1 | tail -5`
Expected: BUILD SUCCEEDED

- [ ] **Step 3: Commit**

```bash
git add peri-middlewares/src/middleware/terminal.rs
git commit -m "refactor(terminal): use cross-platform shell_command in Bash tool

Co-Authored-By: glm-5.1 <zai-org@claude-code-best.win>"
```

---

### Task 4: Refactor Hook executor

**Files:**
- Modify: `peri-middlewares/src/hooks/executor.rs:29-76`

- [ ] **Step 1: Replace `Command::new(&shell)` with `shell_command`**

In `peri-middlewares/src/hooks/executor.rs`, change lines 29-76:

**Before (lines 29-76):**
```rust
    let (command, shell, timeout_secs) = match hook {
        HookType::Command {
            command,
            shell,
            timeout,
            ..
        } => (
            command.clone(),
            shell.clone().unwrap_or_else(|| "bash".to_string()),
            timeout.unwrap_or(600),
        ),
        // ...
    };
    // ...
    let result = tokio::time::timeout(Duration::from_secs(timeout_secs), async {
        let mut cmd = tokio::process::Command::new(&shell);
        cmd.arg("-c")
            .arg(&command)
            .stdin(Stdio::piped())
            // ...
```

**After:**
```rust
    let (command, _shell, timeout_secs) = match hook {
        HookType::Command {
            command,
            shell,
            timeout,
            ..
        } => (
            command.clone(),
            shell.clone(),  // preserved for destructure, no longer used for spawn
            timeout.unwrap_or(600),
        ),
        // ...
    };
    // ...
    let result = tokio::time::timeout(Duration::from_secs(timeout_secs), async {
        let mut cmd = crate::process::shell_command(&command, &[]);
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .env("CLAUDE_PROJECT_DIR", &input.cwd)
            // ... rest unchanged
```

Key change: Remove `shell.clone().unwrap_or_else(|| "bash".to_string())` usage. The `shell` field from `HookType::Command` is no longer used for spawning — `shell_command()` handles platform detection internally. Keep the destructure to avoid changing the `HookType` enum, but bind it to `_shell` to suppress unused warning.

- [ ] **Step 2: Build to verify compilation**

Run: `cargo build -p peri-middlewares 2>&1 | tail -5`
Expected: BUILD SUCCEEDED

- [ ] **Step 3: Commit**

```bash
git add peri-middlewares/src/hooks/executor.rs
git commit -m "refactor(hooks): use cross-platform shell_command in hook executor

Co-Authored-By: glm-5.1 <zai-org@claude-code-best.win>"
```

---

### Task 5: Full build and test

**Files:** None (verification only)

- [ ] **Step 1: Run full build**

Run: `cargo build 2>&1 | tail -10`
Expected: BUILD SUCCEEDED

- [ ] **Step 2: Run peri-middlewares tests**

Run: `cargo test -p peri-middlewares 2>&1 | tail -20`
Expected: ALL TESTS PASS

- [ ] **Step 3: Run clippy**

Run: `cargo clippy -p peri-middlewares 2>&1 | tail -10`
Expected: NO WARNINGS (or only pre-existing ones)
