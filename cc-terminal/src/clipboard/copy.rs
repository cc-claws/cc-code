//! 文本复制多层 fallback 实现
//!
//! 决策逻辑（参照 openai/codex `clipboard_copy.rs`）：
//!
//! 1. **SSH 会话**（`SSH_TTY` / `SSH_CONNECTION`）：native clipboard 写到远端
//!    机器用户拿不到，必须走终端转义。tmux 内优先 tmux load-buffer，否则 OSC 52。
//! 2. **本地会话**：先 arboard；WSL 下回落 powershell.exe；都失败再 OSC 52。
//!
//! OSC 52 是终端转义序列 `\x1b]52;c;{base64}\x07`，kitty/WezTerm/iTerm2/Ghostty
//! 等终端支持。tmux 内需要包裹 `\x1bPtmux;\x1b\x1b]52;c;...\x07\x1b\\`。

use base64::Engine as _;
use std::io::Write;

/// OSC 52 编码前的最大原始字节数。超过则拒绝，避免撑爆终端输入缓冲。
const OSC52_MAX_RAW_BYTES: usize = 100_000;

/// 把文本复制到系统剪贴板。
///
/// 返回 `Ok(lease)` 表示至少有一条路径成功；`lease` 在 Linux X11/Wayland 下
/// 持有 arboard handle，调用方必须保存到 TUI 生命周期级别（如 GlobalUiState），
/// 否则剪贴板内容会随 lease drop 而消失。其他平台 lease 为 None。
///
/// 返回 `Err(String)` 时字符串聚合了所有尝试的错误信息，便于上层向用户展示。
pub fn copy_to_clipboard(text: &str) -> Result<Option<crate::clipboard::ClipboardLease>, String> {
    copy_to_clipboard_with(
        text,
        CopyEnvironment {
            ssh_session: is_ssh_session(),
            wsl_session: is_wsl_session(),
            tmux_session: is_tmux_session(),
        },
        tmux_clipboard_copy,
        osc52_copy,
        arboard_copy,
        wsl_clipboard_copy,
    )
}

/// 环境探测结果，便于单元测试注入。
#[derive(Clone, Copy)]
struct CopyEnvironment {
    ssh_session: bool,
    wsl_session: bool,
    tmux_session: bool,
}

#[allow(clippy::too_many_arguments)]
fn copy_to_clipboard_with(
    text: &str,
    environment: CopyEnvironment,
    tmux_copy_fn: impl Fn(&str) -> Result<(), String>,
    osc52_copy_fn: impl Fn(&str) -> Result<(), String>,
    arboard_copy_fn: impl Fn(&str) -> Result<Option<crate::clipboard::ClipboardLease>, String>,
    wsl_copy_fn: impl Fn(&str) -> Result<(), String>,
) -> Result<Option<crate::clipboard::ClipboardLease>, String> {
    if environment.ssh_session {
        // SSH 下 native clipboard 写到远端机器，没用；走终端转义。lease 不适用。
        return terminal_clipboard_copy_with(
            text,
            environment.tmux_session,
            &tmux_copy_fn,
            &osc52_copy_fn,
        )
        .map(|()| None)
        .map_err(|terminal_err| {
            tracing::warn!("terminal clipboard copy failed over SSH: {terminal_err}");
            if environment.tmux_session {
                format!("terminal clipboard copy failed over SSH: {terminal_err}")
            } else {
                format!("OSC 52 clipboard copy failed over SSH: {terminal_err}")
            }
        });
    }

    match arboard_copy_fn(text) {
        Ok(lease) => Ok(lease),
        Err(native_err) => {
            if environment.wsl_session {
                tracing::warn!(
                    "native clipboard copy failed: {native_err}, falling back to WSL PowerShell"
                );
                match wsl_copy_fn(text) {
                    Ok(()) => return Ok(None),
                    Err(wsl_err) => {
                        tracing::warn!(
                            "WSL PowerShell clipboard copy failed: {wsl_err}, falling back to terminal clipboard"
                        );
                        return terminal_clipboard_copy_with(
                            text,
                            environment.tmux_session,
                            &tmux_copy_fn,
                            &osc52_copy_fn,
                        )
                        .map(|()| None)
                        .map_err(|terminal_err| {
                            if environment.tmux_session {
                                format!(
                                    "native clipboard: {native_err}; WSL fallback: {wsl_err}; terminal fallback: {terminal_err}"
                                )
                            } else {
                                format!(
                                    "native clipboard: {native_err}; WSL fallback: {wsl_err}; OSC 52 fallback: {terminal_err}"
                                )
                            }
                        });
                    }
                }
            }
            tracing::warn!(
                "native clipboard copy failed: {native_err}, falling back to terminal clipboard"
            );
            terminal_clipboard_copy_with(
                text,
                environment.tmux_session,
                &tmux_copy_fn,
                &osc52_copy_fn,
            )
            .map(|()| None)
            .map_err(|terminal_err| {
                if environment.tmux_session {
                    format!("native clipboard: {native_err}; terminal fallback: {terminal_err}")
                } else {
                    format!("native clipboard: {native_err}; OSC 52 fallback: {terminal_err}")
                }
            })
        }
    }
}

/// 走终端转义复制，tmux 内优先 tmux load-buffer，否则 OSC 52。
fn terminal_clipboard_copy_with(
    text: &str,
    tmux_session: bool,
    tmux_copy_fn: &impl Fn(&str) -> Result<(), String>,
    osc52_copy_fn: &impl Fn(&str) -> Result<(), String>,
) -> Result<(), String> {
    if tmux_session {
        match tmux_copy_fn(text) {
            Ok(()) => return Ok(()),
            Err(tmux_err) => {
                tracing::warn!("tmux clipboard copy failed: {tmux_err}, falling back to OSC 52");
                return osc52_copy_fn(text).map_err(|osc_err| {
                    format!("tmux clipboard: {tmux_err}; OSC 52 fallback: {osc_err}")
                });
            }
        }
    }

    osc52_copy_fn(text)
}

/// 当前是否运行在 SSH 会话中。
fn is_ssh_session() -> bool {
    std::env::var_os("SSH_TTY").is_some() || std::env::var_os("SSH_CONNECTION").is_some()
}

/// 当前是否运行在 tmux 内。
fn is_tmux_session() -> bool {
    std::env::var_os("TMUX").is_some() || std::env::var_os("TMUX_PANE").is_some()
}

#[cfg(target_os = "linux")]
fn is_wsl_session() -> bool {
    if let Ok(version) = std::fs::read_to_string("/proc/version") {
        let lower = version.to_lowercase();
        if lower.contains("microsoft") || lower.contains("wsl") {
            return true;
        }
    }
    std::env::var_os("WSL_DISTRO_NAME").is_some() || std::env::var_os("WSL_INTEROP").is_some()
}

#[cfg(not(target_os = "linux"))]
fn is_wsl_session() -> bool {
    false
}

/// 调 arboard 写入本地剪贴板。macOS 下用 SuppressStderr 抑制 NSPasteboard 污染。
///
/// Linux X11/Wayland 需要持有 arboard handle 否则内容消失，返回 ClipboardLease
/// 让调用方保存到 TUI 生命周期。其他平台 lease 为 None。
fn arboard_copy(text: &str) -> Result<Option<crate::clipboard::ClipboardLease>, String> {
    let _guard = crate::clipboard::SuppressStderr::new();
    let mut clipboard =
        arboard::Clipboard::new().map_err(|e| format!("clipboard unavailable: {e}"))?;
    clipboard
        .set_text(text)
        .map_err(|e| format!("failed to set clipboard text: {e}"))?;
    #[cfg(target_os = "linux")]
    {
        Ok(Some(crate::clipboard::ClipboardLease::native_linux(
            clipboard,
        )))
    }
    #[cfg(not(target_os = "linux"))]
    {
        // macOS / Windows：arboard 不需要进程持有，handle 可立即 drop
        drop(clipboard);
        Ok(None)
    }
}

/// WSL 下通过 powershell.exe 写 Windows 剪贴板。
#[cfg(target_os = "linux")]
fn wsl_clipboard_copy(text: &str) -> Result<(), String> {
    let mut child = std::process::Command::new("powershell.exe")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .args([
            "-NoProfile",
            "-Command",
            "[Console]::InputEncoding = [System.Text.Encoding]::UTF8; $ErrorActionPreference = 'Stop'; $text = [Console]::In.ReadToEnd(); Set-Clipboard -Value $text",
        ])
        .spawn()
        .map_err(|e| format!("failed to spawn powershell.exe: {e}"))?;

    let Some(mut stdin) = child.stdin.take() else {
        let _ = child.kill();
        let _ = child.wait();
        return Err("failed to open powershell.exe stdin".to_string());
    };

    if let Err(err) = stdin.write_all(text.as_bytes()) {
        let _ = child.kill();
        let _ = child.wait();
        return Err(format!("failed to write to powershell.exe: {err}"));
    }

    drop(stdin);

    let output = child
        .wait_with_output()
        .map_err(|e| format!("failed to wait for powershell.exe: {e}"))?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if stderr.is_empty() {
            let status = output.status;
            Err(format!("powershell.exe exited with status {status}"))
        } else {
            Err(format!("powershell.exe failed: {stderr}"))
        }
    }
}

#[cfg(not(target_os = "linux"))]
fn wsl_clipboard_copy(_text: &str) -> Result<(), String> {
    Err("WSL clipboard fallback unavailable on this platform".to_string())
}

/// 把文本复制到系统剪贴板：优先直写 tmux client tty，
/// 失败时回落 tmux load-buffer（依赖用户会话的 set-clipboard / Ms 配置）。
///
/// tmux 只在 `set-clipboard` 非 off 且终端声明了 `Ms` 能力时转发剪贴板，
/// 3.3+ 的 `allow-passthrough` 也默认 off。这些都属于用户会话配置，不能要求
/// 用户为 peri 单独修改，因此这里绕过 tmux 转发层，直接向 client 的 pty
/// 写 OSC 52——它和 tmux 自己的输出走同一条通路，最终到达外层终端。
fn tmux_clipboard_copy(text: &str) -> Result<(), String> {
    match tmux_client_tty_copy(text) {
        Ok(()) => return Ok(()),
        Err(tty_err) => {
            tracing::debug!("tmux client tty copy failed: {tty_err}; falling back to tmux buffer")
        }
    }

    tmux_clipboard_copy_ready(
        || tmux_command_output(["show-options", "-gv", "set-clipboard"]),
        || tmux_command_output(["info"]),
    )?;

    tmux_load_buffer(text)
}

/// 向所有已连接的 tmux client tty 直写 OSC 52 序列。
#[cfg(unix)]
fn tmux_client_tty_copy(text: &str) -> Result<(), String> {
    let sequence = osc52_sequence(text, false)?;
    let ttys = tmux_client_ttys()?;
    write_osc52_to_paths(&sequence, &ttys)
}

/// 非 Unix 平台没有可直接写入的 client pty，交给 tmux load-buffer 处理。
#[cfg(not(unix))]
fn tmux_client_tty_copy(_text: &str) -> Result<(), String> {
    Err("tmux client tty copy is unavailable on this platform".to_string())
}

/// 读取当前连接的 tmux client tty 列表（同一 server 可能连接多个终端）。
#[cfg(unix)]
fn tmux_client_ttys() -> Result<Vec<String>, String> {
    let output = std::process::Command::new("tmux")
        .args(["list-clients", "-F", "#{client_tty}"])
        .output()
        .map_err(|e| format!("failed to spawn tmux: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if stderr.is_empty() {
            format!("tmux exited with status {}", output.status)
        } else {
            format!("tmux failed: {stderr}")
        });
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let ttys: Vec<String> = stdout
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with("/dev/"))
        .map(str::to_string)
        .collect();
    if ttys.is_empty() {
        return Err("no tmux client attached".to_string());
    }
    Ok(ttys)
}

/// 依次向 pty 写入序列，任一成功即视为成功。
#[cfg(unix)]
fn write_osc52_to_paths(sequence: &str, paths: &[String]) -> Result<(), String> {
    let mut written = 0_usize;
    let mut errors = Vec::new();
    for path in paths {
        match std::fs::OpenOptions::new().write(true).open(path) {
            Ok(mut file) => match write_osc52_to_writer(&mut file, sequence) {
                Ok(()) => written += 1,
                Err(err) => errors.push(format!("{path}: {err}")),
            },
            Err(err) => errors.push(format!("{path}: {err}")),
        }
    }
    if written > 0 {
        return Ok(());
    }
    Err(format!(
        "failed to write tmux client tty: {}",
        errors.join("; ")
    ))
}

/// 通过 tmux load-buffer 把文本写入剪贴板，tmux 配置 set-clipboard 时会转发到外层终端。
fn tmux_load_buffer(text: &str) -> Result<(), String> {
    let mut child = std::process::Command::new("tmux")
        .args(["load-buffer", "-w", "-"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("failed to spawn tmux: {e}"))?;

    let Some(mut stdin) = child.stdin.take() else {
        let _ = child.kill();
        let _ = child.wait();
        return Err("failed to open tmux stdin".to_string());
    };

    if let Err(err) = stdin.write_all(text.as_bytes()) {
        let _ = child.kill();
        let _ = child.wait();
        return Err(format!("failed to write to tmux: {err}"));
    }

    drop(stdin);

    let output = child
        .wait_with_output()
        .map_err(|e| format!("failed to wait for tmux: {e}"))?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if stderr.is_empty() {
            let status = output.status;
            Err(format!("tmux exited with status {status}"))
        } else {
            Err(format!("tmux failed: {stderr}"))
        }
    }
}

/// 校验 tmux 是否配置了 set-clipboard 转发，否则后续 load-buffer 写入无法到外层剪贴板。
fn tmux_clipboard_copy_ready(
    set_clipboard_fn: impl FnOnce() -> Result<String, String>,
    tmux_info_fn: impl FnOnce() -> Result<String, String>,
) -> Result<(), String> {
    let set_clipboard = set_clipboard_fn()?;
    if set_clipboard.trim() == "off" {
        return Err("tmux clipboard forwarding is disabled".to_string());
    }

    let tmux_info = tmux_info_fn()?;
    if tmux_info.lines().any(|line| line.contains("Ms: [missing]")) {
        return Err("tmux clipboard forwarding is unavailable: missing Ms capability".to_string());
    }

    Ok(())
}

fn tmux_command_output<const N: usize>(args: [&str; N]) -> Result<String, String> {
    let output = std::process::Command::new("tmux")
        .args(args)
        .output()
        .map_err(|e| format!("failed to spawn tmux: {e}"))?;

    if output.status.success() {
        String::from_utf8(output.stdout).map_err(|e| format!("tmux output was not UTF-8: {e}"))
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if stderr.is_empty() {
            let status = output.status;
            Err(format!("tmux exited with status {status}"))
        } else {
            Err(format!("tmux failed: {stderr}"))
        }
    }
}

/// 通过 OSC 52 转义把文本写入终端剪贴板。优先 /dev/tty，回落 stdout。
fn osc52_copy(text: &str) -> Result<(), String> {
    let sequence = osc52_sequence(text, std::env::var_os("TMUX").is_some())?;
    #[cfg(unix)]
    {
        match std::fs::OpenOptions::new().write(true).open("/dev/tty") {
            Ok(tty) => match write_osc52_to_writer(tty, &sequence) {
                Ok(()) => return Ok(()),
                Err(err) => tracing::debug!(
                    "failed to write OSC 52 to /dev/tty: {err}; falling back to stdout"
                ),
            },
            Err(err) => {
                tracing::debug!("failed to open /dev/tty for OSC 52: {err}; falling back to stdout")
            }
        }
    }

    write_osc52_to_writer(std::io::stdout().lock(), &sequence)
}

fn write_osc52_to_writer(mut writer: impl Write, sequence: &str) -> Result<(), String> {
    writer
        .write_all(sequence.as_bytes())
        .map_err(|e| format!("failed to write OSC 52: {e}"))?;
    writer
        .flush()
        .map_err(|e| format!("failed to flush OSC 52: {e}"))
}

fn osc52_sequence(text: &str, tmux: bool) -> Result<String, String> {
    let raw_bytes = text.len();
    if raw_bytes > OSC52_MAX_RAW_BYTES {
        return Err(format!(
            "OSC 52 payload too large ({raw_bytes} bytes; max {OSC52_MAX_RAW_BYTES})"
        ));
    }

    let encoded = base64::engine::general_purpose::STANDARD.encode(text.as_bytes());
    if tmux {
        Ok(format!("\x1bPtmux;\x1b\x1b]52;c;{encoded}\x07\x1b\\"))
    } else {
        Ok(format!("\x1b]52;c;{encoded}\x07"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    fn remote_environment() -> CopyEnvironment {
        CopyEnvironment {
            ssh_session: true,
            wsl_session: true,
            tmux_session: false,
        }
    }

    fn remote_tmux_environment() -> CopyEnvironment {
        CopyEnvironment {
            tmux_session: true,
            ..remote_environment()
        }
    }

    fn local_environment() -> CopyEnvironment {
        CopyEnvironment {
            ssh_session: false,
            wsl_session: false,
            tmux_session: false,
        }
    }

    fn local_wsl_environment() -> CopyEnvironment {
        CopyEnvironment {
            wsl_session: true,
            ..local_environment()
        }
    }

    fn local_tmux_environment() -> CopyEnvironment {
        CopyEnvironment {
            tmux_session: true,
            ..local_environment()
        }
    }

    #[test]
    fn test_osc52_roundtrip_encoding_consistent() {
        let text = "# Hello\n\n```rust\nfn main() {}\n```\n";
        let sequence = osc52_sequence(text, false).expect("OSC 52 sequence");
        let encoded = sequence
            .trim_start_matches("\u{1b}]52;c;")
            .trim_end_matches('\u{7}');
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .unwrap();
        assert_eq!(decoded, text.as_bytes());
    }

    #[test]
    fn test_osc52_rejects_payload_exceeding_max_bytes() {
        let text = "x".repeat(OSC52_MAX_RAW_BYTES + 1);
        assert_eq!(
            osc52_sequence(&text, false),
            Err(format!(
                "OSC 52 payload too large ({} bytes; max {OSC52_MAX_RAW_BYTES})",
                OSC52_MAX_RAW_BYTES + 1
            ))
        );
    }

    #[test]
    fn test_osc52_tmux_passthrough_wraps_correctly() {
        assert_eq!(
            osc52_sequence("hello", true),
            Ok("\u{1b}Ptmux;\u{1b}\u{1b}]52;c;aGVsbG8=\u{7}\u{1b}\\".to_string())
        );
    }

    #[test]
    fn test_osc52_writes_raw_sequence() {
        let sequence = "\u{1b}]52;c;aGVsbG8=\u{7}";
        let mut output = Vec::new();
        assert_eq!(write_osc52_to_writer(&mut output, sequence), Ok(()));
        assert_eq!(output, sequence.as_bytes());
    }

    #[test]
    fn test_copy_uses_osc52_and_skips_native_under_ssh() {
        let tmux_calls = Cell::new(0_u8);
        let osc_calls = Cell::new(0_u8);
        let native_calls = Cell::new(0_u8);
        let wsl_calls = Cell::new(0_u8);
        let result = copy_to_clipboard_with(
            "hello",
            remote_environment(),
            |_| {
                tmux_calls.set(tmux_calls.get() + 1);
                Ok(())
            },
            |_| {
                osc_calls.set(osc_calls.get() + 1);
                Ok(())
            },
            |_| {
                native_calls.set(native_calls.get() + 1);
                Ok(None)
            },
            |_| {
                wsl_calls.set(wsl_calls.get() + 1);
                Ok(())
            },
        );

        assert!(result.is_ok());
        assert_eq!(tmux_calls.get(), 0);
        assert_eq!(osc_calls.get(), 1);
        assert_eq!(native_calls.get(), 0);
        assert_eq!(wsl_calls.get(), 0);
    }

    #[test]
    fn test_copy_skips_native_when_osc52_fails_under_ssh() {
        let result = copy_to_clipboard_with(
            "hello",
            remote_environment(),
            |_| Ok(()),
            |_| Err("blocked".into()),
            |_| Ok(None),
            |_| Ok(()),
        );

        let Err(error) = result else {
            panic!("expected OSC 52 error");
        };
        assert_eq!(error, "OSC 52 clipboard copy failed over SSH: blocked");
    }

    #[test]
    fn test_copy_prefers_tmux_clipboard_inside_ssh_tmux() {
        let tmux_calls = Cell::new(0_u8);
        let osc_calls = Cell::new(0_u8);
        let result = copy_to_clipboard_with(
            "hello",
            remote_tmux_environment(),
            |_| {
                tmux_calls.set(tmux_calls.get() + 1);
                Ok(())
            },
            |_| {
                osc_calls.set(osc_calls.get() + 1);
                Ok(())
            },
            |_| Ok(None),
            |_| Ok(()),
        );

        assert!(result.is_ok());
        assert_eq!(tmux_calls.get(), 1);
        assert_eq!(osc_calls.get(), 0);
    }

    #[test]
    fn test_copy_falls_back_to_osc52_when_tmux_fails_inside_ssh() {
        let tmux_calls = Cell::new(0_u8);
        let osc_calls = Cell::new(0_u8);
        let result = copy_to_clipboard_with(
            "hello",
            remote_tmux_environment(),
            |_| {
                tmux_calls.set(tmux_calls.get() + 1);
                Err("tmux unavailable".into())
            },
            |_| {
                osc_calls.set(osc_calls.get() + 1);
                Ok(())
            },
            |_| Ok(None),
            |_| Ok(()),
        );

        assert!(result.is_ok());
        assert_eq!(tmux_calls.get(), 1);
        assert_eq!(osc_calls.get(), 1);
    }

    #[test]
    fn test_copy_prefers_native_clipboard_locally() {
        let osc_calls = Cell::new(0_u8);
        let native_calls = Cell::new(0_u8);
        let wsl_calls = Cell::new(0_u8);
        let result = copy_to_clipboard_with(
            "hello",
            local_wsl_environment(),
            |_| Ok(()),
            |_| {
                osc_calls.set(osc_calls.get() + 1);
                Ok(())
            },
            |_| {
                native_calls.set(native_calls.get() + 1);
                Ok(None)
            },
            |_| {
                wsl_calls.set(wsl_calls.get() + 1);
                Ok(())
            },
        );

        assert!(result.is_ok());
        assert_eq!(osc_calls.get(), 0);
        assert_eq!(native_calls.get(), 1);
        assert_eq!(wsl_calls.get(), 0);
    }

    #[test]
    fn test_copy_falls_back_to_osc52_when_native_fails_locally_non_wsl() {
        let osc_calls = Cell::new(0_u8);
        let native_calls = Cell::new(0_u8);
        let result = copy_to_clipboard_with(
            "hello",
            local_environment(),
            |_| Ok(()),
            |_| {
                osc_calls.set(osc_calls.get() + 1);
                Ok(())
            },
            |_| {
                native_calls.set(native_calls.get() + 1);
                Err("native unavailable".into())
            },
            |_| Ok(()),
        );

        assert!(result.is_ok());
        assert_eq!(osc_calls.get(), 1);
        assert_eq!(native_calls.get(), 1);
    }

    #[test]
    fn test_copy_prefers_tmux_when_native_fails_locally_in_tmux() {
        let tmux_calls = Cell::new(0_u8);
        let osc_calls = Cell::new(0_u8);
        let result = copy_to_clipboard_with(
            "hello",
            local_tmux_environment(),
            |_| {
                tmux_calls.set(tmux_calls.get() + 1);
                Ok(())
            },
            |_| {
                osc_calls.set(osc_calls.get() + 1);
                Ok(())
            },
            |_| Err("native unavailable".into()),
            |_| Ok(()),
        );

        assert!(result.is_ok());
        assert_eq!(tmux_calls.get(), 1);
        assert_eq!(osc_calls.get(), 0);
    }

    #[test]
    fn test_copy_uses_powershell_and_skips_osc52_when_native_fails_in_local_wsl() {
        let osc_calls = Cell::new(0_u8);
        let native_calls = Cell::new(0_u8);
        let wsl_calls = Cell::new(0_u8);
        let result = copy_to_clipboard_with(
            "hello",
            local_wsl_environment(),
            |_| Ok(()),
            |_| {
                osc_calls.set(osc_calls.get() + 1);
                Ok(())
            },
            |_| {
                native_calls.set(native_calls.get() + 1);
                Err("native unavailable".into())
            },
            |_| {
                wsl_calls.set(wsl_calls.get() + 1);
                Ok(())
            },
        );

        assert!(result.is_ok());
        assert_eq!(osc_calls.get(), 0);
        assert_eq!(native_calls.get(), 1);
        assert_eq!(wsl_calls.get(), 1);
    }

    #[test]
    fn test_copy_falls_back_to_osc52_when_native_and_powershell_fail_in_local_wsl() {
        let osc_calls = Cell::new(0_u8);
        let result = copy_to_clipboard_with(
            "hello",
            local_wsl_environment(),
            |_| Ok(()),
            |_| {
                osc_calls.set(osc_calls.get() + 1);
                Ok(())
            },
            |_| Err("native unavailable".into()),
            |_| Err("powershell unavailable".into()),
        );

        assert!(result.is_ok());
        assert_eq!(osc_calls.get(), 1);
    }

    #[test]
    fn test_copy_returns_aggregated_error_when_native_and_osc52_both_fail_locally() {
        let result = copy_to_clipboard_with(
            "hello",
            local_environment(),
            |_| Ok(()),
            |_| Err("osc blocked".into()),
            |_| Err("native unavailable".into()),
            |_| Ok(()),
        );

        let Err(error) = result else {
            panic!("expected native and OSC 52 errors");
        };
        assert_eq!(
            error,
            "native clipboard: native unavailable; OSC 52 fallback: osc blocked"
        );
    }

    #[test]
    fn test_tmux_clipboard_ready_fails_when_forwarding_disabled() {
        let result = tmux_clipboard_copy_ready(
            || Ok("off\n".to_string()),
            || panic!("tmux info should not be queried when forwarding is disabled"),
        );

        assert_eq!(
            result,
            Err("tmux clipboard forwarding is disabled".to_string())
        );
    }

    #[test]
    fn test_tmux_clipboard_ready_fails_when_missing_ms_capability() {
        let result = tmux_clipboard_copy_ready(
            || Ok("external\n".to_string()),
            || Ok("193: Ms: [missing]\n".to_string()),
        );

        assert_eq!(
            result,
            Err("tmux clipboard forwarding is unavailable: missing Ms capability".to_string())
        );
    }

    #[test]
    fn test_tmux_clipboard_ready_succeeds_when_forwarding_enabled() {
        let result = tmux_clipboard_copy_ready(
            || Ok("external\n".to_string()),
            || Ok("193: Ms: (string) \\033]52;%p1%s;%p2%s\\a\n".to_string()),
        );

        assert_eq!(result, Ok(()));
    }

    #[test]
    #[cfg(unix)]
    fn test_write_osc52_to_paths_writes_each_target() {
        let dir = tempfile::tempdir().expect("临时目录");
        let first = dir.path().join("client-1.tty");
        let second = dir.path().join("client-2.tty");
        std::fs::write(&first, "").expect("创建第一个目标");
        std::fs::write(&second, "").expect("创建第二个目标");
        let sequence = "\u{1b}]52;c;aGVsbG8=\u{7}";
        let paths = vec![first.display().to_string(), second.display().to_string()];

        let result = write_osc52_to_paths(sequence, &paths);

        assert_eq!(result, Ok(()));
        assert_eq!(
            std::fs::read_to_string(&first).expect("读取第一个目标"),
            sequence
        );
        assert_eq!(
            std::fs::read_to_string(&second).expect("读取第二个目标"),
            sequence
        );
    }

    #[test]
    #[cfg(unix)]
    fn test_write_osc52_to_paths_succeeds_when_one_target_writable() {
        let dir = tempfile::tempdir().expect("临时目录");
        let writable = dir.path().join("client.tty");
        std::fs::write(&writable, "").expect("创建可写目标");
        let paths = vec![
            dir.path().join("missing.tty").display().to_string(),
            writable.display().to_string(),
        ];

        let result = write_osc52_to_paths("seq", &paths);

        assert_eq!(result, Ok(()));
    }

    #[test]
    #[cfg(unix)]
    fn test_write_osc52_to_paths_reports_all_failures() {
        let missing = "/nonexistent/peri-client-a".to_string();

        let result = write_osc52_to_paths("seq", std::slice::from_ref(&missing));

        let Err(error) = result else {
            panic!("全部目标不可写时应返回错误");
        };
        assert!(error.starts_with("failed to write tmux client tty: "));
        assert!(error.contains(&missing));
    }
}
