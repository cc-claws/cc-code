use crate::process::shell_command;

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
