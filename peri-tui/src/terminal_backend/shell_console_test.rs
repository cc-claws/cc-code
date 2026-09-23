use std::{future::Future, time::Duration};

use windows_sys::Win32::System::Console::{
    GetConsoleCP, GetConsoleOutputCP, SetConsoleCP, SetConsoleOutputCP,
};

use super::*;
use crate::shell_exec::{execute_shell_command, execute_shell_command_streaming, CommandOutput};

async fn observe_console(
    task: impl Future<Output = anyhow::Result<CommandOutput>>,
) -> anyhow::Result<(CommandOutput, Vec<(u32, u32)>, usize)> {
    // SAFETY: 调用者已验证专属隐藏控制台；不修改活动终端。
    anyhow::ensure!(unsafe { SetConsoleCP(936) } != 0);
    anyhow::ensure!(unsafe { SetConsoleOutputCP(936) } != 0);
    let mut probe = ConsoleWidthProbe::new();
    anyhow::ensure!(probe.fingerprint.is_some(), "需要真实的控制台输出句柄");
    let mut samples = vec![(936, 936)];
    let mut refreshes = 0;
    let mut interval = tokio::time::interval(Duration::from_millis(2));
    tokio::pin!(task);
    let output = loop {
        tokio::select! {
            result = &mut task => break result?,
            _ = interval.tick() => {
                let sample = unsafe { (GetConsoleCP(), GetConsoleOutputCP()) };
                if samples.last() != Some(&sample) {
                    samples.push(sample);
                }
                refreshes += usize::from(probe.refresh());
            }
        }
    };
    let sample = unsafe { (GetConsoleCP(), GetConsoleOutputCP()) };
    if samples.last() != Some(&sample) {
        samples.push(sample);
    }
    refreshes += usize::from(probe.refresh());
    Ok((output, samples, refreshes))
}

#[tokio::test]
#[ignore = "需由 scripts/test-shell-console.ps1 在独立隐藏控制台运行，且 PHP CLI 在 PATH 中"]
async fn test_shell_console_php_isolation() -> anyhow::Result<()> {
    anyhow::ensure!(std::env::var("PERI_ISOLATED_CONSOLE_TEST").as_deref() == Ok("1"));
    let evidence = std::env::var("PERI_CONSOLE_TEST_RESULT")?;
    let command = "php -r \"echo 'ready'; usleep(300000);\"";
    let mut cases = Vec::new();
    for shell in [None, Some("powershell"), Some("bash")] {
        for isolated in [false, true] {
            let mut cmd = peri_middlewares::process::shell_command_with_shell(command, &[], shell);
            if !isolated {
                // 仅在专属控制台恢复旧的共享模式，证明修复前的实际触发条件。
                cmd.creation_flags(0);
            }
            let (output, samples, refreshes) = observe_console(async {
                let output = cmd.output().await?;
                Ok(CommandOutput {
                    stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
                    stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
                    exit_code: output.status.code().unwrap_or(-1),
                })
            })
            .await?;
            assert_eq!(output.exit_code, 0, "PHP 应执行成功：{output:?}");
            assert_eq!(output.stdout, "ready");
            cases.push(serde_json::json!({
                "shell": shell.unwrap_or("cmd"), "isolated": isolated,
                "samples": samples, "clear_triggers": refreshes,
            }));
            std::fs::write(&evidence, serde_json::to_string_pretty(&cases)?)?;
            if isolated {
                assert_eq!(samples, vec![(936, 936)], "隔离后不应改变父控制台代码页");
                assert_eq!(refreshes, 0, "隔离后不应触发列宽缓存失效和物理清屏");
            } else if shell.is_none() {
                assert!(refreshes > 0, "旧 cmd 路径必须先复现清屏触发条件");
            }
        }
    }
    for streaming in [false, true] {
        let (output, samples, refreshes) = observe_console(async {
            if streaming {
                let mut execution = execute_shell_command_streaming(command, ".", None);
                let drain =
                    tokio::spawn(
                        async move { while execution.output_rx.recv().await.is_some() {} },
                    );
                let result = execution.result.await?;
                drain.await?;
                result
            } else {
                execute_shell_command(command, ".").await
            }
        })
        .await?;
        assert_eq!(output.exit_code, 0, "TUI shell 路径应成功：{output:?}");
        assert_eq!(output.stdout, "ready");
        assert_eq!(samples, vec![(936, 936)]);
        assert_eq!(refreshes, 0);
        cases.push(serde_json::json!({
            "executor": if streaming { "streaming" } else { "captured" },
            "samples": samples, "clear_triggers": refreshes,
        }));
    }
    std::fs::write(evidence, serde_json::to_string_pretty(&cases)?)?;
    Ok(())
}
