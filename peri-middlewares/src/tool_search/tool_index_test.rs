use super::*;
use serde_json::json;

struct MockTool {
    name_str: String,
    desc_str: String,
    params: serde_json::Value,
}

impl MockTool {
    fn new(name: &str, desc: &str) -> Self {
        Self {
            name_str: name.to_string(),
            desc_str: desc.to_string(),
            params: json!({"type": "object", "properties": {}}),
        }
    }
}

#[async_trait::async_trait]
impl BaseTool for MockTool {
    fn name(&self) -> &str {
        &self.name_str
    }
    fn description(&self) -> &str {
        &self.desc_str
    }
    fn parameters(&self) -> serde_json::Value {
        self.params.clone()
    }
    async fn invoke(
        &self,
        _input: serde_json::Value,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        Ok("mock".to_string())
    }
}

fn make_mock_tools() -> Vec<Arc<dyn BaseTool>> {
    vec![
        Arc::new(MockTool::new(
            "CronRegister",
            "Register a cron scheduled task",
        )),
        Arc::new(MockTool::new("CronList", "List all cron tasks")),
        Arc::new(MockTool::new("CronRemove", "Remove a cron task by ID")),
        Arc::new(MockTool::new(
            "mcp__slack__send_message",
            "Send a message to Slack channel",
        )),
        Arc::new(MockTool::new(
            "mcp__github__create_issue",
            "Create a GitHub issue",
        )),
    ]
}

#[test]
fn test_build_index() {
    let index = ToolSearchIndex::new();
    let tools = make_mock_tools();
    index.build(tools);
    assert_eq!(index.list_names().len(), 5);
}

#[test]
fn test_keyword_search() {
    let index = ToolSearchIndex::new();
    let tools = make_mock_tools();
    index.build(tools);

    let results = index.search("cron create", 3);
    assert!(!results.is_empty());
    // CronRegister should rank high
    assert!(results[0].name.contains("Cron"));
}

#[test]
fn test_tfidf_search() {
    let index = ToolSearchIndex::new();
    let tools = make_mock_tools();
    index.build(tools);

    let results = index.search("schedule task", 3);
    assert!(!results.is_empty());
}

#[test]
fn test_hybrid_search() {
    let index = ToolSearchIndex::new();
    let tools = make_mock_tools();
    index.build(tools);

    let results = index.search("+slack message", 5);
    // Required word "slack" should filter to only slack tools
    assert!(results
        .iter()
        .all(|r| r.name.to_lowercase().contains("slack")));
}

#[test]
fn test_get_tool() {
    let index = ToolSearchIndex::new();
    let tools = make_mock_tools();
    index.build(tools);

    assert!(index.get_tool("CronRegister").is_some());
    assert!(index.get_tool("NonExistent").is_none());
}

#[test]
fn test_format_deferred_list() {
    let index = ToolSearchIndex::new();
    let tools = make_mock_tools();
    index.build(tools);

    let list = index.format_deferred_list();
    assert!(list.contains("CronRegister"));
    // MCP 工具不出现在 Deferred Tools 段（避免 system prompt 不稳定导致缓存失效）
    assert!(!list.contains("mcp__slack__send_message"));
    assert!(!list.contains("mcp__github__create_issue"));
}

#[test]
fn test_total_count() {
    let index = ToolSearchIndex::new();
    assert_eq!(index.total_count(), 0);

    let tools = make_mock_tools();
    index.build(tools);
    assert_eq!(index.total_count(), 5);
}

#[test]
fn test_select_exact_match() {
    let index = ToolSearchIndex::new();
    let tools = make_mock_tools();
    index.build(tools);

    let results = index.search("select:CronRegister,CronList", 10);
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].name, "CronRegister");
    assert_eq!(results[1].name, "CronList");
}

#[test]
fn test_select_partial_miss() {
    let index = ToolSearchIndex::new();
    let tools = make_mock_tools();
    index.build(tools);

    let results = index.search("select:CronRegister,NonExistent", 10);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "CronRegister");
}

#[test]
fn test_select_empty_result() {
    let index = ToolSearchIndex::new();
    let tools = make_mock_tools();
    index.build(tools);

    let results = index.search("select:NonExistent", 10);
    assert!(results.is_empty());
}
