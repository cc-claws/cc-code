use super::*;

#[test]
fn test_strip_ansi_colors() {
    let colored = "\x1b[32mSuccess\x1b[0m: \x1b[1;31mError message\x1b[0m";
    assert_eq!(strip_ansi(colored), "Success: Error message");
}

#[test]
fn test_strip_ansi_carriage_return() {
    let progress = "Downloading 10%\rDownloading 50%\rDownloading 100%\nDone";
    assert_eq!(strip_ansi(progress), "Downloading 100%\nDone");
}

#[test]
fn test_filter_git_status() {
    let raw = r#"On branch master
Changes not staged for commit:
  (use "git add <file>..." to update what will be committed)
  (use "git restore <file>..." to discard changes in working directory)
	modified:   src/main.rs

Untracked files:
  (use "git add <file>..." to include in what will be committed)
	new_file.txt

no changes added to commit (use "git add" to track)"#;

    let filtered = filter_git_status(raw);
    assert!(!filtered.contains("use \"git add"));
    assert!(!filtered.contains("use \"git restore"));
    assert!(!filtered.contains("no changes added to commit"));
    assert!(filtered.contains("On branch master"));
    assert!(filtered.contains("modified:   src/main.rs"));
    assert!(filtered.contains("new_file.txt"));
}

#[test]
fn test_filter_cargo_test_success() {
    let raw = r#"running 3 tests
test test_one ... ok
test test_two ... ok
test test_three ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s"#;

    let filtered = filter_cargo_test(raw, 0);
    assert!(!filtered.contains("test_one ... ok"));
    assert!(!filtered.contains("test_two ... ok"));
    assert!(!filtered.contains("test_three ... ok"));
    assert!(filtered.contains("test result: ok. 3 passed"));
}

#[test]
fn test_filter_cargo_test_failure() {
    let raw = r#"running 3 tests
test test_one ... ok
test test_two ... FAILED
test test_three ... ok

failures:

---- test_two stdout ----
thread 'test_two' panicked at 'assertion failed: `(left == right)`', tests/foo.rs:10:5

failures:
    test_two

test result: FAILED. 1 failed; 2 passed; 0 ignored"#;

    let filtered = filter_cargo_test(raw, 101);
    assert!(!filtered.contains("test_one ... ok"));
    assert!(!filtered.contains("test_three ... ok"));
    assert!(filtered.contains("test test_two ... FAILED"));
    assert!(filtered.contains("failures:"));
    assert!(filtered.contains("panicked at"));
}

#[test]
fn test_filter_cargo_build() {
    let raw = r#"   Compiling libc v0.2.169
   Compiling proc-macro2 v1.0.93
   Compiling quote v1.0.38
   Compiling syn v2.0.96
warning: unused variable: `x`
  --> src/main.rs:10:9
   |
10 |     let x = 1;
   |         ^
   |
   = note: `#[warn(unused_variables)]` on by default

    Finished `dev` profile [unoptimized + debuginfo] target(s) in 2.34s"#;

    let filtered = filter_cargo_build_or_check(raw, 0);
    assert!(!filtered.contains("Compiling libc"));
    assert!(!filtered.contains("Compiling syn"));
    assert!(filtered.contains("warning: unused variable: `x`"));
    assert!(filtered.contains("Finished `dev` profile"));
}

#[test]
fn test_clean_rtk_stderr_noise_only_warning() {
    let raw = "[rtk] /!\\ No hook installed — run `rtk init -g` for automatic token savings";
    assert_eq!(clean_rtk_stderr_noise(raw), "");
}

#[test]
fn test_clean_rtk_stderr_noise_outdated_warning() {
    let raw = "[rtk] /!\\ Hook outdated — run `rtk init -g` to update";
    assert_eq!(clean_rtk_stderr_noise(raw), "");
}

#[test]
fn test_clean_rtk_stderr_noise_mixed_with_real_error() {
    let raw = "[rtk] /!\\ No hook installed — run `rtk init -g` for automatic token savings\nfatal: not a git repository (or any of the parent directories): .git\n";
    assert_eq!(
        clean_rtk_stderr_noise(raw),
        "fatal: not a git repository (or any of the parent directories): .git"
    );
}

#[test]
fn test_clean_rtk_stderr_noise_no_warning() {
    let raw = "some regular stderr message\nanother error line";
    assert_eq!(clean_rtk_stderr_noise(raw), raw);
}

#[test]
fn test_filter_command_output_dispatch() {
    let git_raw = "On branch main\n  (use \"git add\"...)\nmodified: a.rs";
    let filtered = filter_command_output("git status", git_raw, 0);
    assert!(!filtered.contains("use \"git add"));
    assert!(filtered.contains("modified: a.rs"));

    let other_raw = "Hello World\nLine 2";
    let not_filtered = filter_command_output("echo hello", other_raw, 0);
    assert_eq!(not_filtered, "Hello World\nLine 2");
}

// ── RTK git status 格式测试（issue #207）──────────────────────────

#[test]
fn test_filter_git_status_rtk_clean_output() {
    // RTK `rtk git status` 在干净工作区输出 `clean — nothing to commit`（em dash）
    let rtk_clean = "* main\nclean — nothing to commit";
    let filtered = filter_git_status(rtk_clean);
    assert!(!filtered.contains("clean — nothing to commit"), "RTK 干净工作区噪音应被过滤，实际：{filtered}");
    assert!(filtered.contains("* main"), "分支信息应保留，实际：{filtered}");
}

#[test]
fn test_filter_command_output_rtk_git_status_clean() {
    // 经过 filter_command_output 入口，RTK 重写的 git status 干净输出应被过滤
    let rtk_output = "* feature/my-branch\nclean — nothing to commit";
    let filtered = filter_command_output("git status", rtk_output, 0);
    assert!(!filtered.contains("nothing to commit"), "RTK 格式的 git status 噪音应经 filter_command_output 被过滤，实际：{filtered}");
    assert!(filtered.contains("* feature/my-branch"), "分支信息应保留，实际：{filtered}");
}
