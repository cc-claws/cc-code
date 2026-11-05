use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use cc_agent::{
    agent::{
        react::{AgentInput, ReactLLM, Reasoning, ToolCall},
        state::AgentState,
        ReActAgent,
    },
    error::AgentResult,
    kernel::{Kernel, MutationForm},
    llm::types::StreamingContext,
    messages::BaseMessage,
    tools::{BaseTool, ToolInvocationContext},
};
use cc_tools::tools::{filesystem::EditFileTool, output_persist::persist_truncated_output};
use serde_json::{json, Value};

struct OnceLlm {
    input: Value,
    turn: AtomicUsize,
}

#[async_trait]
impl ReactLLM for OnceLlm {
    async fn generate_reasoning(
        &self,
        _: &[BaseMessage],
        _: &[&dyn BaseTool],
        _: Option<StreamingContext>,
    ) -> AgentResult<Reasoning> {
        Ok(if self.turn.fetch_add(1, Ordering::SeqCst) == 0 {
            Reasoning::with_tools(
                "edit",
                vec![ToolCall::new("edit-1", "edit", self.input.clone())],
            )
        } else {
            Reasoning::with_answer("", "done")
        })
    }
}

async fn run_pi_edit(cwd: &str, input: Value) -> cc_agent::agent::react::AgentOutput {
    let kernel = Kernel::from_config_checked("pi").unwrap();
    let agent = ReActAgent::new(OnceLlm {
        input,
        turn: AtomicUsize::new(0),
    })
    .max_iterations(3)
    .with_kernel(kernel)
    .with_tool_filter(move |name| kernel.should_hide(name))
    .register_tool(Box::new(
        EditFileTool::new(cwd).with_form(MutationForm::BatchReplace),
    ));
    agent
        .execute(AgentInput::text("audit"), &mut AgentState::new(cwd), None)
        .await
        .unwrap()
}

#[tokio::test]
async fn audit_pi_rejects_conflicting_nested_edit_fields_without_writing() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sample.txt");
    std::fs::write(&path, "alpha beta").unwrap();
    let output = run_pi_edit(dir.path().to_str().unwrap(), json!({
        "path": "sample.txt",
        "edits": [{"oldText": "alpha", "old_string": "beta", "newText": "DIALECT", "new_string": "CANONICAL"}]
    })).await;
    let after = std::fs::read_to_string(path).unwrap();
    assert!(
        output.tool_calls[0].1.is_error && after == "alpha beta",
        "conflict must reject with unchanged file; error={}, output={:?}, file={after:?}",
        output.tool_calls[0].1.is_error,
        output.tool_calls[0].1.output
    );
}

#[tokio::test]
async fn audit_pi_rejects_conflicting_top_level_paths_control() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sample.txt");
    std::fs::write(&path, "alpha beta").unwrap();
    let output = run_pi_edit(
        dir.path().to_str().unwrap(),
        json!({
            "path": "other.txt", "file_path": "sample.txt",
            "edits": [{"oldText": "alpha", "newText": "changed"}]
        }),
    )
    .await;
    assert!(output.tool_calls[0].1.is_error);
    assert_eq!(std::fs::read_to_string(path).unwrap(), "alpha beta");
}

#[tokio::test]
async fn audit_pi_normal_edit_control() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sample.txt");
    std::fs::write(&path, "alpha beta").unwrap();
    let output = run_pi_edit(
        dir.path().to_str().unwrap(),
        json!({
            "path": "sample.txt", "edits": [{"oldText": "alpha", "newText": "changed"}]
        }),
    )
    .await;
    assert!(
        !output.tool_calls[0].1.is_error,
        "{:?}",
        output.tool_calls[0].1.output
    );
    assert_eq!(std::fs::read_to_string(path).unwrap(), "changed beta");
}

#[tokio::test]
async fn audit_pi_preserves_real_output_file_path() {
    let dir = tempfile::tempdir().unwrap();
    let output_dir = dir.path().join("file_path");
    std::fs::create_dir(&output_dir).unwrap();
    let result = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--ignored",
            "--exact",
            "audit_output_file_probe",
            "--nocapture",
        ])
        .env("TMP", &output_dir)
        .env("TEMP", &output_dir)
        .env("TMPDIR", &output_dir)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stdout)
    );
}

#[tokio::test]
#[ignore = "run in a subprocess with its own temporary directory environment"]
async fn audit_output_file_probe() {
    let kernel = Kernel::from_config_checked("pi").unwrap();
    let hint = ToolInvocationContext {
        tool_call_id: "output-1".into(),
        source_agent_id: None,
    }
    .scope_with_kernel(kernel, async {
        persist_truncated_output("full output sentinel")
    })
    .await;
    let reported = hint
        .split("[Full output saved to ")
        .nth(1)
        .unwrap()
        .split(']')
        .next()
        .unwrap();
    assert!(
        std::path::Path::new(reported).exists(),
        "reported file must exist: {hint}"
    );
    assert_eq!(
        std::fs::read_to_string(reported).unwrap(),
        "full output sentinel"
    );
    std::fs::remove_file(reported).unwrap();
}

#[tokio::test]
async fn audit_pi_crlf_edit_preserves_expected_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sample.txt");
    std::fs::write(&path, "alpha\r\nbeta\r\n").unwrap();
    let output = run_pi_edit(
        dir.path().to_str().unwrap(),
        json!({
            "path": "sample.txt", "edits": [{"oldText": "alpha", "newText": "new\r\nline"}]
        }),
    )
    .await;
    assert!(
        !output.tool_calls[0].1.is_error,
        "{:?}",
        output.tool_calls[0].1.output
    );
    assert_eq!(
        std::fs::read_to_string(path).unwrap(),
        "new\r\nline\r\nbeta\r\n"
    );
}
