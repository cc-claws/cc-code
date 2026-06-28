use super::commands::build_available_commands;
use peri_middlewares::skills::SkillMetadata;
use std::path::PathBuf;

#[test]
fn test_build_available_commands_includes_builtins() {
    let cmds = build_available_commands(&[]);
    // 至少 22 个内置命令
    assert!(cmds.len() >= 20, "至少 20 个内置命令，实际: {}", cmds.len());
    // 验证关键命令存在
    let names: Vec<&str> = cmds.iter().map(|c| c.name.as_str()).collect();
    assert!(names.contains(&"help"), "help 命令应存在");
    assert!(names.contains(&"clear"), "clear 命令应存在");
    assert!(names.contains(&"compact"), "compact 命令应存在");
    assert!(names.contains(&"model"), "model 命令应存在");
    // 回归：commit/review 必须暴露给 TUI 的 Hints/补全
    // 历史 bug：available_commands 列表遗漏 commit/review，
    // 导致 TUI 端永远看不到这两个命令（即便 ACP 能执行）。
    assert!(names.contains(&"commit"), "commit 命令应存在");
    assert!(names.contains(&"review"), "review 命令应存在");
}

#[test]
fn test_build_available_commands_includes_skills() {
    let skills = vec![
        SkillMetadata {
            name: "my-skill".into(),
            description: "My custom skill".into(),
            path: PathBuf::from("/fake/my-skill/SKILL.md"),
        },
        SkillMetadata {
            name: "other".into(),
            description: "Other skill".into(),
            path: PathBuf::from("/fake/other/SKILL.md"),
        },
    ];
    let cmds = build_available_commands(&skills);
    let names: Vec<&str> = cmds.iter().map(|c| c.name.as_str()).collect();
    assert!(names.contains(&"my-skill"), "my-skill 应存在");
    assert!(names.contains(&"other"), "other 应存在");
}

#[test]
fn test_build_available_commands_no_skills_no_leak() {
    let cmds = build_available_commands(&[]);
    assert!(
        !cmds.iter().any(|c| c.name.as_str().starts_with("skill:")),
        "不应包含 skill: 前缀命令"
    );
}
