//! Update mechanism: downloads and runs the remote install script.
//!
//! On Unix: curl install.sh | bash
//! On Windows: irm install.ps1 | iex
//!
//! Delegates all update logic (download, checksum, extract, symlink)
//! to the remote install scripts.

use anyhow::{Context, Result};
use std::process::Stdio;
use tokio::process::Command;

const SCRIPT_URL_SH: &str =
    "https://raw.githubusercontent.com/cc-claws/cc-code/main/scripts/install.sh";
/// #19 fix: Expected SHA256 of the install script.
/// UPDATE THIS when scripts/install.sh changes, or the update will be blocked.
const SCRIPT_SHA256_SH: &str =
    "2916bdb81b4bc6a21ff6c1e36e226f565ea4325c5539dc88c6030f96418d629a";
const SCRIPT_URL_PS1: &str =
    "https://raw.githubusercontent.com/cc-claws/cc-code/main/scripts/install.ps1";

/// Run the update flow. Returns Ok(new_tag) on success.
///
/// Streams the remote install script's stdout/stderr to the terminal.
pub async fn run_update() -> Result<String> {
    println!("cc-code update");

    if cfg!(target_os = "windows") {
        run_update_windows().await
    } else {
        run_update_unix().await
    }
}

async fn run_update_unix() -> Result<String> {
    println!("  Downloading install script...");
    // #19 fix: Download to temp file and verify SHA256 before executing.
    // Prevents execution of tampered scripts (MITM or compromised source).
    let tmp = std::env::temp_dir().join(format!("cc-code-install-{}.sh", std::process::id()));
    let download = Command::new("curl")
        .args(["-fsSL", SCRIPT_URL_SH, "-o", tmp.to_str().unwrap()])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("Failed to spawn curl. Is curl available?")?;
    let out = download.wait_with_output().await?;
    if !out.status.success() {
        anyhow::bail!("Failed to download install script");
    }
    // Verify SHA256
    let content = std::fs::read(&tmp).context("Failed to read downloaded script")?;
    let hash = {
        use ring::digest;
        let d = digest::digest(&digest::SHA256, &content);
        d.as_ref().iter().map(|b| format!("{:02x}", b)).collect::<String>()
    };
    if hash != SCRIPT_SHA256_SH {
        let _ = std::fs::remove_file(&tmp);
        anyhow::bail!(
            "Install script SHA256 mismatch! Expected {}, got {}.              The script may have been tampered with. Update aborted for safety.",
            SCRIPT_SHA256_SH, hash
        );
    }
    println!("  Script verified (SHA256 OK). Running...");

    let mut child = Command::new("bash")
        .arg(tmp.to_str().unwrap())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("Failed to spawn update process. Is bash available?")?;

    let result = stream_output(&mut child).await;
    let _ = std::fs::remove_file(&tmp);
    result?;
    read_installed_version()
}

async fn run_update_windows() -> Result<String> {
    println!("  Running remote install script...");

    // 先强制子进程控制台输出为 UTF-8：在非英文 Windows 上默认代码页（如 GBK）
    // 会把中文（错误信息等）写成非 UTF-8 字节，见 `stream_lines_lossy`。
    let ps_command = format!(
        "try {{ [Console]::OutputEncoding = [System.Text.Encoding]::UTF8 }} catch {{}}; irm {SCRIPT_URL_PS1} | iex"
    );

    let mut child = Command::new("powershell")
        .args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            &ps_command,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("Failed to spawn update process. Is PowerShell available?")?;

    stream_output(&mut child).await?;
    read_installed_version()
}

async fn stream_output(child: &mut tokio::process::Child) -> Result<()> {
    if let Some(stdout) = child.stdout.take() {
        stream_lines_lossy(stdout, |line| println!("{line}")).await?;
    }
    if let Some(stderr) = child.stderr.take() {
        stream_lines_lossy(stderr, |line| eprintln!("{line}")).await?;
    }

    let status = child.wait().await?;
    if !status.success() {
        anyhow::bail!("Update script exited with status {}", status);
    }

    Ok(())
}

/// Stream a child process's output line by line, decoding each line **lossily**.
///
/// The install scripts run under `bash` (Unix) or Windows PowerShell. The latter
/// writes to a redirected pipe using the console code page, which on non-English
/// Windows (e.g. GBK) is not valid UTF-8 — a strict UTF-8 reader would abort with
/// "stream did not contain valid UTF-8" and hide the real error message. Decode
/// lossily so non-ASCII diagnostics never crash the update.
async fn stream_lines_lossy<R>(mut reader: R, mut emit: impl FnMut(&str)) -> std::io::Result<()>
where
    R: tokio::io::AsyncRead + Unpin,
{
    use tokio::io::AsyncReadExt;

    let mut pending: Vec<u8> = Vec::with_capacity(8192);
    let mut chunk = [0u8; 8192];
    loop {
        let n = reader.read(&mut chunk).await?;
        if n == 0 {
            break;
        }
        pending.extend_from_slice(&chunk[..n]);
        for line in take_complete_lines(&mut pending) {
            emit(&line);
        }
    }
    if !pending.is_empty() {
        emit(&String::from_utf8_lossy(&pending));
    }
    Ok(())
}

/// Drain and return every `\n`-terminated line currently in `buf`.
///
/// A trailing `\r` (CRLF) is stripped. Bytes that are not valid UTF-8 are
/// replaced (lossy) rather than rejected.
fn take_complete_lines(buf: &mut Vec<u8>) -> Vec<String> {
    let mut out = Vec::new();
    while let Some(pos) = buf.iter().position(|&b| b == b'\n') {
        let mut line: Vec<u8> = buf.drain(..=pos).collect();
        line.pop(); // 去掉结尾 '\n'
        if line.last() == Some(&b'\r') {
            line.pop();
        }
        out.push(String::from_utf8_lossy(&line).into_owned());
    }
    out
}

fn read_installed_version() -> Result<String> {
    let version_file = dirs_next::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(".cc-code")
        .join("current-version.txt");
    let tag = std::fs::read_to_string(&version_file)
        .ok()
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    Ok(tag)
}

#[cfg(test)]
#[path = "update_test.rs"]
mod tests;
