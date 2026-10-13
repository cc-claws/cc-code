//! 判定服务健康状态：**端点级**熔断。
//!
//! 判定服务的可用性是端点级的全局事实——同一 endpoint 在会话 A 失败，会话 B 的
//! 请求也必然失败。因此按 endpoint 分桶、进程内共享，而不是每会话各持一份：
//! 后者只会把「同一故障弹 N 次」变成「每个会话各弹 N 次」。
//!
//! 存在的理由：[`super::JevGate::has_judge`] 只回答「有没有配凭据」，不回答
//! 「服务能不能用」。凭据在、额度耗尽（402）时，每条命令都要先跑一次注定失败的
//! 请求（吃掉超时 + 重试）再弹窗；熔断把这个时间窗收敛成一次。

use std::{
    collections::HashMap,
    sync::{Arc, OnceLock},
    time::{Duration, Instant},
};

use parking_lot::Mutex;

/// 熔断持续时长。
///
/// 覆盖「配置性错误不会自愈」的窗口，同时留一个到期探测点：用户改完配置后
/// 不必重启进程就能恢复判定。
const CIRCUIT_OPEN: Duration = Duration::from_secs(300);

/// 端点级判定服务健康状态。
#[derive(Default)]
pub struct JevHealth {
    /// 熔断开始时刻。`None` = 未熔断。
    trip: Mutex<Option<Instant>>,
}

impl JevHealth {
    pub fn new() -> Self {
        Self::default()
    }

    /// 当前是否处于熔断中（纯读，不改变状态）。
    pub fn is_open(&self) -> bool {
        match *self.trip.lock() {
            Some(at) => at.elapsed() < CIRCUIT_OPEN,
            None => false,
        }
    }

    /// 打开熔断。返回 `true` 表示**本次刚跨入熔断窗口**（调用方据此决定是否告警）。
    ///
    /// 已在窗口内时返回 `false`——重复失败不重复刷屏。窗口过期后再次失败会再报一次。
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

    /// 判定恢复成功 → 复位。
    pub fn reset(&self) {
        *self.trip.lock() = None;
    }
}

/// 按 endpoint 取（或创建）共享的健康状态。
///
/// 换 endpoint（改配置）即换一份独立状态——这正好符合「新网关是新的事实来源」。
pub fn health_for(endpoint: &str) -> Arc<JevHealth> {
    static REGISTRY: OnceLock<Mutex<HashMap<String, Arc<JevHealth>>>> = OnceLock::new();
    let registry = REGISTRY.get_or_init(|| Mutex::new(HashMap::new()));
    registry
        .lock()
        .entry(endpoint.to_string())
        .or_insert_with(|| Arc::new(JevHealth::new()))
        .clone()
}

#[cfg(test)]
#[path = "health_test.rs"]
mod tests;
