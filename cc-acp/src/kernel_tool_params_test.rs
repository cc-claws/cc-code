//! 使用真实工具 schema 和执行器验证 #1006，避免仅断言工具名掩盖参数错误。

use cc_agent::{
    agent::{
        react::{AgentInput, Reasoning, ToolCall},
        state::AgentState,
        ReActAgent,
    },
    kernel::Kernel,
    llm::MockLLM,
    tools::{validate_against_schema, BaseTool, ToolDefinition},
};
use cc_middlewares::{
    ask_user::{ask_user_tool_definition, parse_ask_user},
    middleware::{FilesystemMiddleware, TerminalMiddleware},
    tools::{GrepTool, ReadFileTool, SkillTool},
};
use serde_json::{json, Value};

fn make_translation(kernel: Kernel, def: ToolDefinition, input: Value) -> Value {
    let canonical = def.name.clone();
    let translated = kernel.translate_definitions(vec![def.clone()]);
    assert_eq!(
        validate_against_schema(&input, translated[0].parameters.as_json().unwrap()),
        Ok(())
    );
    let normalized = kernel.normalize_input(&canonical, &input);
    assert_eq!(
        validate_against_schema(&normalized, def.parameters.as_json().unwrap()),
        Ok(())
    );
    assert_eq!(
        kernel.dialectize_input(&canonical, &normalized),
        input,
        "历史参数必须可回译"
    );
    normalized
}

#[test]
fn test_cc_real_tool_definitions_remain_byte_equivalent() {
    let mut tools = FilesystemMiddleware::build_tools(".", Default::default());
    tools.extend(TerminalMiddleware::new().build_tools("."));
    tools.push(Box::new(SkillTool::new(vec![])));
    let mut definitions: Vec<_> = tools.iter().map(|tool| tool.definition()).collect();
    definitions.push(ask_user_tool_definition());
    let before = serde_json::to_vec(&definitions).unwrap();
    let after = serde_json::to_vec(&Kernel::cc().translate_definitions(definitions)).unwrap();
    assert_eq!(before, after, "cc 内核的真实工具定义必须逐字节不变");
}

#[test]
fn test_dsh_tool_parameters_validate_against_real_schemas() {
    let dsh = Kernel::try_from_id("dsh").unwrap();
    let skill = SkillTool::new(vec![]);
    let normalized = make_translation(
        dsh,
        skill.definition(),
        json!({"name": "review", "args": "file.rs"}),
    );
    assert_eq!(normalized, json!({"skill": "review", "args": "file.rs"}));
    let bash = TerminalMiddleware::new().build_tools(".");
    let normalized = make_translation(
        dsh,
        bash[0].definition(),
        json!({"command": "echo ok", "timeoutMs": 30000}),
    );
    assert_eq!(normalized["timeout"], 30000);
    let input = json!({"questions": [{
        "question": "选择目标", "header": "目标", "multi_select": true,
        "options": [{"label": "A"}, {"label": "B"}]
    }]});
    let normalized = make_translation(dsh, ask_user_tool_definition(), input);
    let questions = parse_ask_user(&ToolCall::new("ask", "AskUserQuestion", normalized)).unwrap();
    assert!(questions[0].multi_select, "嵌套字段必须传入实际交互解析器");
}

#[test]
fn test_dsh_unsupported_parameters_remain_explicit_errors() {
    let dsh = Kernel::try_from_id("dsh").unwrap();
    let bash = TerminalMiddleware::new().build_tools(".");
    let defs = dsh.translate_definitions(vec![bash[0].definition(), ask_user_tool_definition()]);
    let result = validate_against_schema(
        &json!({"command": "echo ok", "workdir": "somewhere"}),
        defs[0].parameters.as_json().unwrap(),
    );
    assert!(result.is_err());
    assert!(defs[0].description.contains("workdir is not supported"));
    let result = validate_against_schema(
        &json!({"questions": [{"id": "q1", "question": "Q", "header": "Q", "options": [{"label": "A"}, {"label": "B"}]}]}),
        defs[1].parameters.as_json().unwrap(),
    );
    assert!(result.is_err());
    assert!(defs[1].description.contains("id is not supported"));
}

#[tokio::test]
async fn test_pi_grep_parameters_change_real_search_results(
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let directory = tempfile::tempdir()?;
    std::fs::write(
        directory.path().join("fixture.txt"),
        "HELLO.*one\nHELLOxx\nHELLO.*two\n",
    )?;
    let tool = GrepTool::new(directory.path().to_string_lossy());
    let pi = Kernel::try_from_id("pi")?;
    let normalized = make_translation(
        pi,
        tool.definition(),
        json!({"pattern": "hello.*", "ignoreCase": true, "literal": true, "limit": 1, "output_mode": "content"}),
    );
    assert_eq!(normalized["-i"], true);
    assert_eq!(normalized["fixed_strings"], true);
    assert_eq!(normalized["head_limit"], 1);
    let output = tool.invoke(normalized).await?;
    assert!(output.contains("HELLO.*one"));
    assert!(!output.contains("HELLOxx"));
    assert!(!output.contains("HELLO.*two"));
    Ok(())
}

#[tokio::test]
async fn test_pi_find_limit_through_executor(
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let directory = tempfile::tempdir()?;
    for file in ["a.rs", "b.rs", "c.rs"] {
        std::fs::write(directory.path().join(file), "fixture")?;
    }
    let pi = Kernel::try_from_id("pi")?;
    for registered in [false, true] {
        let reasoning = pi.normalize_reasoning(Reasoning::with_tools(
            "",
            vec![ToolCall::new(
                "find",
                "find",
                json!({"pattern": "*.rs", "limit": 1}),
            )],
        ));
        let mut agent = ReActAgent::new(MockLLM::new(vec![
            reasoning,
            Reasoning::with_answer("", "done"),
        ]))
        .with_kernel(pi);
        if registered {
            let tools = FilesystemMiddleware::build_tools(
                &directory.path().to_string_lossy(),
                Default::default(),
            );
            agent = agent.register_tool(
                tools
                    .into_iter()
                    .find(|tool| tool.name() == "Glob")
                    .unwrap(),
            );
        } else {
            agent = agent.add_middleware(Box::new(FilesystemMiddleware::new()));
        }
        let mut state = AgentState::new(directory.path().to_string_lossy());
        let output = agent
            .execute(AgentInput::text("find rust files"), &mut state, None)
            .await?;
        assert_eq!(output.tool_calls.len(), 1);
        let result = &output.tool_calls[0].1;
        assert!(
            !result.is_error,
            "find.limit 不能被执行器 schema 拒绝: {}",
            result.output
        );
        assert_eq!(
            result
                .output
                .lines()
                .filter(|line| line.ends_with(".rs"))
                .count(),
            1
        );
        assert!(result.output.contains("find limit reached"));
    }
    Ok(())
}

#[tokio::test]
async fn test_kernel_offset_and_timeout_preserve_documented_peri_semantics(
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let directory = tempfile::tempdir()?;
    std::fs::write(
        directory.path().join("fixture.txt"),
        "first\nsecond\nthird\n",
    )?;
    let tool = ReadFileTool::new(directory.path().to_string_lossy());
    for id in ["dsh", "pi"] {
        let kernel = Kernel::try_from_id(id)?;
        let defs = kernel.translate_definitions(vec![tool.definition()]);
        assert!(defs[0].description.contains("0-based"));
        assert!(kernel.prompt_for("test").contains("0-based"));
        let input = kernel.dialectize_input(
            "Read",
            &json!({"file_path": "fixture.txt", "offset": 0, "limit": 1}),
        );
        let output = tool
            .invoke(make_translation(kernel, tool.definition(), input))
            .await?;
        assert!(output.contains("first"));
        assert!(!output.contains("second"));
        let bash = TerminalMiddleware::new().build_tools(".");
        let defs = kernel.translate_definitions(vec![bash[0].definition()]);
        assert!(defs[0].description.contains("milliseconds"));
        assert!(kernel.prompt_for("test").contains("milliseconds"));
    }
    Ok(())
}
