use super::*;
use cc_agent::{shell::ShellDialect, tools::BaseTool};
use cc_middlewares::middleware::terminal::BashTool;
use std::time::Duration;

#[tokio::test]
async fn test_executor_background_spawn_failure_is_not_registered() {
    let dir = tempfile::tempdir().expect("创建隔离目录");
    let cwd = dir
        .path()
        .join("missing-directory")
        .to_string_lossy()
        .to_string();
    let (tx, mut rx) = mpsc::unbounded_channel();
    let executor = AgentShellExecutor::new(tx, cwd.clone(), "failed-contract".into());
    let result = executor
        .execute(ShellRequest {
            owner_session_id: None,
            invocation: None,
            command: "printf must-not-run".into(),
            original_command: "printf must-not-run".into(),
            shell: ShellDialect::Bash,
            cwd,
            timeout_ms: 5000,
            execution_timeout_ms: 600_000,
            run_in_background: true,
        })
        .await;
    assert!(result.is_err(), "spawn 失败不能返回后台句柄");
    assert!(rx.try_recv().is_err(), "spawn 失败不能注册为运行任务");
}

#[tokio::test]
async fn test_bash_ctrl_b_keeps_same_single_posix_execution() {
    let dir = tempfile::tempdir().expect("创建隔离目录");
    let cwd = dir.path().to_string_lossy().to_string();
    let (tx, mut rx) = mpsc::unbounded_channel();
    let executor = Arc::new(AgentShellExecutor::new(tx, cwd.clone(), "contract".into()));
    let tool = BashTool::with_executor(cwd, executor);
    // 单行 POSIX 链用于复现旧 CMD 首次失败、后续 fallback 脱管的路径。
    let command = "printf x >> count && sleep 0.4 && printf finished";
    let invoke = tokio::spawn(async move {
        tool.invoke(serde_json::json!({"command": command, "timeout": 5000}))
            .await
    });
    let registration = tokio::time::timeout(Duration::from_secs(5), rx.recv())
        .await
        .expect("长 Bash 应进入 UI 注册链路")
        .expect("应收到任务");
    assert_eq!(registration.command, command, "应关联原始命令");
    assert!(!registration.direct_background, "首次执行应是前台");
    assert!(
        registration.handoff.background(),
        "应支持 Ctrl+B 共享状态移交"
    );
    let output = tokio::time::timeout(Duration::from_secs(5), invoke)
        .await
        .expect("后台化应及时返回")
        .expect("工具任务应正常完成")
        .expect("应返回句柄");
    assert!(
        output.contains(&format!("<task-id>{}</task-id>", registration.task_id)),
        "后台化不能新建任务：{output}"
    );
    tokio::time::timeout(Duration::from_secs(5), registration.exit_signal.wait())
        .await
        .expect("同一进程应继续直到完成");
    assert_eq!(
        std::fs::read(dir.path().join("count")).expect("读取执行次数"),
        b"x",
        "副作用只能执行一次"
    );
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if tokio::fs::read_to_string(&registration.output_path)
                .await
                .is_ok_and(|text| text.contains("finished"))
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("后台完成输出必须保存在同一个任务文件");
    assert!(rx.try_recv().is_err(), "不能为 fallback 注册第二个任务");
}

#[tokio::test]
async fn test_executor_background_preserves_original_command_after_rewrite() {
    let dir = tempfile::tempdir().expect("创建隔离目录");
    let cwd = dir.path().to_string_lossy().to_string();
    let (tx, mut rx) = mpsc::unbounded_channel();
    let executor = AgentShellExecutor::new(tx, cwd.clone(), "rewritten-contract".into());
    let handle = executor
        .execute(ShellRequest {
            owner_session_id: None,
            invocation: None,
            command: "printf rewritten".into(),
            original_command: "cargo test".into(),
            shell: ShellDialect::Bash,
            cwd,
            timeout_ms: 5000,
            execution_timeout_ms: 600_000,
            run_in_background: true,
        })
        .await
        .expect("应执行请求");
    let registration = rx.recv().await.expect("直接后台应立即注册");
    assert_eq!(
        registration.command, "cargo test",
        "UI 用原文，不能因重写丢失关联"
    );
    assert_eq!(registration.task_id, handle.task_id, "同一任务身份不能变化");
    assert!(registration.direct_background);
    let output = tokio::time::timeout(Duration::from_secs(5), handle.result_rx)
        .await
        .expect("命令应完成")
        .expect("应收到结果")
        .expect("执行应成功");
    assert_eq!(
        output.stdout, "rewritten",
        "应执行 effective command，而非展示原文"
    );
    assert_eq!(output.exit_code, 0);
}
