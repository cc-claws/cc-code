use super::*;
use tempfile::tempdir;

fn make_skill(directory: &Path, name: &str, content: &str) -> PathBuf {
    let path = directory.join(name).join("SKILL.md");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, content).unwrap();
    path
}

#[test]
fn test_parse_skill_extracts_frontmatter_without_leaking_yaml() {
    let source = "\u{feff}---\r\nname: audit\r\ndescription: 审计\r\nallowed-tools: Bash(git log:*), Read Grep\r\ncontext: fork\r\nagent: explore\r\nmodel: haiku\r\narguments: [target, mode]\r\n---\r\n# 工作流\r\n检查 $target";
    let (frontmatter, body) = parse_skill(source).expect("有效 YAML 应解析成功");
    assert_eq!(frontmatter.name.as_deref(), Some("audit"));
    assert_eq!(frontmatter.context.as_deref(), Some("fork"));
    assert_eq!(frontmatter.agent.as_deref(), Some("explore"));
    assert_eq!(frontmatter.model.as_deref(), Some("haiku"));
    assert!(frontmatter.user_invocable, "默认允许显式调用");
    assert_eq!(frontmatter.allowed_tools.expect("应有权限元数据").to_vec(), vec!["Bash(git log:*)", "Read", "Grep"]);
    assert!(body.contains("# 工作流"));
    assert!(!body.contains("allowed-tools:"), "返回正文不能包含 YAML");
}

#[test]
fn test_parse_skill_accepts_list_permissions_and_plain_markdown() {
    let (frontmatter, _) = parse_skill("---\nallowed-tools: [Read, 'Bash(git status:*)']\n---\n正文").expect("列表应有效");
    assert_eq!(frontmatter.allowed_tools.expect("应有权限列表").to_vec(), vec!["Read", "Bash(git status:*)"]);
    let (frontmatter, body) = parse_skill("# 不含元数据\n正文").expect("纯 Markdown 也应加载");
    assert!(frontmatter.user_invocable);
    assert!(body.contains("正文"));
}

#[test]
fn test_parse_skill_rejects_invalid_frontmatter_types() {
    let result = parse_skill("---\nallowed-tools: 42\n---\n正文");
    assert!(result.is_err(), "错误元数据应走错误路径");
}

#[test]
fn test_find_skill_resolves_exact_names_before_unique_aliases() {
    let make_metadata = |name: &str| SkillMetadata { name: name.to_string(), description: String::new(), path: PathBuf::new() };
    let skills = vec![make_metadata("one:review"), make_metadata("two:review"), make_metadata("deploy")];
    assert_eq!(find_skill(&skills, "/ONE:REVIEW").map(|skill| skill.name.as_str()), Some("one:review"));
    assert!(find_skill(&skills, "review").is_none(), "歧义简称不能任意选中插件");
    assert!(find_skill(&skills, "three:review").is_none(), "限定名称不能跨插件匹配");
    assert_eq!(find_skill(&skills, "compat:deploy").map(|skill| skill.name.as_str()), Some("deploy"));
}

#[test]
fn test_find_skill_rejects_paths_and_unknown_names() {
    let skills = vec![SkillMetadata { name: "audit".into(), description: String::new(), path: PathBuf::new() }];
    for name in ["", "../audit", "..\\audit", "D:/audit", "missing"] {
        assert!(find_skill(&skills, name).is_none(), "不得从输入拼接技能路径：{name}");
    }
}

#[test]
fn test_substitute_arguments_preserves_quoted_unicode_and_raw_arguments() {
    let result = substitute_arguments("$0|$ARGUMENTS[1]|$ARGUMENTS[8]|$ARGUMENTS", "\"中文 项目\" '严格 审计'", &[]);
    assert_eq!(result, "中文 项目|严格 审计||\"中文 项目\" '严格 审计'");
}

#[test]
fn test_substitute_arguments_handles_adjacent_named_placeholders_and_boundaries() {
    let result = substitute_arguments("$target$target|$targetLong|$target[0]|$mode", "项目 strict", &["target".into(), "mode".into()]);
    assert_eq!(result, "项目项目|$targetLong|$target[0]|strict");
}

#[test]
fn test_substitute_arguments_handles_empty_and_missing_placeholders() {
    assert_eq!(substitute_arguments("$0|$ARGUMENTS[1]|$ARGUMENTS", "", &[]), "||");
    assert_eq!(substitute_arguments("正文", "", &[]), "正文");
    assert_eq!(substitute_arguments("正文", "参数 中文", &[]), "正文\n\nARGUMENTS: 参数 中文");
}

#[test]
fn test_parse_arguments_keeps_variables_literal_and_ignores_shell_operators() {
    assert_eq!(parse_arguments("one | two && '$TOKEN' \"D:\\code\\peri\""), vec!["one", "two", "$TOKEN", "D:\\code\\peri"]);
    assert_eq!(parse_arguments("one \"\" 'two three' # comment"), vec!["one", "", "two three"]);
    assert_eq!(parse_arguments("one \"broken quote"), vec!["one", "\"broken", "quote"]);
}

#[test]
fn test_load_skill_uses_first_directory_and_expands_skill_directory() {
    let directory = tempdir().unwrap();
    let first = directory.path().join("first");
    let second = directory.path().join("second");
    let path = make_skill(&first, "audit", "---\nname: Audit\narguments: target\n---\n$target ${CLAUDE_SKILL_DIR}/scripts/run.py");
    make_skill(&second, "audit", "---\nname: audit\n---\n错误的低优先级正文");
    let loaded = load_skill(&[first, second], "audit", Some("\"中文 项目\"")).expect("技能应成功加载");
    assert_eq!(loaded.path, path);
    assert_eq!(loaded.name, "Audit");
    assert!(loaded.instruction.contains("中文 项目"));
    assert!(!loaded.instruction.contains("错误的低优先级正文"));
    assert!(!loaded.instruction.contains("${CLAUDE_SKILL_DIR}"));
    assert!(loaded.instruction.starts_with("Base directory for this skill: "));
}

#[test]
fn test_load_skill_uses_directory_name_without_frontmatter_name() {
    let directory = tempdir().unwrap();
    make_skill(directory.path(), "plain-skill", "# 纯 Markdown\n$ARGUMENTS");
    let loaded = load_skill(&[directory.path().to_path_buf()], "plain-skill", None).expect("目录名应作为默认技能名");
    assert_eq!(loaded.name, "plain-skill");
    assert!(!loaded.instruction.contains("$ARGUMENTS"));
}
