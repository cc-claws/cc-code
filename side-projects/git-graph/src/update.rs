//! gig 自更新：从 GitHub Release 下载最新版本并替换当前二进制
//!
//! 安装目录布局（与 peri 共享 ~/.peri）：
//! ```text
//! ~/.peri/
//! ├── gig                              # symlink → gig-v0.1.0/gig
//! ├── gig-current-version.txt          # gig 版本标记
//! ├── gig-v0.1.0/
//! │   └── gig                          # 实际二进制
//! └── ...
//! ```

use anyhow::{bail, Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

const GITHUB_API: &str = "https://api.github.com/repos/konghayao/peri/releases";
const TOOL_NAME: &str = "gig";

// ── 公开入口 ──────────────────────────────────────

pub fn run_update() -> Result<()> {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async { do_update().await })
}

// ── 内部实现 ──────────────────────────────────────

async fn do_update() -> Result<()> {
    let install_dir = install_dir();
    let current = read_current_version(&install_dir);

    println!("Checking for updates...");

    let releases = fetch_releases().await?;
    let latest = releases
        .iter()
        .find(|r| r.tag_name.starts_with("gig-v"))
        .context("No gig release found")?;

    if let Some(ref cur) = current {
        if &latest.tag_name == cur {
            println!("Already on latest version: {}", cur);
            return Ok(());
        }
        println!("Current: {}", cur);
    }

    println!("Latest:  {}", latest.tag_name);

    let platform = detect_platform()?;
    let asset_name = format!("gig-{}.tar.gz", platform);
    let asset = latest
        .assets
        .iter()
        .find(|a| a.name == asset_name)
        .with_context(|| {
            let available: Vec<&str> = latest.assets.iter().map(|a| a.name.as_str()).collect();
            format!(
                "No binary found for platform '{}'. Available: {:?}",
                platform, available
            )
        })?;

    let version_dir = install_dir.join(&latest.tag_name);
    fs::create_dir_all(&version_dir)
        .with_context(|| format!("Failed to create {}", version_dir.display()))?;

    let archive_path = version_dir.join(&asset_name);
    println!("Downloading {}...", asset.browser_download_url);
    download_file(&asset.browser_download_url, &archive_path).await?;

    println!("Extracting...");
    extract_tar_gz(&archive_path, &version_dir)?;
    fs::remove_file(&archive_path).ok();

    // tarball 内文件名如 gig-linux-x86_64，重命名为 gig
    rename_extracted_binary(&version_dir, &platform)?;

    // 创建/更新 symlink: ~/.peri/gig → ~/.peri/gig-vX.Y.Z/gig
    let target = version_dir.join(TOOL_NAME);
    let link = install_dir.join(TOOL_NAME);
    update_symlink(&target, &link)?;

    // 写版本标记
    let version_file = install_dir.join("gig-current-version.txt");
    fs::write(&version_file, &latest.tag_name)?;

    // 清理旧版本（保留最新 2 个）
    cleanup_old_versions(&install_dir, &latest.tag_name)?;

    println!();
    println!("Updated to {} successfully!", latest.tag_name);
    Ok(())
}

// ── GitHub API ──────────────────────────────────────

#[derive(serde::Deserialize)]
struct Release {
    tag_name: String,
    assets: Vec<Asset>,
}

#[derive(serde::Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
}

async fn fetch_releases() -> Result<Vec<Release>> {
    let client = reqwest::Client::builder()
        .user_agent("gig-update")
        .build()?;
    let resp = client
        .get(format!("{}?per_page=20", GITHUB_API))
        .send()
        .await?;
    if !resp.status().is_success() {
        bail!("GitHub API returned {}", resp.status());
    }
    let releases: Vec<Release> = resp.json().await?;
    Ok(releases)
}

// ── 平台检测 ─────────────────────────────────────────

fn detect_platform() -> Result<String> {
    let os = match std::env::consts::OS {
        "macos" => "macos",
        "linux" => "linux",
        "windows" => "windows",
        other => bail!("Unsupported OS: {}", other),
    };
    let arch = match std::env::consts::ARCH {
        "x86_64" => "x86_64",
        "aarch64" => "aarch64",
        other => bail!("Unsupported arch: {}", other),
    };
    Ok(format!("{}-{}", os, arch))
}

// ── 版本管理 ─────────────────────────────────────────

fn install_dir() -> PathBuf {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".peri")
}

fn read_current_version(install_dir: &Path) -> Option<String> {
    let f = install_dir.join("gig-current-version.txt");
    fs::read_to_string(f).ok().map(|s| s.trim().to_string())
}

// ── 下载 ─────────────────────────────────────────────

async fn download_file(url: &str, dest: &Path) -> Result<()> {
    // 支持代理：GITHUB_PROXY 环境变量替换前缀
    let final_url = apply_proxy(url);
    let client = reqwest::Client::builder()
        .user_agent("gig-update")
        .build()?;
    let resp = client.get(&final_url).send().await?;
    if !resp.status().is_success() {
        bail!("Download failed: HTTP {}", resp.status());
    }
    let bytes = resp.bytes().await?;
    fs::write(dest, &bytes)?;
    Ok(())
}

fn apply_proxy(url: &str) -> String {
    if let Ok(proxy) = std::env::var("GITHUB_PROXY") {
        url.replace("https://github.com", &proxy)
    } else {
        url.to_string()
    }
}

// ── 解压 ─────────────────────────────────────────────

fn extract_tar_gz(archive: &Path, dest: &Path) -> Result<()> {
    let file = fs::File::open(archive)?;
    let gz = flate2::read::GzDecoder::new(file);
    let mut tar = tar::Archive::new(gz);
    tar.unpack(dest)?;
    Ok(())
}

fn rename_extracted_binary(version_dir: &Path, platform: &str) -> Result<()> {
    let expected_name = format!("gig-{}", platform);
    let src = version_dir.join(&expected_name);
    if src.exists() {
        let dest = version_dir.join(TOOL_NAME);
        fs::rename(&src, &dest).with_context(|| {
            format!(
                "Failed to rename {} → {}",
                src.display(),
                dest.display()
            )
        })?;
        // 确保可执行
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&dest, fs::Permissions::from_mode(0o755))?;
        }
    }
    Ok(())
}

// ── Symlink ──────────────────────────────────────────

fn update_symlink(target: &Path, link: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        // 先删旧 link（不存在也无所谓）
        fs::remove_file(link).ok();
        std::os::unix::fs::symlink(target, link)
            .with_context(|| format!("symlink {} → {}", link.display(), target.display()))?;
    }
    #[cfg(windows)]
    {
        // Windows 上用复制代替 symlink（无需管理员权限）
        fs::copy(target, link).with_context(|| {
            format!(
                "copy {} → {}",
                target.display(),
                link.display()
            )
        })?;
    }
    Ok(())
}

// ── 清理旧版本 ───────────────────────────────────────

fn cleanup_old_versions(install_dir: &Path, current: &str) -> Result<()> {
    let entries = fs::read_dir(install_dir)?;
    let mut versions: Vec<String> = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map_or(false, |t| t.is_dir()))
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            if name.starts_with("gig-v") && name != current {
                Some(name)
            } else {
                None
            }
        })
        .collect();

    // 按版本号排序，保留最新 2 个
    versions.sort_by(|a, b| compare_versions(a, b));

    // 删除旧版本（保留最新 2 个 + 当前版本已在上面排除）
    let keep = 2;
    if versions.len() > keep {
        for old in &versions[..versions.len() - keep] {
            let dir = install_dir.join(old);
            if fs::remove_dir_all(&dir).is_ok() {
                println!("  Cleaned up: {}", old);
            }
        }
    }
    Ok(())
}

/// 简单版本号比较：gig-v0.1.0 → [0,1,0]
fn compare_versions(a: &str, b: &str) -> std::cmp::Ordering {
    let va: Vec<u64> = a
        .trim_start_matches("gig-v")
        .split('.')
        .filter_map(|s| s.parse().ok())
        .collect();
    let vb: Vec<u64> = b
        .trim_start_matches("gig-v")
        .split('.')
        .filter_map(|s| s.parse().ok())
        .collect();
    for i in 0..va.len().max(vb.len()) {
        let na = va.get(i).unwrap_or(&0);
        let nb = vb.get(i).unwrap_or(&0);
        match na.cmp(nb) {
            std::cmp::Ordering::Equal => continue,
            other => return other,
        }
    }
    std::cmp::Ordering::Equal
}
