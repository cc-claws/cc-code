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

// ── fold_repeated_lines 测试 ────────────────────────────────────────

#[test]
fn test_fold_repeated_lines_below_threshold() {
    // 连续相同行 < 3 次不折叠
    let input = "AAA\nAAA\nBBB";
    assert_eq!(fold_repeated_lines(input), "AAA\nAAA\nBBB");
}

#[test]
fn test_fold_repeated_lines_at_threshold() {
    // 连续相同行 = 3 次触发折叠：保留前 2 行 + 摘要
    let input = "WARNING: foo\nWARNING: foo\nWARNING: foo";
    let result = fold_repeated_lines(input);
    assert!(
        result.contains("WARNING: foo"),
        "应保留样例行，实际：{result}"
    );
    assert!(
        result.contains("... (+1 more identical lines)"),
        "应有折叠摘要，实际：{result}"
    );
    // 只出现 2 次 WARNING 行（不含摘要行中的）
    assert_eq!(
        result.lines().filter(|l| *l == "WARNING: foo").count(),
        2,
        "应保留 2 个样例，实际：{result}"
    );
}

#[test]
fn test_fold_repeated_lines_many_duplicates() {
    // 10 个相同行 → 保留 2 + 折叠 8
    let input = ["same line"; 10].join("\n");
    let result = fold_repeated_lines(&input);
    assert!(
        result.contains("... (+8 more identical lines)"),
        "应折叠 8 行，实际：{result}"
    );
    assert_eq!(
        result.lines().count(),
        3,
        "总行数应为 3（2 样例 + 1 摘要），实际：{result}"
    );
}

#[test]
fn test_fold_repeated_lines_preserves_unique_lines() {
    // 不相同的行不受影响
    let input = "A\nB\nC\nD";
    assert_eq!(fold_repeated_lines(input), input);
}

#[test]
fn test_fold_repeated_lines_mixed() {
    // 混合场景：唯一行 + 重复行
    let input = "header\nX\nX\nX\nX\nX\nfooter";
    let result = fold_repeated_lines(input);
    assert!(result.starts_with("header"), "header 应保留");
    assert!(result.ends_with("footer"), "footer 应保留");
    assert!(
        result.contains("... (+3 more identical lines)"),
        "应折叠 3 行，实际：{result}"
    );
}

// ── fold_repeated_blocks 测试 ───────────────────────────────────────

#[test]
fn test_fold_repeated_blocks_warning_pattern() {
    // 模拟 Next.js 构建输出：3 个相同首行的 warning 块
    let input = "\
Warning: Dynamic filesystem access
  ./file1.js:10:5
  import trace #1
Warning: Dynamic filesystem access
  ./file2.js:20:3
  import trace #2
Warning: Dynamic filesystem access
  ./file3.js:30:7
  import trace #3";
    let result = fold_repeated_blocks(input);
    // 应保留前 2 个完整块
    assert!(
        result.contains("./file1.js"),
        "第 1 个块应保留，实际：{result}"
    );
    assert!(
        result.contains("./file2.js"),
        "第 2 个块应保留，实际：{result}"
    );
    // 第 3 个块应被折叠
    assert!(
        !result.contains("./file3.js"),
        "第 3 个块应被折叠，实际：{result}"
    );
    assert!(
        result.contains("... (+1 more similar blocks"),
        "应有块折叠摘要，实际：{result}"
    );
}

#[test]
fn test_fold_repeated_blocks_below_threshold() {
    // 同类块 < 3 次不折叠
    let input = "\
Warning: foo
  detail 1
Warning: foo
  detail 2";
    let result = fold_repeated_blocks(input);
    assert_eq!(result, input, "2 个同类块不应折叠");
}

#[test]
fn test_fold_repeated_blocks_preserves_different_blocks() {
    // 不同首行的块各自独立，不应被折叠
    let input = "\
Error: compile failed
  at main.rs:10
Warning: unused var
  at lib.rs:20
Info: build complete
  in 2.34s";
    let result = fold_repeated_blocks(input);
    assert_eq!(result, input, "不同首行的块不应折叠");
}

#[test]
fn test_filter_command_output_folds_repeated_generic_output() {
    // 通过 filter_command_output 入口验证通用折叠生效
    let mut lines = vec!["WARN: deprecated API call"; 5];
    lines.push("Build completed successfully");
    let input = lines.join("\n");
    let result = filter_command_output("npm run build", &input, 0);
    // 块级或行级折叠均可触发——只要重复内容被压缩
    assert!(
        result.contains("more similar blocks") || result.contains("more identical lines"),
        "npm 命令输出应触发通用折叠，实际：{result}"
    );
    assert!(
        result.contains("Build completed successfully"),
        "非重复行应保留，实际：{result}"
    );
    // 验证压缩效果：输出行数应明显少于原始 6 行
    assert!(
        result.lines().count() < 6,
        "折叠后行数应减少，实际：{result}"
    );
}
