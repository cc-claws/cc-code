//! 回归：`peri/*` 自定义通知的路由判定。
//!
//! 背景：服务端 `event_sink` 发送的子 Agent 生命周期方法名是
//! `peri/subagent_lifecycle`（**不带** `notifications/` 前缀）。早期客户端只认
//! `notifications/peri/` 前缀，导致该通知被降级为 `Other` 并被 TUI 丢弃，
//! 底栏抽屉整条链路静默失效（`background_agents` 永远为空）。

use super::AcpTuiClient;

#[test]
fn test_peri_notification_routing_for_subagent_lifecycle() {
    // 服务端实际发送的方法名必须被识别
    assert!(AcpTuiClient::is_peri_notification(
        "peri/subagent_lifecycle"
    ));
    // 历史前缀形式保持兼容
    assert!(AcpTuiClient::is_peri_notification(
        "notifications/peri/anything"
    ));
    // 有专用通道的两个方法不走此分支
    assert!(!AcpTuiClient::is_peri_notification("peri/agent_event"));
    assert!(!AcpTuiClient::is_peri_notification("peri/agent_event_done"));
    assert!(!AcpTuiClient::is_peri_notification("session/update"));
}
