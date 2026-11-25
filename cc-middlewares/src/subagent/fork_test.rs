use super::*;
use cc_agent::tools::BaseTool;

fn make_tool(name: &'static str) -> Arc<dyn BaseTool> {
    struct DummyTool(&'static str);

    #[async_trait::async_trait]
    impl BaseTool for DummyTool {
        fn name(&self) -> &str {
            self.0
        }
        fn description(&self) -> &str {
            "dummy"
        }
        fn parameters(&self) -> serde_json::Value {
            serde_json::json!({})
        }
        async fn invoke(
            &self,
            _input: serde_json::Value,
        ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
            Ok(format!("{} result", self.0))
        }
    }

    Arc::new(DummyTool(name))
}

// ─── filter_tools tests ─────────────────────────────────────────────────

#[test]
fn test_filter_inherit_all() {
    let parent_tools = vec![make_tool("Read"), make_tool("Write"), make_tool("Agent")];
    let filtered = filter_tools(&parent_tools, &ToolsValue::Empty, &ToolsValue::Empty);
    let names: Vec<&str> = filtered.iter().map(|t| t.name()).collect();

    assert!(names.contains(&"Read"));
    assert!(names.contains(&"Write"));
    assert!(!names.contains(&"Agent"), "Agent should not be inherited");
}

#[test]
fn test_filter_allowlist() {
    let parent_tools = vec![make_tool("Read"), make_tool("Write"), make_tool("Glob")];
    let filtered = filter_tools(
        &parent_tools,
        &ToolsValue::List(vec!["Read".to_string(), "Glob".to_string()]),
        &ToolsValue::Empty,
    );
    let names: Vec<&str> = filtered.iter().map(|t| t.name()).collect();

    assert!(names.contains(&"Read"));
    assert!(names.contains(&"Glob"));
    assert!(
        !names.contains(&"Write"),
        "Write not in allowlist should be excluded"
    );
}

#[test]
fn test_filter_disallow() {
    let parent_tools = vec![make_tool("Read"), make_tool("Write"), make_tool("Edit")];
    let filtered = filter_tools(
        &parent_tools,
        &ToolsValue::Empty,
        &ToolsValue::List(vec!["Write".to_string(), "Edit".to_string()]),
    );
    let names: Vec<&str> = filtered.iter().map(|t| t.name()).collect();

    assert!(names.contains(&"Read"));
    assert!(!names.contains(&"Write"));
    assert!(!names.contains(&"Edit"));
}

#[test]
fn test_filter_wildcard_star() {
    let parent_tools = vec![
        make_tool("Read"),
        make_tool("Write"),
        make_tool("Bash"),
        make_tool("Agent"),
    ];
    let filtered = filter_tools(
        &parent_tools,
        &ToolsValue::List(vec!["*".to_string()]),
        &ToolsValue::Empty,
    );
    let names: Vec<&str> = filtered.iter().map(|t| t.name()).collect();

    assert!(names.contains(&"Read"));
    assert!(names.contains(&"Write"));
    assert!(names.contains(&"Bash"));
    assert!(
        !names.contains(&"Agent"),
        "Agent should still be excluded even with tools: *"
    );
}

#[test]
fn test_filter_wildcard_star_with_disallowed() {
    let parent_tools = vec![
        make_tool("Read"),
        make_tool("Write"),
        make_tool("Edit"),
        make_tool("Bash"),
    ];
    let filtered = filter_tools(
        &parent_tools,
        &ToolsValue::List(vec!["*".to_string()]),
        &ToolsValue::List(vec!["Write".to_string(), "Edit".to_string()]),
    );
    let names: Vec<&str> = filtered.iter().map(|t| t.name()).collect();

    assert!(names.contains(&"Read"));
    assert!(names.contains(&"Bash"));
    assert!(!names.contains(&"Write"));
    assert!(!names.contains(&"Edit"));
}

#[test]
fn test_filter_agent_excluded_even_when_explicitly_allowed() {
    let parent_tools = vec![make_tool("Read"), make_tool("Agent")];
    let filtered = filter_tools(
        &parent_tools,
        &ToolsValue::List(vec!["Agent".to_string(), "Read".to_string()]),
        &ToolsValue::Empty,
    );
    let names: Vec<&str> = filtered.iter().map(|t| t.name()).collect();

    assert!(names.contains(&"Read"));
    assert!(
        !names.contains(&"Agent"),
        "Agent must be excluded even when explicitly in allowlist (recursion prevention)"
    );
}

#[test]
fn test_filter_agent_excluded_when_in_disallowed() {
    let parent_tools = vec![make_tool("Read"), make_tool("Agent")];
    let filtered = filter_tools(
        &parent_tools,
        &ToolsValue::Empty,
        &ToolsValue::List(vec!["Agent".to_string()]),
    );
    let names: Vec<&str> = filtered.iter().map(|t| t.name()).collect();

    assert!(names.contains(&"Read"));
    assert!(!names.contains(&"Agent"));
}

#[test]
fn test_filter_case_insensitive() {
    let parent_tools = vec![make_tool("Read"), make_tool("Write"), make_tool("Glob")];

    let filtered = filter_tools(
        &parent_tools,
        &ToolsValue::List(vec!["READ".to_string(), "glob".to_string()]),
        &ToolsValue::Empty,
    );
    let names: Vec<&str> = filtered.iter().map(|t| t.name()).collect();

    assert!(
        names.contains(&"Read"),
        "Case-insensitive: READ should match Read"
    );
    assert!(
        names.contains(&"Glob"),
        "Case-insensitive: glob should match Glob"
    );
    assert!(
        !names.contains(&"Write"),
        "Write not in allowlist should be excluded"
    );

    // disallowedTools case-insensitive
    let filtered2 = filter_tools(
        &parent_tools,
        &ToolsValue::Empty,
        &ToolsValue::List(vec!["WRITE".to_string()]),
    );
    let names2: Vec<&str> = filtered2.iter().map(|t| t.name()).collect();

    assert!(names2.contains(&"Read"));
    assert!(names2.contains(&"Glob"));
    assert!(
        !names2.contains(&"Write"),
        "WRITE should case-insensitively exclude Write"
    );
}

#[test]
fn test_filter_prefix_wildcard_mcp_server() {
    let parent_tools = vec![
        make_tool("Read"),
        make_tool("mcp__nt-query-mcp__nt-order-query"),
        make_tool("mcp__other-server__some-tool"),
    ];
    let filtered = filter_tools(
        &parent_tools,
        &ToolsValue::List(vec!["Read".to_string(), "mcp__nt-query-mcp__*".to_string()]),
        &ToolsValue::Empty,
    );
    let names: Vec<&str> = filtered.iter().map(|t| t.name()).collect();

    assert!(
        names.contains(&"Read"),
        "plain tool in allowlist should be kept"
    );
    assert!(
        names.contains(&"mcp__nt-query-mcp__nt-order-query"),
        "prefix wildcard must match tools of the target MCP server"
    );
    assert!(
        !names.contains(&"mcp__other-server__some-tool"),
        "prefix wildcard must not match other MCP servers"
    );
}

#[test]
fn test_filter_prefix_wildcard_does_not_match_bare_prefix() {
    let parent_tools = vec![make_tool("mcp__srv"), make_tool("mcp__srv__tool")];
    let filtered = filter_tools(
        &parent_tools,
        &ToolsValue::List(vec!["mcp__srv__*".to_string()]),
        &ToolsValue::Empty,
    );
    let names: Vec<&str> = filtered.iter().map(|t| t.name()).collect();

    assert!(names.contains(&"mcp__srv__tool"));
    assert!(
        !names.contains(&"mcp__srv"),
        "prefix with trailing __ must not match the bare server name"
    );
}

#[test]
fn test_filter_prefix_wildcard_respects_disallowed() {
    let parent_tools = vec![
        make_tool("mcp__srv__read"),
        make_tool("mcp__srv__write"),
        make_tool("mcp__srv__delete"),
    ];
    let filtered = filter_tools(
        &parent_tools,
        &ToolsValue::List(vec!["mcp__srv__*".to_string()]),
        &ToolsValue::List(vec!["mcp__srv__delete".to_string()]),
    );
    let names: Vec<&str> = filtered.iter().map(|t| t.name()).collect();

    assert!(names.contains(&"mcp__srv__read"));
    assert!(names.contains(&"mcp__srv__write"));
    assert!(
        !names.contains(&"mcp__srv__delete"),
        "disallowedTools must still win over prefix wildcard"
    );
}

#[test]
fn test_filter_empty_parent_tools() {
    let filtered = filter_tools(&[], &ToolsValue::Empty, &ToolsValue::Empty);
    assert!(filtered.is_empty());
}

// ─── build_fork_directive tests ─────────────────────────────────────────

#[test]
fn test_build_fork_directive_contains_rules() {
    let directive = build_fork_directive("do the thing");
    assert!(directive.contains("<fork_directive>"));
    assert!(directive.contains("RULES"));
    assert!(directive.contains("do not spawn sub-agents"));
    assert!(directive.contains("do the thing"));
    assert!(directive.contains("</fork_directive>"));
}

#[test]
fn test_build_fork_directive_preserves_prompt() {
    let directive = build_fork_directive("analyze the performance bottleneck in main.rs");
    assert!(directive.contains("analyze the performance bottleneck in main.rs"));
}

// ─── overrides_from_agent_def tests ─────────────────────────────────────

#[test]
fn test_overrides_all_fields() {
    let ov = overrides_from_agent_def(
        "You are a reviewer.",
        &Some("Be thorough.".to_string()),
        &Some("Proactively suggest.".to_string()),
    );
    let ov = ov.unwrap();
    assert_eq!(ov.persona.as_deref().unwrap(), "You are a reviewer.");
    assert_eq!(ov.tone.as_deref().unwrap(), "Be thorough.");
    assert_eq!(ov.proactiveness.as_deref().unwrap(), "Proactively suggest.");
}

#[test]
fn test_overrides_empty_returns_none() {
    let ov = overrides_from_agent_def("", &None, &None);
    assert!(ov.is_none(), "All-empty fields should return None");
}

#[test]
fn test_overrides_persona_only() {
    let ov = overrides_from_agent_def("I am a helper.", &None, &None);
    let ov = ov.unwrap();
    assert_eq!(ov.persona.as_deref().unwrap(), "I am a helper.");
    assert!(ov.tone.is_none());
    assert!(ov.proactiveness.is_none());
}

#[test]
fn test_overrides_tone_only() {
    let ov = overrides_from_agent_def("", &Some("Be concise.".to_string()), &None);
    let ov = ov.unwrap();
    assert!(ov.persona.is_none());
    assert_eq!(ov.tone.as_deref().unwrap(), "Be concise.");
}
