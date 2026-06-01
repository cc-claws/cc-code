use std::collections::HashMap;

use thiserror::Error;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::TcpListener,
};
use tracing::{info, warn};

const CALLBACK_TIMEOUT_SECS: u64 = 120;

#[derive(Debug, Error)]
pub enum CallbackError {
    #[error("回调服务器绑定失败: {0}")]
    BindFailed(String),
    #[error("回调服务器 IO 错误: {0}")]
    IoError(#[from] std::io::Error),
    #[error("回调服务器等待超时")]
    Timeout,
    #[error("回调 URL 解析失败: {0}")]
    ParseFailed(String),
}

pub struct OAuthCallbackServer {
    listener: TcpListener,
    state_param: String,
}

impl OAuthCallbackServer {
    pub async fn bind() -> Result<(Self, String), CallbackError> {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .map_err(|e| CallbackError::BindFailed(e.to_string()))?;
        let addr = listener
            .local_addr()
            .map_err(|e| CallbackError::BindFailed(e.to_string()))?;
        let redirect_uri = format!("http://{}/callback", addr);
        info!("OAuth 回调服务器已启动: {}", redirect_uri);
        Ok((
            Self {
                listener,
                state_param: String::new(),
            },
            redirect_uri,
        ))
    }

    pub async fn wait_for_code(mut self) -> Result<(String, String), CallbackError> {
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(CALLBACK_TIMEOUT_SECS),
            self.wait_inner(),
        )
        .await;
        match result {
            Ok(Ok(pair)) => Ok(pair),
            Ok(Err(e)) => Err(e),
            Err(_) => Err(CallbackError::Timeout),
        }
    }

    async fn wait_inner(&mut self) -> Result<(String, String), CallbackError> {
        let (mut stream, addr) = self
            .listener
            .accept()
            .await
            .map_err(CallbackError::IoError)?;
        info!("OAuth 回调服务器收到连接: {}", addr);

        let mut reader = BufReader::new(&mut stream);
        let mut request_line = String::new();
        reader
            .read_line(&mut request_line)
            .await
            .map_err(CallbackError::IoError)?;

        loop {
            let mut line = String::new();
            reader
                .read_line(&mut line)
                .await
                .map_err(CallbackError::IoError)?;
            if line == "\r\n" || line == "\n" {
                break;
            }
        }

        let url_path = request_line.split_whitespace().nth(1).unwrap_or("");
        let callback_result = parse_callback_url(url_path, &self.state_param);

        let response = match &callback_result {
            Ok((code, _)) => {
                info!(code = %code, "OAuth 回调成功");
                "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\n\r\n<html><body><h1>OAuth 授权成功</h1><p>您可以关闭此窗口并返回终端。</p></body></html>"
            }
            Err(e) => {
                warn!(error = %e, "OAuth 回调处理失败");
                &format!("HTTP/1.1 400 Bad Request\r\nContent-Type: text/html; charset=utf-8\r\n\r\n<html><body><h1>OAuth 授权失败</h1><p>{}</p></body></html>", e)[..]
            }
        };

        let resp_bytes = response.as_bytes();
        let resp_vec: Vec<u8> = resp_bytes.to_vec();
        stream
            .write_all(&resp_vec)
            .await
            .map_err(CallbackError::IoError)?;
        stream.shutdown().await.map_err(CallbackError::IoError)?;

        callback_result
    }
}

pub(crate) fn parse_callback_url(
    url_path: &str,
    expected_state: &str,
) -> Result<(String, String), CallbackError> {
    let url_str = if url_path.starts_with('/') {
        &format!("http://localhost{}", url_path)[..]
    } else {
        url_path
    };
    let parsed: url::Url = url_str
        .parse()
        .map_err(|e| CallbackError::ParseFailed(format!("URL 解析失败: {}", e)))?;
    let pairs: HashMap<String, String> = parsed
        .query_pairs()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    let code = pairs
        .get("code")
        .ok_or_else(|| CallbackError::ParseFailed("回调 URL 缺少 code 参数".into()))?
        .clone();
    let state = pairs
        .get("state")
        .ok_or_else(|| CallbackError::ParseFailed("回调 URL 缺少 state 参数".into()))?
        .clone();
    if !expected_state.is_empty() && state != expected_state {
        return Err(CallbackError::ParseFailed(format!(
            "CSRF state 不匹配: 期望 {}, 收到 {}",
            expected_state, state
        )));
    }
    Ok((code, state))
}

pub fn parse_code_from_url(url: &str) -> Result<(String, String), CallbackError> {
    let parsed: url::Url = url
        .parse()
        .map_err(|e| CallbackError::ParseFailed(format!("URL 解析失败: {}", e)))?;
    let pairs: HashMap<std::borrow::Cow<str>, std::borrow::Cow<str>> =
        parsed.query_pairs().collect();
    let code = pairs
        .get("code")
        .ok_or_else(|| CallbackError::ParseFailed("URL 缺少 code 参数".into()))?
        .to_string();
    let state = pairs
        .get("state")
        .ok_or_else(|| CallbackError::ParseFailed("URL 缺少 state 参数".into()))?
        .to_string();
    Ok((code, state))
}

#[cfg(test)]
#[path = "callback_server_test.rs"]
mod tests;
