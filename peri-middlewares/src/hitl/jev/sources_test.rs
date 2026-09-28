//! sources.rs 单元测试

use super::*;

fn write_hook(dir: &Path, name: &str, body: &str) {
    let hooks = dir.join(".claude").join("hooks");
    std::fs::create_dir_all(&hooks).unwrap();
    std::fs::write(hooks.join(name), body).unwrap();
}

#[test]
fn test_collect_hook_rules_none_without_dir() {
    let dir = tempfile::tempdir().unwrap();
    assert!(collect_hook_rules(&dir.path().to_string_lossy()).is_none());
}

#[test]
fn test_collect_hook_rules_reads_sh_sorted() {
    let dir = tempfile::tempdir().unwrap();
    write_hook(dir.path(), "b.sh", "# 规则 B\nexit 0\n");
    write_hook(dir.path(), "a.sh", "# 规则 A\nexit 0\n");
    // 非 .sh 不应被采集
    write_hook(dir.path(), "notes.md", "不该出现");

    let out = collect_hook_rules(&dir.path().to_string_lossy()).unwrap();
    let a = out.find("### hook: a.sh").unwrap();
    let b = out.find("### hook: b.sh").unwrap();
    assert!(a < b, "应按文件名排序: {out}");
    assert!(!out.contains("不该出现"), "非 .sh 应被忽略: {out}");
}

#[test]
fn test_collect_hook_rules_captures_blocked_messages() {
    // hook 里的 BLOCKED/原因 消息本身就是自然语言规则——必须被采集到
    let dir = tempfile::tempdir().unwrap();
    write_hook(
        dir.path(),
        "guard.sh",
        "if echo \"$INPUT\" | grep -qE 'git\\s+push'; then\n\
         \x20 echo \"[BLOCKED] git push 被拦截\" >&2\n\
         \x20 echo \"原因：CLAUDE.md 规定禁止任何情况下自动 git push。\" >&2\n\
         \x20 exit 2\nfi\n",
    );
    let out = collect_hook_rules(&dir.path().to_string_lossy()).unwrap();
    assert!(out.contains("禁止任何情况下自动 git push"), "应采集到规则文本: {out}");
}

#[test]
fn test_collect_hook_rules_skips_empty_files() {
    let dir = tempfile::tempdir().unwrap();
    write_hook(dir.path(), "empty.sh", "   \n\n");
    assert!(collect_hook_rules(&dir.path().to_string_lossy()).is_none());
}
