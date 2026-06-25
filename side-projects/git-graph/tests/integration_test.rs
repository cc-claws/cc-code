use assert_cmd::Command;
use std::process::Command as StdCommand;

fn setup_repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path();
    StdCommand::new("git")
        .args(["init"])
        .current_dir(p)
        .output()
        .unwrap();
    StdCommand::new("git")
        .args(["config", "user.email", "t@t.com"])
        .current_dir(p)
        .output()
        .unwrap();
    StdCommand::new("git")
        .args(["config", "user.name", "t"])
        .current_dir(p)
        .output()
        .unwrap();
    std::fs::write(p.join("a.txt"), "a").unwrap();
    StdCommand::new("git")
        .args(["add", "."])
        .current_dir(p)
        .output()
        .unwrap();
    StdCommand::new("git")
        .args(["commit", "-m", "init"])
        .current_dir(p)
        .output()
        .unwrap();
    dir
}

#[test]
fn test_gig_help() {
    let _dir = setup_repo();
    let mut cmd = Command::cargo_bin("gig").unwrap();
    cmd.arg("--help")
        .assert()
        .success()
        .stdout(predicates::str::contains("Git Graph TUI"));
}

#[test]
fn test_gig_invalid_path() {
    let mut cmd = Command::cargo_bin("gig").unwrap();
    cmd.arg("/nonexistent/path").assert().failure();
}
