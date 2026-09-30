#[cfg(windows)]
use crate::process::git_bash_path;
#[cfg(windows)]
use crate::process::try_shell_command_with_shell;
use crate::process::{
    git_bash_command, is_potential_rtk_command, shell_command, shell_command_with_shell,
};
use std::path::Path;

#[cfg(windows)]
#[test]
fn test_managed_bash_missing_interpreter_is_error_not_cmd() {
    let error = super::selected_git_bash_command("echo must-not-run", None)
        .expect_err("缺失 Git Bash 不能改用 CMD 或 PATH 中的 WSL");
    assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
    assert!(error.to_string().contains("No command was executed"));
}

#[cfg(windows)]
#[test]
fn test_managed_bash_rejects_non_msys_path_without_running_it() {
    let dir = tempfile::tempdir().expect("创建隔离目录");
    let bash = dir.path().join("bash.exe");
    std::fs::write(&bash, b"not an executable").expect("创建探测文件");
    assert!(
        !super::has_msys_runtime(&bash),
        "WSL/普通 bash 不含 MSYS runtime"
    );
    let error = super::selected_git_bash_command("echo must-not-run", Some(&bash))
        .expect_err("非 MSYS Bash 不能被接受");
    assert!(error.to_string().contains("not WSL"));
    std::fs::write(dir.path().join("msys-2.0.dll"), b"layout marker").expect("创建布局标记");
    let command = super::selected_git_bash_command("echo test", Some(&bash))
        .expect("只验证构造，不执行伪造文件");
    assert_eq!(command.as_std().get_program(), bash.as_os_str());
}

#[cfg(windows)]
#[test]
fn test_explicit_bash_never_resolves_to_path_bash_or_wsl() {
    let explicit = shell_command_with_shell("echo must-not-run", &[], Some("bash"));
    let default = shell_command("bash hook.sh", &[]);
    for command in [explicit, default] {
        let program = command.as_std().get_program();
        assert_ne!(program, "bash", "不得经 PATH 解析到 WSL launcher");
        assert_ne!(program, "cmd", "显式 POSIX 命令不得退回 CMD");
    }
}

#[test]
fn test_managed_bash_uses_explicit_interpreter_for_single_line() {
    let command = "printf once; exit 2";
    let cmd = super::managed_shell_command(command, peri_agent::shell::ShellDialect::Bash)
        .expect("测试环境必须安装 Bash/Git Bash");
    let args: Vec<_> = cmd
        .as_std()
        .get_args()
        .map(|arg| arg.to_string_lossy())
        .collect();
    assert_eq!(args, ["-c", command], "单行也必须直接使用 Bash");
    assert_ne!(cmd.as_std().get_program(), "cmd", "不能通过 CMD 套壳");
}

#[test]
fn test_shell_command_unix_bash_c() {
    let cmd = shell_command("echo", &["hello"]);
    let formatted = format!("{cmd:?}");
    #[cfg(unix)]
    {
        assert!(
            formatted.contains("bash"),
            "expected bash, got: {formatted}"
        );
        assert!(
            formatted.contains("-c"),
            "expected -c flag, got: {formatted}"
        );
    }
    #[cfg(windows)]
    {
        assert!(formatted.contains("cmd"), "expected cmd, got: {formatted}");
        assert!(
            formatted.contains("/C"),
            "expected /C flag, got: {formatted}"
        );
    }
}

#[test]
fn test_shell_command_no_args() {
    let cmd = shell_command("ls", &[]);
    let formatted = format!("{cmd:?}");
    #[cfg(unix)]
    {
        assert!(
            formatted.contains("bash"),
            "expected bash, got: {formatted}"
        );
        assert!(
            formatted.contains("ls"),
            "expected 'ls' in command, got: {formatted}"
        );
    }
    #[cfg(windows)]
    {
        assert!(formatted.contains("cmd"), "expected cmd, got: {formatted}");
        assert!(
            formatted.contains("ls"),
            "expected 'ls' in command, got: {formatted}"
        );
    }
}

#[test]
fn test_shell_command_multi_args() {
    let cmd = shell_command("npx", &["-y", "@anthropic/mcp-server"]);
    let formatted = format!("{cmd:?}");
    #[cfg(unix)]
    {
        assert!(
            formatted.contains("bash"),
            "expected bash, got: {formatted}"
        );
        assert!(
            formatted.contains("npx"),
            "expected 'npx', got: {formatted}"
        );
    }
    #[cfg(windows)]
    {
        assert!(formatted.contains("cmd"), "expected cmd, got: {formatted}");
        assert!(
            formatted.contains("npx"),
            "expected 'npx', got: {formatted}"
        );
    }
}

#[cfg(windows)]
#[tokio::test]
async fn test_shell_command_windows_preserves_quoted_absolute_path() {
    // Windows 上 `cmd /C` 不能把整段带引号命令再作为普通参数转义，
    // 否则 `type "D:\path\file"` 里的引号会泄漏到文件名中。
    let cargo_toml = Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
    let command = format!("type \"{}\"", cargo_toml.display());
    let output = shell_command(&command, &[])
        .output()
        .await
        .expect("带引号绝对路径命令应能启动");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "带引号绝对路径命令应执行成功，stderr: {stderr}"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("peri-middlewares"),
        "应读取 peri-middlewares Cargo.toml，实际输出: {stdout}"
    );
}

#[cfg(windows)]
#[tokio::test]
async fn test_shell_command_windows_keeps_cmd_operators() {
    let output = shell_command("echo alpha && echo beta", &[])
        .output()
        .await
        .expect("cmd 操作符命令应能启动");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "cmd 操作符命令应执行成功，stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        stdout.contains("alpha"),
        "stdout 应包含第一段输出: {stdout}"
    );
    assert!(stdout.contains("beta"), "stdout 应包含第二段输出: {stdout}");
}

#[test]
fn test_is_potential_rtk_command() {
    assert!(is_potential_rtk_command("git status"));
    assert!(is_potential_rtk_command("cargo test --lib"));
    assert!(is_potential_rtk_command("npm install"));
    assert!(is_potential_rtk_command("RUST_LOG=info cargo check"));
    assert!(!is_potential_rtk_command("echo hello"));
    assert!(!is_potential_rtk_command("cd /foo"));
    assert!(!is_potential_rtk_command("mkdir bar"));
}

// ── MSYS_NO_PATHCONV 测试 ────────────────────────────────────────

#[tokio::test]
async fn test_git_bash_command_sets_msys_no_pathconv() {
    // 通过实际执行验证 MSYS_NO_PATHCONV 环境变量已注入
    let bash_path = Path::new("bash");
    let mut cmd = git_bash_command(bash_path, "echo $MSYS_NO_PATHCONV", &[]);
    let output = cmd.output().await;
    match output {
        Ok(out) if out.status.success() => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            assert!(
                stdout.trim() == "1",
                "MSYS_NO_PATHCONV 应为 1，实际: {}",
                stdout.trim()
            );
        }
        // bash 不可用时跳过（非 Windows CI 环境可能没有 bash）
        _ => {}
    }
}

// ── shell_command_with_shell 测试 ──────────────────────────────────

// ── shell_command_with_shell 测试 ──────────────────────────────────

#[cfg(windows)]
#[test]
fn test_shell_command_multiline_uses_git_bash_on_windows() {
    // Issue #212：含字面换行符的多行命令应走 Git Bash（bash -c），
    // 而非 cmd /C（只执行第一行）
    let multiline = "echo A\necho B";
    let cmd = shell_command(multiline, &[]);
    let formatted = format!("{cmd:?}");
    // 如果 Git Bash 可用，应走 bash -c 而非 cmd /C
    if git_bash_path().is_some() {
        assert!(
            formatted.to_lowercase().contains("bash"),
            "多行命令应走 Git Bash，实际：{formatted}"
        );
        assert!(
            formatted.contains("-c"),
            "多行命令应走 bash -c，实际：{formatted}"
        );
    } else {
        // Git Bash 不可用时多行命令必须 hard-error，不能回退 cmd /C
        // （cmd 只执行第一行且静默报成功，#309）。
        // shell_command（infallible 版）会把错误转成必然 spawn 失败的占位命令，
        // 这里直接测 try_ 版本断言错误本身。
        let err = try_shell_command_with_shell(multiline, &[], None)
            .expect_err("无 Git Bash 时多行命令应 hard-error，而非静默截断");
        assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
        assert!(
            err.to_string().contains("Git Bash"),
            "错误信息应提示需要 Git Bash，实际：{err}"
        );
    }
}

#[cfg(windows)]
#[test]
fn test_shell_command_single_line_still_uses_cmd_on_windows() {
    // 确保非多行命令仍走 cmd /C（不破坏现有行为）
    let single = "echo hello && echo world";
    let cmd = shell_command(single, &[]);
    let formatted = format!("{cmd:?}");
    assert!(
        formatted.contains("cmd"),
        "单行命令应走 cmd /C，实际：{formatted}"
    );
}

#[cfg(windows)]
#[tokio::test]
async fn test_shell_command_multiline_produces_complete_output() {
    // Issue #212 回归测试：多行命令的所有行 stdout 都应被捕获
    if git_bash_path().is_none() {
        return; // Git Bash 不可用，跳过
    }
    let multiline = "echo AAA\necho BBB";
    let output = shell_command(multiline, &[])
        .output()
        .await
        .expect("多行命令应能启动");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("AAA"),
        "应包含第一行输出 AAA，实际：{stdout}"
    );
    assert!(
        stdout.contains("BBB"),
        "应包含第二行输出 BBB，实际：{stdout}"
    );
}

#[test]
fn test_shell_command_with_shell_powershell() {
    // PowerShell: 应使用 powershell -NoProfile -NonInteractive -Command
    let cmd = shell_command_with_shell("Write-Host hello", &[], Some("powershell"));
    let formatted = format!("{cmd:?}");
    assert!(
        formatted.contains("powershell"),
        "expected powershell, got: {formatted}"
    );
    assert!(
        formatted.contains("-Command"),
        "expected -Command flag, got: {formatted}"
    );
    assert!(
        formatted.contains("Write-Host hello"),
        "expected command retained, got: {formatted}"
    );
}

#[test]
fn test_shell_command_with_shell_pwsh_executable() {
    // 显式选择 PowerShell 7，不能替换为 Windows PowerShell。
    let cmd = shell_command_with_shell("echo test", &[], Some("pwsh"));
    let formatted = format!("{cmd:?}");
    assert!(
        formatted.contains("pwsh"),
        "显式 pwsh 应保留解释器，实际：{formatted}"
    );
}

#[test]
fn test_shell_command_with_shell_powershell_with_args() {
    // PowerShell 带参数
    let cmd = shell_command_with_shell("Get-Process", &["node"], Some("powershell"));
    let formatted = format!("{cmd:?}");
    assert!(
        formatted.contains("Get-Process node"),
        "expected command with args, got: {formatted}"
    );
}

#[test]
fn test_shell_command_with_shell_none_uses_platform_default() {
    // shell=None 应使用平台默认
    let cmd = shell_command_with_shell("echo", &["hello"], None);
    let formatted = format!("{cmd:?}");
    #[cfg(windows)]
    {
        assert!(
            formatted.contains("cmd"),
            "expected cmd on Windows, got: {formatted}"
        );
    }
    #[cfg(unix)]
    {
        assert!(
            formatted.contains("bash"),
            "expected bash on Unix, got: {formatted}"
        );
    }
}

#[test]
fn test_shell_command_with_shell_bash_explicit() {
    // 显式 bash 应使用 bash -c
    let _cmd = shell_command_with_shell("ls", &[], Some("bash"));
    #[cfg(unix)]
    {
        let formatted = format!("{_cmd:?}");
        assert!(
            formatted.contains("bash"),
            "expected bash on Unix, got: {formatted}"
        );
        assert!(
            formatted.contains("-c"),
            "expected -c flag, got: {formatted}"
        );
    }
    #[cfg(windows)]
    assert!(
        _cmd.as_std()
            .get_program()
            .to_string_lossy()
            .ends_with("bash.exe")
            || _cmd.as_std().get_program() == "bash",
        "显式 bash 不得回退到 cmd"
    );
}

#[test]
fn test_starts_with_posix_shell_command_boundaries() {
    for command in [
        "bash",
        " sh\tfile.sh",
        "bash.exe -c 'echo ok'",
        "\"bash\" script.sh",
        "'sh' < input",
        "BASH test.sh",
        "bash<input",
    ] {
        assert!(
            super::starts_with_posix_shell(command),
            "应识别完整解释器：{command}"
        );
    }
    for command in [
        "",
        "bashful script",
        "shell script",
        "echo bash",
        "\"bash\"suffix",
        "'bash",
        "python bash.py",
        "\"C:/Program Files/Git/bin/bash.exe\" script.sh",
    ] {
        assert!(
            !super::starts_with_posix_shell(command),
            "不得误识别：{command}"
        );
    }
}

#[cfg(windows)]
#[test]
fn test_shell_command_explicit_cmd_overrides_auto_routing() {
    for command in ["bash test.sh", "echo one\necho two"] {
        let cmd = shell_command_with_shell(command, &[], Some("cmd"));
        assert_eq!(cmd.as_std().get_program(), "cmd", "显式 cmd 必须优先");
    }
}

#[tokio::test]
async fn test_shell_command_missing_explicit_shell_does_not_execute() {
    let dir = tempfile::tempdir().expect("创建隔离目录");
    let missing = dir.path().join("missing-shell");
    let result = shell_command_with_shell("echo unexpected", &[], missing.to_str())
        .output()
        .await;
    assert!(
        matches!(result, Err(ref error) if error.kind() == std::io::ErrorKind::NotFound),
        "缺失解释器应返回启动错误，不能改用系统 shell：{result:?}"
    );
}

#[cfg(windows)]
#[tokio::test]
async fn test_shell_command_bash_prefix_without_bash_on_path() {
    let bash = git_bash_path().expect("Windows 回归测试需要安装 Git Bash");
    for command in [
        "bash -c 'printf routed'",
        "sh -c 'printf routed'",
        "bash.exe\t-c 'printf routed'",
        "\"bash\" -c 'printf routed'",
    ] {
        let mut cmd = shell_command(command, &[]);
        assert_eq!(
            cmd.as_std().get_program(),
            bash.as_os_str(),
            "应在启动前选择 Git Bash"
        );
        // 只修改子进程 PATH，不污染并行测试或用户环境。
        cmd.env("PATH", "C:\\Windows\\System32");
        let output = cmd.output().await.expect("启动 Git Bash");
        assert!(
            output.status.success(),
            "路由后应执行成功：{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout, b"routed", "原始引号和参数应完整保留");
    }
}

// ─────────────────────────────────────────────────────────────────────────
// #288：门控 rtk 预测必须是纯函数 —— 审批前绝不执行外部二进制
// ─────────────────────────────────────────────────────────────────────────

/// 纯预测内核：白名单命令 → `rtk <cmd>`；非白名单 / 改写不可能 → None。
#[test]
fn test_predict_rtk_rewrite_inner_is_pure() {
    use crate::process::predict_rtk_rewrite_inner;
    assert_eq!(
        predict_rtk_rewrite_inner("git status", true).as_deref(),
        Some("rtk git status")
    );
    assert_eq!(
        predict_rtk_rewrite_inner("cargo build --release", true).as_deref(),
        Some("rtk cargo build --release")
    );
    // 非白名单命令永不预测（与 rtk 是否存在无关）
    assert_eq!(predict_rtk_rewrite_inner("echo hi", true), None);
    assert_eq!(predict_rtk_rewrite_inner("rm -rf /tmp/x", true), None);
    // 改写不可能发生时不预测
    assert_eq!(predict_rtk_rewrite_inner("git status", false), None);
}

/// #288 回归：即使 RTK_PATH 指向恶意二进制，门控预测也不得执行它。
///
/// 旧代码在此处调用 `rtk_rewrite_command`（`--version` 探测 + `rewrite` 调用），
/// 会在审批弹窗出现前执行 PATH/RTK_PATH 上的不可信代码。
#[test]
fn test_predict_rtk_rewrite_never_executes_rtk_binary() {
    let dir = tempfile::tempdir().expect("创建隔离目录");
    let marker = dir.path().join("executed.marker");
    let fake_rtk = dir.path().join("fake-rtk");
    // 恶意 rtk：一旦被执行（无论 --version 探测还是 rewrite 调用）就留下标记
    std::fs::write(
        &fake_rtk,
        format!("#!/bin/sh\ntouch '{}'\n", marker.display()),
    )
    .expect("写入伪造 rtk");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&fake_rtk, std::fs::Permissions::from_mode(0o755))
            .expect("chmod +x");
    }

    let saved = std::env::var("RTK_PATH").ok();
    std::env::set_var("RTK_PATH", &fake_rtk);
    let predicted = crate::process::predict_rtk_rewrite("git status");
    match saved {
        Some(v) => std::env::set_var("RTK_PATH", v),
        None => std::env::remove_var("RTK_PATH"),
    }

    assert_eq!(
        predicted.as_deref(),
        Some("rtk git status"),
        "应给出纯字符串预测"
    );
    assert!(
        !marker.exists(),
        "#288 回归失败：门控预测执行了 RTK_PATH 指向的二进制！"
    );
}
