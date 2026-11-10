//! Worktree 的 ACP 扩展请求；独立于会话，Git 子进程在阻塞线程执行。

use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::Value;

use crate::{
    transport::{
        types::{AcpError, RequestId},
        AcpTransport,
    },
    worktree,
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RequestParams {
    cwd: String,
    branch: Option<String>,
    base_ref: Option<String>,
}

fn parse_params(params: &Value) -> Result<RequestParams, AcpError> {
    let parsed: RequestParams = serde_json::from_value(params.clone())
        .map_err(|error| AcpError::new(-32602, format!("Invalid worktree parameters: {error}")))?;
    if parsed.cwd.trim().is_empty() {
        return Err(AcpError::new(
            -32602,
            "cwd must be a nonempty directory path",
        ));
    }
    Ok(parsed)
}

fn storage_root(config_path: &Path) -> Result<PathBuf, AcpError> {
    let root = config_path
        .parent()
        .unwrap_or(Path::new("."))
        .join("worktrees");
    if root.is_absolute() {
        Ok(root)
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(root))
            .map_err(|error| {
                AcpError::new(-32603, format!("Cannot resolve worktree storage: {error}"))
            })
    }
}

pub(super) async fn handle(
    method: &str,
    params: &Value,
    config_path: &Path,
) -> Result<Value, AcpError> {
    let params = parse_params(params)?;
    let cwd = PathBuf::from(params.cwd);
    match method {
        "workspace/worktrees" => {
            let listing = tokio::task::spawn_blocking(move || worktree::list(&cwd))
                .await
                .map_err(|error| AcpError::new(-32603, format!("Worktree task failed: {error}")))?
                .map_err(|error| {
                    AcpError::new(-32000, format!("Cannot list worktrees: {error:#}"))
                })?;
            serde_json::to_value(listing)
                .map_err(|error| AcpError::new(-32603, format!("Serialize failed: {error}")))
        }
        "workspace/worktree/create" => {
            let storage = storage_root(config_path)?;
            let checkout = tokio::task::spawn_blocking(move || {
                worktree::create(
                    &cwd,
                    params.branch.as_deref(),
                    params.base_ref.as_deref(),
                    &storage,
                )
            })
            .await
            .map_err(|error| AcpError::new(-32603, format!("Worktree task failed: {error}")))?
            .map_err(|error| AcpError::new(-32000, format!("Cannot create worktree: {error:#}")))?;
            serde_json::to_value(checkout)
                .map_err(|error| AcpError::new(-32603, format!("Serialize failed: {error}")))
        }
        _ => Err(AcpError::new(-32601, format!("Method not found: {method}"))),
    }
}

pub(super) async fn handle_and_respond(
    transport: &dyn AcpTransport,
    id: RequestId,
    method: &str,
    params: &Value,
    config_path: &Path,
) {
    let result = handle(method, params, config_path).await;
    let _ = transport.send_response(id, result).await;
}

#[cfg(test)]
#[path = "worktree_requests_test.rs"]
mod tests;
