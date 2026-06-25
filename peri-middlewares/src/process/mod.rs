//! Cross-platform shell command spawning.
//!
//! On Unix, wraps commands in `bash -c "<command> <args...>"`.
//! On Windows, wraps commands in `cmd /C <command> <args...>`.

/// Build a `tokio::process::Command` that executes the given command through the
/// platform shell.
///
/// - **Unix**: `bash -c "<command> <args...>"`
/// - **Windows**: `cmd /C <command> <args...>`
///
/// Returns the `Command` object so callers can add custom configuration
/// (env, current_dir, stdin/stdout/stderr, kill_on_drop, etc.).
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

#[cfg(test)]
mod process_test;
