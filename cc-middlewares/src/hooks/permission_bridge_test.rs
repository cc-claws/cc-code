use super::HookPermissionBridge;

#[test]
fn test_hook_permission_bridge_take_consumes_allow_marker() {
    let bridge = HookPermissionBridge::new();
    bridge.allow("call-1");

    assert!(bridge.take("call-1"), "已放行的调用应命中");
    assert!(!bridge.take("call-1"), "取走后不得重复命中");
}

#[test]
fn test_hook_permission_bridge_take_returns_false_for_unknown_id() {
    let bridge = HookPermissionBridge::new();

    assert!(!bridge.take("call-x"));
}

#[test]
fn test_hook_permission_bridge_discard_removes_marker() {
    let bridge = HookPermissionBridge::new();
    bridge.allow("call-2");
    bridge.discard("call-2");

    assert!(!bridge.take("call-2"), "discard 后不应再命中");
}

#[test]
fn test_hook_permission_bridge_isolates_by_tool_call_id() {
    let bridge = HookPermissionBridge::new();
    bridge.allow("call-a");

    assert!(!bridge.take("call-b"));
    assert!(bridge.take("call-a"));
}
