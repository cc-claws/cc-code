use super::*;
use cc_agent::session_cwd::new_session_cwd;

/// 测试专用：把会话登记文件指到临时目录，避免覆盖开发者本地登记。
///
/// 复用调用方传入的同一个 `SessionCwd`——工具与测试必须共享同一个句柄，
/// 否则目录切换在两边看到的不一致。
fn isolated_pair(
    handle: &cc_agent::session_cwd::SessionCwd,
    sessions: &std::path::Path,
) -> (EnterWorktreeTool, ExitWorktreeTool) {
    let file = sessions.join("worktree_sessions.json");
    (
        EnterWorktreeTool::new(Arc::clone(handle), None).with_sessions_file(&file),
        ExitWorktreeTool::new(Arc::clone(handle), None).with_sessions_file(&file),
    )
}

fn tool_pair(cwd: &str) -> (EnterWorktreeTool, ExitWorktreeTool) {
    let handle = new_session_cwd(cwd);
    (
        EnterWorktreeTool::new(Arc::clone(&handle), None),
        ExitWorktreeTool::new(handle, None),
    )
}

#[test]
fn test_validate_name_allows_documented_characters() {
    assert_eq!(validate_name("fix-auth").as_deref(), Ok("fix-auth"));
    assert_eq!(validate_name("a/b.c_d").as_deref(), Ok("a/b.c_d"));
    assert_eq!(validate_name(" padded ").as_deref(), Ok("padded"));
}

#[test]
fn test_validate_name_rejects_path_escape_and_empty_segments() {
    assert!(validate_name("../escape").is_err());
    assert!(validate_name("a//b").is_err());
    assert!(validate_name(".").is_err());
    assert!(validate_name("..").is_err());
    assert!(validate_name("").is_err());
}

#[test]
fn test_validate_name_rejects_illegal_characters_and_long_names() {
    assert!(validate_name("has space").is_err());
    assert!(validate_name("semi;colon").is_err());
    assert!(validate_name(&"a".repeat(65)).is_err());
    assert!(validate_name(&"a".repeat(64)).is_ok());
}

#[tokio::test]
async fn test_enter_worktree_rejects_invalid_name_before_touching_git() {
    let (enter, _exit) = tool_pair("/definitely/not/a/repository");

    let error = enter
        .invoke(serde_json::json!({"name": "../escape"}))
        .await
        .expect_err("非法名称必须拒绝");

    assert!(
        error.to_string().contains("worktree name"),
        "unexpected error: {error}"
    );
}

#[tokio::test]
async fn test_enter_worktree_rejects_nested_session() {
    let storage = storage_root("/tmp");
    let nested = storage.join("already-here");
    let (enter, _exit) = tool_pair(&nested.to_string_lossy());

    let error = enter
        .invoke(serde_json::json!({"name": "x"}))
        .await
        .expect_err("已在托管 worktree 内时必须拒绝嵌套");

    assert!(
        error
            .to_string()
            .contains("Already inside a managed worktree"),
        "unexpected error: {error}"
    );
}

#[tokio::test]
async fn test_exit_worktree_outside_session_is_noop() {
    let (_enter, exit) = tool_pair("/definitely/not/a/worktree");

    let output = exit
        .invoke(serde_json::json!({"action": "keep"}))
        .await
        .expect("会话外调用必须是 no-op 而不是报错");

    assert!(output.contains("No active worktree session"));
    assert!(output.contains("Nothing was changed"));
}

#[tokio::test]
async fn test_exit_worktree_requires_action_and_rejects_unknown_value() {
    let directory = tempfile::tempdir().unwrap();
    let sessions = directory.path().join("worktree_sessions.json");
    let handle = new_session_cwd("/some/worktree");
    persist_session(
        Some(&sessions),
        "/some/worktree",
        &WorktreeSession {
            original_cwd: "/repo".to_string(),
            base_commit: "abc".to_string(),
            branch: Some("worktree/x".to_string()),
            hook_based: false,
        },
    );
    let exit = ExitWorktreeTool::new(handle, None).with_sessions_file(&sessions);

    let missing = exit
        .invoke(serde_json::json!({}))
        .await
        .expect_err("缺少 action 必须报错");
    assert!(missing
        .to_string()
        .contains("Missing required parameter: action"));

    let invalid = exit
        .invoke(serde_json::json!({"action": "delete"}))
        .await
        .expect_err("未知 action 必须报错");
    assert!(invalid.to_string().contains("Invalid action"));
}

#[tokio::test]
async fn test_exit_worktree_keep_restores_directory() {
    let directory = tempfile::tempdir().unwrap();
    let sessions = directory.path().join("worktree_sessions.json");
    let handle = new_session_cwd("/repo");
    let exit = ExitWorktreeTool::new(Arc::clone(&handle), None).with_sessions_file(&sessions);
    let worktree = "/managed/worktree";
    session_cwd::switch(&handle, worktree);
    persist_session(
        Some(&sessions),
        worktree,
        &WorktreeSession {
            original_cwd: "/repo".to_string(),
            base_commit: "abc".to_string(),
            branch: Some("worktree/x".to_string()),
            hook_based: false,
        },
    );

    let output = exit
        .invoke(serde_json::json!({"action": "keep"}))
        .await
        .expect("keep 不应删除任何东西");

    assert!(output.contains("Kept worktree (branch worktree/x)"));
    assert!(output.contains("Returned to /repo"));
    assert_eq!(session_cwd::current(&handle), "/repo");
    assert!(
        load_sessions(Some(&sessions)).is_empty(),
        "会话登记应被清理"
    );
}

fn init_repo(path: &std::path::Path) {
    let git = |args: &[&str]| {
        std::process::Command::new("git")
            .args(args)
            .current_dir(path)
            .output()
            .expect("git 必须可用")
    };
    git(&["init", "-q", "."]);
    git(&["config", "user.email", "worktree-test@example.invalid"]);
    git(&["config", "user.name", "Worktree Test"]);
    git(&["config", "commit.gpgSign", "false"]);
    std::fs::write(path.join("tracked.txt"), "committed\n").unwrap();
    git(&["add", "tracked.txt"]);
    git(&["commit", "-qm", "initial"]);
}

#[tokio::test]
async fn test_enter_and_exit_worktree_round_trip() {
    let directory = tempfile::tempdir().unwrap();
    let repository = directory.path();
    init_repo(repository);

    let handle = new_session_cwd(repository.to_string_lossy().to_string());
    let (enter, exit) = isolated_pair(&handle, directory.path());

    let entered = enter
        .invoke(serde_json::json!({"name": "round-trip"}))
        .await
        .expect("创建 worktree 应成功");
    assert!(
        entered.contains("worktree/round-trip"),
        "unexpected: {entered}"
    );
    let worktree = session_cwd::current(&handle);
    assert!(Path::new(&worktree).is_dir(), "worktree 目录必须存在");
    assert!(
        worktree
            .replace(std::path::MAIN_SEPARATOR, "/")
            .contains(".claude/worktrees/round-trip"),
        "托管目录应位于仓库内 .claude/worktrees：{worktree}"
    );

    let exited = exit
        .invoke(serde_json::json!({"action": "remove"}))
        .await
        .expect("删除 worktree 应成功");
    assert!(exited.contains("Removed worktree"), "unexpected: {exited}");
    assert!(exited.contains("Returned to"));
    assert_eq!(
        session_cwd::current(&handle),
        repository.to_string_lossy().to_string(),
        "退出后必须回到原目录"
    );
    assert!(!Path::new(&worktree).exists(), "remove 必须删掉 checkout");
}

#[tokio::test]
async fn test_enter_worktree_reuses_same_name() {
    let directory = tempfile::tempdir().unwrap();
    let repository = directory.path();
    init_repo(repository);

    let handle = new_session_cwd(repository.to_string_lossy().to_string());
    let (enter, exit) = isolated_pair(&handle, directory.path());

    enter
        .invoke(serde_json::json!({"name": "again"}))
        .await
        .expect("首次创建应成功");
    let first = session_cwd::current(&handle);
    exit.invoke(serde_json::json!({"action": "keep"}))
        .await
        .expect("keep 应成功");

    let second_output = enter
        .invoke(serde_json::json!({"name": "again"}))
        .await
        .expect("同名再次进入应复用而不是失败");
    assert!(
        second_output.starts_with("Reusing worktree"),
        "unexpected: {second_output}"
    );
    assert_eq!(session_cwd::current(&handle), first, "复用的应是同一个目录");
}
