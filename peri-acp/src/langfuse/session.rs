use std::{sync::Arc, time::Duration};

use langfuse_client::{BackpressurePolicy, Batcher, BatcherConfig, LangfuseClient};

use super::config::LangfuseConfig;

/// Langfuse 进程级共享连接状态。
///
/// 生命周期：进程启动时构造一次，所有 session 的 `LangfuseTracer` 共享同一个 client + batcher。
/// `session_id` 在 per-turn `LangfuseTracer` 级别传入（每次 execute_prompt 的 session_id 不同）。
pub struct LangfuseSession {
    pub client: Arc<LangfuseClient>,
    pub batcher: Arc<Batcher>,
}

impl LangfuseSession {
    /// 从配置构造 Session，失败时返回 None（静默降级）
    pub async fn new(config: LangfuseConfig) -> Option<Self> {
        let client = Arc::new(LangfuseClient::new(
            &config.public_key,
            &config.secret_key,
            &config.host,
            3, // max_retries
        ));

        let batcher_config = BatcherConfig {
            max_events: 50,
            flush_interval: Duration::from_secs(10),
            backpressure: BackpressurePolicy::DropNew,
            max_retries: 3,
        };
        let batcher = Batcher::new((*client).clone(), batcher_config);

        Some(Self {
            client,
            batcher: Arc::new(batcher),
        })
    }
}
