//! Update mechanism: downloads and runs the remote install script.
//!
//! On Unix: curl install.sh | bash
//! On Windows: irm install.ps1 | iex
//!
//! Delegates all update logic (download, checksum, extract, symlink)
//! to the remote install scripts.

use anyhow::{Context, Result};
use std::process::Stdio;
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::Command,
};

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

    let ps_command = format!("irm {SCRIPT_URL_PS1} | iex");

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
    // 流式输出 stdout
    if let Some(stdout) = child.stdout.take() {
        let reader = BufReader::new(stdout);
        let mut lines = reader.lines();
        while let Some(line) = lines.next_line().await? {
            println!("{line}");
        }
    }

    // 流式输出 stderr
    if let Some(stderr) = child.stderr.take() {
        let reader = BufReader::new(stderr);
        let mut lines = reader.lines();
        while let Some(line) = lines.next_line().await? {
            eprintln!("{line}");
        }
    }

    let status = child.wait().await?;
    if !status.success() {
        anyhow::bail!("Update script exited with status {}", status);
    }

    Ok(())
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
