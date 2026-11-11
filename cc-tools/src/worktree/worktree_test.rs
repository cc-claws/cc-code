use super::*;

struct Repository {
    _temporary: tempfile::TempDir,
    path: PathBuf,
    storage: PathBuf,
}

impl Repository {
    fn new() -> Result<Self> {
        let temporary = tempfile::tempdir()?;
        let path = temporary.path().join("项目 with spaces");
        let storage = temporary.path().join("独立 checkout storage");
        std::fs::create_dir(&path)?;
        git_checked(&path, ["init"])?;
        git_checked(&path, ["config", "user.name", "Worktree Test"])?;
        git_checked(
            &path,
            ["config", "user.email", "worktree-test@example.invalid"],
        )?;
        // 隔离用户签名、换行和 hooks 配置，测试只改动临时仓库。
        git_checked(&path, ["config", "commit.gpgSign", "false"])?;
        git_checked(&path, ["config", "core.autocrlf", "false"])?;
        git_checked(&path, ["config", "core.hooksPath", ".git/hooks"])?;
        std::fs::write(path.join("tracked.txt"), "committed\n")?;
        git_checked(&path, ["add", "tracked.txt"])?;
        git_checked(&path, ["commit", "-m", "Initial test commit"])?;
        Ok(Self {
            _temporary: temporary,
            path,
            storage,
        })
    }

    fn output(&self, args: &[&str]) -> Result<String> {
        Ok(String::from_utf8(git_checked(&self.path, args)?.stdout)?
            .trim_end()
            .to_owned())
    }
}

#[test]
fn detached_creation_preserves_dirty_source() -> Result<()> {
    let repository = Repository::new()?;
    let head = repository.output(&["rev-parse", "HEAD"])?;
    let branch = repository.output(&["symbolic-ref", "HEAD"])?;
    std::fs::write(repository.path.join("tracked.txt"), "uncommitted\n")?;
    std::fs::write(repository.path.join("untracked.txt"), "keep me\n")?;
    let status = repository.output(&["status", "--porcelain"])?;

    let tree = create(&repository.path, None, None, &repository.storage)?;

    assert!(tree.branch.is_none(), "默认创建 detached HEAD");
    assert_eq!(tree.head, head, "新 checkout 应从当前提交创建");
    assert_eq!(
        std::fs::read_to_string(Path::new(&tree.path).join("tracked.txt"))?,
        "committed\n"
    );
    assert!(!Path::new(&tree.path).join("untracked.txt").exists());
    assert_eq!(
        repository.output(&["status", "--porcelain"])?,
        status,
        "脏文件应保持原样"
    );
    assert_eq!(repository.output(&["symbolic-ref", "HEAD"])?, branch);
    assert_eq!(repository.output(&["rev-parse", "HEAD"])?, head);
    assert_eq!(
        std::fs::read_to_string(repository.path.join("untracked.txt"))?,
        "keep me\n"
    );
    let listing = list(&repository.path)?;
    assert_eq!(listing.worktrees.len(), 2);
    assert!(listing
        .worktrees
        .iter()
        .any(|entry| entry.path == tree.path));
    Ok(())
}

#[test]
fn creates_branch_and_another_checkout_from_linked_worktree() -> Result<()> {
    let repository = Repository::new()?;
    let head = repository.output(&["rev-parse", "HEAD"])?;
    let named = create(
        &repository.path,
        Some("feature/worktree-test"),
        Some(&head),
        &repository.storage,
    )?;
    assert_eq!(named.branch.as_deref(), Some("feature/worktree-test"));
    assert_eq!(
        repository.output(&["rev-parse", "refs/heads/feature/worktree-test"])?,
        head
    );

    let detached = create(Path::new(&named.path), None, None, &repository.storage)?;

    assert_eq!(detached.head, named.head);
    assert!(detached.branch.is_none());
    let listing = list(Path::new(&named.path))?;
    assert_eq!(
        Path::new(&listing.repository_root).canonicalize()?,
        Path::new(&named.path).canonicalize()?
    );
    assert_eq!(listing.worktrees.len(), 3);
    assert!(Path::new(&detached.path).is_dir());
    assert!(Path::new(&detached.path)
        .parent()
        .unwrap()
        .ends_with("独立 checkout storage"));
    Ok(())
}

#[test]
fn invalid_inputs_and_existing_branch_leave_source_untouched() -> Result<()> {
    let repository = Repository::new()?;
    let original_branch = repository.output(&["symbolic-ref", "--short", "HEAD"])?;
    let head = repository.output(&["rev-parse", "HEAD"])?;
    for branch in ["", "-invalid", "bad branch", "a..b", "refs/heads/", "@{-1}"] {
        assert!(
            create(&repository.path, Some(branch), None, &repository.storage).is_err(),
            "应拒绝分支名 {branch:?}"
        );
    }
    for base_ref in ["", "-invalid", " HEAD", "missing-revision"] {
        assert!(
            create(&repository.path, None, Some(base_ref), &repository.storage).is_err(),
            "应拒绝起始提交 {base_ref:?}"
        );
    }
    assert!(create(
        &repository.path,
        Some(&original_branch),
        None,
        &repository.storage
    )
    .is_err());
    assert!(!repository.storage.exists(), "校验失败不应创建目录");
    assert_eq!(repository.output(&["rev-parse", "HEAD"])?, head);
    assert_eq!(
        repository.output(&["symbolic-ref", "--short", "HEAD"])?,
        original_branch
    );
    assert!(repository.output(&["status", "--porcelain"])?.is_empty());
    assert_eq!(list(&repository.path)?.worktrees.len(), 1);
    Ok(())
}

#[test]
fn storage_inside_current_checkout_is_allowed_and_excluded_from_status() -> Result<()> {
    let repository = Repository::new()?;
    let storage = repository.path.join(".claude/worktrees");

    let created = create(&repository.path, None, None, &storage)?;

    assert!(Path::new(&created.path).starts_with(&storage));
    // 托管目录位于仓库内时必须自动加入 .git/info/exclude，否则 git status 被污染
    assert!(
        repository.output(&["status", "--porcelain"])?.is_empty(),
        "仓库内的托管目录不应出现在 git status 中"
    );
    let exclude = std::fs::read_to_string(repository.path.join(".git/info/exclude"))?;
    assert!(
        exclude.contains("/.claude/worktrees/"),
        "unexpected exclude: {exclude}"
    );
    Ok(())
}

#[test]
fn failed_storage_creation_preserves_existing_file() -> Result<()> {
    let repository = Repository::new()?;
    std::fs::write(&repository.storage, "existing user data")?;
    let error = create(
        &repository.path,
        Some("feature/untouched"),
        None,
        &repository.storage,
    )
    .unwrap_err();
    assert!(error.to_string().contains("Cannot create worktree storage"));
    assert_eq!(
        std::fs::read_to_string(&repository.storage)?,
        "existing user data"
    );
    assert!(!git_output(
        &repository.path,
        [
            "show-ref",
            "--verify",
            "--quiet",
            "refs/heads/feature/untouched"
        ]
    )?
    .status
    .success());
    assert_eq!(list(&repository.path)?.worktrees.len(), 1);
    Ok(())
}

#[test]
fn failed_checkout_hook_preserves_created_directory_and_branch() -> Result<()> {
    let repository = Repository::new()?;
    let original_head = repository.output(&["rev-parse", "HEAD"])?;
    let hook_directory = repository.path.join(".git/hooks");
    git_checked(
        &repository.path,
        [
            OsStr::new("config"),
            OsStr::new("core.hooksPath"),
            hook_directory.as_os_str(),
        ],
    )?;
    let hook = hook_directory.join("post-checkout");
    std::fs::write(&hook, "#!/bin/sh\nexit 1\n")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755))?;
    }

    let error = create(
        &repository.path,
        Some("feature/hook-failed"),
        None,
        &repository.storage,
    )
    .unwrap_err();

    let entries = std::fs::read_dir(&repository.storage)?.collect::<std::io::Result<Vec<_>>>()?;
    assert_eq!(entries.len(), 1, "Git 创建失败后的目录必须保留");
    let checkout = entries[0].path();
    assert!(error.to_string().contains(&checkout.display().to_string()));
    assert!(error.to_string().contains("preserved"));
    assert!(checkout.join("tracked.txt").is_file());
    assert_eq!(
        repository.output(&["rev-parse", "refs/heads/feature/hook-failed"])?,
        original_head,
        "已创建的分支不能自动删除"
    );
    assert_eq!(repository.output(&["rev-parse", "HEAD"])?, original_head);
    assert!(repository.output(&["status", "--porcelain"])?.is_empty());
    assert_eq!(list(&repository.path)?.worktrees.len(), 2);
    Ok(())
}

#[test]
fn core_worktree_configuration_is_rejected_without_touching_dirty_source() -> Result<()> {
    let repository = Repository::new()?;
    git_checked(
        &repository.path,
        [
            OsStr::new("config"),
            OsStr::new("core.worktree"),
            repository.path.as_os_str(),
        ],
    )?;
    std::fs::write(repository.path.join("tracked.txt"), "dirty original\n")?;
    std::fs::write(repository.path.join("untracked.txt"), "keep original\n")?;
    let before = repository.output(&["status", "--porcelain"])?;
    let head = repository.output(&["rev-parse", "HEAD"])?;

    let error = create(&repository.path, None, None, &repository.storage).unwrap_err();

    assert!(error.to_string().contains("core.worktree"));
    assert!(error.to_string().contains("not supported"));
    assert!(
        !repository.storage.exists(),
        "不支持的配置应在分配目录前拒绝"
    );
    assert_eq!(repository.output(&["status", "--porcelain"])?, before);
    assert_eq!(repository.output(&["rev-parse", "HEAD"])?, head);
    assert_eq!(
        std::fs::read_to_string(repository.path.join("tracked.txt"))?,
        "dirty original\n"
    );
    assert_eq!(
        std::fs::read_to_string(repository.path.join("untracked.txt"))?,
        "keep original\n"
    );
    Ok(())
}

#[test]
fn git_command_removes_injected_repository_environment() -> Result<()> {
    let repository = Repository::new()?;
    let mut command = Command::new("git");
    command
        .current_dir(&repository.path)
        .args(["status", "--porcelain"]);
    for key in LOCAL_GIT_ENVIRONMENT {
        command.env(key, "invalid inherited repository configuration");
    }
    command.env("GIT_CONFIG_KEY_0", "core.worktree");
    command.env("GIT_CONFIG_VALUE_0", "invalid checkout");

    isolate_git_environment(&mut command);

    for (key, value) in command.get_envs() {
        if LOCAL_GIT_ENVIRONMENT
            .iter()
            .any(|name| key == OsStr::new(name))
            || key == OsStr::new("GIT_CONFIG_KEY_0")
            || key == OsStr::new("GIT_CONFIG_VALUE_0")
        {
            assert!(value.is_none(), "命令应显式移除仓库定位环境变量 {key:?}");
        }
    }
    let output = command.output()?;
    assert!(
        output.status.success(),
        "清除继承环境后应操作 cwd 仓库: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stdout.is_empty(),
        "不得因继承索引或工作目录而改变状态"
    );
    Ok(())
}

#[test]
fn nul_listing_preserves_special_paths_and_worktree_states() -> Result<()> {
    let output = concat!(
        "worktree /repo 主树\0HEAD abc123\0branch refs/heads/main\0\0",
        "worktree /checkout with \"quotes\"\nand newline\0HEAD def456\0detached\0locked maintenance\0\0",
        "worktree /missing checkout\0HEAD fed987\0branch refs/heads/feature/new\0prunable gitdir missing\0\0",
        "worktree /bare.git\0bare\0\0"
    );
    let trees = parse_listing(output.as_bytes(), true)?;
    assert_eq!(trees.len(), 4);
    assert_eq!(trees[0].path, "/repo 主树");
    assert_eq!(trees[0].branch.as_deref(), Some("main"));
    assert_eq!(trees[1].path, "/checkout with \"quotes\"\nand newline");
    assert!(trees[1].locked);
    assert!(trees[1].branch.is_none());
    assert!(trees[2].prunable);
    assert_eq!(trees[2].branch.as_deref(), Some("feature/new"));
    assert!(trees[3].bare);
    assert!(trees[3].head.is_empty());
    Ok(())
}

#[test]
fn legacy_listing_decodes_quoted_paths() -> Result<()> {
    let output = b"worktree \"/repo \\344\\270\\255\\346\\226\\207\\tquoted\\\"\\\\path\"\nHEAD abc123\ndetached\nlocked\nprunable\n\n";
    let trees = parse_listing(output, false)?;
    assert_eq!(trees.len(), 1);
    assert_eq!(trees[0].path, "/repo 中文\tquoted\"\\path");
    assert!(trees[0].locked);
    assert!(trees[0].prunable);
    Ok(())
}

#[cfg(windows)]
#[test]
fn converts_verbatim_disk_and_unc_paths_for_git() {
    for (original, expected) in [
        (
            r"\\?\D:\项目 with spaces\checkout",
            r"D:\项目 with spaces\checkout",
        ),
        (r"\\?\D:\", r"D:\"),
        (
            r"\\?\UNC\server\share\项目 with spaces\checkout",
            r"\\server\share\项目 with spaces\checkout",
        ),
        (r"\\?\UNC\server\share\", r"\\server\share\"),
        (r"D:\checkout", r"D:\checkout"),
        (r"\\server\share\checkout", r"\\server\share\checkout"),
        (r"relative\checkout", r"relative\checkout"),
        (r"\\.\PhysicalDrive0", r"\\.\PhysicalDrive0"),
    ] {
        assert_eq!(
            git_path(Path::new(original)),
            PathBuf::from(expected),
            "应只转换已知的 verbatim 盘符和 UNC 前缀"
        );
    }
}

#[cfg(windows)]
#[test]
fn accepts_verbatim_source_and_storage_paths() -> Result<()> {
    let repository = Repository::new()?;
    let cwd = repository.path.canonicalize()?;
    let storage = repository
        ._temporary
        .path()
        .canonicalize()?
        .join("checkout storage");

    let tree = create(&cwd, None, None, &storage)?;

    assert!(Path::new(&tree.path).is_dir());
    assert_eq!(list(&cwd)?.worktrees.len(), 2);
    Ok(())
}

#[test]
fn count_changes_reports_clean_worktree() -> Result<()> {
    let repository = Repository::new()?;
    let head = repository.output(&["rev-parse", "HEAD"])?;
    let tree = create(&repository.path, None, None, &repository.storage)?;

    let changes = count_changes(Path::new(&tree.path), &head).expect("应能统计变更");

    assert_eq!(changes.uncommitted_files, 0);
    assert_eq!(changes.new_commits, 0);
    assert!(!changes.is_dirty());
    Ok(())
}

#[test]
fn count_changes_detects_uncommitted_and_new_commits() -> Result<()> {
    let repository = Repository::new()?;
    let head = repository.output(&["rev-parse", "HEAD"])?;
    let tree = create(&repository.path, None, None, &repository.storage)?;
    let checkout = Path::new(&tree.path);

    std::fs::write(checkout.join("tracked.txt"), "dirty\n")?;
    let dirty_only = count_changes(checkout, &head).expect("应能统计变更");
    assert_eq!(dirty_only.uncommitted_files, 1);
    assert_eq!(dirty_only.new_commits, 0);

    git_checked(checkout, ["add", "tracked.txt"])?;
    git_checked(checkout, ["commit", "-m", "Worktree change"])?;
    let committed = count_changes(checkout, &head).expect("应能统计变更");
    assert_eq!(committed.uncommitted_files, 0);
    assert_eq!(committed.new_commits, 1);
    assert!(committed.is_dirty());
    Ok(())
}

#[test]
fn count_changes_returns_none_for_unknown_base() -> Result<()> {
    let repository = Repository::new()?;
    let tree = create(&repository.path, None, None, &repository.storage)?;

    assert!(count_changes(Path::new(&tree.path), "").is_none());
    assert!(count_changes(
        Path::new(&tree.path),
        "0000000000000000000000000000000000000000"
    )
    .is_none());
    Ok(())
}

#[test]
fn remove_rejects_paths_outside_storage() -> Result<()> {
    let repository = Repository::new()?;
    let error = remove(
        &repository.path,
        &repository.path,
        None,
        &repository.storage,
    )
    .expect_err("不得删除托管目录之外的 checkout");

    assert!(
        error
            .to_string()
            .contains("outside the managed worktree storage"),
        "unexpected error: {error}"
    );
    assert!(
        repository.path.join("tracked.txt").exists(),
        "原工作区必须完好"
    );
    Ok(())
}

#[test]
fn remove_deletes_managed_checkout() -> Result<()> {
    let repository = Repository::new()?;
    let tree = create(&repository.path, None, None, &repository.storage)?;

    remove(
        &repository.path,
        Path::new(&tree.path),
        None,
        &repository.storage,
    )?;

    assert!(!Path::new(&tree.path).exists(), "worktree 目录应被删除");
    let listing = list(&repository.path)?;
    assert_eq!(listing.worktrees.len(), 1, "只剩主工作区");
    Ok(())
}

#[test]
fn create_named_reuses_existing_checkout() -> Result<()> {
    let repository = Repository::new()?;

    let first = create_named(
        &repository.path,
        "feature-x",
        Some("worktree/feature-x"),
        None,
        &repository.storage,
    )?;
    let second = create_named(
        &repository.path,
        "feature-x",
        Some("worktree/feature-x"),
        None,
        &repository.storage,
    )?;

    assert_eq!(first.path, second.path, "同名创建必须复用同一个 checkout");
    assert_eq!(second.branch.as_deref(), Some("worktree/feature-x"));
    assert_eq!(
        list(&repository.path)?.worktrees.len(),
        2,
        "不应多出一个 checkout"
    );
    Ok(())
}

#[test]
fn create_named_rejects_directory_not_belonging_to_repository() -> Result<()> {
    let repository = Repository::new()?;
    std::fs::create_dir_all(repository.storage.join("occupied"))?;

    let error = create_named(
        &repository.path,
        "occupied",
        None,
        None,
        &repository.storage,
    )
    .expect_err("已存在的非 worktree 目录必须拒绝");

    assert!(
        error.to_string().contains("already exists"),
        "unexpected error: {error}"
    );
    Ok(())
}

#[test]
fn create_named_rejects_invalid_name() -> Result<()> {
    let repository = Repository::new()?;

    assert!(create_named(&repository.path, "", None, None, &repository.storage).is_err());
    assert!(create_named(&repository.path, "-bad", None, None, &repository.storage).is_err());
    assert!(create_named(
        &repository.path,
        " padded ",
        None,
        None,
        &repository.storage
    )
    .is_err());
    Ok(())
}
