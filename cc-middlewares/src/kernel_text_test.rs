use super::*;
use cc_agent::kernel::Kernel;

#[test]
fn test_kernel_text_injection() {
    let pi = Kernel::from_config_checked("pi").unwrap();
    let cases = [
        (
            "call the Read tool with file_path=...",
            "call the read tool with file_path=...",
        ),
        (
            "Grep the full MEMORY.md for relevant entries, then Read their topic files",
            "grep the full MEMORY.md for relevant entries, then read their topic files",
        ),
        (
            "Use Glob to locate relevant files and Read to inspect",
            "Use find to locate relevant files and read to inspect",
        ),
        (
            "write to it directly with the Write tool",
            "write to it directly with the write tool",
        ),
        (
            "invoke Skill with its name. NEVER read SKILL.md directly with Read",
            "invoke skill with its name. NEVER read SKILL.md directly with read",
        ),
        ("use Read on the skill path", "use read on the skill path"),
    ];
    for (input, expected) in cases {
        assert_eq!(dialectize_hint(&pi, input), expected, "input={input}");
    }
    // cc 内核是恒等变换。
    assert_eq!(
        dialectize_hint(&Kernel::cc(), "call the Read tool"),
        "call the Read tool"
    );
}

#[test]
fn test_dialectize_known_hints_does_not_touch_data() {
    let pi = Kernel::from_config_checked("pi").unwrap();
    let data = "fn main() { let Read = 1; }";
    assert_eq!(dialectize_known_hints(&pi, data), data);
    let hint = "[If your answer depends on omitted content, you must call the Read tool with file_path=\"x\"]";
    let out = dialectize_known_hints(&pi, hint);
    assert!(out.contains("call the read tool"), "unexpected: {out}");
}
