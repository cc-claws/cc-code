//! redact.rs 单元测试

use super::*;

#[test]
fn test_redact_pem_private_key() {
    let text = "-----BEGIN RSA PRIVATE KEY-----\nMIIEow...\n-----END RSA PRIVATE KEY-----";
    assert!(!redact_secrets(text).contains("MIIEow"));
    assert!(redact_secrets(text).contains("REDACTED"));
}

#[test]
fn test_redact_bearer_token() {
    let out = redact_secrets("Authorization: Bearer abcdefghijklmnop1234");
    assert!(!out.contains("abcdefghijklmnop1234"), "got {out}");
}

#[test]
fn test_redact_sk_key() {
    let out = redact_secrets("key sk-abcdefghijklmnopqrstuvwx");
    assert!(!out.contains("sk-abcdefghijklmnopqrstuvwx"), "got {out}");
}

#[test]
fn test_redact_generic_assignments() {
    let out = redact_secrets("PASSWORD=hunter2secret TOKEN=abcdef123456");
    assert!(!out.contains("hunter2secret"), "got {out}");
    assert!(!out.contains("abcdef123456"), "got {out}");
}

#[test]
fn test_redact_leaves_ordinary_text() {
    assert_eq!(redact_secrets("ls -la"), "ls -la");
    assert_eq!(redact_secrets("git status"), "git status");
}

#[test]
fn test_truncate_cjk_safe() {
    let s = "中文中文中文中文";
    let t = truncate(s, 2);
    assert_eq!(t, "中文...");
    assert!(!truncate("short", 10).contains("..."));
}
