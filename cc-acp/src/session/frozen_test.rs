use super::*;
use crate::provider::config::AppConfig;

/// #359 回归：`claude_md_excludes` 必须贯通**冻结路径**。
///
/// 此前 `session/new` 用 `AgentsMdConfig::default()` 渲染冻结指引（不含 excludes），
/// 主会话的排除配置**完全无效**；中间件那侧也过滤不了（拿到的是成品字符串）。
#[test]
fn test_frozen_instructions_respect_excludes() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join(".git")).unwrap();
    std::fs::write(dir.path().join("AGENTS.md"), "AGENTS_BODY").unwrap();
    std::fs::write(dir.path().join("CLAUDE.md"), "CLAUDE_BODY").unwrap();

    let app_config = AppConfig {
        claude_md_excludes: Some(vec![dir.path().join("AGENTS.md").display().to_string()]),
        ..AppConfig::default()
    };

    let data = build_frozen_session_data(
        dir.path().to_str().unwrap(),
        app_config,
        &[],
        &[],
        "2026-10-08",
        None,
    );

    let instructions = data.instructions.expect("应有指引注入");
    assert!(
        !instructions.contains("AGENTS_BODY"),
        "被 excludes 命中的文件不得进入冻结指引: {instructions}"
    );
    assert!(instructions.contains("CLAUDE_BODY"), "{instructions}");
}

/// 反向对照：没有 excludes 时两个候选都应进入冻结指引（证明上一条不是在“什么都没注入”下蒙对）。
#[test]
fn test_frozen_instructions_without_excludes_loads_all() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join(".git")).unwrap();
    std::fs::write(dir.path().join("AGENTS.md"), "AGENTS_BODY").unwrap();
    std::fs::write(dir.path().join("CLAUDE.md"), "CLAUDE_BODY").unwrap();

    let data = build_frozen_session_data(
        dir.path().to_str().unwrap(),
        AppConfig::default(),
        &[],
        &[],
        "2026-10-08",
        None,
    );

    let instructions = data.instructions.expect("应有指引注入");
    assert!(instructions.contains("AGENTS_BODY"), "{instructions}");
    assert!(instructions.contains("CLAUDE_BODY"), "{instructions}");
}
