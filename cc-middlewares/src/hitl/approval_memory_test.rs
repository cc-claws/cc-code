use super::*;
use std::path::Path;

fn fp(tool: &str, path: &str) -> Option<Fingerprint> {
    ApprovalMemory::fingerprint(tool, Some(Path::new(path)), Path::new("/repo"))
}

#[test]
fn test_fingerprint_normalizes_absolute_path() {
    // 绝对路径：`..` 与 `.` 折叠后同一目标命中同一条目
    let a = fp("Edit", "/repo/src/../src/lib.rs").unwrap();
    let b = fp("Edit", "/repo/src/lib.rs").unwrap();
    assert_eq!(a, b);
}

#[test]
fn test_fingerprint_relative_path_joined_with_cwd() {
    let a = fp("Edit", "src/lib.rs").unwrap();
    let b = fp("Edit", "/repo/src/lib.rs").unwrap();
    assert_eq!(a, b);
}

#[test]
fn test_fingerprint_none_path_is_not_tracked() {
    // 命令类工具（无 path）不参与记忆
    assert!(ApprovalMemory::fingerprint("Bash", None, Path::new("/repo")).is_none());
}

#[test]
fn test_fingerprint_distinguishes_tool_name() {
    let a = fp("Edit", "/repo/src/lib.rs").unwrap();
    let b = fp("Write", "/repo/src/lib.rs").unwrap();
    assert_ne!(a, b);
}

#[test]
fn test_fingerprint_distinguishes_path() {
    let a = fp("Edit", "/repo/a.rs").unwrap();
    let b = fp("Edit", "/repo/b.rs").unwrap();
    assert_ne!(a, b);
}

#[test]
fn test_record_and_is_approved() {
    let mem = ApprovalMemory::new();
    let key = fp("Edit", "/repo/src/lib.rs").unwrap();
    assert!(!mem.is_approved(&key));
    mem.record(key.clone());
    assert!(mem.is_approved(&key));
    assert_eq!(mem.len(), 1);
}

#[test]
fn test_clear_removes_all() {
    let mem = ApprovalMemory::new();
    mem.record(fp("Edit", "/repo/a.rs").unwrap());
    mem.record(fp("Edit", "/repo/b.rs").unwrap());
    assert_eq!(mem.len(), 2);
    mem.clear();
    assert_eq!(mem.len(), 0);
}

#[test]
fn test_record_is_idempotent() {
    let mem = ApprovalMemory::new();
    let key = fp("Edit", "/repo/a.rs").unwrap();
    mem.record(key.clone());
    mem.record(key.clone());
    assert_eq!(mem.len(), 1);
}
