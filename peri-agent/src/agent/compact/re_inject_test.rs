use super::*;
use crate::messages::{MessageContent, ToolCallRequest};
use serde_json::json;

fn ai_read_file(tc_id: &str, path: &str) -> BaseMessage {
    BaseMessage::ai_with_tool_calls(
        MessageContent::text("reading file"),
        vec![ToolCallRequest::new(tc_id, "Read", json!({"path": path}))],
    )
}

fn ai_skill_preload(index: usize, skill_path: &str) -> BaseMessage {
    BaseMessage::ai_with_tool_calls(
        MessageContent::text(""),
        vec![ToolCallRequest::new(
            format!("skill_preload_{}", index),
            "Read",
            json!({"path": skill_path}),
        )],
    )
}

fn ai_plain(text: &str) -> BaseMessage {
    BaseMessage::ai(text)
}

fn create_temp_file(dir: &std::path::Path, name: &str, content: &str) -> String {
    let file_path = dir.join(name);
    std::fs::write(&file_path, content).unwrap();
    file_path.to_string_lossy().to_string()
}

fn create_temp_skill(dir: &std::path::Path, name: &str, content: &str) -> String {
    let skill_dir = dir.join(".claude").join("skills").join(name);
    std::fs::create_dir_all(&skill_dir).unwrap();
    let skill_path = skill_dir.join("SKILL.md");
    std::fs::write(&skill_path, content).unwrap();
    skill_path.to_string_lossy().to_string()
}

// is_skills_path tests
#[test]
fn test_is_skills_path_cclaude() {
    assert!(is_skills_path(
        "/home/user/.claude/skills/my-skill/SKILL.md"
    ));
}

#[test]
fn test_is_skills_path_project() {
    assert!(is_skills_path("/project/.claude/skills/other/SKILL.md"));
}

#[test]
fn test_is_skills_path_custom_dir() {
    assert!(is_skills_path("/custom/skills/my-skill/SKILL.md"));
}

#[test]
fn test_is_skills_path_normal_file() {
    assert!(!is_skills_path("/project/src/main.rs"));
}

#[test]
fn test_is_skills_path_skills_but_not_skill_md() {
    assert!(is_skills_path("/project/.claude/skills/some-config.json"));
}

// extract_recent_files tests
#[test]
fn test_extract_recent_files_basic() {
    let msgs = vec![
        ai_read_file("tc1", "/a.rs"),
        ai_read_file("tc2", "/b.rs"),
        ai_read_file("tc3", "/c.rs"),
    ];
    let paths = extract_recent_files(&msgs, 2);
    assert_eq!(paths, vec!["/c.rs", "/b.rs"]);
}

#[test]
fn test_extract_recent_files_dedup() {
    let msgs = vec![
        ai_read_file("tc1", "/a.rs"),
        ai_plain("done"),
        ai_read_file("tc2", "/a.rs"),
    ];
    let paths = extract_recent_files(&msgs, 5);
    assert_eq!(paths, vec!["/a.rs"]);
}

#[test]
fn test_extract_recent_files_excludes_skills() {
    let msgs = vec![
        ai_read_file("tc1", "/project/.claude/skills/test/SKILL.md"),
        ai_read_file("tc2", "/src/main.rs"),
    ];
    let paths = extract_recent_files(&msgs, 5);
    assert_eq!(paths, vec!["/src/main.rs"]);
}

#[test]
fn test_extract_recent_files_empty() {
    let msgs = vec![ai_plain("no tools")];
    let paths = extract_recent_files(&msgs, 5);
    assert!(paths.is_empty());
}

#[test]
fn test_extract_recent_files_mixed_message_types() {
    // Tool 消息和 Human 消息没有 tool_calls，不应影响提取
    let msgs = vec![
        BaseMessage::human("question"),
        ai_read_file("tc1", "/a.rs"),
        BaseMessage::tool_result("tc1", "content"),
        ai_read_file("tc2", "/b.rs"),
    ];
    let paths = extract_recent_files(&msgs, 5);
    assert_eq!(paths, vec!["/b.rs", "/a.rs"]);
}

#[test]
fn test_extract_recent_files_max_files() {
    let msgs: Vec<BaseMessage> = (0..10)
        .map(|i| ai_read_file(&format!("tc{}", i), &format!("/file{}.rs", i)))
        .collect();
    let paths = extract_recent_files(&msgs, 3);
    assert_eq!(paths.len(), 3);
    assert_eq!(paths[0], "/file9.rs");
}

// extract_skills_paths tests
#[test]
fn test_extract_skills_paths_basic() {
    let msgs = vec![
        ai_skill_preload(0, "/home/.claude/skills/a/SKILL.md"),
        ai_skill_preload(1, "/home/.claude/skills/b/SKILL.md"),
    ];
    let paths = extract_skills_paths(&msgs);
    assert_eq!(paths.len(), 2);
}

#[test]
fn test_extract_skills_paths_dedup() {
    let msgs = vec![
        ai_skill_preload(0, "/skills/a/SKILL.md"),
        ai_skill_preload(1, "/skills/a/SKILL.md"),
    ];
    let paths = extract_skills_paths(&msgs);
    assert_eq!(paths.len(), 1);
}

#[test]
fn test_extract_skills_paths_excludes_normal_files() {
    let msgs = vec![
        ai_read_file("tc1", "/src/main.rs"),
        ai_skill_preload(0, "/skills/x/SKILL.md"),
    ];
    let paths = extract_skills_paths(&msgs);
    assert_eq!(paths.len(), 1);
    assert_eq!(paths[0], "/skills/x/SKILL.md");
}

#[test]
fn test_extract_skills_paths_empty() {
    let msgs = vec![ai_plain("no tools")];
    let paths = extract_skills_paths(&msgs);
    assert!(paths.is_empty());
}

#[test]
fn test_extract_skills_paths_from_human_message() {
    // Arrange
    let content =
        "[Skill: /home/.claude/skills/commit/SKILL.md]\n---\nname: commit\n---\n\nCommit content.";
    let msgs = vec![BaseMessage::human(content)];

    // Act
    let paths = extract_skills_paths(&msgs);

    // Assert
    assert_eq!(paths.len(), 1);
    assert_eq!(paths[0], "/home/.claude/skills/commit/SKILL.md");
}

#[test]
fn test_extract_skills_paths_multiple_in_human_message() {
    // Arrange
    let content = "[Skill: /home/.claude/skills/a/SKILL.md]\ncontent a\n\n[Skill: /home/.claude/skills/b/SKILL.md]\ncontent b";
    let msgs = vec![BaseMessage::human(content)];

    // Act
    let paths = extract_skills_paths(&msgs);

    // Assert
    assert_eq!(paths.len(), 2);
}

#[test]
fn test_extract_skills_paths_dedup_across_formats() {
    // Arrange：旧格式 tool_calls + Human 消息包含同一路径
    let msgs = vec![
        BaseMessage::human("[Skill: /skills/x/SKILL.md]\nskill content"),
        ai_skill_preload(0, "/skills/x/SKILL.md"),
    ];

    // Act
    let paths = extract_skills_paths(&msgs);

    // Assert
    assert_eq!(paths.len(), 1, "跨格式同一路径应去重");
}

#[test]
fn test_extract_skills_paths_human_message_non_skill_path() {
    // Arrange：Human 消息包含 [Skill: ...] 但路径不是 skills 目录
    let content = "[Skill: /src/main.rs]\nfile content";
    let msgs = vec![BaseMessage::human(content)];

    // Act
    let paths = extract_skills_paths(&msgs);

    // Assert
    assert!(paths.is_empty(), "非 skills 路径应被过滤");
}

// truncate_to_budget tests
#[test]
fn test_truncate_to_budget_within_budget() {
    let mut contents: Vec<(String, String)> = (0..3)
        .map(|i| (format!("/f{}", i), "x".repeat(1000)))
        .collect();
    let count = truncate_to_budget(&mut contents, 5000);
    assert_eq!(count, 3);
}

#[test]
fn test_truncate_to_budget_exceeds_budget() {
    let mut contents: Vec<(String, String)> = (0..3)
        .map(|i| (format!("/f{}", i), "x".repeat(8000)))
        .collect();
    let count = truncate_to_budget(&mut contents, 5000);
    assert_eq!(count, 2);
}

#[test]
fn test_truncate_to_budget_empty() {
    let mut contents: Vec<(String, String)> = vec![];
    let count = truncate_to_budget(&mut contents, 5000);
    assert_eq!(count, 0);
}

#[test]
fn test_truncate_to_budget_exact_boundary() {
    // 3 个文件各 4000 字符，总计 12000 字符，budget = 12000 tokens = 48000 字符
    let mut contents: Vec<(String, String)> = (0..3)
        .map(|i| (format!("/f{}", i), "x".repeat(4000)))
        .collect();
    let count = truncate_to_budget(&mut contents, 12000);
    assert_eq!(count, 3, "恰好等于 budget 时应全部保留");
}

#[test]
fn test_truncate_to_budget_single_exceeds() {
    // 单个文件超过 budget
    let mut contents: Vec<(String, String)> = vec![("/big".to_string(), "x".repeat(10000))];
    let count = truncate_to_budget(&mut contents, 100);
    assert_eq!(count, 0, "单个文件超过 budget 时不应保留");
}

// read_file_with_budget tests
#[tokio::test]
async fn test_read_file_with_budget_basic() {
    let dir = tempfile::tempdir().unwrap();
    let path = create_temp_file(dir.path(), "test.txt", "hello world");
    let result = read_file_with_budget(&path, 100).await;
    assert_eq!(result, Some("hello world".to_string()));
}

#[tokio::test]
async fn test_read_file_with_budget_truncation() {
    let dir = tempfile::tempdir().unwrap();
    let path = create_temp_file(dir.path(), "big.txt", &"x".repeat(1000));
    let result = read_file_with_budget(&path, 10).await;
    assert!(result.unwrap().ends_with("...(已截断)"));
}

#[tokio::test]
async fn test_read_file_with_budget_nonexistent() {
    let result = read_file_with_budget("/nonexistent/file.txt", 100).await;
    assert_eq!(result, None);
}

// re_inject integration tests
#[tokio::test]
async fn test_re_inject_with_files() {
    let dir = tempfile::tempdir().unwrap();
    let f1 = create_temp_file(dir.path(), "a.rs", "fn main() {}");
    let f2 = create_temp_file(dir.path(), "b.rs", "fn helper() {}");
    let msgs = vec![ai_read_file("tc1", &f1), ai_read_file("tc2", &f2)];
    let config = CompactConfig::default();
    let result = re_inject(&msgs, &config, dir.path().to_str().unwrap()).await;
    assert_eq!(result.files_injected, 2);
    assert_eq!(result.skills_injected, 0);
    assert_eq!(result.messages.len(), 2);
    assert!(result.messages[0].content().contains("[最近读取的文件:"));
}

#[tokio::test]
async fn test_re_inject_with_skills() {
    let dir = tempfile::tempdir().unwrap();
    let skill_path = create_temp_skill(dir.path(), "my-skill", "# My Skill\nDo stuff");
    let msgs = vec![ai_skill_preload(0, &skill_path)];
    let config = CompactConfig::default();
    let result = re_inject(&msgs, &config, dir.path().to_str().unwrap()).await;
    assert_eq!(result.files_injected, 0);
    assert_eq!(result.skills_injected, 1);
    assert_eq!(result.messages.len(), 1);
    assert!(result.messages[0].content().contains("[激活的 Skill 指令:"));
}

#[tokio::test]
async fn test_re_inject_with_both() {
    let dir = tempfile::tempdir().unwrap();
    let f1 = create_temp_file(dir.path(), "a.rs", "code");
    let skill_path = create_temp_skill(dir.path(), "s1", "# Skill");
    let msgs = vec![ai_read_file("tc1", &f1), ai_skill_preload(0, &skill_path)];
    let config = CompactConfig::default();
    let result = re_inject(&msgs, &config, dir.path().to_str().unwrap()).await;
    assert!(result.files_injected >= 1);
    assert!(result.skills_injected >= 1);
}

#[tokio::test]
async fn test_re_inject_empty_messages() {
    let config = CompactConfig::default();
    let result = re_inject(&[], &config, "/tmp").await;
    assert_eq!(result.files_injected, 0);
    assert_eq!(result.skills_injected, 0);
    assert!(result.messages.is_empty());
}

#[tokio::test]
async fn test_re_inject_no_matching_files() {
    let msgs = vec![BaseMessage::ai_with_tool_calls(
        MessageContent::text("running"),
        vec![ToolCallRequest::new(
            "tc1",
            "Bash",
            json!({"command": "ls"}),
        )],
    )];
    let config = CompactConfig::default();
    let result = re_inject(&msgs, &config, "/tmp").await;
    assert_eq!(result.files_injected, 0);
    assert!(result.messages.is_empty());
}

#[tokio::test]
async fn test_re_inject_file_not_found() {
    let msgs = vec![ai_read_file("tc1", "/nonexistent/file.rs")];
    let config = CompactConfig::default();
    let result = re_inject(&msgs, &config, "/tmp").await;
    assert_eq!(result.files_injected, 0);
}

#[tokio::test]
async fn test_re_inject_respects_file_budget() {
    let dir = tempfile::tempdir().unwrap();
    let msgs: Vec<BaseMessage> = (0..3)
        .map(|i| {
            let f = create_temp_file(dir.path(), &format!("f{}.rs", i), &"x".repeat(8000));
            ai_read_file(&format!("tc{}", i), &f)
        })
        .collect();
    let config = CompactConfig {
        re_inject_file_budget: 5000,
        re_inject_max_files: 5,
        ..Default::default()
    };
    let result = re_inject(&msgs, &config, dir.path().to_str().unwrap()).await;
    assert!(result.files_injected < 3);
}

#[tokio::test]
async fn test_re_inject_respects_max_files() {
    let dir = tempfile::tempdir().unwrap();
    let msgs: Vec<BaseMessage> = (0..10)
        .map(|i| {
            let f = create_temp_file(dir.path(), &format!("f{}.rs", i), "content");
            ai_read_file(&format!("tc{}", i), &f)
        })
        .collect();
    let config = CompactConfig {
        re_inject_max_files: 3,
        ..Default::default()
    };
    let result = re_inject(&msgs, &config, dir.path().to_str().unwrap()).await;
    assert!(result.files_injected <= 3);
}

#[tokio::test]
async fn test_re_inject_relative_path_resolution() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("src")).unwrap();
    let _ = create_temp_file(&dir.path().join("src"), "main.rs", "fn main() {}");
    let msgs = vec![ai_read_file("tc1", "src/main.rs")];
    let config = CompactConfig::default();
    let result = re_inject(&msgs, &config, dir.path().to_str().unwrap()).await;
    assert!(result.files_injected >= 1);
}
