use super::is_readonly_shell_command;

#[test]
fn test_is_readonly_shell_command_detects_file_inspection() {
    let yes = [
        "grep -rn \"x\" app/ | head -5",
        "cd /work/project && sed -n '159,260p' app/a.php | grep -n needle",
        "grep -rn A app/ ; echo '---' ; grep -rn B app/",
        "ls -la",
        "find . -name '*.rs'",
        "wc -l file",
        "VAR=1 grep x file",
        "cd /tmp && cat a.txt",
    ];
    for cmd in yes {
        assert!(is_readonly_shell_command(cmd), "应判定为只读查看: {cmd}");
    }
    let no = [
        "cargo build -p cc-tui",
        "git show origin/test:app/x.php",
        "cd /work/project && git status",
        "cat a.txt > b.txt",
        "echo hello",
        "cd /tmp",
        "./target/debug/cc-code --version",
        "grep x f | xargs rm",
    ];
    for cmd in no {
        assert!(!is_readonly_shell_command(cmd), "不应判定为只读查看: {cmd}");
    }
}

#[test]
fn test_is_readonly_shell_command_rejects_write_variants() {
    let no = [
        "sed -n -i 's/a/b/' f",
        "find . -name x -delete",
        r"find . -name x -exec rm {} \;",
        "sort -o out.txt in.txt",
        "cat \"$(rm -f important)\"",
        "grep x f | tee out.txt",
    ];
    for cmd in no {
        assert!(!is_readonly_shell_command(cmd), "不应判定为只读查看: {cmd}");
    }
    let yes = [
        "sed -n '1,10p' f",
        "find . -name '*.rs'",
        "sort in.txt | uniq",
    ];
    for cmd in yes {
        assert!(is_readonly_shell_command(cmd), "应判定为只读查看: {cmd}");
    }
}

/// 回归：`2>/dev/null`、`2>&1`、`>&2` 只丢弃/合并 stderr，不是写盘重定向。
#[test]
fn test_is_readonly_shell_command_allows_stderr_redirect() {
    let yes = [
        "grep -rn foo app/ 2>/dev/null | head -5",
        "find . -type f -name '*.sql' 2>/dev/null | head",
        "ls -la 2> /dev/null",
        "cat missing.txt 2>&1 | head -3",
        "grep -n needle f >&2",
    ];
    for cmd in yes {
        assert!(is_readonly_shell_command(cmd), "应判定为只读查看: {cmd}");
    }
    // 同一段里再带上真正的写盘重定向，仍须拒绝
    let no = [
        "grep -rn foo app/ 2>/dev/null > out.txt",
        "ls 2>&1 >> log.txt",
        "find . -name '*.rs' 2>/dev/null | xargs rm",
    ];
    for cmd in no {
        assert!(!is_readonly_shell_command(cmd), "不应判定为只读查看: {cmd}");
    }
}

/// 回归：引号内的 `;`、`|`、`&&` 是参数内容，不能当命令分隔符拆开。
#[test]
fn test_is_readonly_shell_command_ignores_quoted_separators() {
    let yes = [
        r#"grep -n "expand\|shell command\|Ran " cc-tui/src/ui/message_render/mod.rs | head -30"#,
        r#"awk 'NR>=2900 && NR<=2960' f | grep -n "fn \|detail_mode""#,
        r#"grep -rn 'a;b' app/ | head -5"#,
        r#"grep -F 'foo && bar' README.md"#,
        r#"grep -n 'a > b' notes.md"#,
    ];
    for cmd in yes {
        assert!(is_readonly_shell_command(cmd), "应判定为只读查看: {cmd}");
    }
    let no = [
        r#"grep -n "x" app/ && rm important.txt"#,
        r#"cat 'a.txt'; rm b.txt"#,
    ];
    for cmd in no {
        assert!(!is_readonly_shell_command(cmd), "不应判定为只读查看: {cmd}");
    }
}
