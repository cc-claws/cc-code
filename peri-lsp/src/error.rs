use thiserror::Error;

#[derive(Debug, Error)]
pub enum LspError {
    #[error("Failed to launch LSP server \"{server}\": {reason}")]
    LaunchFailed { server: String, reason: String },

    #[error("Failed to initialize LSP server \"{server}\": {reason}")]
    InitFailed { server: String, reason: String },

    #[error("LSP request timeout ({method}, {timeout_ms}ms)")]
    RequestTimeout { method: String, timeout_ms: u64 },

    #[error("LSP request failed ({method}): {reason}")]
    RequestFailed { method: String, reason: String },

    #[error("File content has been modified, retry needed")]
    ContentModified,

    #[error("Server \"{server}\" crashed (restarts: {restart_count}/{max_restarts})")]
    ServerCrashed {
        server: String,
        restart_count: u32,
        max_restarts: u32,
    },

    #[error("No LSP server available for file: {file_path}")]
    NoServerForFile { file_path: String },

    #[error("LSP server \"{server}\" not ready")]
    NotReady { server: String },

    #[error("LSP server connection closed")]
    TransportClosed,

    #[error("JSON-RPC 错误 (code {code}): {message}")]
    JsonRpcError { code: i64, message: String },

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),
}

impl LspError {
    /// 检查是否为 ContentModified 错误 (LSP error code -32801)
    pub fn is_content_modified(&self) -> bool {
        matches!(
            self,
            LspError::JsonRpcError { code: -32801, .. } | LspError::ContentModified
        )
    }
}
