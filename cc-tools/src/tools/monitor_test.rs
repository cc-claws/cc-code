//! Monitor 工具的单测：只验证入参与后台请求形态，不真正起进程。

use crate::tools::monitor::MonitorTool;
use async_trait::async_trait;
use cc_agent::shell::{
    AgentShellHandle, ExitSignal, ShellAbortHandle, ShellExecutor, ShellHandoff, ShellRequest,
};
use cc_agent::tools::BaseTool;
use serde_json::json;
use std::path::PathBuf;
use std::sync::Arc;

/// 记录请求并直接返回一个假 handle 的执行器。
struct RecordingExecutor {
    requests: tokio::sync::Mutex<Vec<ShellRequest>>,
}

impl RecordingExecutor {
    fn new() -> Self {
        Self {
            requests: tokio::sync::Mutex::new(Vec::new()),
        }
    }
}

#[async_trait]
impl ShellExecutor for RecordingExecutor {
    async fn execute(&self, req: ShellRequest) -> anyhow::Result<AgentShellHandle> {
        self.requests.lock().await.push(req);
        let (_tx, rx) = tokio::sync::oneshot::channel();
        Ok(AgentShellHandle {
            task_id: "monitor-task-1".to_string(),
            output_path: PathBuf::from("/tmp/monitor-task-1.log"),
            result_rx: rx,
            exit_signal: Arc::new(ExitSignal::new()),
            handoff: Arc::new(ShellHandoff::new(true, true)),
            kill: ShellAbortHandle::noop(),
        })
    }
}

#[tokio::test]
async fn rejects_empty_command() {
    let error = MonitorTool::with_executor("/tmp", Arc::new(RecordingExecutor::new()))
        .invoke(json!({ "command": "  ", "description": "watch" }))
        .await
        .expect_err("空命令应报错");
    assert!(error.to_string().contains("cannot be empty"), "{error}");
}

#[tokio::test]
async fn rejects_empty_description() {
    let error = MonitorTool::with_executor("/tmp", Arc::new(RecordingExecutor::new()))
        .invoke(json!({ "command": "tail -f app.log", "description": "" }))
        .await
        .expect_err("空描述应报错");
    assert!(error.to_string().contains("cannot be empty"), "{error}");
}

#[tokio::test]
async fn runs_in_background_and_reports_output_path() {
    let executor = Arc::new(RecordingExecutor::new());
    let tool = MonitorTool::with_executor("/tmp", executor.clone());
    let output = tool
        .invoke(json!({ "command": "tail -f app.log", "description": "Watch app log" }))
        .await
        .expect("正常监控应成功");

    // 必须走后台通道，否则会占住前台轮次。
    let requests = executor.requests.lock().await;
    assert_eq!(requests.len(), 1);
    assert!(requests[0].run_in_background, "Monitor 必须以后台方式启动");
    assert_eq!(requests[0].command, "tail -f app.log");
    assert_eq!(
        requests[0].description.as_deref(),
        Some("Watch app log"),
        "description 应透传给后台任务面板"
    );

    // 返回值要让模型知道 task id 和输出文件，便于后续 Read。
    assert!(output.contains("<monitor-started>"), "{output}");
    assert!(output.contains("monitor-task-1"), "{output}");
    assert!(output.contains("/tmp/monitor-task-1.log"), "{output}");
}

#[test]
fn exposes_expected_schema() {
    let tool = MonitorTool::with_executor("/tmp", Arc::new(RecordingExecutor::new()));
    assert_eq!(tool.name(), "Monitor");
    let params = tool.parameters();
    assert_eq!(params["required"], json!(["command", "description"]));
}
