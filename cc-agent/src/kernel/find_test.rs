use super::*;

struct MockGlob;

#[async_trait::async_trait]
impl BaseTool for MockGlob {
    fn name(&self) -> &str {
        "Glob"
    }
    fn description(&self) -> &str {
        "Find files"
    }
    fn parameters(&self) -> Value {
        json!({"type": "object", "properties": {"pattern": {"type": "string"}}, "required": ["pattern"]})
    }
    async fn invoke(
        &self,
        input: Value,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        crate::tools::validate_against_schema(&input, &self.parameters())?;
        Ok("一.rs\n二.rs\n三.rs".into())
    }
}

#[tokio::test]
async fn test_find_limit_normalizes_numbers_and_removes_adapter_parameter(
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let tool = PiFindTool(&MockGlob);
    for (limit, count) in [(2.9, 2), (0.0, 1), (-5.0, 1), (9999.0, 3)] {
        let output = tool
            .invoke(json!({"pattern": "*.rs", "limit": limit}))
            .await?;
        assert_eq!(
            output.lines().take_while(|line| !line.is_empty()).count(),
            count
        );
    }
    assert_eq!(
        tool.invoke(json!({"pattern": "*.rs"})).await?,
        "一.rs\n二.rs\n三.rs"
    );
    for invalid in [Value::Null, json!(true), json!("2")] {
        assert!(tool
            .invoke(json!({"pattern": "*.rs", "limit": invalid}))
            .await
            .is_err());
    }
    Ok(())
}

#[test]
fn test_find_limit_preserves_search_warnings_and_saved_output_hint() {
    let footer = "(Results are truncated.)\n\n[Full output saved to fixture.txt]\n[Warning: search was truncated due to safety limits.]";
    let output = limit_output(format!("a.rs\nb.rs\nc.rs\n\n{footer}"), 1);
    assert!(output.starts_with("a.rs\n\n[Showing first 1 files; find limit reached.]"));
    assert!(
        output.ends_with(footer),
        "不能丢弃底层安全限制和完整输出提示"
    );
    assert!(!output.contains("b.rs"));
    for original in ["No files found", "a.rs", "a.rs\nb.rs"] {
        assert_eq!(limit_output(original.to_string(), 2), original);
    }
}

#[test]
fn test_find_adapter_only_applies_to_pi_glob() {
    use crate::kernel::Kernel;
    assert!(Kernel::cc().adapt_tool(&MockGlob).is_none());
    assert!(Kernel::try_from_id("dsh")
        .unwrap()
        .adapt_tool(&MockGlob)
        .is_none());
    let pi = Kernel::try_from_id("pi").unwrap();
    let tool = pi.adapt_tool(&MockGlob).unwrap();
    let defs = pi.translate_definitions(vec![tool.definition()]);
    assert_eq!(defs[0].name, "find");
    assert_eq!(
        defs[0].parameters.as_json().unwrap()["properties"]["limit"]["type"],
        "number"
    );
}
