use super::*;
use crate::hooks::types::HookEvent;
use std::path::Path;

fn make_script_hook(cwd: &Path, script: &str) -> (HookType, HookInput, RegisteredHook) {
    std::fs::write(cwd.join("检查 hook.sh"), script).expect("写入隔离测试脚本");
    let hook: HookType = serde_json::from_value(serde_json::json!({
        "type": "command",
        "command": "bash \"检查 hook.sh\"",
        "timeout": 5
    }))
    .unwrap();
    let input = HookInput::session_start(
        "test-session",
        "transcript.json",
        cwd.to_str().expect("测试目录为 UTF-8"),
        "startup",
        "test-model",
    );
    let registered = RegisteredHook {
        hook: hook.clone(),
        event: HookEvent::SessionStart,
        matcher: None,
        plugin_name: "test-plugin".into(),
        plugin_id: "test-plugin".into(),
        plugin_root: cwd.into(),
        plugin_data_dir: cwd.join("data"),
        plugin_options: std::collections::HashMap::new(),
    };
    (hook, input, registered)
}

#[tokio::test]
async fn test_command_hook_routes_bash_and_preserves_context() {
    let dir = tempfile::Builder::new()
        .prefix("hook 中文 ")
        .tempdir()
        .expect("创建临时目录");
    let decision = serde_json::json!({"decision": "block", "reason": "真实执行成功"});
    let script = format!("cat > input.json\nprintf '%s' \"$CLAUDE_PROJECT_DIR\" > project.txt\nprintf '%s' \"$CLAUDE_PLUGIN_ROOT\" > root.txt\nprintf '%s' \"$CLAUDE_PLUGIN_DATA\" > data.txt\nprintf '%s' \"$CLAUDE_HOOK_EVENT_NAME\" > event.txt\nprintf '%s' \"$CLAUDE_PLUGIN_OPTION_MODE\" > option.txt\nprintf '%s' '{}'\n", decision);
    let (hook, input, mut registered) = make_script_hook(dir.path(), &script);
    registered
        .plugin_options
        .insert("mode".into(), serde_json::json!("test"));
    let action = execute_command_hook(&hook, &input, &registered).await;
    assert!(
        matches!(action, HookAction::Block { ref reason } if reason == "真实执行成功"),
        "必须解析真实脚本结果：{action:?}"
    );
    let captured: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.path().join("input.json")).expect("读取 stdin"))
            .expect("解析输入 JSON");
    assert_eq!(
        captured,
        serde_json::to_value(&input).expect("序列化输入"),
        "stdin 应完整传递"
    );
    for (file, expected) in [
        ("project.txt", input.cwd.clone()),
        (
            "root.txt",
            registered.plugin_root.to_string_lossy().into_owned(),
        ),
        (
            "data.txt",
            registered.plugin_data_dir.to_string_lossy().into_owned(),
        ),
        ("event.txt", "SessionStart".into()),
        ("option.txt", "\"test\"".into()),
    ] {
        assert_eq!(
            std::fs::read_to_string(dir.path().join(file)).expect("读取环境变量输出"),
            expected,
            "应保留 {file} 对应环境变量"
        );
    }
}

#[tokio::test]
async fn test_command_hook_nonzero_exit_is_never_replayed() {
    let dir = tempfile::tempdir().expect("创建临时目录");
    for code in [1, 2] {
        let script = format!("printf x >> count-{code}\nprintf denied >&2\nexit {code}\n");
        let (hook, mut input, registered) = make_script_hook(dir.path(), &script);
        // 脚本不读 stdin；大输入可以覆盖提前退出时的 BrokenPipe。
        input.prompt = Some("x".repeat(256 * 1024));
        let action = execute_command_hook(&hook, &input, &registered).await;
        if code == 2 {
            assert!(
                matches!(action, HookAction::Block { ref reason } if reason == "denied"),
                "exit 2 必须保留拦截结果：{action:?}"
            );
        } else {
            assert!(
                matches!(action, HookAction::Allow),
                "exit 1 保持告警放行语义"
            );
        }
        assert_eq!(
            std::fs::read(dir.path().join(format!("count-{code}"))).expect("读取执行计数"),
            b"x",
            "副作用必须只发生一次"
        );
    }
}

#[tokio::test]
async fn test_command_hook_root_exit_block_is_not_masked_by_descendant_pipes() {
    let dir = tempfile::tempdir().expect("创建临时目录");
    let (mut hook, input, registered) =
        make_script_hook(dir.path(), "sleep 4 &\nprintf denied >&2\nexit 2\n");
    if let HookType::Command { timeout, .. } = &mut hook {
        *timeout = Some(1);
    }
    let action = tokio::time::timeout(
        Duration::from_secs(5),
        execute_command_hook(&hook, &input, &registered),
    )
    .await
    .expect("Hook 根进程退出后收尾必须有界");
    assert!(
        matches!(action, HookAction::Block { ref reason } if reason == "denied"),
        "后代继承管道不能使已知 exit 2 被超时策略放行：{action:?}"
    );
}

#[tokio::test]
async fn test_command_hook_drains_output_while_writing_input() {
    let dir = tempfile::tempdir().expect("创建临时目录");
    let decision = serde_json::json!({"decision": "block", "reason": "双向管道成功"});
    let script = format!("printf '%262144s' ' '\nprintf '%262144s' ' ' >&2\ncat > large-input.json\nprintf '%s' '{}'\n", decision);
    let (hook, mut input, registered) = make_script_hook(dir.path(), &script);
    input.prompt = Some("测试".repeat(128 * 1024));
    let action = execute_command_hook(&hook, &input, &registered).await;
    assert!(
        matches!(action, HookAction::Block { ref reason } if reason == "双向管道成功"),
        "大输入输出不能互相阻塞或被超时放行：{action:?}"
    );
    let captured: serde_json::Value = serde_json::from_slice(
        &std::fs::read(dir.path().join("large-input.json")).expect("读取大输入"),
    )
    .expect("解析大输入");
    assert_eq!(
        captured["prompt"],
        input.prompt.expect("测试输入包含 prompt")
    );
}

#[tokio::test]
async fn test_command_hook_timeout_covers_stdin_write() {
    let dir = tempfile::tempdir().expect("创建临时目录");
    let (mut hook, mut input, registered) = make_script_hook(dir.path(), "exit 0");
    if let HookType::Command {
        command,
        shell,
        timeout,
        ..
    } = &mut hook
    {
        *command = "printf started > started; sleep 2; printf leaked > leaked".into();
        *shell = Some("bash".into());
        *timeout = Some(1);
    }
    input.prompt = Some("x".repeat(256 * 1024));
    let start = std::time::Instant::now();
    let action = execute_command_hook(&hook, &input, &registered).await;
    assert!(matches!(action, HookAction::Allow), "超时保持告警放行语义");
    assert!(
        dir.path().join("started").exists(),
        "必须实际启动脚本，不能把启动失败当作超时通过"
    );
    assert!(
        start.elapsed() >= Duration::from_millis(900) && start.elapsed() < Duration::from_secs(4),
        "stdin 阻塞也必须受总超时约束"
    );
    tokio::time::sleep(Duration::from_millis(1500)).await;
    assert!(
        !dir.path().join("leaked").exists(),
        "超时后实际脚本不能继续执行"
    );
}

#[cfg(windows)]
#[tokio::test]
async fn test_command_hook_abort_cleans_git_bash_descendants() {
    let dir = tempfile::tempdir().expect("创建临时目录");
    let (hook, input, registered) = make_script_hook(
        dir.path(),
        "printf started > started\n( sleep 2; printf leaked > leaked ) &\nwait\n",
    );
    let handle =
        tokio::spawn(async move { execute_command_hook(&hook, &input, &registered).await });
    tokio::time::timeout(Duration::from_secs(5), async {
        while !dir.path().join("started").exists() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("脚本应实际启动");
    handle.abort();
    assert!(handle.await.expect_err("任务应被取消").is_cancelled());
    tokio::time::sleep(Duration::from_millis(2500)).await;
    assert!(
        !dir.path().join("leaked").exists(),
        "取消后 Git Bash 后代不能继续写文件"
    );
}

#[cfg(windows)]
#[tokio::test]
async fn test_command_hook_native_cmd_and_powershell_still_work() {
    let dir = tempfile::tempdir().expect("创建临时目录");
    let (_, input, registered) = make_script_hook(dir.path(), "exit 0");
    for (shell, command) in [
        (None, "echo cmd-ok> native.txt & exit /b 2"),
        (
            Some("powershell"),
            "[Console]::Out.Write('powershell-ok'); exit 2",
        ),
    ] {
        let hook = serde_json::from_value(serde_json::json!({"type": "command", "shell": shell, "command": command, "timeout": 5})).unwrap();
        let action = execute_command_hook(&hook, &input, &registered).await;
        assert!(
            matches!(action, HookAction::Block { .. }),
            "原生 shell 必须实际执行并拦截：{action:?}"
        );
    }
    assert_eq!(
        std::fs::read_to_string(dir.path().join("native.txt"))
            .expect("读取 CMD 输出")
            .trim(),
        "cmd-ok"
    );
}
