use super::*;

fn make_repository() -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    std::fs::create_dir(directory.path().join(".git")).unwrap();
    directory
}

fn make_config(directory: &Path) -> AgentsMdConfig {
    AgentsMdConfig {
        user_global_file: directory.join("no-global.md"),
        ..Default::default()
    }
}

#[test]
fn test_discover_parent_components_skip_unrelated_directory() {
    let directory = make_repository();
    let root = directory.path();
    std::fs::create_dir_all(root.join("actual/unrelated")).unwrap();
    std::fs::write(root.join("AGENTS.md"), "ROOT_RULES").unwrap();
    std::fs::write(root.join("actual/AGENTS.md"), "ACTUAL_RULES").unwrap();
    std::fs::write(root.join("actual/unrelated/AGENTS.md"), "UNRELATED_RULES").unwrap();
    let files = discover_instruction_files(&root.join("actual/unrelated/.."), &make_config(root));
    assert_eq!(files.len(), 2, "被 .. 抵消的目录不得进入祖先链");
    assert_eq!(files[0].content, "ROOT_RULES");
    assert_eq!(files[1].content, "ACTUAL_RULES");
}

#[test]
fn test_discover_parent_components_preserve_excludes_source_path() {
    let directory = make_repository();
    let root = directory.path();
    std::fs::create_dir_all(root.join("actual/unrelated")).unwrap();
    std::fs::write(root.join("actual/AGENTS.md"), "EXCLUDED_RULES").unwrap();
    std::fs::write(root.join("actual/CLAUDE.md"), "RETAINED_RULES").unwrap();
    let config = AgentsMdConfig {
        excludes: vec![root.join("actual/AGENTS.md").display().to_string()],
        ..make_config(root)
    };
    let files = discover_instruction_files(&root.join("actual/unrelated/.."), &config);
    assert_eq!(files.len(), 1, "解析 .. 之后仍应匹配发现路径的 excludes");
    assert_eq!(files[0].content, "RETAINED_RULES");
}

#[cfg(windows)]
#[test]
fn test_discover_parent_components_allow_missing_cancelled_directory() {
    let directory = make_repository();
    let root = directory.path();
    std::fs::write(root.join("AGENTS.md"), "ROOT_RULES").unwrap();
    let files = discover_instruction_files(&root.join("missing/.."), &make_config(root));
    assert_eq!(files.len(), 1, "Win32 词法抵消的不存在目录不应阻止加载");
    assert_eq!(files[0].content, "ROOT_RULES");
}

#[cfg(unix)]
#[test]
fn test_discover_parent_components_preserve_symlink_alias_excludes() {
    let directory = make_repository();
    let root = directory.path();
    std::fs::create_dir_all(root.join("actual/unrelated")).unwrap();
    std::os::unix::fs::symlink(root.join("actual"), root.join("alias")).unwrap();
    std::fs::write(root.join("actual/AGENTS.md"), "EXCLUDED_RULES").unwrap();
    let config = AgentsMdConfig {
        excludes: vec![root.join("alias/AGENTS.md").display().to_string()],
        ..make_config(root)
    };
    let files = discover_instruction_files(&root.join("alias/unrelated/.."), &config);
    assert!(
        files.is_empty(),
        ".. 未跨链接本身时必须保留 alias 供 excludes 匹配"
    );
}

#[cfg(unix)]
#[test]
fn test_discover_parent_components_use_physical_parent_of_symlink() {
    let directory = make_repository();
    let root = directory.path();
    std::fs::create_dir_all(root.join("actual/nested")).unwrap();
    std::os::unix::fs::symlink(root.join("actual/nested"), root.join("alias")).unwrap();
    std::fs::write(root.join("actual/AGENTS.md"), "PHYSICAL_PARENT_RULES").unwrap();
    std::fs::write(root.join("AGENTS.md"), "PROJECT_RULES").unwrap();
    let files = discover_instruction_files(&root.join("alias/.."), &make_config(root));
    assert_eq!(files.len(), 2, "不能把 alias/.. 错当成词法项目根");
    assert_eq!(files[1].content, "PHYSICAL_PARENT_RULES");
}

#[test]
fn test_discover_deduplicated_candidate_does_not_reserve_later_path() {
    let directory = make_repository();
    let root = directory.path();
    std::fs::create_dir_all(root.join("middle/deep")).unwrap();
    std::fs::write(root.join("AGENTS.md"), "PYTHON_RULES").unwrap();
    std::fs::write(root.join("middle/AGENTS.md"), "RUST_RULES").unwrap();
    std::fs::write(root.join("middle/deep/AGENTS.md"), "PYTHON_RULES").unwrap();
    // 相对候选与符号链接别名具有相同的 canonical 路径，不依赖 Windows 链接权限。
    let config = AgentsMdConfig {
        instruction_file_candidates: vec!["AGENTS.md".into(), "middle/deep/AGENTS.md".into()],
        ..make_config(root)
    };
    let files = discover_instruction_files(&root.join("middle/deep"), &config);
    assert_eq!(files.len(), 3, "被内容去重淘汰的别名不得挤掉深层规则");
    assert_eq!(files[2].content, "PYTHON_RULES");
    assert_eq!(files[2].display, "middle/deep/AGENTS.md");
}

#[test]
fn test_discover_large_candidates_keep_bounded_content() {
    let directory = make_repository();
    let root = directory.path();
    std::fs::write(root.join("AGENTS.md"), "SMALL_PRIMARY_RULES").unwrap();
    let large = format!("HEAD{}TAIL", "x".repeat(2 * DEFAULT_MAX_SOURCE_BYTES));
    std::fs::write(root.join("CLAUDE.md"), large).unwrap();
    let config = AgentsMdConfig {
        max_source_bytes: 256,
        ..make_config(root)
    };
    let files = discover_instruction_files(root, &config);
    assert_eq!(files.len(), 2);
    assert!(files[1].content.len() <= 256, "发现阶段不得保留完整大文件");
    assert!(files[1].content.starts_with("HEAD"));
    assert!(files[1].content.ends_with("TAIL"));
    assert!(files[1].content.contains("truncated"));
}

#[test]
fn test_import_expansion_stops_when_source_budget_is_exhausted() {
    let directory = make_repository();
    let root = directory.path();
    std::fs::write(
        root.join("AGENTS.md"),
        "<!-- @import first.md --><!-- @import later.md -->",
    )
    .unwrap();
    std::fs::write(root.join("first.md"), "A".repeat(4096)).unwrap();
    std::fs::write(root.join("later.md"), "LATER_RULES").unwrap();
    let config = AgentsMdConfig {
        max_source_bytes: 256,
        ..make_config(root)
    };
    let files = discover_instruction_files(root, &config);
    assert_eq!(files.len(), 1);
    assert!(files[0].content.len() <= 256, "展开后的内容也必须有界");
    assert!(files[0].content.contains("truncated"));
    assert!(
        !files[0].content.contains("LATER_RULES"),
        "预算耗尽后不得继续展开后续文件"
    );
}

#[test]
fn test_import_exact_budget_preserves_rules_before_empty_expansion() {
    let directory = make_repository();
    let root = directory.path();
    let tail = "EXACT_TAIL_RULES";
    let full = format!("{}{tail}", "A".repeat(256 - tail.len()));
    std::fs::write(root.join("full.md"), &full).unwrap();
    std::fs::write(root.join("empty.md"), "").unwrap();
    std::fs::write(
        root.join("AGENTS.md"),
        "<!-- @import full.md --><!-- @import later.md -->",
    )
    .unwrap();
    let config = AgentsMdConfig {
        max_source_bytes: 256,
        ..make_config(root)
    };
    for empty_content in ["", "\u{feff}", "<!-- @import empty.md -->"] {
        std::fs::write(root.join("later.md"), empty_content).unwrap();
        let files = discover_instruction_files(root, &config);
        assert_eq!(files.len(), 1);
        assert_eq!(
            files[0].content, full,
            "零增长导入不得裁掉已满预算的末尾规则"
        );
    }
}

#[test]
fn test_import_exact_budget_marks_nonempty_following_expansion() {
    let directory = make_repository();
    let root = directory.path();
    std::fs::write(root.join("full.md"), "A".repeat(256)).unwrap();
    std::fs::write(root.join("later.md"), "RULE_GROWTH").unwrap();
    std::fs::write(
        root.join("AGENTS.md"),
        "<!-- @import full.md --><!-- @import later.md -->",
    )
    .unwrap();
    let config = AgentsMdConfig {
        max_source_bytes: 256,
        ..make_config(root)
    };
    let files = discover_instruction_files(root, &config);
    assert_eq!(files.len(), 1);
    assert!(files[0].content.len() <= 256);
    assert!(
        files[0].content.contains("truncated imports"),
        "实际增长超限必须标记"
    );
    assert!(!files[0].content.contains("RULE_GROWTH"));
}

#[test]
fn test_import_many_small_files_cannot_exceed_source_budget() {
    let directory = make_repository();
    let root = directory.path();
    let mut imports = String::new();
    for index in 0..8 {
        let name = format!("rules-{index}.md");
        std::fs::write(root.join(&name), format!("RULE_{index}{}", "x".repeat(100))).unwrap();
        imports.push_str(&format!("<!-- @import {name} -->"));
    }
    std::fs::write(root.join("AGENTS.md"), imports).unwrap();
    let config = AgentsMdConfig {
        max_source_bytes: 256,
        ..make_config(root)
    };
    let files = discover_instruction_files(root, &config);
    assert_eq!(files.len(), 1);
    assert!(files[0].content.len() <= 256);
    assert!(files[0].content.contains("RULE_0"), "保留已展开的头部规则");
    assert!(
        files[0].content.contains("truncated"),
        "展开达到预算时必须标记"
    );
    assert!(!files[0].content.contains("RULE_7"), "停止后续导入");
}
