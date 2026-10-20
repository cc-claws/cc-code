//! 构建脚本：向二进制注入安装脚本的「提交引用 + SHA256」，并嵌入 Windows 资源。
//!
//! `cc-code update` 会从 raw.githubusercontent.com 下载 `scripts/install.sh` 后校验
//! SHA256。若下载地址取浮动的 `main`、而哈希取构建时的固定值，脚本一旦在 `main` 上
//! 有任何改动，所有已发布二进制的 update 都会永久失败。这里让两者同源：统一钉在
//! 构建时的 HEAD 提交上。

use std::path::{Path, PathBuf};
use std::process::Command;

const INSTALL_SCRIPT: &str = "scripts/install.sh";

fn main() {
    emit_install_script_metadata();

    #[cfg(target_os = "windows")]
    {
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
        let icon_path = std::path::PathBuf::from(&manifest_dir)
            .join("assets")
            .join("icon.ico");

        let mut res = winresource::WindowsResource::new();
        res.set_icon(icon_path.to_str().unwrap());
        res.set("FileVersion", "0.2.0.0");
        res.set("ProductVersion", "0.2.0.0");
        res.set("ProductName", "CC Code");
        res.set("FileDescription", "CC Code");
        res.set("CompanyName", "CC Code");
        res.set("LegalCopyright", "Copyright (c) 2026 CC Code");
        res.set_manifest(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
    <security>
      <requestedPrivileges>
        <requestedExecutionLevel level="asInvoker" uiAccess="false"/>
      </requestedPrivileges>
    </security>
  </trustInfo>
  <compatibility xmlns="urn:schemas-microsoft-com:compatibility.v1">
    <application>
      <supportedOS Id="{8e0f7a12-bfb3-4fe8-b9a5-48fd50a15a9a}"/>
    </application>
  </compatibility>
</assembly>"#,
        );
        res.compile().expect("failed to embed windows resource");
    }
}

/// 注入 `CC_INSTALL_SCRIPT_REF` / `CC_INSTALL_SCRIPT_SHA256`。
///
/// 两者必须来自同一个提交，否则就退化成「地址浮动 + 哈希固定」——正是最近
/// `cc-code update` 报 `SHA256 mismatch` 的成因。
fn emit_install_script_metadata() {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    // cc-tui/ 的上一级即 workspace 根
    let repo_root = manifest_dir
        .parent()
        .expect("cc-tui should live in a workspace subdirectory")
        .to_path_buf();

    println!(
        "cargo:rerun-if-changed={}",
        repo_root.join(INSTALL_SCRIPT).display()
    );
    // HEAD 变化（切分支、新提交）后需要重新生成引用
    println!(
        "cargo:rerun-if-changed={}",
        repo_root.join(".git/HEAD").display()
    );

    // 优先取 HEAD 提交里的 blob 原文：raw.githubusercontent.com/<commit>/... 返回的
    // 就是这份字节；工作区文件可能被 CRLF checkout 改写，哈希会对不上。
    let pinned =
        git_bytes(&repo_root, &["show", &format!("HEAD:{INSTALL_SCRIPT}")]).and_then(|content| {
            let commit = String::from_utf8(git_bytes(&repo_root, &["rev-parse", "HEAD"])?).ok()?;
            let commit = commit.trim().to_string();
            (!commit.is_empty()).then_some((content, commit))
        });

    let (content, git_ref) = match pinned {
        Some(pinned) => pinned,
        None => {
            // 源码包（无 git）等场景：退回浮动 `main`，与改动前的行为一致。
            let content = std::fs::read(repo_root.join(INSTALL_SCRIPT))
                .expect("scripts/install.sh not found");
            println!("cargo:warning=git unavailable; install script URL falls back to `main`");
            (content, "main".to_string())
        }
    };

    println!("cargo:rustc-env=CC_INSTALL_SCRIPT_REF={git_ref}");
    println!(
        "cargo:rustc-env=CC_INSTALL_SCRIPT_SHA256={}",
        sha256_hex(&content)
    );
}

fn git_bytes(dir: &Path, args: &[&str]) -> Option<Vec<u8>> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .ok()?;
    out.status.success().then_some(out.stdout)
}

fn sha256_hex(data: &[u8]) -> String {
    ring::digest::digest(&ring::digest::SHA256, data)
        .as_ref()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
