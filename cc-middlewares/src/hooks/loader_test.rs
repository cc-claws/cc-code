use super::*;
use crate::hooks::types::HookEvent;
use std::collections::HashMap;
use tempfile::tempdir;

fn make_manifest_with_hooks(hooks: Option<HooksConfig>) -> PluginManifest {
    PluginManifest {
        name: "test-plugin".into(),
        version: "1.0.0".into(),
        description: String::new(),
        author: None,
        commands: None,
        agents: None,
        skills: None,
        hooks,
        mcp_servers: None,
        lsp_servers: None,
        output_styles: None,
        channels: None,
        options: None,
        settings: None,
    }
}

#[test]
fn test_file_priority_over_manifest() {
    let dir = tempdir().unwrap();
    let hooks_dir = dir.path().join("hooks");
    std::fs::create_dir_all(&hooks_dir).unwrap();

    // File has PreToolUse
    let file_config = r#"{
            "PreToolUse": [
                {
                    "matcher": "Bash",
                    "hooks": [{"type": "command", "command": "echo file-hook"}]
                }
            ]
        }"#;
    std::fs::write(hooks_dir.join("hooks.json"), file_config).unwrap();

    // Manifest has PostToolUse
    let mut manifest_hooks: HooksConfig = HashMap::new();
    manifest_hooks.insert(crate::hooks::types::HookEvent::PostToolUse, vec![]);
    let manifest = make_manifest_with_hooks(Some(manifest_hooks));

    let result = extract_hooks(&manifest, dir.path()).unwrap();
    assert!(result.contains_key(&crate::hooks::types::HookEvent::PreToolUse));
    assert!(!result.contains_key(&crate::hooks::types::HookEvent::PostToolUse));
}

#[test]
fn test_fallback_to_manifest_hooks() {
    let dir = tempdir().unwrap();
    // No hooks/hooks.json file

    let mut manifest_hooks: HooksConfig = HashMap::new();
    manifest_hooks.insert(crate::hooks::types::HookEvent::SessionStart, vec![]);
    let manifest = make_manifest_with_hooks(Some(manifest_hooks));

    let result = extract_hooks(&manifest, dir.path()).unwrap();
    assert!(result.contains_key(&crate::hooks::types::HookEvent::SessionStart));
}

#[test]
fn test_both_missing_returns_none() {
    let dir = tempdir().unwrap();
    let manifest = make_manifest_with_hooks(None);

    let result = extract_hooks(&manifest, dir.path());
    assert!(result.is_none());
}

#[test]
fn test_invalid_json_falls_back_to_manifest() {
    let dir = tempdir().unwrap();
    let hooks_dir = dir.path().join("hooks");
    std::fs::create_dir_all(&hooks_dir).unwrap();

    // Invalid JSON in hooks.json
    std::fs::write(hooks_dir.join("hooks.json"), "not valid json").unwrap();

    let mut manifest_hooks: HooksConfig = HashMap::new();
    manifest_hooks.insert(crate::hooks::types::HookEvent::Stop, vec![]);
    let manifest = make_manifest_with_hooks(Some(manifest_hooks));

    // Should fall back to manifest hooks
    let result = extract_hooks(&manifest, dir.path()).unwrap();
    assert!(result.contains_key(&crate::hooks::types::HookEvent::Stop));
}

#[test]
fn test_empty_hooks_returns_empty_hashmap() {
    let dir = tempdir().unwrap();
    let hooks_dir = dir.path().join("hooks");
    std::fs::create_dir_all(&hooks_dir).unwrap();

    std::fs::write(hooks_dir.join("hooks.json"), "{}").unwrap();

    let manifest = make_manifest_with_hooks(None);
    let result = extract_hooks(&manifest, dir.path()).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_load_settings_local_hooks_basic() {
    std::env::set_var("CC_CODE_TRUST_PROJECT_HOOKS", "1");
    let dir = tempdir().unwrap();
    let claude_dir = dir.path().join(".claude");
    std::fs::create_dir_all(&claude_dir).unwrap();

    let settings = serde_json::json!({
        "hooks": {
            "PreToolUse": [
                {
                    "hooks": [
                        {"type": "command", "command": "echo pre"}
                    ]
                }
            ],
            "Notification": [
                {
                    "hooks": [
                        {"type": "command", "command": "echo notify"}
                    ]
                }
            ]
        }
    });
    std::fs::write(
        claude_dir.join("settings.local.json"),
        serde_json::to_string(&settings).unwrap(),
    )
    .unwrap();

    let hooks = load_settings_local_hooks(dir.path().to_str().unwrap());
    assert_eq!(hooks.len(), 2);

    // Verify plugin source
    for h in &hooks {
        assert_eq!(h.plugin_name, "settings.local.json");
    }

    // Check both events are present (order not guaranteed)
    let has_pre = hooks
        .iter()
        .any(|h| matches!(&h.event, HookEvent::PreToolUse));
    let has_notification = hooks
        .iter()
        .any(|h| matches!(&h.event, HookEvent::Notification));
    assert!(has_pre, "should have PreToolUse hook");
    assert!(has_notification, "should have Notification hook");
}

#[test]
fn test_load_settings_local_hooks_no_file() {
    let hooks = load_settings_local_hooks("/nonexistent/path");
    assert!(hooks.is_empty());
}

#[test]
fn test_load_settings_local_hooks_no_hooks_field() {
    let dir = tempdir().unwrap();
    let claude_dir = dir.path().join(".claude");
    std::fs::create_dir_all(&claude_dir).unwrap();
    std::fs::write(claude_dir.join("settings.local.json"), "{}").unwrap();

    let hooks = load_settings_local_hooks(dir.path().to_str().unwrap());
    assert!(hooks.is_empty());
}

#[test]
fn test_load_settings_local_hooks_with_matcher() {
    std::env::set_var("CC_CODE_TRUST_PROJECT_HOOKS", "1");
    let dir = tempdir().unwrap();
    let claude_dir = dir.path().join(".claude");
    std::fs::create_dir_all(&claude_dir).unwrap();

    let settings = serde_json::json!({
        "hooks": {
            "FileChanged": [
                {
                    "matcher": ".env|.env.local",
                    "hooks": [
                        {"type": "command", "command": "echo changed"}
                    ]
                }
            ]
        }
    });
    std::fs::write(
        claude_dir.join("settings.local.json"),
        serde_json::to_string(&settings).unwrap(),
    )
    .unwrap();

    let hooks = load_settings_local_hooks(dir.path().to_str().unwrap());
    assert_eq!(hooks.len(), 1);
    assert_eq!(hooks[0].matcher.as_deref(), Some(".env|.env.local"));
}

#[test]
fn test_load_from_real_project_dir() {
    // Test loading from the actual peri project directory
    let cwd = std::env::current_dir()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let settings_path = std::path::Path::new(&cwd)
        .join(".claude")
        .join("settings.local.json");
    if !settings_path.exists() {
        eprintln!(
            "Skipping: no settings.local.json at {}",
            settings_path.display()
        );
        return;
    }
    let hooks = load_settings_local_hooks(&cwd);
    assert!(
        !hooks.is_empty(),
        "Should load hooks from project settings.local.json"
    );
    // Should have hooks for known events
    let has_pre = hooks
        .iter()
        .any(|h| matches!(&h.event, HookEvent::PreToolUse));
    let has_perm = hooks
        .iter()
        .any(|h| matches!(&h.event, HookEvent::PermissionRequest));
    assert!(has_pre, "Should have PreToolUse hook");
    assert!(has_perm, "Should have PermissionRequest hook");
}

#[test]
fn test_load_settings_project_hooks_basic() {
    std::env::set_var("CC_CODE_TRUST_PROJECT_HOOKS", "1");
    let dir = tempdir().unwrap();
    let claude_dir = dir.path().join(".claude");
    std::fs::create_dir_all(&claude_dir).unwrap();

    let settings = serde_json::json!({
        "hooks": {
            "PreToolUse": [
                {
                    "matcher": "Bash",
                    "hooks": [
                        {"type": "command", "command": "echo project-pre"}
                    ]
                }
            ]
        }
    });
    std::fs::write(
        claude_dir.join("settings.json"),
        serde_json::to_string(&settings).unwrap(),
    )
    .unwrap();

    let hooks = load_settings_project_hooks(dir.path().to_str().unwrap());
    assert_eq!(hooks.len(), 1);
    assert_eq!(hooks[0].plugin_name, "settings.json (project)");
    assert_eq!(hooks[0].plugin_id, "settings.project");
    assert!(matches!(&hooks[0].event, HookEvent::PreToolUse));
    assert_eq!(hooks[0].matcher.as_deref(), Some("Bash"));
}

#[test]
fn test_load_settings_project_hooks_no_file() {
    let dir = tempdir().unwrap();
    let hooks = load_settings_project_hooks(dir.path().to_str().unwrap());
    assert!(hooks.is_empty());
}

#[test]
fn test_load_settings_project_hooks_no_hooks_field() {
    std::env::set_var("CC_CODE_TRUST_PROJECT_HOOKS", "1");
    let dir = tempdir().unwrap();
    let claude_dir = dir.path().join(".claude");
    std::fs::create_dir_all(&claude_dir).unwrap();
    std::fs::write(claude_dir.join("settings.json"), "{}").unwrap();

    let hooks = load_settings_project_hooks(dir.path().to_str().unwrap());
    assert!(hooks.is_empty());
}

#[test]
fn test_is_cwd_trusted_matches() {
    use std::path::PathBuf;
    let entries = vec![PathBuf::from("/proj"), PathBuf::from("/other/deep")];
    // Exact match
    assert!(is_cwd_trusted(std::path::Path::new("/proj"), &entries));
    // Subdirectory of a trusted entry
    assert!(is_cwd_trusted(
        std::path::Path::new("/proj/sub/dir"),
        &entries
    ));
    assert!(is_cwd_trusted(
        std::path::Path::new("/other/deep/sub"),
        &entries
    ));
    // Sibling prefix is NOT a match (component-aware starts_with)
    assert!(!is_cwd_trusted(std::path::Path::new("/proj2"), &entries));
    assert!(!is_cwd_trusted(std::path::Path::new("/other"), &entries));
    assert!(!is_cwd_trusted(
        std::path::Path::new("/unrelated"),
        &entries
    ));
}

// ─── trusted_projects.jsonl（#18 信任清单） ───────────────────────────────

#[test]
fn test_strip_verbatim_prefix() {
    // Windows verbatim 盘符路径
    assert_eq!(strip_verbatim(r"\\?\D:\code\proj"), r"D:\code\proj");
    // UNC 形式要还原成普通 UNC
    assert_eq!(
        strip_verbatim(r"\\?\UNC\server\share\proj"),
        r"\\server\share\proj"
    );
    // 普通路径保持原样
    assert_eq!(strip_verbatim(r"D:\code\proj"), r"D:\code\proj");
    assert_eq!(strip_verbatim("/home/user/proj"), "/home/user/proj");
}

#[test]
fn test_parse_jsonl_basic() {
    let content = r#"{"path":"/proj/a","trusted_at":"2026-10-28T07:12:10Z"}
{"path":"/proj/b"}
"#;
    let records = parse_trusted_projects_jsonl(content);
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].path, "/proj/a");
    assert_eq!(
        records[0].trusted_at.as_deref(),
        Some("2026-10-28T07:12:10Z")
    );
    // trusted_at 可缺省
    assert_eq!(records[1].path, "/proj/b");
    assert!(records[1].trusted_at.is_none());
}

/// 关键契约：坏行只丢弃该行，其余记录仍然生效。
/// 这正是选 JSONL 而非整体 JSON 的理由——一条坏记录不该让白名单全废。
#[test]
fn test_parse_jsonl_skips_bad_lines() {
    let content = r#"{"path":"/proj/good"}
this is not json
{"path":}
{"path":"/proj/also-good"}
"#;
    let records = parse_trusted_projects_jsonl(content);
    assert_eq!(records.len(), 2, "坏行应被跳过，好行应保留");
    assert_eq!(records[0].path, "/proj/good");
    assert_eq!(records[1].path, "/proj/also-good");
}

#[test]
fn test_parse_jsonl_ignores_blank_and_comments() {
    let content = r#"
# 这是注释

{"path":"/proj/a"}

# 尾注释
"#;
    let records = parse_trusted_projects_jsonl(content);
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].path, "/proj/a");
}

#[test]
fn test_trust_project_writes_jsonl_and_is_idempotent() {
    let home = tempdir().unwrap();
    let project = tempdir().unwrap();
    let cwd = project.path().to_str().unwrap();

    trust_project_in(home.path(), cwd).unwrap();

    let jsonl = home.path().join(TRUST_FILE_NAME);
    assert!(jsonl.exists(), "应创建 {TRUST_FILE_NAME}");

    let content = std::fs::read_to_string(&jsonl).unwrap();
    assert_eq!(content.lines().count(), 1, "一行一条记录");
    // 必须合法 JSON，且 path 就是项目目录
    let record: TrustedProject = serde_json::from_str(content.lines().next().unwrap()).unwrap();
    assert_eq!(normalize_path(&record.path), normalize_path(cwd));
    assert!(record.trusted_at.is_some(), "应记录 trusted_at");

    // 幂等：重复信任同一目录不追加
    trust_project_in(home.path(), cwd).unwrap();
    let content = std::fs::read_to_string(&jsonl).unwrap();
    assert_eq!(content.lines().count(), 1, "重复信任不应追加新行");
}

#[test]
fn test_is_project_trusted_in_reads_jsonl() {
    let home = tempdir().unwrap();
    let project = tempdir().unwrap();
    let cwd = project.path().to_str().unwrap();

    // 信任前：不信任
    assert!(!is_project_trusted_in(home.path(), cwd));

    trust_project_in(home.path(), cwd).unwrap();
    assert!(is_project_trusted_in(home.path(), cwd), "写入后应被信任");

    // 子目录继承父目录信任
    let sub = project.path().join("sub");
    std::fs::create_dir_all(&sub).unwrap();
    assert!(
        is_project_trusted_in(home.path(), sub.to_str().unwrap()),
        "子目录应继承父目录信任"
    );

    // 兄弟目录不继承（组件级比较）
    let sibling = tempdir().unwrap();
    assert!(
        !is_project_trusted_in(home.path(), sibling.path().to_str().unwrap()),
        "兄弟目录不应被信任"
    );
}

/// 旧版纯文本清单必须自动迁移成 JSONL，且不丢数据、不删除原文件。
#[test]
fn test_legacy_plaintext_is_migrated() {
    let home = tempdir().unwrap();
    let project = tempdir().unwrap();
    let cwd = project.path().to_str().unwrap();

    // 造一份旧格式（纯文本，一行一个路径）
    let legacy = home.path().join(LEGACY_TRUST_FILE_NAME);
    std::fs::write(&legacy, format!("{cwd}\n")).unwrap();

    // 读取即触发迁移：旧数据应被认作已信任
    assert!(
        is_project_trusted_in(home.path(), cwd),
        "旧版清单里的项目迁移后仍应受信任"
    );

    // 新文件已生成，且是合法 JSONL
    let jsonl = home.path().join(TRUST_FILE_NAME);
    assert!(jsonl.exists(), "应生成 {TRUST_FILE_NAME}");
    let content = std::fs::read_to_string(&jsonl).unwrap();
    let record: TrustedProject = serde_json::from_str(content.lines().next().unwrap()).unwrap();
    assert_eq!(normalize_path(&record.path), normalize_path(cwd));

    // 旧文件改名留档，不删除用户数据
    assert!(!legacy.exists(), "旧文件应已改名");
    assert!(
        home.path().join(LEGACY_BACKUP_FILE_NAME).exists(),
        "旧文件应留档为 {LEGACY_BACKUP_FILE_NAME}"
    );
}

/// 旧版清单里残留的 `\\?\` 前缀应在迁移时被剥掉（Windows 真实历史数据）。
#[cfg(windows)]
#[test]
fn test_legacy_verbatim_prefix_is_stripped_on_migration() {
    let home = tempdir().unwrap();
    let project = tempdir().unwrap();
    let cwd = project.path().to_str().unwrap();

    let legacy = home.path().join(LEGACY_TRUST_FILE_NAME);
    std::fs::write(&legacy, format!("\\\\?\\{cwd}\n")).unwrap();

    assert!(
        is_project_trusted_in(home.path(), cwd),
        "带 \\\\?\\ 前缀的旧记录迁移后应仍能匹配"
    );

    let content = std::fs::read_to_string(home.path().join(TRUST_FILE_NAME)).unwrap();
    assert!(
        !content.contains(r"\\?\"),
        "迁移后不应残留 verbatim 前缀：{content}"
    );
}

/// 未信任提示：只在"有 hooks 且未信任"时出现，已信任或无 hooks 时静默。
#[test]
fn test_untrusted_notice_only_when_hooks_exist_and_untrusted() {
    let home = tempdir().unwrap();
    let project = tempdir().unwrap();
    let cwd = project.path().to_str().unwrap();

    // 无 .claude/settings*.json → 无提示
    assert!(
        untrusted_project_hooks_notice_in(home.path(), cwd).is_none(),
        "没有 hooks 文件时不应提示"
    );

    // 有 hooks 且未信任 → 有提示，且包含放行命令
    let claude_dir = project.path().join(".claude");
    std::fs::create_dir_all(&claude_dir).unwrap();
    std::fs::write(
        claude_dir.join("settings.json"),
        r#"{"hooks":{"PreToolUse":[{"hooks":[{"type":"command","command":"echo hi"}]}]}}"#,
    )
    .unwrap();

    let notice =
        untrusted_project_hooks_notice_in(home.path(), cwd).expect("有 hooks 且未信任时应提示");
    assert!(
        notice.contains("cc-code trust-project"),
        "提示应给出放行命令：{notice}"
    );

    // 信任后 → 不再提示
    trust_project_in(home.path(), cwd).unwrap();
    assert!(
        untrusted_project_hooks_notice_in(home.path(), cwd).is_none(),
        "已信任后不应再提示"
    );
}

/// 只有 settings.local.json 带 hooks 时也要提示（不能只看 settings.json）。
#[test]
fn test_untrusted_notice_covers_local_settings() {
    let home = tempdir().unwrap();
    let project = tempdir().unwrap();
    let cwd = project.path().to_str().unwrap();

    let claude_dir = project.path().join(".claude");
    std::fs::create_dir_all(&claude_dir).unwrap();
    std::fs::write(
        claude_dir.join("settings.local.json"),
        r#"{"hooks":{"SessionStart":[{"hooks":[{"type":"command","command":"echo hi"}]}]}}"#,
    )
    .unwrap();

    assert!(
        untrusted_project_hooks_notice_in(home.path(), cwd).is_some(),
        "settings.local.json 有 hooks 时也应提示"
    );
}

/// 已有 .jsonl 时不应再走迁移，避免覆盖用户后续新增的记录。
#[test]
fn test_existing_jsonl_wins_over_legacy() {
    let home = tempdir().unwrap();
    let project = tempdir().unwrap();
    let cwd = project.path().to_str().unwrap();
    let other = tempdir().unwrap();

    // 前置：.jsonl 已存在（内容指向另一个目录），旧的纯文本清单同时残留。
    // 必须手动建 .jsonl，因为 trust_project_in 内部会先读取，
    // 若此时 .jsonl 缺失就会先把旧文件迁移掉，测不到「已有 jsonl」这条路径。
    std::fs::write(
        home.path().join(TRUST_FILE_NAME),
        format!(
            "{}\n",
            serde_json::to_string(&TrustedProject {
                path: other.path().to_string_lossy().to_string(),
                trusted_at: None,
            })
            .unwrap()
        ),
    )
    .unwrap();
    std::fs::write(
        home.path().join(LEGACY_TRUST_FILE_NAME),
        other.path().to_str().unwrap(),
    )
    .unwrap();

    trust_project_in(home.path(), cwd).unwrap();

    // 新目录被追加进 .jsonl
    assert!(is_project_trusted_in(home.path(), cwd));
    // .jsonl 已存在，迁移不会触发，旧文件原样保留
    assert!(
        home.path().join(LEGACY_TRUST_FILE_NAME).exists(),
        "已有 .jsonl 时不应触发迁移"
    );
    // 也没有生成迁移留档
    assert!(
        !home.path().join(LEGACY_BACKUP_FILE_NAME).exists(),
        "未触发迁移时不应生成留档文件"
    );
}
