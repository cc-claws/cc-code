use std::time::Duration;

/// Langfuse Client 认证配置
#[derive(Debug, Clone)]
pub struct ClientConfig {
    pub public_key: String,
    pub secret_key: String,
    pub base_url: String,
}

impl ClientConfig {
    /// 从环境变量构造配置
    /// 读取 LANGFUSE_PUBLIC_KEY、LANGFUSE_SECRET_KEY、LANGFUSE_BASE_URL
    /// base_url 默认值为 "https://cloud.langfuse.com"
    pub fn from_env() -> Result<Self, crate::LangfuseError> {
        let public_key = std::env::var("LANGFUSE_PUBLIC_KEY")
            .map_err(|_| crate::LangfuseError::Config("LANGFUSE_PUBLIC_KEY not set".into()))?;
        let secret_key = std::env::var("LANGFUSE_SECRET_KEY")
            .map_err(|_| crate::LangfuseError::Config("LANGFUSE_SECRET_KEY not set".into()))?;
        let base_url = std::env::var("LANGFUSE_BASE_URL")
            .unwrap_or_else(|_| "https://cloud.langfuse.com".to_string());
        Ok(Self {
            public_key,
            secret_key,
            base_url,
        })
    }
}

/// 背压策略
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BackpressurePolicy {
    /// 队列满时丢弃新事件
    #[default]
    DropNew,
    /// 队列满时阻塞等待
    Block,
}

/// Batcher 批量聚合配置
#[derive(Debug, Clone)]
pub struct BatcherConfig {
    pub max_events: usize,
    pub flush_interval: Duration,
    pub backpressure: BackpressurePolicy,
    pub max_retries: usize,
}

impl Default for BatcherConfig {
    fn default() -> Self {
        Self {
            max_events: 50,
            flush_interval: Duration::from_secs(10),
            backpressure: BackpressurePolicy::default(),
            max_retries: 3,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    include!("config_test.rs");
}
