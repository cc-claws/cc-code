use super::*;

#[tokio::test]
async fn test_output_with_input_timeout_preserves_exit_with_inherited_pipes() {
    let mut command = crate::process::shell_command_with_shell(
        "sleep 4 & printf denied; exit 2",
        &[],
        Some("bash"),
    );
    let dir = tempfile::tempdir().unwrap();
    command.current_dir(dir.path());
    let output = tokio::time::timeout(
        Duration::from_secs(5),
        output_with_input_timeout(command, &[], Duration::from_secs(1)),
    )
    .await
    .expect("根进程退出后不能无限等待继承管道的后代")
    .expect("根进程已按期退出，排空超过执行期限也不能误报超时");
    assert_eq!(output.status.code(), Some(2), "必须保留拦截退出码");
    assert_eq!(output.stdout, b"denied", "有界排空应保留已收到的输出");
}

#[tokio::test]
async fn test_output_with_input_timeout_closes_blocked_input() {
    let dir = tempfile::tempdir().unwrap();
    let mut command = crate::process::shell_command_with_shell(
        "printf started > started; sleep 3; printf leaked > leaked",
        &[],
        Some("bash"),
    );
    command.current_dir(dir.path());
    let input = vec![b'x'; 256 * 1024];
    let result = tokio::time::timeout(
        Duration::from_secs(5),
        output_with_input_timeout(command, &input, Duration::from_millis(500)),
    )
    .await
    .expect("大输入未被消费时执行超时也必须生效");
    assert_eq!(
        result.expect_err("命令未按期退出").kind(),
        io::ErrorKind::TimedOut
    );
    assert!(dir.path().join("started").exists(), "命令应实际启动");
    tokio::time::sleep(Duration::from_secs(3)).await;
    assert!(
        !dir.path().join("leaked").exists(),
        "超时后不能继续执行命令"
    );
}

#[tokio::test]
async fn test_output_with_input_timeout_preserves_exit_without_consuming_input() {
    let command =
        crate::process::shell_command_with_shell("printf denied >&2; exit 2", &[], Some("bash"));
    let input = vec![b'x'; 256 * 1024];
    let output = output_with_input_timeout(command, &input, Duration::from_secs(5))
        .await
        .expect("BrokenPipe 不能覆盖提前退出的拦截结果");
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(output.stderr, b"denied");
}
