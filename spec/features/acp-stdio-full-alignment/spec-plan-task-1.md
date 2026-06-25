### Task 1: 新增 dispatch/commands.rs — 统一 build_available_commands

**背景：** `build_available_commands()` 函数在 TUI 路径（`notify.rs:121-156`）和 stdio 路径（`acp_stdio.rs:89-128`）存在完全相同的实现。该函数接收 `&[SkillMetadata]` 返回 `Vec<AvailableCommand>`，无任何 transport 依赖。

#### 执行步骤

- [ ] **Step 1.1**: 创建 `peri-acp/src/dispatch/commands.rs`

```rust
//! Build ACP available commands list, shared by TUI and stdio transports.

use agent_client_protocol_schema::AvailableCommand;
use peri_middlewares::skills::SkillMetadata;

/// Build the list of available slash commands for ACP clients,
/// including discovered skills as `skill:<name>` entries.
pub fn build_available_commands(skills: &[SkillMetadata]) -> Vec<AvailableCommand> {
    let mut commands = vec![
        AvailableCommand::new("help", "Show available commands and their descriptions"),
        AvailableCommand::new("clear", "Clear the current conversation"),
        AvailableCommand::new("compact", "Compress the conversation history to save context"),
        AvailableCommand::new("context", "Display context usage / token statistics"),
        AvailableCommand::new("cost", "Show token usage and estimated cost"),
        AvailableCommand::new("model", "Switch the current LLM model"),
        AvailableCommand::new("mode", "Switch the current permission mode"),
        AvailableCommand::new("effort", "Configure LLM reasoning/thinking effort"),
        AvailableCommand::new("loop", "Control agent iteration loop"),
        AvailableCommand::new("history", "View and resume previous conversations"),
        AvailableCommand::new("doctor", "Diagnose configuration and connection issues"),
        AvailableCommand::new("mcp", "Manage MCP (Model Context Protocol) servers"),
        AvailableCommand::new("hooks", "Manage Claude Code hooks"),
        AvailableCommand::new("plugin", "Manage installed plugins"),
        AvailableCommand::new("cron", "Manage scheduled/cron tasks"),
        AvailableCommand::new("agents", "Manage sub-agent definitions"),
        AvailableCommand::new("memory", "Manage persistent memory entries"),
        AvailableCommand::new("login", "Configure authentication"),
        AvailableCommand::new("split", "Manage split session layouts"),
        AvailableCommand::new("rename", "Rename the current session"),
        AvailableCommand::new("lang", "Switch display language / locale"),
        AvailableCommand::new("exit", "Exit the application"),
    ];
    for skill in skills {
        commands.push(AvailableCommand::new(
            format!("skill:{}", skill.name),
            skill.description.clone(),
        ));
    }
    commands
}
```

**注意：** 此文件内容严格复制自 `notify.rs:121-156` 的 `build_available_commands()`，仅将 `fn` 改为 `pub fn`。

- [ ] **Step 1.2**: 添加单元测试文件 `peri-acp/src/dispatch/commands_test.rs`

```rust
use super::commands::build_available_commands;
use peri_middlewares::skills::SkillMetadata;

#[test]
fn test_build_available_commands_includes_builtins() {
    let cmds = build_available_commands(&[]);
    // 内置命令数量验证
    assert!(cmds.len() >= 22, "至少 22 个内置命令");
    // 验证关键命令存在
    let names: Vec<&str> = cmds.iter().map(|c| c.name.as_ref()).collect();
    assert!(names.contains(&"help"), "help 命令应存在");
    assert!(names.contains(&"clear"), "clear 命令应存在");
    assert!(names.contains(&"compact"), "compact 命令应存在");
    assert!(names.contains(&"model"), "model 命令应存在");
}

#[test]
fn test_build_available_commands_includes_skills() {
    let skills = vec![
        SkillMetadata { name: "my-skill".into(), description: "My custom skill".into() },
        SkillMetadata { name: "other".into(), description: "Other skill".into() },
    ];
    let cmds = build_available_commands(&skills);
    let names: Vec<&str> = cmds.iter().map(|c| c.name.as_ref()).collect();
    assert!(names.contains(&"skill:my-skill"), "skill:my-skill 应存在");
    assert!(names.contains(&"skill:other"), "skill:other 应存在");
}

#[test]
fn test_build_available_commands_no_skills_only_builtins() {
    let cmds = build_available_commands(&[]);
    assert!(!cmds.iter().any(|c| c.name.as_ref().starts_with("skill:")),
        "无 skills 时不应包含 skill: 前缀命令");
}
```

#### 检查步骤

- [ ] `cargo build -p peri-acp` 编译通过
- [ ] `cargo test -p peri-acp --lib commands_test` 测试通过
- [ ] `cargo clippy -p peri-acp` 通过

---
