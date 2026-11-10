use serde_json::json;

use super::*;

#[test]
fn rejects_missing_or_malformed_worktree_parameters() {
    for value in [
        json!({}),
        json!({ "cwd": null }),
        json!({ "cwd": 42 }),
        json!({ "cwd": "  " }),
        json!({ "cwd": "/repo", "branch": true }),
        json!({ "cwd": "/repo", "baseRef": [] }),
    ] {
        let error = parse_params(&value).unwrap_err();
        assert_eq!(error.code, -32602, "格式错误应返回 invalid params");
    }
}

#[test]
fn accepts_optional_create_parameters_in_camel_case() {
    let params = parse_params(
        &json!({ "cwd": "项目 with spaces", "branch": "feature/new", "baseRef": "HEAD~1" }),
    )
    .unwrap();
    assert_eq!(params.cwd, "项目 with spaces");
    assert_eq!(params.branch.as_deref(), Some("feature/new"));
    assert_eq!(params.base_ref.as_deref(), Some("HEAD~1"));
    let minimal = parse_params(&json!({ "cwd": "/repo" })).unwrap();
    assert!(minimal.branch.is_none());
    assert!(minimal.base_ref.is_none());
}

#[tokio::test]
async fn git_failure_is_reported_without_creating_storage() {
    let temporary = tempfile::tempdir().unwrap();
    let config_path = temporary.path().join("settings.json");
    let error = handle(
        "workspace/worktrees",
        &json!({ "cwd": temporary.path() }),
        &config_path,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, -32000, "非 Git 仓库应返回操作错误");
    assert!(error.message.contains("Cannot list worktrees"));
    assert!(!temporary.path().join("worktrees").exists());
}

#[tokio::test]
async fn independent_workspace_task_returns_error_to_requester() {
    use crate::transport::{mpsc::mpsc_transport_pair, types::IncomingMessage};

    let (client, server) = mpsc_transport_pair();
    let request = tokio::spawn(async move {
        client
            .send_request("workspace/worktree/create", json!({}))
            .await
    });
    let Some(IncomingMessage::Request { id, method, params }) = server.recv().await else {
        panic!("服务端应收到 workspace 请求");
    };
    // 独立任务不持有会话集合，参数错误也必须通过原请求 ID 返回。
    tokio::spawn(async move {
        handle_and_respond(&server, id, &method, &params, Path::new("settings.json")).await;
    });

    let error = tokio::time::timeout(std::time::Duration::from_secs(2), request)
        .await
        .expect("后台任务应及时发送响应")
        .expect("客户端请求任务应完成")
        .unwrap_err();
    assert_eq!(error.code, -32602);
    assert!(error.message.contains("cwd"));
}
