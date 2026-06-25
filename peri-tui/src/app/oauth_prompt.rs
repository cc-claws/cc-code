use super::events::OAuthCallbackResult;

/// OAuth 授权弹窗状态
pub struct OAuthPrompt {
    /// 服务器名称
    pub server_name: String,
    /// 浏览器授权 URL
    pub authorization_url: String,
    /// 用户手动粘贴的回调 URL（或含 code 的文本）
    pub input: String,
    /// 输入光标位置（字符索引）
    pub cursor: usize,
    /// 回调通道（传回后台 OAuth 流程）
    pub callback_tx: Option<tokio::sync::oneshot::Sender<OAuthCallbackResult>>,
    /// 错误提示信息（粘贴内容解析失败时显示）
    pub error_message: Option<String>,
}

impl OAuthPrompt {
    pub fn new(
        server_name: String,
        authorization_url: String,
        callback_tx: tokio::sync::oneshot::Sender<OAuthCallbackResult>,
    ) -> Self {
        Self {
            server_name,
            authorization_url,
            input: String::new(),
            cursor: 0,
            callback_tx: Some(callback_tx),
            error_message: None,
        }
    }

    /// 提交用户输入的回调 URL，返回 true 表示成功发送
    pub fn submit(&mut self) -> bool {
        use peri_middlewares::mcp::parse_code_from_url;
        match parse_code_from_url(&self.input) {
            Ok((code, state)) => {
                if let Some(tx) = self.callback_tx.take() {
                    let _ = tx.send(OAuthCallbackResult { code, state });
                }
                true
            }
            Err(e) => {
                self.error_message = Some(format!("无法解析回调 URL: {}", e));
                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    include!("oauth_prompt_test.rs");
}
