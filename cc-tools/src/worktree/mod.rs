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
    create_at(cwd, None, branch, base_ref, storage_root)
}

/// 按名称创建（或复用）worktree。
///
/// 名称直接作为托管目录下的目录名，因此同名重复进入会**复用**已有 checkout，
/// 而不是再建一个新的（对齐参考实现的 `getOrCreateWorktree` 快速恢复路径）。
pub fn create_named(
    cwd: &Path,
    name: &str,
    branch: Option<&str>,
    base_ref: Option<&str>,
    storage_root: &Path,
) -> Result<Worktree> {
    if name.is_empty() || name.starts_with('-') || name.trim() != name {
        bail!("Invalid worktree name: {name:?}");
    }
    create_at(cwd, Some(name), branch, base_ref, storage_root)
}

fn create_at(
    cwd: &Path,
    name: Option<&str>,
    branch: Option<&str>,
    base_ref: Option<&str>,
    storage_root: &Path,
) -> Result<Worktree> {
    let existing = list(cwd)?;
    // 先定目标目录：命名创建时同名目录直接复用（参考实现的快速恢复路径），
    // 这样后续的分支校验、core.worktree 校验都不会把「复用」误判成「新建冲突」。
    let storage_root = absolute_storage_path(storage_root)?;
    let target = match name {
        Some(name) => storage_root.join(name),
        None => storage_root.join(uuid::Uuid::new_v4().simple().to_string()),
    };
    if target.exists() {
        if let Some(reuse) = existing.worktrees.iter().find(|tree| {
            Path::new(&tree.path)
                .canonicalize()
                .is_ok_and(|path| path == target)
        }) {
            return Ok(reuse.clone());
        }
        bail!(
            "Worktree path already exists but is not a checkout of this repository: {}",
            target.display()
        );
    }

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
    // 托管目录可以位于**当前** checkout 内（参考实现就放在仓库内），但不得
    // 落在其他 checkout 里——那会污染别人的工作区。
    let current_root = Path::new(&existing.repository_root).canonicalize().ok();
    for worktree in existing.worktrees.iter().filter(|tree| !tree.bare) {
        if let Ok(path) = Path::new(&worktree.path).canonicalize() {
            let is_current = current_root.as_ref().is_some_and(|root| root == &path);
            if !is_current && storage_root.starts_with(&path) {
                bail!(
                    "Worktree storage must not be inside another checkout: {}",
                    storage_root.display()
                );
            }
        }
    }
    std::fs::create_dir_all(&storage_root)
        .with_context(|| format!("Cannot create worktree storage: {}", storage_root.display()))?;
    // 位于仓库内时把托管目录写进 `.git/info/exclude`（目录已存在，路径可规范化），
    // 否则 `git status` 会把这个目录当成未跟踪内容。
    if current_root
        .as_ref()
        .is_some_and(|root| storage_root.starts_with(root))
    {
        exclude_from_git(cwd, &storage_root);
    }
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

/// worktree 相对基准提交的变更统计。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeChanges {
    /// 未提交（含未跟踪）的文件数。
    pub uncommitted_files: usize,
    /// 相对基准提交新增的提交数。
    pub new_commits: usize,
}

impl WorktreeChanges {
    /// 是否有任何需要人工确认的变更。
    pub fn is_dirty(&self) -> bool {
        self.uncommitted_files > 0 || self.new_commits > 0
    }
}

/// 统计 worktree 相对基准提交的变更。
///
/// 返回 `None` 表示「无法确定」（Git 调用失败、基准提交缺失等）。调用方必须
/// 按「未知 = 不安全」处理——宁可让用户手工清理，也不冒险丢弃工作。
pub fn count_changes(worktree_path: &Path, base_commit: &str) -> Option<WorktreeChanges> {
    if base_commit.trim().is_empty() {
        return None;
    }
    let status = git_output(worktree_path, ["status", "--porcelain"]).ok()?;
    if !status.status.success() {
        return None;
    }
    let uncommitted_files = String::from_utf8_lossy(&status.stdout)
        .lines()
        .filter(|line| !line.trim().is_empty())
        .count();
    let range = format!("{base_commit}..HEAD");
    let commits = git_output(worktree_path, ["rev-list", "--count", &range]).ok()?;
    if !commits.status.success() {
        return None;
    }
    let new_commits = String::from_utf8_lossy(&commits.stdout)
        .trim()
        .parse()
        .ok()?;
    Some(WorktreeChanges {
        uncommitted_files,
        new_commits,
    })
}

/// 删除一个由本工具创建的 worktree，可选连带删除其分支。
///
/// 只接受位于 `storage_root` 之下的路径：托管目录以外的 checkout（含用户主
/// 工作区）一律拒绝，避免把「删除 worktree」误用成「删仓库」。
pub fn remove(
    repo: &Path,
    worktree_path: &Path,
    branch: Option<&str>,
    storage_root: &Path,
) -> Result<()> {
    let storage = absolute_storage_path(storage_root)?;
    let target = worktree_path
        .canonicalize()
        .with_context(|| format!("Cannot resolve worktree path: {}", worktree_path.display()))?;
    if !target.starts_with(&storage) {
        bail!(
            "Refusing to remove a checkout outside the managed worktree storage: {}",
            target.display()
        );
    }
    let git_target = git_path(&target);
    git_checked(
        repo,
        [
            OsStr::new("worktree"),
            OsStr::new("remove"),
            OsStr::new("--force"),
            OsStr::new("--"),
            git_target.as_os_str(),
        ],
    )
    .with_context(|| {
        format!(
            "Cannot remove worktree at {}; checkout and branch were preserved",
            git_target.display()
        )
    })?;
    // 只删工具自己创建的分支：复用路径可能返回用户手工检出的 checkout，
    // 它的分支（如 `main`）不属于本工具，不能连带删除。
    //
    // 分支删除失败**不能**让整个 remove 变成失败：上面的 checkout 已经删掉了，
    // 返回 Err 会让调用方按「整体失败」处理并跳过必须做的状态复位（会话目录仍
    // 指向已删除的路径），把一次「残留分支」升级成会话卡死。分支名可能在
    // worktree 内被改名，此处失败是意料之中的情形。
    if let Some(branch) = branch {
        let managed = branch.strip_prefix("worktree/").is_some();
        if managed && !branch.starts_with('-') && branch.trim() == branch {
            if let Err(error) = git_checked(
                repo,
                [
                    OsStr::new("branch"),
                    OsStr::new("-D"),
                    OsStr::new("--"),
                    OsStr::new(branch),
                ],
            ) {
                let detail = format!("{error:#}");
                tracing::warn!(
                    branch,
                    error = %detail,
                    "worktree 已删除，但残留分支未能清理（可能是分支被改名）"
                );
            }
        }
    }
    Ok(())
}

/// 换行符常量（避免字符字面量在不同 shell 转义层里被写坏）。
const NEWLINE: char = '\n';

/// 把仓库内的托管目录加入 `.git/info/exclude`（幂等，失败不影响创建）。
fn exclude_from_git(cwd: &Path, storage_root: &Path) {
    let Ok(path_output) = git_checked(cwd, ["rev-parse", "--git-path", "info/exclude"]) else {
        return;
    };
    let exclude_path = String::from_utf8_lossy(&path_output.stdout)
        .trim()
        .to_string();
    if exclude_path.is_empty() {
        return;
    }
    // `--git-path` 可能返回相对路径（相对仓库根），必须显式拼回绝对路径，
    // 否则会写到进程当前目录下的 `info/exclude`。
    let exclude_path = {
        let candidate = Path::new(&exclude_path);
        if candidate.is_absolute() {
            candidate.to_path_buf()
        } else {
            cwd.join(candidate)
        }
    };
    let exclude_path = exclude_path.as_path();
    let Ok(root_output) = git_checked(cwd, ["rev-parse", "--show-toplevel"]) else {
        return;
    };
    let root = String::from_utf8_lossy(&root_output.stdout)
        .trim()
        .to_string();
    let root_path = Path::new(&root)
        .canonicalize()
        .unwrap_or_else(|_| Path::new(&root).to_path_buf());
    let storage_path = storage_root
        .canonicalize()
        .unwrap_or_else(|_| storage_root.to_path_buf());
    let relative = storage_path
        .strip_prefix(&root_path)
        .map(|path| {
            path.to_string_lossy()
                .replace(std::path::MAIN_SEPARATOR, "/")
        })
        .unwrap_or_else(|_| {
            storage_root
                .to_string_lossy()
                .replace(std::path::MAIN_SEPARATOR, "/")
        });
    let entry = format!("/{}/", relative.trim_matches('/'));
    let existing = std::fs::read_to_string(exclude_path).unwrap_or_default();
    if existing.lines().any(|line| line.trim() == entry) {
        return;
    }
    if let Some(dir) = exclude_path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let mut updated = existing;
    if !updated.is_empty() && !updated.ends_with(NEWLINE) {
        updated.push(NEWLINE);
    }
    updated.push_str(&entry);
    updated.push(NEWLINE);
    let _ = std::fs::write(exclude_path, updated);
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
