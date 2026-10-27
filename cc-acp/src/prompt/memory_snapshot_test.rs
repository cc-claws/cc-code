use super::*;
use cc_agent::{
    agent::state::{AgentState, State},
    middleware::r#trait::Middleware,
};

fn make_memory_prompt(cwd: &str, memory: &AutoMemorySnapshot) -> String {
    build_system_prompt_with_memory(
        None,
        cwd,
        PromptFeatures {
            memory_enabled: true,
            ..PromptFeatures::none()
        },
        &[],
        Some("2026-10-26"),
        None,
        Some(memory),
    )
}

#[test]
fn test_memory_snapshot_renders_one_index_after_cache_boundary() {
    let directory = tempfile::tempdir().unwrap();
    let entrypoint = directory.path().join("MEMORY.md");
    std::fs::write(&entrypoint, "- [MEMORY_SNAPSHOT_INITIAL](topic.md)").unwrap();
    let snapshot = AutoMemorySnapshot::load(directory.path().into(), "~/project/memory/".into());
    std::fs::write(&entrypoint, "- [MEMORY_SNAPSHOT_CHANGED](other.md)").unwrap();
    let prompt = make_memory_prompt(directory.path().to_str().unwrap(), &snapshot);
    let boundary = prompt
        .find("__SYSTEM_PROMPT_DYNAMIC_BOUNDARY__")
        .expect("应有缓存边界");
    assert_eq!(
        prompt.matches("MEMORY_SNAPSHOT_INITIAL").count(),
        1,
        "冻结索引只能出现一次"
    );
    assert!(
        !prompt[..boundary].contains("MEMORY_SNAPSHOT_INITIAL"),
        "记忆不能改变缓存前缀"
    );
    assert!(
        !prompt.contains("MEMORY_SNAPSHOT_CHANGED"),
        "渲染不得再次读取磁盘"
    );
    assert!(
        !prompt.contains("## Types of memory"),
        "主请求不应携带完整写入教学"
    );
    assert_eq!(snapshot.rendered_entrypoint(), "Contents of ~/project/memory/MEMORY.md (auto-memory):\n\n- [MEMORY_SNAPSHOT_INITIAL](topic.md)", "兼容字段必须派生自同一次读取");
}

#[tokio::test]
async fn test_memory_snapshot_empty_remains_empty_until_new_capture() {
    let directory = tempfile::tempdir().unwrap();
    let cwd = directory.path().to_str().unwrap();
    let snapshot = AutoMemorySnapshot::load(directory.path().into(), "~/project/memory/".into());
    let initial_prompt = make_memory_prompt(cwd, &snapshot);
    std::fs::write(directory.path().join("MEMORY.md"), "MEMORY_CREATED_LATER").unwrap();
    let later_prompt = make_memory_prompt(cwd, &snapshot);
    let mut state = AgentState::new(cwd);
    let compatibility_middleware = cc_middlewares::MemoryMiddleware::new()
        .with_frozen_entrypoint(Some(snapshot.rendered_entrypoint()));
    let result = compatibility_middleware.before_agent(&mut state).await;
    assert!(result.is_ok(), "冻结空记忆中间件应正常返回");
    assert_eq!(initial_prompt, later_prompt, "空快照不能因中途建文件而改变");
    assert!(state.messages().is_empty(), "Some空串不得回退读盘注入");
    let next_snapshot =
        AutoMemorySnapshot::load(directory.path().into(), "~/project/memory/".into());
    let next_prompt = make_memory_prompt(cwd, &next_snapshot);
    assert_eq!(
        next_prompt.matches("MEMORY_CREATED_LATER").count(),
        1,
        "新快照才能看到新索引"
    );
}

#[test]
fn test_memory_snapshot_updates_only_on_new_capture() {
    let directory = tempfile::tempdir().unwrap();
    let cwd = directory.path().to_str().unwrap();
    let entrypoint = directory.path().join("MEMORY.md");
    std::fs::write(&entrypoint, "MEMORY_PREVIOUS_SESSION").unwrap();
    let previous = AutoMemorySnapshot::load(directory.path().into(), "~/project/memory/".into());
    let initial_prompt = make_memory_prompt(cwd, &previous);
    std::fs::write(&entrypoint, "MEMORY_NEXT_SESSION").unwrap();
    let repeated_prompt = make_memory_prompt(cwd, &previous);
    let next = AutoMemorySnapshot::load(directory.path().into(), "~/project/memory/".into());
    let next_prompt = make_memory_prompt(cwd, &next);
    assert_eq!(
        initial_prompt, repeated_prompt,
        "同一会话的提示词应保持完全稳定"
    );
    assert!(
        !next_prompt.contains("MEMORY_PREVIOUS_SESSION"),
        "新会话不应沿用过期快照"
    );
    assert_eq!(
        next_prompt.matches("MEMORY_NEXT_SESSION").count(),
        1,
        "新会话索引只能注入一次"
    );
}

#[test]
fn test_memory_snapshot_disabled_does_not_inject_context() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(
        directory.path().join("MEMORY.md"),
        "DISABLED_MEMORY_SENTINEL",
    )
    .unwrap();
    let snapshot = AutoMemorySnapshot::load(directory.path().into(), "~/project/memory/".into());
    let prompt = build_system_prompt_with_memory(
        None,
        directory.path().to_str().unwrap(),
        PromptFeatures::none(),
        &[],
        Some("2026-10-26"),
        None,
        Some(&snapshot),
    );
    assert!(
        !prompt.contains("DISABLED_MEMORY_SENTINEL"),
        "关闭记忆功能不得注入索引"
    );
}
