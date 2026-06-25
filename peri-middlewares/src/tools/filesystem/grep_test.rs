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
        result.contains("[Showing results with pagination = limit: 3, offset: 0]"),
        "应显示分页提示: {result}"
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
        result.contains("[Showing results with pagination = limit: 2, offset: 0]"),
        "截断时应输出分页提示: {result}"
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
