use super::*;

#[tokio::test]
async fn test_grep_hit() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("test.txt"),
        "needle in a haystack\nother line",
    )
    .unwrap();
    let tool = GrepTool::new(dir.path().to_str().unwrap());
    let result = tool
        .invoke(serde_json::json!({"pattern": "needle", "output_mode": "content", "path": "./"}))
        .await
        .unwrap();
    assert!(result.contains("needle"), "should find needle: {result}");
}

#[tokio::test]
async fn test_grep_no_match() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("test.txt"), "haystack only").unwrap();
    let tool = GrepTool::new(dir.path().to_str().unwrap());
    let result = tool
        .invoke(
            serde_json::json!({"pattern": "zzz_not_here", "output_mode": "content", "path": "./"}),
        )
        .await
        .unwrap();
    assert!(
        result.contains("No matches found"),
        "should report no match: {result}"
    );
}

#[tokio::test]
async fn test_grep_missing_pattern() {
    let dir = tempfile::tempdir().unwrap();
    let tool = GrepTool::new(dir.path().to_str().unwrap());
    let result = tool.invoke(serde_json::json!({})).await;
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("Missing required parameter 'pattern'"),
        "should report missing pattern: {err_msg}"
    );
}

#[tokio::test]
async fn test_grep_regex() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("test.txt"), "needle123\nneedle456").unwrap();
    let tool = GrepTool::new(dir.path().to_str().unwrap());
    let result = tool
        .invoke(
            serde_json::json!({"pattern": "needle[0-9]+", "output_mode": "content", "path": "./"}),
        )
        .await
        .unwrap();
    assert!(result.contains("needle"), "regex should match: {result}");
}

#[test]
fn test_grep_description_extended() {
    let tool = GrepTool::new("/tmp");
    let desc = tool.description();
    assert!(desc.contains("regex"), "description 应提及正则支持");
    assert!(
        desc.contains("Output modes:"),
        "description 应包含 Output modes 段落"
    );
    assert!(desc.len() > 200, "description 应为扩展后的多段落文本");
}

#[tokio::test]
async fn test_grep_files_only() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.txt"), "needle here\nother line").unwrap();
    std::fs::write(dir.path().join("b.txt"), "no match here").unwrap();
    std::fs::write(dir.path().join("c.txt"), "needle again").unwrap();
    let tool = GrepTool::new(dir.path().to_str().unwrap());
    let result = tool
            .invoke(serde_json::json!({"pattern": "needle", "output_mode": "files_with_matches", "path": "./"}))
            .await
            .unwrap();
    assert!(result.contains("a.txt"), "should find a.txt: {result}");
    assert!(result.contains("c.txt"), "should find c.txt: {result}");
    assert!(
        !result.contains("needle here"),
        "should not include line content: {result}"
    );
}

#[tokio::test]
async fn test_grep_count() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.txt"), "needle\nneedle\nneedle").unwrap();
    std::fs::write(dir.path().join("b.txt"), "needle once").unwrap();
    let tool = GrepTool::new(dir.path().to_str().unwrap());
    let result = tool
        .invoke(serde_json::json!({"pattern": "needle", "output_mode": "count", "path": "./"}))
        .await
        .unwrap();
    assert!(
        result.contains("a.txt:3"),
        "a.txt should have 3 matches: {result}"
    );
    assert!(
        result.contains("b.txt:1"),
        "b.txt should have 1 match: {result}"
    );
}

#[tokio::test]
async fn test_grep_case_insensitive() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("test.txt"), "NEEDLE\nneedle\nNeedle").unwrap();
    let tool = GrepTool::new(dir.path().to_str().unwrap());
    let result = tool
            .invoke(serde_json::json!({"pattern": "NEEDLE", "output_mode": "content", "-i": true, "path": "./"}))
            .await
            .unwrap();
    assert!(
        result.contains("NEEDLE"),
        "should match uppercase: {result}"
    );
    assert!(
        result.contains("needle"),
        "should match lowercase: {result}"
    );
    assert!(
        result.contains("Needle"),
        "should match mixed case: {result}"
    );
}

#[tokio::test]
async fn test_grep_glob_filter() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("test.txt"), "needle in txt").unwrap();
    std::fs::write(dir.path().join("test.rs"), "needle in rs").unwrap();
    let tool = GrepTool::new(dir.path().to_str().unwrap());
    let result = tool
            .invoke(serde_json::json!({"pattern": "needle", "output_mode": "content", "glob": "*.txt", "path": "./"}))
            .await
            .unwrap();
    assert!(result.contains("test.txt"), "should find in .txt: {result}");
    assert!(
        !result.contains("test.rs"),
        "should not find in .rs: {result}"
    );
}

#[tokio::test]
async fn test_grep_type_filter() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("test.txt"), "needle in txt").unwrap();
    std::fs::write(dir.path().join("test.rs"), "needle in rs").unwrap();
    let tool = GrepTool::new(dir.path().to_str().unwrap());
    let result = tool
        .invoke(serde_json::json!({
            "pattern": "needle",
            "output_mode": "content",
            "type": "rust",
            "path": "./"
        }))
        .await
        .unwrap();
    assert!(result.contains("test.rs"), "should find in .rs: {result}");
    assert!(
        !result.contains("test.txt"),
        "should not find in .txt with type=rust: {result}"
    );
}

#[test]
fn test_grep_tool_name() {
    let tool = GrepTool::new("/tmp");
    assert_eq!(tool.name(), "Grep");
}

#[tokio::test]
async fn test_grep_invalid_output_mode() {
    let dir = tempfile::tempdir().unwrap();
    let tool = GrepTool::new(dir.path().to_str().unwrap());
    let result = tool
        .invoke(serde_json::json!({
            "pattern": "needle",
            "output_mode": "invalid_mode"
        }))
        .await;
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("Error"),
        "should report invalid output_mode: {err_msg}"
    );
}

#[tokio::test]
async fn test_grep_offset() {
    let dir = tempfile::tempdir().unwrap();
    let lines: Vec<String> = (0..10).map(|i| format!("line {} needle", i)).collect();
    std::fs::write(dir.path().join("test.txt"), lines.join("\n")).unwrap();
    let tool = GrepTool::new(dir.path().to_str().unwrap());
    let result = tool
        .invoke(serde_json::json!({
            "pattern": "needle",
            "output_mode": "content",
            "path": "./",
            "offset": 5
        }))
        .await
        .unwrap();
    assert!(
        !result.contains("line 0"),
        "should skip first 5 lines: {result}"
    );
    assert!(
        result.contains("line 5"),
        "should include line 5+: {result}"
    );
}

// === Task 4 新增测试 ===

#[tokio::test]
async fn test_grep_multiline() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("test.txt"), "foo\nbar\nbaz").unwrap();
    let tool = GrepTool::new(dir.path().to_str().unwrap());
    let result = tool
        .invoke(serde_json::json!({
            "pattern": "foo.*bar",
            "multiline": true,
            "output_mode": "content",
            "path": "./"
        }))
        .await
        .unwrap();
    assert!(result.contains("foo"), "multiline 应匹配跨行模式: {result}");
}

#[tokio::test]
async fn test_grep_line_number_off() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("test.txt"), "needle here").unwrap();
    let tool = GrepTool::new(dir.path().to_str().unwrap());
    let result = tool
        .invoke(serde_json::json!({
            "pattern": "needle",
            "-n": false,
            "output_mode": "content",
            "path": "./"
        }))
        .await
        .unwrap();
    // line_number=false 格式为 "path: content"（无行号），不含 "path:num: content" 的双冒号模式
    assert!(
        !result.contains("test.txt:1:"),
        "line_number=false 时不应含行号: {result}"
    );
}

#[tokio::test]
async fn test_grep_whole_word() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("test.txt"), "test testing tested").unwrap();
    let tool = GrepTool::new(dir.path().to_str().unwrap());
    // whole_word=true 应只匹配独立单词 "test"
    let result_word = tool
        .invoke(serde_json::json!({
            "pattern": "test",
            "whole_word": true,
            "output_mode": "content",
            "path": "./"
        }))
        .await
        .unwrap();
    assert!(
        result_word.contains("test testing tested"),
        "whole_word=true 应匹配包含独立 test 的行: {result_word}"
    );
    // whole_word=false 时同一行也应匹配
    let result_no_word = tool
        .invoke(serde_json::json!({
            "pattern": "test",
            "whole_word": false,
            "output_mode": "content",
            "path": "./"
        }))
        .await
        .unwrap();
    assert!(
        result_no_word.contains("test testing tested"),
        "whole_word=false 也应匹配该行: {result_no_word}"
    );
}

#[tokio::test]
async fn test_grep_invert_match() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("test.txt"), "foo\nbar\nbaz\nfoo2").unwrap();
    let tool = GrepTool::new(dir.path().to_str().unwrap());
    let result = tool
        .invoke(serde_json::json!({
            "pattern": "foo",
            "invert_match": true,
            "output_mode": "content",
            "path": "./"
        }))
        .await
        .unwrap();
    assert!(
        !result.contains("foo"),
        "invert_match=true 不应输出匹配行: {result}"
    );
    assert!(
        result.contains("bar"),
        "invert_match=true 应输出不匹配行: {result}"
    );
}

#[tokio::test]
async fn test_grep_fixed_strings() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("test.txt"), "[ERROR] something\n[INFO] ok").unwrap();
    let tool = GrepTool::new(dir.path().to_str().unwrap());
    let result = tool
        .invoke(serde_json::json!({
            "pattern": "[ERROR]",
            "fixed_strings": true,
            "output_mode": "content",
            "path": "./"
        }))
        .await
        .unwrap();
    assert!(
        result.contains("[ERROR]"),
        "fixed_strings=true 应匹配字面 [ERROR]: {result}"
    );
    assert!(
        !result.contains("[INFO]"),
        "fixed_strings=true 不应匹配 [INFO]: {result}"
    );
}

#[tokio::test]
async fn test_grep_asymmetric_context() {
    let dir = tempfile::tempdir().unwrap();
    let lines = [
        "line1 before\n",
        "line2 before\n",
        "needle match\n",
        "line4 after\n",
    ];
    std::fs::write(dir.path().join("test.txt"), lines.join("")).unwrap();
    let tool = GrepTool::new(dir.path().to_str().unwrap());
    let result = tool
        .invoke(serde_json::json!({
            "pattern": "needle",
            "-B": 2,
            "-A": 0,
            "output_mode": "content",
            "path": "./"
        }))
        .await
        .unwrap();
    assert!(
        result.contains("line1 before"),
        "应包含前 2 行上下文: {result}"
    );
    assert!(
        result.contains("line2 before"),
        "应包含前 2 行上下文: {result}"
    );
}

#[tokio::test]
async fn test_grep_files_without_matches() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.txt"), "needle here").unwrap();
    std::fs::write(dir.path().join("b.txt"), "no match here").unwrap();
    let tool = GrepTool::new(dir.path().to_str().unwrap());
    let result = tool
        .invoke(serde_json::json!({
            "pattern": "needle",
            "output_mode": "files_without_matches",
            "path": "./"
        }))
        .await
        .unwrap();
    assert!(result.contains("b.txt"), "应列出无匹配的文件: {result}");
    assert!(!result.contains("a.txt"), "不应列出有匹配的文件: {result}");
}

#[tokio::test]
async fn test_grep_output_mode_default() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("test.txt"), "needle here").unwrap();
    let tool = GrepTool::new(dir.path().to_str().unwrap());
    let result = tool
        .invoke(serde_json::json!({
            "pattern": "needle",
            "path": "./"
        }))
        .await
        .unwrap();
    assert!(
        result.contains("test.txt"),
        "不传 output_mode 时应默认为 files_with_matches 模式（输出文件名）: {result}"
    );
    assert!(
        !result.contains("needle here"),
        "默认 files_with_matches 不应输出匹配行内容: {result}"
    );
}

// === Task 5: multi_line 兼容性验证 ===

#[tokio::test]
async fn test_grep_multiline_with_invert_match() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("test.txt"), "foo\nbar\nbaz").unwrap();
    let tool = GrepTool::new(dir.path().to_str().unwrap());
    // multi_line + invert_match: 跨行模式匹配 foo.*baz，反转后应输出不包含跨行匹配的文件
    let result = tool
        .invoke(serde_json::json!({
            "pattern": "foo.*baz",
            "multiline": true,
            "invert_match": true,
            "output_mode": "content",
            "path": "./"
        }))
        .await
        .unwrap();
    // foo.*baz 跨行匹配整个文件内容，反转后应为空
    assert!(
        result.contains("No matches found"),
        "multi_line + invert_match: 跨行匹配整个文件后反转应无结果: {result}"
    );
}

#[tokio::test]
async fn test_grep_multiline_with_context() {
    let dir = tempfile::tempdir().unwrap();
    let lines = ["before1\n", "START\n", "middle\n", "END\n", "after1\n"];
    std::fs::write(dir.path().join("test.txt"), lines.join("")).unwrap();
    let tool = GrepTool::new(dir.path().to_str().unwrap());
    let result = tool
        .invoke(serde_json::json!({
            "pattern": "START.*END",
            "multiline": true,
            "-A": 1,
            "output_mode": "content",
            "path": "./"
        }))
        .await
        .unwrap();
    assert!(
        result.contains("START"),
        "multi_line + context: 应包含 START: {result}"
    );
    assert!(
        result.contains("END"),
        "multi_line + context: 应包含 END: {result}"
    );
}

#[tokio::test]
async fn test_grep_max_depth() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("root.txt"), "needle").unwrap();
    let sub = dir.path().join("sub");
    std::fs::create_dir_all(&sub).unwrap();
    std::fs::write(sub.join("deep.txt"), "needle").unwrap();
    let tool = GrepTool::new(dir.path().to_str().unwrap());
    let result = tool
        .invoke(serde_json::json!({
            "pattern": "needle",
            "max_depth": 1,
            "output_mode": "files_with_matches",
            "path": "./"
        }))
        .await
        .unwrap();
    assert!(
        result.contains("root.txt"),
        "max_depth=1 应找到根目录文件: {result}"
    );
    assert!(
        !result.contains("deep.txt"),
        "max_depth=1 不应找到子目录文件: {result}"
    );
}

#[tokio::test]
async fn test_grep_truncation_persists_full_output() {
    let dir = tempfile::tempdir().unwrap();
    let lines: Vec<String> = (0..10).map(|i| format!("line {} needle", i)).collect();
    std::fs::write(dir.path().join("test.txt"), lines.join("\n")).unwrap();
    let tool = GrepTool::new(dir.path().to_str().unwrap());
    let result = tool
        .invoke(serde_json::json!({
            "pattern": "needle",
            "output_mode": "content",
            "path": "./",
            "head_limit": 3
        }))
        .await
        .unwrap();
    assert!(
        result.contains("[Showing results with pagination = limit: 3]"),
        "应显示分页提示（offset=0 不输出 offset 字段，对齐上游）: {result}"
    );
    assert!(
        result.contains("Read tool"),
        "应包含 Read tool 提示: {result}"
    );
    assert!(
        result.contains("peri-tool-output-"),
        "应包含文件路径: {result}"
    );
}

// === P0：默认模式 + max-columns + 分页提示 ===

#[tokio::test]
async fn test_grep_default_output_mode_is_files_with_matches() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("a.rs"),
        "fn needle() {}\nfn other() {}",
    )
    .unwrap();
    std::fs::write(dir.path().join("b.rs"), "no match").unwrap();
    let tool = GrepTool::new(dir.path().to_str().unwrap());
    let result = tool
        .invoke(serde_json::json!({
            "pattern": "needle",
            "path": "./"
        }))
        .await
        .unwrap();
    assert!(
        result.contains("a.rs"),
        "默认 output_mode 应为 files_with_matches，输出含匹配的文件名: {result}"
    );
    assert!(
        !result.contains("b.rs"),
        "无匹配文件不应出现: {result}"
    );
    assert!(
        !result.contains("fn needle"),
        "默认 files_with_matches 不应输出匹配行内容: {result}"
    );
}

#[tokio::test]
async fn test_grep_max_columns_skips_long_lines() {
    let dir = tempfile::tempdir().unwrap();
    // 构造一行 > 500 bytes 的匹配（600 字节前缀 + needle）
    let long_prefix = "a".repeat(600);
    let content = format!("{} needle\nshort needle", long_prefix);
    std::fs::write(dir.path().join("test.txt"), content).unwrap();
    let tool = GrepTool::new(dir.path().to_str().unwrap());
    let result = tool
        .invoke(serde_json::json!({
            "pattern": "needle",
            "output_mode": "content",
            "path": "./"
        }))
        .await
        .unwrap();
    assert!(
        result.contains("short needle"),
        "短行应正常输出: {result}"
    );
    assert!(
        !long_prefix.is_empty() && !result.contains(&long_prefix[..100]),
        "超 500 字节的长行不应出现在输出: {result}"
    );
}

#[tokio::test]
async fn test_grep_truncated_shows_pagination_hint() {
    let dir = tempfile::tempdir().unwrap();
    let lines: Vec<String> = (0..5).map(|i| format!("line {} needle", i)).collect();
    std::fs::write(dir.path().join("test.txt"), lines.join("\n")).unwrap();
    let tool = GrepTool::new(dir.path().to_str().unwrap());
    let result = tool
        .invoke(serde_json::json!({
            "pattern": "needle",
            "output_mode": "content",
            "path": "./",
            "head_limit": 2
        }))
        .await
        .unwrap();
    assert!(
        result.contains("[Showing results with pagination = limit: 2]"),
        "截断时应输出分页提示（offset=0 不输出 offset 字段）: {result}"
    );
    let result_with_offset = tool
        .invoke(serde_json::json!({
            "pattern": "needle",
            "output_mode": "content",
            "path": "./",
            "head_limit": 2,
            "offset": 2
        }))
        .await
        .unwrap();
    assert!(
        result_with_offset.contains("[Showing results with pagination = limit: 2, offset: 2]"),
        "带 offset 时分页提示应反映当前 offset: {result_with_offset}"
    );
}

// === P0+ : 三种 output_mode 差异化输出格式 + offset slice 语义 ===

/// 对齐上游 files_with_matches 模式：`Found N file[s][ limit: X[, offset: Y]]\n{files}`
#[tokio::test]
async fn test_grep_files_with_matches_format_aligned_upstream() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.txt"), "needle here").unwrap();
    std::fs::write(dir.path().join("b.txt"), "needle again").unwrap();
    std::fs::write(dir.path().join("c.txt"), "no match").unwrap();
    let tool = GrepTool::new(dir.path().to_str().unwrap());
    let result = tool
        .invoke(serde_json::json!({
            "pattern": "needle",
            "output_mode": "files_with_matches",
            "path": "./"
        }))
        .await
        .unwrap();
    assert!(
        result.starts_with("Found 2 files\n"),
        "应对齐上游 files_with_matches 头部格式 `Found N files\\n...`: {result}"
    );
    assert!(
        result.contains("a.txt") && result.contains("b.txt"),
        "应列出两个匹配文件: {result}"
    );
    assert!(
        !result.contains("limit:") && !result.contains("offset:"),
        "未截断时不应输出 pagination 信息: {result}"
    );
}

/// 对齐上游 count 模式：`{path:count}\n\nFound N total occurrences across M files[ with pagination = ...]`
#[tokio::test]
async fn test_grep_count_format_aligned_upstream() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.txt"), "needle\nneedle\nneedle").unwrap();
    std::fs::write(dir.path().join("b.txt"), "needle once").unwrap();
    std::fs::write(dir.path().join("c.txt"), "no match").unwrap();
    let tool = GrepTool::new(dir.path().to_str().unwrap());
    let result = tool
        .invoke(serde_json::json!({
            "pattern": "needle",
            "output_mode": "count",
            "path": "./"
        }))
        .await
        .unwrap();
    assert!(
        result.contains("a.txt:3") && result.contains("b.txt:1"),
        "应输出 path:count 行: {result}"
    );
    assert!(
        result.contains("Found 4 total occurrences across 2 files"),
        "应对齐上游 count summary 格式: {result}"
    );
}

/// 对齐上游 count 模式无匹配：`No matches found\n\nFound 0 total occurrences across 0 files.`
#[tokio::test]
async fn test_grep_count_no_matches_format() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.txt"), "nothing here").unwrap();
    let tool = GrepTool::new(dir.path().to_str().unwrap());
    let result = tool
        .invoke(serde_json::json!({
            "pattern": "zzz_not_here",
            "output_mode": "count",
            "path": "./"
        }))
        .await
        .unwrap();
    assert_eq!(
        result, "No matches found\n\nFound 0 total occurrences across 0 files.",
        "count 模式无匹配应对齐上游 summary（不省略）"
    );
}

/// 对齐上游 files_with_matches 无匹配：`No files found`
#[tokio::test]
async fn test_grep_files_with_matches_no_matches() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.txt"), "nothing here").unwrap();
    let tool = GrepTool::new(dir.path().to_str().unwrap());
    let result = tool
        .invoke(serde_json::json!({
            "pattern": "zzz_not_here",
            "output_mode": "files_with_matches",
            "path": "./"
        }))
        .await
        .unwrap();
    assert_eq!(
        result, "No files found",
        "files_with_matches 无匹配应对齐上游（No files found）"
    );
}

/// 对齐上游 content 无匹配：`No matches found`（注意上游没有句点）
#[tokio::test]
async fn test_grep_content_no_matches_format() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.txt"), "nothing here").unwrap();
    let tool = GrepTool::new(dir.path().to_str().unwrap());
    let result = tool
        .invoke(serde_json::json!({
            "pattern": "zzz_not_here",
            "output_mode": "content",
            "path": "./"
        }))
        .await
        .unwrap();
    assert_eq!(
        result, "No matches found",
        "content 无匹配应对齐上游（No matches found，无句点）"
    );
}

/// 对齐上游 applyHeadLimit slice 语义：slice(offset, offset+limit)
/// 5 行匹配，head_limit=2, offset=2 → 返回 line 2、line 3（共 2 行），且判定为截断
#[tokio::test]
async fn test_grep_offset_slice_semantics() {
    let dir = tempfile::tempdir().unwrap();
    let lines: Vec<String> = (0..5).map(|i| format!("line {} needle", i)).collect();
    std::fs::write(dir.path().join("test.txt"), lines.join("\n")).unwrap();
    let tool = GrepTool::new(dir.path().to_str().unwrap());
    let result = tool
        .invoke(serde_json::json!({
            "pattern": "needle",
            "output_mode": "content",
            "path": "./",
            "head_limit": 2,
            "offset": 2
        }))
        .await
        .unwrap();
    // 应跳过前 2 行（line 0、line 1），保留 line 2、line 3
    assert!(
        !result.contains("line 0") && !result.contains("line 1"),
        "offset=2 应跳过前 2 行: {result}"
    );
    assert!(
        result.contains("line 2") && result.contains("line 3"),
        "应包含 line 2 和 line 3: {result}"
    );
    // 关键：slice 语义要求返回 limit=2 行，而不是 limit-offset 行
    let line_count = result
        .lines()
        .filter(|l| l.contains("line ") && l.contains("needle"))
        .count();
    assert_eq!(
        line_count, 2,
        "slice(offset, offset+limit) 应返回 limit 行，实际 {} 行: {result}",
        line_count
    );
    assert!(
        result.contains("[Showing results with pagination = limit: 2, offset: 2]"),
        "截断时应同时显示 limit 和 offset: {result}"
    );
}

/// 边界用例：offset 超过总匹配数 → 返回 No matches found
#[tokio::test]
async fn test_grep_offset_exceeds_total() {
    let dir = tempfile::tempdir().unwrap();
    let lines: Vec<String> = (0..3).map(|i| format!("line {} needle", i)).collect();
    std::fs::write(dir.path().join("test.txt"), lines.join("\n")).unwrap();
    let tool = GrepTool::new(dir.path().to_str().unwrap());
    let result = tool
        .invoke(serde_json::json!({
            "pattern": "needle",
            "output_mode": "content",
            "path": "./",
            "head_limit": 10,
            "offset": 100
        }))
        .await
        .unwrap();
    assert_eq!(
        result, "No matches found",
        "offset 超过总匹配数应返回 No matches found: {result}"
    );
}

/// 单文件时 files_with_matches 用单数 `file`（对齐上游 plural）
#[tokio::test]
async fn test_grep_files_singular_plural() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.txt"), "needle here").unwrap();
    let tool = GrepTool::new(dir.path().to_str().unwrap());
    let result = tool
        .invoke(serde_json::json!({
            "pattern": "needle",
            "output_mode": "files_with_matches",
            "path": "./"
        }))
        .await
        .unwrap();
    assert!(
        result.starts_with("Found 1 file\n"),
        "单文件应用单数 `file`（对齐上游 plural）: {result}"
    );
}

/// 测试环境（cfg(test)）下 files_with_matches 按纯 filename 升序排序，deterministic
/// 对齐上游 GrepTool.ts:543-545：NODE_ENV=test 时 localeCompare
#[tokio::test]
async fn test_grep_files_with_matches_sorted_by_filename_in_tests() {
    let dir = tempfile::tempdir().unwrap();
    // 故意按非字母序写入，验证排序生效
    std::fs::write(dir.path().join("z.txt"), "needle").unwrap();
    std::fs::write(dir.path().join("m.txt"), "needle").unwrap();
    std::fs::write(dir.path().join("a.txt"), "needle").unwrap();
    let tool = GrepTool::new(dir.path().to_str().unwrap());
    let result = tool
        .invoke(serde_json::json!({
            "pattern": "needle",
            "output_mode": "files_with_matches",
            "path": "./"
        }))
        .await
        .unwrap();
    let lines: Vec<&str> = result.lines().collect();
    // 头部是 "Found 3 files"，随后 3 行文件名应按字母升序
    assert_eq!(lines[0], "Found 3 files");
    assert!(lines[1].ends_with("a.txt"), "第一项应是 a.txt: {result}");
    assert!(lines[2].ends_with("m.txt"), "第二项应是 m.txt: {result}");
    assert!(lines[3].ends_with("z.txt"), "第三项应是 z.txt: {result}");
}

/// FilesWithoutMatch 模式同样按 filename 升序（cfg(test) 下），保持 FilesOnly 风格一致
#[tokio::test]
async fn test_grep_files_without_matches_sorted_by_filename() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("z.txt"), "needle").unwrap(); // 有匹配
    std::fs::write(dir.path().join("m.txt"), "no match").unwrap();
    std::fs::write(dir.path().join("a.txt"), "no match").unwrap();
    let tool = GrepTool::new(dir.path().to_str().unwrap());
    let result = tool
        .invoke(serde_json::json!({
            "pattern": "needle",
            "output_mode": "files_without_matches",
            "path": "./"
        }))
        .await
        .unwrap();
    let lines: Vec<&str> = result.lines().collect();
    assert_eq!(lines[0], "Found 2 files");
    assert!(lines[1].ends_with("a.txt"), "应按字母序：a.txt: {result}");
    assert!(lines[2].ends_with("m.txt"), "应按字母序：m.txt: {result}");
    assert!(!result.contains("z.txt"), "有匹配的 z.txt 不应出现");
}

/// content 模式不排序（按目录遍历顺序输出）— 验证排序仅作用于 FilesOnly/FilesWithoutMatch
#[tokio::test]
async fn test_grep_content_mode_not_sorted() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("z.txt"), "needle").unwrap();
    std::fs::write(dir.path().join("a.txt"), "needle").unwrap();
    let tool = GrepTool::new(dir.path().to_str().unwrap());
    let result = tool
        .invoke(serde_json::json!({
            "pattern": "needle",
            "output_mode": "content",
            "path": "./"
        }))
        .await
        .unwrap();
    // content 模式应输出两个文件的匹配行
    assert!(result.contains("a.txt:1: needle"), "应包含 a.txt: {result}");
    assert!(result.contains("z.txt:1: needle"), "应包含 z.txt: {result}");
    // 不应有 Found N files 头部（content 模式不参与 mtime/filename 排序格式化）
    assert!(
        !result.contains("Found"),
        "content 模式不应有 Found 头部: {result}"
    );
}

// === 审计补漏：context 行 max_columns + 边界组合 ===

/// 对齐上游 `rg --max-columns 500` 影响所有输出行（含 -A/-B/-C 上下文）
/// 构造：needle 短匹配行 + 上下文行超 500 bytes，验证上下文也被跳过
#[tokio::test]
async fn test_grep_context_lines_also_filtered_by_max_columns() {
    let dir = tempfile::tempdir().unwrap();
    let long_line = "b".repeat(600);
    // 第 1 行：超长 before 上下文
    // 第 2 行：短匹配行（needle）
    // 第 3 行：短 after 上下文
    std::fs::write(
        dir.path().join("test.txt"),
        format!("{}\nneedle here\nshort after", long_line),
    )
    .unwrap();
    let tool = GrepTool::new(dir.path().to_str().unwrap());
    let result = tool
        .invoke(serde_json::json!({
            "pattern": "needle",
            "output_mode": "content",
            "path": "./",
            "-B": 1,
            "-A": 1
        }))
        .await
        .unwrap();
    assert!(
        result.contains("needle here"),
        "短匹配行应正常输出: {result}"
    );
    assert!(
        result.contains("short after"),
        "短 after 上下文应输出: {result}"
    );
    // 关键断言：超长 before 上下文应被 max_columns 跳过
    assert!(
        !result.contains(&long_line[..100]),
        "context 行超 500 bytes 也应被 max_columns 过滤（对齐上游 rg --max-columns）: {result}"
    );
}

/// FilesWithoutMatch + offset 切片：验证 cc-code 扩展模式也支持分页
#[tokio::test]
async fn test_grep_files_without_matches_with_offset() {
    let dir = tempfile::tempdir().unwrap();
    // 5 个无匹配文件（按字母序：a/b/c/d/e），1 个有匹配文件 z
    for c in ['a', 'b', 'c', 'd', 'e'] {
        let name = format!("{c}.txt");
        std::fs::write(dir.path().join(&name), "no match here").unwrap();
    }
    std::fs::write(dir.path().join("z.txt"), "needle here").unwrap();
    let tool = GrepTool::new(dir.path().to_str().unwrap());
    let result = tool
        .invoke(serde_json::json!({
            "pattern": "needle",
            "output_mode": "files_without_matches",
            "path": "./",
            "head_limit": 2,
            "offset": 1
        }))
        .await
        .unwrap();
    // cfg(test) 下按 filename 升序：a/b/c/d/e
    // slice(1, 3) → b, c
    let lines: Vec<&str> = result.lines().collect();
    assert!(
        result.starts_with("Found 2 files limit: 2, offset: 1"),
        "头部应反映 limit 和 offset: {result}"
    );
    assert_eq!(lines.len(), 3, "应有 header + 2 个文件: {result}");
    assert!(lines[1].ends_with("b.txt"), "slice 第 1 项: {result}");
    assert!(lines[2].ends_with("c.txt"), "slice 第 2 项: {result}");
    assert!(
        !lines.iter().skip(1).any(|l| l.ends_with("a.txt")),
        "a.txt 应被 offset 跳过: {result}"
    );
    assert!(
        !lines.iter().skip(1).any(|l| l.ends_with("z.txt")),
        "z.txt 有匹配，不应出现在无匹配列表: {result}"
    );
}

/// head_limit=0（unlimited）+ offset>0 边界组合
/// 对齐上游 applyHeadLimit L116：limit === 0 时走 `items.slice(offset)`，appliedLimit=undefined
#[tokio::test]
async fn test_grep_unlimited_with_offset() {
    let dir = tempfile::tempdir().unwrap();
    let lines: Vec<String> = (0..5).map(|i| format!("line {} needle", i)).collect();
    std::fs::write(dir.path().join("test.txt"), lines.join("\n")).unwrap();
    let tool = GrepTool::new(dir.path().to_str().unwrap());
    let result = tool
        .invoke(serde_json::json!({
            "pattern": "needle",
            "output_mode": "content",
            "path": "./",
            "head_limit": 0,
            "offset": 2
        }))
        .await
        .unwrap();
    // head_limit=0 表示不限制，offset=2 跳过前 2 行
    assert!(
        !result.contains("line 0") && !result.contains("line 1"),
        "offset=2 应跳过前 2 行: {result}"
    );
    assert!(
        result.contains("line 2") && result.contains("line 3") && result.contains("line 4"),
        "unlimited 模式应保留剩余所有匹配: {result}"
    );
    // 关键：head_limit=0 时 was_truncated=false，不应有分页提示
    assert!(
        !result.contains("[Showing results"),
        "head_limit=0 不截断不应显示分页提示: {result}"
    );
}
