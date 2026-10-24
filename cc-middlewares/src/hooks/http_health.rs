use parking_lot::Mutex;
use std::{
    collections::HashMap,
    sync::{Arc, OnceLock},
    time::{Duration, Instant},
};

/// HTTP Hook 熔断持续时长（300 秒），对齐 Jev 语义层熔断窗口。
pub const CIRCUIT_OPEN_SECS: u64 = 300;
pub const CIRCUIT_OPEN: Duration = Duration::from_secs(CIRCUIT_OPEN_SECS);

/// HTTP Hook 建立 TCP 连接的最大容忍时间（800 毫秒）。
///
/// 800ms 足以覆盖正常的本地环回（<1ms）和局域网/互联网连接，
/// 同时在目标端口未监听或不可达时迅速 fail-fast，绝不阻塞用户长达数百秒。
pub const DEFAULT_CONNECT_TIMEOUT_MS: u64 = 800;

/// 获取实际连接超时（支持环境变量 `HTTP_HOOK_CONNECT_TIMEOUT_MS` 覆盖）。
pub fn connect_timeout() -> Duration {
    if let Ok(val) = std::env::var("HTTP_HOOK_CONNECT_TIMEOUT_MS") {
        if let Ok(ms) = val.parse::<u64>() {
            return Duration::from_millis(ms);
        }
    }
    Duration::from_millis(DEFAULT_CONNECT_TIMEOUT_MS)
}

/// 端点级 HTTP Hook 健康状态。
#[derive(Default)]
pub struct HttpHookHealth {
    /// 熔断开始时刻。`None` = 未熔断（正常）。
    trip: Mutex<Option<Instant>>,
}

impl HttpHookHealth {
    pub fn new() -> Self {
        Self::default()
    }

    /// 当前端点是否处于熔断冷却中。
    pub fn is_open(&self) -> bool {
        match *self.trip.lock() {
            Some(at) => at.elapsed() < CIRCUIT_OPEN,
            None => false,
        }
    }

    /// 触发熔断。
    ///
    /// 返回 `true` 表示**本次刚跨入熔断窗口**（调用方用于打 WARN 日志）；
    /// 返回 `false` 表示已在熔断冷却期内，避免重复刷屏。
    pub fn trip(&self) -> bool {
        let mut guard = self.trip.lock();
        match *guard {
            Some(at) if at.elapsed() < CIRCUIT_OPEN => false,
            _ => {
                *guard = Some(Instant::now());
                true
            }
        }
    }

    /// 判定恢复成功 → 复位熔断状态。
    pub fn reset(&self) {
        *self.trip.lock() = None;
    }
}

/// 从完整 URL 中提取出基础端点 `scheme://host:port`，使同一服务的不同 path 共享熔断判定。
/// 如果解析失败，则回退到原始 url 字符串。
pub fn extract_endpoint(raw_url: &str) -> String {
    if let Ok(parsed) = url::Url::parse(raw_url) {
        let scheme = parsed.scheme();
        let host = parsed.host_str().unwrap_or("");
        if let Some(port) = parsed.port() {
            format!("{scheme}://{host}:{port}")
        } else {
            format!("{scheme}://{host}")
        }
    } else {
        raw_url.to_string()
    }
}

/// 按 endpoint 获取（或创建）共享的健康状态。
pub fn http_health_for(raw_url: &str) -> Arc<HttpHookHealth> {
    let registry = registry();
    let endpoint = extract_endpoint(raw_url);
    registry
        .lock()
        .entry(endpoint)
        .or_insert_with(|| Arc::new(HttpHookHealth::new()))
        .clone()
}

/// 进程内共享的 endpoint → 健康状态注册表。
fn registry() -> &'static Mutex<HashMap<String, Arc<HttpHookHealth>>> {
    static REGISTRY: OnceLock<Mutex<HashMap<String, Arc<HttpHookHealth>>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 清除所有 HTTP Hook 端点熔断记录（供测试使用）。
#[cfg(test)]
pub fn reset_all_http_health_for_test() {
    registry().lock().clear();
}

/// 串行化所有触碰全局熔断注册表的测试。
///
/// 注册表是进程级静态状态，cargo test 默认多线程并行 —— 若两个测试同时
/// reset/trip，会互相清掉对方的熔断标记，制造假失败。拿这把锁把测试串起来。
///
/// 用 tokio 的异步锁而非 std 锁：涉及的测试都要 `await` HTTP 请求，
/// 持 std guard 跨 await 会触发 clippy::await_holding_lock。
#[cfg(test)]
pub async fn test_lock() -> tokio::sync::MutexGuard<'static, ()> {
    static LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
        .lock()
        .await
}

#[cfg(test)]
#[path = "http_health_test.rs"]
mod tests;
