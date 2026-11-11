//! Hook → HITL 的权限裁决通道。
//!
//! `HookMiddleware` 在 `PreToolUse` / `PermissionRequest` 阶段解析 hook 返回的
//! `permissionDecision`（对齐参考实现的 `allow` / `deny` / `ask`）：
//!
//! - `deny`：由 `HookMiddleware` 当场拒绝，不进入审批；
//! - `allow`：写入本通道，`HumanInTheLoopMiddleware` 在弹框前取走并跳过审批；
//! - `ask` / `passthrough`：不写入，保持弹框行为。
//!
//! 两个中间件在链上的先后顺序固定（Hook 在 HITL 之前），但彼此不持有对方引用，
//! 因此用「按 `tool_call_id` 传递放行标记」的共享结构解耦。

use std::collections::HashSet;
use std::sync::Arc;

use parking_lot::Mutex;

/// 按 `tool_call_id` 传递「hook 已放行」标记的共享通道。
///
/// 语义：出现在集合里的调用表示权限层已由 hook 裁决为 `allow`，
/// 审批中间件不得再弹框。取走即清除（`take`），避免同一 id 被重复消费。
#[derive(Debug, Default)]
pub struct HookPermissionBridge {
    allowed: Mutex<HashSet<String>>,
}

impl HookPermissionBridge {
    /// 创建可直接 `Arc` 共享的空通道。
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// 记录一次 hook 放行。
    pub fn allow(&self, tool_call_id: &str) {
        self.allowed.lock().insert(tool_call_id.to_string());
    }

    /// 取走放行标记；命中表示本次调用无需审批。
    pub fn take(&self, tool_call_id: &str) -> bool {
        self.allowed.lock().remove(tool_call_id)
    }

    /// 丢弃残留标记。工具执行结束（含被拒、非敏感工具不经过审批）后调用，
    /// 避免集合随会话无限增长。
    pub fn discard(&self, tool_call_id: &str) {
        self.allowed.lock().remove(tool_call_id);
    }
}

#[cfg(test)]
#[path = "permission_bridge_test.rs"]
mod tests;
