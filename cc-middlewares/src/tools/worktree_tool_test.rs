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
    git(&["config", "core.hooksPath", ".git/hooks"]);
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

fn make_live_file_tools(handle: &SessionCwd, cwd: &str) -> Vec<Box<dyn BaseTool>> {
    use crate::middleware::FilesystemMiddleware;
    use cc_agent::{agent::state::AgentState, middleware::Middleware};
    let middleware = FilesystemMiddleware::new().with_session_cwd(Arc::clone(handle));
    <FilesystemMiddleware as Middleware<AgentState>>::collect_tools(&middleware, cwd)
}

fn make_file_tool<'a>(tools: &'a [Box<dyn BaseTool>], name: &str) -> &'a dyn BaseTool {
    tools
        .iter()
        .find(|tool| tool.name() == name)
        .expect("文件工具必须被中间件注入")
        .as_ref()
}

#[tokio::test]
async fn test_worktree_before_model_syncs_state_cwd_after_each_switch() {
    use cc_agent::{
        agent::state::{AgentState, State},
        middleware::Middleware,
    };
    let handle = new_session_cwd("original-directory");
    let middleware = WorktreeMiddleware::new(Arc::clone(&handle), None);
    let mut state = AgentState::new("original-directory");
    for cwd in ["worktree-directory", "original-directory"] {
        session_cwd::switch(&handle, cwd);
        middleware.before_model(&mut state).await.unwrap();
        assert_eq!(state.cwd(), cwd, "每个后续 step 的审批必须使用当前目录");
    }
}

#[tokio::test]
async fn test_same_file_tools_follow_enter_and_removed_worktree() {
    use crate::middleware::FilesystemMiddleware;
    let directory = tempfile::tempdir().unwrap();
    let repository = directory.path();
    init_repo(repository);
    std::fs::write(
        repository.join("original-only.txt"),
        "original-search-token\n",
    )
    .unwrap();
    std::fs::create_dir(repository.join("nested")).unwrap();
    std::fs::write(repository.join("nested/original.txt"), "original-nested\n").unwrap();
    let initial = repository.to_string_lossy().into_owned();
    let handle = new_session_cwd(&initial);
    let tools = make_live_file_tools(&handle, &initial);
    let static_tools =
        FilesystemMiddleware::build_tools(&initial, cc_agent::kernel::MutationForm::default());
    let (enter, exit) = isolated_pair(&handle, repository);
    enter
        .invoke(serde_json::json!({"name": "live-file-tools"}))
        .await
        .expect("进入临时 worktree");
    let worktree = PathBuf::from(session_cwd::current(&handle));
    let bash = cc_tools::bash::BashTool::new(worktree.to_string_lossy())
        .with_session_cwd(Arc::clone(&handle));
    bash.invoke(serde_json::json!({"command": "echo before-exit", "timeout": 5000}))
        .await
        .expect("在 worktree 中构建的同一 BashTool 应能正常执行");
    std::fs::write(worktree.join("tracked.txt"), "worktree-before\n").unwrap();
    std::fs::write(
        worktree.join("worktree-only.txt"),
        "worktree-search-token\n",
    )
    .unwrap();
    std::fs::create_dir(worktree.join("nested")).unwrap();
    std::fs::write(worktree.join("nested/worktree.txt"), "worktree-nested\n").unwrap();

    let read = make_file_tool(&tools, "Read");
    for content in [
        read.invoke(serde_json::json!({"file_path": "tracked.txt"}))
            .await
            .unwrap(),
        read.invoke_content(serde_json::json!({"file_path": "tracked.txt"}))
            .await
            .unwrap()
            .output,
    ] {
        assert!(
            content.contains("worktree-before"),
            "同一工具应切换相对读取目录：{content}"
        );
    }
    let original = make_file_tool(&static_tools, "Read")
        .invoke(serde_json::json!({"file_path": "tracked.txt"}))
        .await
        .unwrap();
    assert!(original.contains("committed"), "静态工具仍留在原作用域");
    let absolute = read
        .invoke(serde_json::json!({"file_path": repository.join("tracked.txt")}))
        .await
        .unwrap();
    assert!(
        absolute.contains("committed"),
        "绝对路径不受会话目录变化影响"
    );

    let write = make_file_tool(&tools, "Write");
    let write_input = serde_json::json!({"file_path": "tracked.txt", "content": "write-token\n"});
    assert_eq!(
        write
            .file_snapshot(&write_input)
            .unwrap()
            .old_content
            .as_deref(),
        Some("worktree-before\n"),
        "写入快照必须来自实际执行目录"
    );
    write.invoke(write_input).await.unwrap();
    let edit = make_file_tool(&tools, "Edit");
    edit.invoke(serde_json::json!({
        "file_path": "tracked.txt", "old_string": "write-token", "new_string": "single-token"
    }))
    .await
    .unwrap();
    let batch = serde_json::json!({
        "path": "tracked.txt", "edits": [{"oldText": "single-token", "newText": "batch-token"}]
    });
    assert_eq!(
        edit.file_snapshot(&batch).unwrap().old_content.as_deref(),
        Some("single-token\n"),
        "批量编辑及别名路径的快照也必须切换"
    );
    edit.invoke(batch).await.unwrap();
    assert_eq!(
        std::fs::read_to_string(worktree.join("tracked.txt")).unwrap(),
        "batch-token\n"
    );
    assert_eq!(
        std::fs::read_to_string(repository.join("tracked.txt")).unwrap(),
        "committed\n"
    );

    let glob = make_file_tool(&tools, "Glob");
    let matches = glob
        .invoke(serde_json::json!({"pattern": "*.txt"}))
        .await
        .unwrap();
    assert!(
        matches.contains("worktree-only.txt"),
        "默认搜索根应跟随：{matches}"
    );
    assert!(!matches.contains("original-only.txt"), "不能搜索原仓库");
    let nested = glob
        .invoke(serde_json::json!({"pattern": "*.txt", "path": "nested"}))
        .await
        .unwrap();
    assert!(
        nested.contains("worktree.txt"),
        "相对搜索根也应跟随：{nested}"
    );
    let grep = make_file_tool(&tools, "Grep");
    let matches = grep
        .invoke(serde_json::json!({"pattern": "worktree-search-token"}))
        .await
        .unwrap();
    assert!(
        matches.contains("worktree-only.txt"),
        "Grep默认目录应跟随：{matches}"
    );
    let nested = grep
        .invoke(serde_json::json!({"pattern": "worktree-nested", "path": "nested"}))
        .await
        .unwrap();
    assert!(
        nested.contains("worktree.txt"),
        "Grep相对路径应跟随：{nested}"
    );

    // 复用原工具集；真实 ExitWorktree 删除目录后不能再访问或重建旧 checkout。
    exit.invoke(serde_json::json!({"action": "remove", "discard_changes": true}))
        .await
        .expect("退出并删除临时 worktree");
    assert!(!worktree.exists(), "必须真正删除旧目录");
    let output = bash
        .invoke(serde_json::json!({"command": "pwd && echo after-exit && cat tracked.txt", "timeout": 5000}))
        .await
        .expect("真实 ExitWorktree 删除后同一 BashTool 仍能启动");
    assert!(output.contains("after-exit") && output.contains("committed"));
    let restored = read
        .invoke(serde_json::json!({"file_path": "tracked.txt"}))
        .await
        .unwrap();
    assert!(restored.contains("committed"), "退出后恢复原仓库读取");
    write
        .invoke(serde_json::json!({"file_path": "returned.txt", "content": "returned\n"}))
        .await
        .unwrap();
    edit.invoke(serde_json::json!({
        "file_path": "tracked.txt", "old_string": "committed", "new_string": "original-edited"
    }))
    .await
    .unwrap();
    assert_eq!(
        std::fs::read_to_string(repository.join("returned.txt")).unwrap(),
        "returned\n"
    );
    assert_eq!(
        std::fs::read_to_string(repository.join("tracked.txt")).unwrap(),
        "original-edited\n"
    );
    let matches = glob
        .invoke(serde_json::json!({"pattern": "*.txt"}))
        .await
        .unwrap();
    assert!(
        matches.contains("original-only.txt"),
        "Glob恢复原目录：{matches}"
    );
    assert!(!matches.contains("worktree-only.txt"));
    let matches = grep
        .invoke(serde_json::json!({"pattern": "original-search-token"}))
        .await
        .unwrap();
    assert!(
        matches.contains("original-only.txt"),
        "Grep恢复原目录：{matches}"
    );
    assert!(!worktree.exists(), "所有后续相对操作都不能重建已删除目录");
}

#[tokio::test]
async fn test_live_read_image_uses_current_session_directory() {
    use base64::Engine;
    let directory = tempfile::tempdir().unwrap();
    let current = directory.path().join("current");
    std::fs::create_dir(&current).unwrap();
    let handle = new_session_cwd(directory.path().to_string_lossy());
    let tool = crate::tools::ReadFileTool::new(directory.path().to_string_lossy())
        .with_session_cwd(Arc::clone(&handle));
    let image = base64::engine::general_purpose::STANDARD
        .decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAACklEQVR4nGIAAAACAAHlJ978AAAAAElFTkSuQmCC")
        .unwrap();
    std::fs::write(current.join("image.png"), image).unwrap();
    session_cwd::switch(&handle, current.to_string_lossy());
    let output = tool
        .invoke_content(serde_json::json!({"file_path": "image.png"}))
        .await
        .unwrap();
    assert!(output.content.is_some(), "图片多模态入口必须读取新目录");
    assert!(output.output.contains("image/png"));
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
