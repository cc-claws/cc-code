//! gig 自更新：调用 install 脚本完成下载安装
//!
//! Unix:  `curl -fsSL <install-gig.sh> | bash`
//! Windows: `irm <install-gig.ps1> | iex`
//!
//! 所有下载、解压、symlink、版本管理逻辑都在脚本中，
//! 二进制只负责拉起脚本，保持轻量。

use anyhow::{bail, Result};
use std::process::Command;

const SCRIPT_URL_UNIX: &str =
    "https://raw.githubusercontent.com/konghayao/peri/main/side-projects/git-graph/install-gig.sh";
const SCRIPT_URL_WINDOWS: &str =
    "https://raw.githubusercontent.com/konghayao/peri/main/side-projects/git-graph/install-gig.ps1";

pub fn run_update() -> Result<()> {
    if cfg!(windows) {
        run_update_windows()
    } else {
        run_update_unix()
    }
}

fn run_update_unix() -> Result<()> {
    // bash -c 'curl -fsSL <url> | bash'
    let script = format!("curl -fsSL {} | bash", SCRIPT_URL_UNIX);
    let status = Command::new("bash").arg("-c").arg(&script).status()?;

    if !status.success() {
        bail!("Update script exited with {}", status);
    }
    Ok(())
}

fn run_update_windows() -> Result<()> {
    // powershell -Command "irm <url> | iex"
    let script = format!("irm {} | iex", SCRIPT_URL_WINDOWS);
    let status = Command::new("powershell")
        .arg("-NoProfile")
        .arg("-Command")
        .arg(&script)
        .status()?;

    if !status.success() {
        bail!("Update script exited with {}", status);
    }
    Ok(())
}
