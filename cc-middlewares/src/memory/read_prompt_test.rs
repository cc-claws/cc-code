fn make_index_section(prompt: &str) -> &str {
    prompt
        .split_once("\n\nMEMORY.md:\n")
        .map(|(_, index)| index)
        .expect("非空记忆提示词应包含索引小节")
}

#[test]
fn test_build_memory_read_prompt_includes_index_once() {
    let entrypoint = "\n- [MEMORY_READ_SENTINEL](topic.md) — 测试索引\n";
    let prompt = build_memory_read_prompt("/project/memory/", Some(entrypoint));
    assert_eq!(
        prompt.matches("MEMORY_READ_SENTINEL").count(),
        1,
        "索引正文应只注入一次"
    );
    assert_eq!(
        make_index_section(&prompt),
        entrypoint.trim(),
        "预算内的索引只应去除首尾空白"
    );
    assert!(
        prompt.contains("Read relevant topic files with Read only when needed"),
        "只应按相关性按需读取详情"
    );
    assert!(
        prompt.contains("current source takes precedence over memory"),
        "应明确当前源码优先"
    );
}

#[test]
fn test_build_memory_read_prompt_empty_index_is_compact() {
    let empty_prompt = build_memory_read_prompt("/project/memory/", None);
    for entrypoint in [Some(""), Some(" \r\n\t")] {
        assert_eq!(
            build_memory_read_prompt("/project/memory/", entrypoint),
            empty_prompt,
            "缺省、空和空白索引应保持一致"
        );
    }
    assert!(
        empty_prompt.contains("Directory: `/project/memory/`."),
        "应保留记忆目录"
    );
    assert!(
        !empty_prompt.contains("MEMORY.md:\n"),
        "空索引不应产生索引小节"
    );
    assert!(
        !empty_prompt.contains("currently empty"),
        "空索引不应附加旧版长提示"
    );
    assert!(
        !empty_prompt.contains("Read relevant"),
        "空索引仅保留路径和显式保存规则"
    );
}

#[test]
fn test_build_memory_read_prompt_requires_explicit_memory_writes() {
    let prompt = build_memory_read_prompt("/project/memory/", None);
    assert!(
        prompt.contains("Save, update, or delete memory only when the user explicitly requests it"),
        "写入、更新和删除记忆都须由用户明确要求"
    );
    assert!(prompt.contains("Write/Edit"), "应使用现有通用工具维护记忆");
    assert!(
        prompt.contains("topic files and a short MEMORY.md index"),
        "应保留主题文件和简短索引的组织约定"
    );
    assert!(
        prompt.contains("frontmatter type: user/feedback/project/reference"),
        "应简短说明记忆类型"
    );
    assert!(
        !prompt.contains("build up this memory system"),
        "不得保留自动积累记忆的指令"
    );
}

#[test]
fn test_build_memory_read_prompt_preserves_index_at_byte_limit() {
    for size in [MAX_MEMORY_READ_INDEX_BYTES - 1, MAX_MEMORY_READ_INDEX_BYTES] {
        let entrypoint = "x".repeat(size);
        let prompt = build_memory_read_prompt("/project/memory/", Some(&entrypoint));
        assert_eq!(
            make_index_section(&prompt),
            entrypoint,
            "未超过预算的索引应完整保留"
        );
        assert!(
            !prompt.contains("Partial index"),
            "刚好达到预算不应误报截断"
        );
    }
}

#[test]
fn test_build_memory_read_prompt_truncated_index_includes_notice_within_budget() {
    let entrypoint = "x".repeat(MAX_MEMORY_READ_INDEX_BYTES + 1);
    let prompt = build_memory_read_prompt("/project/memory/", Some(&entrypoint));
    let index = make_index_section(&prompt);
    assert_eq!(
        index.len(),
        MAX_MEMORY_READ_INDEX_BYTES,
        "单行截断提示也必须计入字节预算"
    );
    assert!(
        index.ends_with(TRUNCATED_INDEX_SUFFIX),
        "应说明完整索引仅在需要时读取"
    );
}

#[test]
fn test_build_memory_read_prompt_truncates_multibyte_index_safely() {
    for character in ["记", "🧠", "a记"] {
        let entrypoint = character.repeat(MAX_MEMORY_READ_INDEX_BYTES);
        let prompt = build_memory_read_prompt("/project/memory/", Some(&entrypoint));
        let index = make_index_section(&prompt);
        assert!(
            index.len() <= MAX_MEMORY_READ_INDEX_BYTES,
            "多字节索引及提示不得超过字节预算"
        );
        assert!(index.starts_with(character), "超长单行应在字符边界保留内容");
        assert!(
            index.ends_with(TRUNCATED_INDEX_SUFFIX),
            "多字节截断也应保留按需读取提示"
        );
    }
}

#[test]
fn test_build_memory_read_prompt_truncation_prefers_complete_lines() {
    let entrypoint = format!(
        "- [保留](topic.md)\n{}",
        "未完成索引".repeat(MAX_MEMORY_READ_INDEX_BYTES)
    );
    let prompt = build_memory_read_prompt("/project/memory/", Some(&entrypoint));
    let index = make_index_section(&prompt);
    assert_eq!(
        index,
        format!("- [保留](topic.md){TRUNCATED_INDEX_SUFFIX}"),
        "多行索引应优先保留完整条目"
    );
    assert!(
        index.len() <= MAX_MEMORY_READ_INDEX_BYTES,
        "完整行截断也须遵守预算"
    );
}

#[test]
fn test_build_memory_read_prompt_has_small_fixed_overhead() {
    let memory_dir = "/project/memory/";
    let entrypoint = "MEMORY_READ_SENTINEL";
    let prompt = build_memory_read_prompt(memory_dir, Some(entrypoint));
    let legacy_prompt = crate::memory::build_auto_memory_prompt(memory_dir, Some(entrypoint));
    let fixed_bytes = prompt.len() - memory_dir.len() - entrypoint.len();
    assert!(
        fixed_bytes <= 800,
        "目录和索引以外的固定规则不得超过800字节，实际为{fixed_bytes}"
    );
    assert!(
        prompt.len() < legacy_prompt.len() / 5,
        "读取提示词应明显小于旧版长规则"
    );
}
