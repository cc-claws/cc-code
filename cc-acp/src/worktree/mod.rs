//! Git worktree 的查询与创建；调用方负责放到阻塞任务中执行。
//!
//! 仅创建独立 checkout，不修改原目录、暂存区或已有分支，不自动清理失败产物。

use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
};

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Worktree {
    pub path: String,
    pub head: String,
    pub branch: Option<String>,
    pub locked: bool,
    pub prunable: bool,
    pub bare: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeList {
    pub repository_root: String,
    pub worktrees: Vec<Worktree>,
}

/// 列出当前仓库的 checkout，保留 Git 返回的真实路径及不可用状态。
pub fn list(cwd: &Path) -> Result<WorktreeList> {
    let root = git_checked(cwd, ["rev-parse", "--show-toplevel"])?;
    let repository_root = String::from_utf8(root.stdout)
        .context("Git repository path is not UTF-8")?
        .trim_end_matches(['\r', '\n'])
        .to_owned();
    let output = git_output(
        cwd,
        [
            "-c",
            "core.quotePath=false",
            "worktree",
            "list",
            "--porcelain",
            "-z",
        ],
    )?;
    let worktrees = if output.status.success() {
        parse_listing(&output.stdout, true)?
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // 旧版 Git 没有 -z；只对明确的选项不支持错误回退，仓库错误仍直接返回。
        if !(stderr.contains("unknown switch `z'") || stderr.contains("unknown option `z'")) {
            bail!("git worktree list failed: {}", stderr.trim());
        }
        let output = git_checked(
            cwd,
            [
                "-c",
                "core.quotePath=false",
                "worktree",
                "list",
                "--porcelain",
            ],
        )?;
        parse_listing(&output.stdout, false)?
    };
    Ok(WorktreeList {
        repository_root,
        worktrees,
    })
}

/// 从指定提交创建独立 checkout；无 branch 时使用 detached HEAD。
pub fn create(
    cwd: &Path,
    branch: Option<&str>,
    base_ref: Option<&str>,
    storage_root: &Path,
) -> Result<Worktree> {
    let existing = list(cwd)?;
    let configured_worktree = git_output(cwd, ["config", "--get", "core.worktree"])?;
    if configured_worktree.status.success() {
        bail!("Creating worktrees with core.worktree configured is not supported; source checkout was preserved");
    }
    if configured_worktree.status.code() != Some(1) {
        bail!(
            "Cannot inspect core.worktree configuration: {}",
            String::from_utf8_lossy(&configured_worktree.stderr).trim()
        );
    }
    if let Some(branch) = branch {
        if branch.is_empty() || branch.starts_with('-') || branch.trim() != branch {
            bail!("Invalid branch name: {branch:?}");
        }
        let checked = git_checked(cwd, ["check-ref-format", "--branch", branch])?;
        if String::from_utf8_lossy(&checked.stdout).trim_end() != branch {
            bail!("Branch name must be literal: {branch:?}");
        }
        let reference = format!("refs/heads/{branch}");
        let existing_branch = git_output(cwd, ["show-ref", "--verify", "--quiet", &reference])?;
        if existing_branch.status.success() {
            bail!("Branch already exists: {branch}");
        }
        if existing_branch.status.code() != Some(1) {
            bail!(
                "Cannot inspect branch: {}",
                String::from_utf8_lossy(&existing_branch.stderr).trim()
            );
        }
    }
    let base_ref = base_ref.unwrap_or("HEAD");
    if base_ref.is_empty() || base_ref.starts_with('-') || base_ref.trim() != base_ref {
        bail!("Invalid starting revision: {base_ref:?}");
    }
    let revision = format!("{base_ref}^{{commit}}");
    let checked = git_checked(cwd, ["rev-parse", "--verify", &revision])?;
    let commit = String::from_utf8(checked.stdout)
        .context("Git commit id is not UTF-8")?
        .trim_end()
        .to_owned();
    // 托管目录不能位于任何现有 checkout 中，避免创建动作污染原工作区。
    let storage_root = absolute_storage_path(storage_root)?;
    for worktree in existing.worktrees.iter().filter(|tree| !tree.bare) {
        if let Ok(path) = Path::new(&worktree.path).canonicalize() {
            if storage_root.starts_with(path) {
                bail!(
                    "Worktree storage must be outside existing checkouts: {}",
                    storage_root.display()
                );
            }
        }
    }
    std::fs::create_dir_all(&storage_root)
        .with_context(|| format!("Cannot create worktree storage: {}", storage_root.display()))?;
    let target = storage_root.join(uuid::Uuid::new_v4().simple().to_string());
    std::fs::create_dir(&target)
        .with_context(|| format!("Cannot reserve worktree directory: {}", target.display()))?;
    // 保留 verbatim 路径用于文件系统操作，仅向 Git 传入其支持的盘符或 UNC 路径。
    let git_target = git_path(&target);
    let result = if let Some(branch) = branch {
        git_checked(
            cwd,
            [
                OsStr::new("worktree"),
                OsStr::new("add"),
                OsStr::new("-b"),
                OsStr::new(branch),
                OsStr::new("--"),
                git_target.as_os_str(),
                OsStr::new(&commit),
            ],
        )
    } else {
        git_checked(
            cwd,
            [
                OsStr::new("worktree"),
                OsStr::new("add"),
                OsStr::new("--detach"),
                OsStr::new("--"),
                git_target.as_os_str(),
                OsStr::new(&commit),
            ],
        )
    };
    result.with_context(|| {
        format!(
            "Cannot create worktree at {}. Any created directory or branch has been preserved",
            git_target.display()
        )
    })?;
    let target_path = target.canonicalize().with_context(|| {
        format!(
            "Created worktree at {}, but could not resolve its path; checkout was preserved",
            git_target.display()
        )
    })?;
    list(cwd).with_context(|| format!("Created worktree at {}, but could not refresh the worktree list; checkout was preserved", git_target.display()))?
        .worktrees
        .into_iter()
        .find(|tree| Path::new(&tree.path).canonicalize().is_ok_and(|path| path == target_path))
        .ok_or_else(|| anyhow!("Created worktree at {}, but could not inspect it; checkout was preserved", git_target.display()))
}

fn absolute_storage_path(path: &Path) -> Result<PathBuf> {
    if !path.is_absolute() {
        bail!("Worktree storage path must be absolute: {}", path.display());
    }
    let mut ancestor = path;
    let mut suffix = Vec::new();
    while !ancestor.exists() {
        suffix.push(
            ancestor
                .file_name()
                .context("Invalid worktree storage path")?,
        );
        ancestor = ancestor.parent().context("Invalid worktree storage path")?;
    }
    let mut resolved = ancestor.canonicalize()?;
    for component in suffix.into_iter().rev() {
        resolved.push(component);
    }
    Ok(resolved)
}

fn git_output<I, S>(cwd: &Path, args: I) -> Result<Output>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut command = Command::new("git");
    command
        .args(args)
        .current_dir(git_path(cwd))
        .stdin(Stdio::null())
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("LC_ALL", "C");
    isolate_git_environment(&mut command);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    command.output().context("Cannot run Git")
}

// 清除调用进程的仓库、索引和临时配置定位，始终以请求传入的 cwd 操作。
const LOCAL_GIT_ENVIRONMENT: &[&str] = &[
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_COMMON_DIR",
    "GIT_CONFIG",
    "GIT_CONFIG_PARAMETERS",
    "GIT_CONFIG_COUNT",
    "GIT_DIR",
    "GIT_GRAFT_FILE",
    "GIT_IMPLICIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_INTERNAL_SUPER_PREFIX",
    "GIT_NO_REPLACE_OBJECTS",
    "GIT_OBJECT_DIRECTORY",
    "GIT_PREFIX",
    "GIT_REPLACE_REF_BASE",
    "GIT_SHALLOW_FILE",
    "GIT_WORK_TREE",
];

fn isolate_git_environment(command: &mut Command) {
    for name in LOCAL_GIT_ENVIRONMENT {
        command.env_remove(name);
    }
    // COUNT 被移除后编号项已不会生效，同时移除它们，避免透传到 Git hook。
    let dynamic_keys = std::env::vars_os()
        .map(|(key, _)| key)
        .chain(command.get_envs().map(|(key, _)| key.to_os_string()))
        .filter(|key| {
            key.to_str().is_some_and(|key| {
                key.starts_with("GIT_CONFIG_KEY_") || key.starts_with("GIT_CONFIG_VALUE_")
            })
        })
        .collect::<Vec<_>>();
    for name in dynamic_keys {
        command.env_remove(name);
    }
}

/// Git for Windows 的旧版本不能识别 canonicalize 返回的 verbatim 路径。
#[cfg(windows)]
fn git_path(path: &Path) -> PathBuf {
    use std::{
        ffi::OsString,
        path::{Component, Prefix},
    };

    let mut components = path.components();
    let Some(Component::Prefix(prefix)) = components.next() else {
        return path.to_path_buf();
    };
    let mut normalized = match prefix.kind() {
        Prefix::VerbatimDisk(drive) => PathBuf::from(format!("{}:", char::from(drive))),
        Prefix::VerbatimUNC(server, share) => {
            let mut prefix = OsString::from(r"\\");
            prefix.push(server);
            prefix.push(r"\");
            prefix.push(share);
            PathBuf::from(prefix)
        }
        _ => return path.to_path_buf(),
    };
    normalized.push(components.as_path());
    normalized
}

#[cfg(not(windows))]
fn git_path(path: &Path) -> PathBuf {
    path.to_path_buf()
}

fn git_checked<I, S>(cwd: &Path, args: I) -> Result<Output>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let output = git_output(cwd, args)?;
    if !output.status.success() {
        bail!(
            "Git failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(output)
}

fn parse_listing(output: &[u8], nul_delimited: bool) -> Result<Vec<Worktree>> {
    let mut trees = Vec::new();
    let mut current: Option<Worktree> = None;
    for field in output.split(|byte| *byte == if nul_delimited { 0 } else { b'\n' }) {
        let field = if nul_delimited {
            field
        } else {
            field.strip_suffix(b"\r").unwrap_or(field)
        };
        if field.is_empty() {
            if let Some(tree) = current.take() {
                trees.push(tree);
            }
            continue;
        }
        if let Some(path) = field.strip_prefix(b"worktree ") {
            if let Some(tree) = current.take() {
                trees.push(tree);
            }
            let path = if nul_delimited {
                path.to_vec()
            } else {
                unquote_path(path)?
            };
            current = Some(Worktree {
                path: String::from_utf8(path).context("Git worktree path is not UTF-8")?,
                head: String::new(),
                branch: None,
                locked: false,
                prunable: false,
                bare: false,
            });
            continue;
        }
        let tree = current.as_mut().context("Malformed Git worktree listing")?;
        let field = std::str::from_utf8(field).context("Git worktree metadata is not UTF-8")?;
        if let Some(head) = field.strip_prefix("HEAD ") {
            tree.head = head.to_owned();
        } else if let Some(branch) = field.strip_prefix("branch ") {
            tree.branch = Some(
                branch
                    .strip_prefix("refs/heads/")
                    .unwrap_or(branch)
                    .to_owned(),
            );
        } else if field == "bare" {
            tree.bare = true;
        } else if field == "locked" || field.starts_with("locked ") {
            tree.locked = true;
        } else if field == "prunable" || field.starts_with("prunable ") {
            tree.prunable = true;
        }
    }
    if let Some(tree) = current {
        trees.push(tree);
    }
    Ok(trees)
}

/// 兼容旧版 Git porcelain 对特殊路径的 C 风格转义（含 UTF-8 字节的八进制）。
fn unquote_path(path: &[u8]) -> Result<Vec<u8>> {
    if !path.starts_with(b"\"") {
        return Ok(path.to_vec());
    }
    let path = path
        .strip_prefix(b"\"")
        .and_then(|path| path.strip_suffix(b"\""))
        .context("Invalid quoted Git worktree path")?;
    let mut decoded = Vec::with_capacity(path.len());
    let mut bytes = path.iter().copied();
    while let Some(byte) = bytes.next() {
        if byte != b'\\' {
            decoded.push(byte);
            continue;
        }
        let byte = bytes.next().context("Incomplete Git path escape")?;
        decoded.push(match byte {
            b'\\' | b'"' => byte,
            b'a' => 7,
            b'b' => 8,
            b't' => b'\t',
            b'n' => b'\n',
            b'v' => 11,
            b'f' => 12,
            b'r' => b'\r',
            b'0'..=b'3' => {
                let second = bytes
                    .next()
                    .filter(|byte| (b'0'..=b'7').contains(byte))
                    .context("Invalid Git path octal escape")?;
                let third = bytes
                    .next()
                    .filter(|byte| (b'0'..=b'7').contains(byte))
                    .context("Invalid Git path octal escape")?;
                (byte - b'0') * 64 + (second - b'0') * 8 + (third - b'0')
            }
            _ => bail!("Unknown Git path escape"),
        });
    }
    Ok(decoded)
}

#[cfg(test)]
#[path = "worktree_test.rs"]
mod tests;
