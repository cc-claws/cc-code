use super::*;
use std::thread;

// 权限模式只剩两档：AutoMode（默认）↔ Bypass

#[test]
fn test_next_cycle() {
    assert_eq!(PermissionMode::AutoMode.next(), PermissionMode::Bypass);
    assert_eq!(PermissionMode::Bypass.next(), PermissionMode::AutoMode);
}

#[test]
fn test_default_is_auto() {
    assert_eq!(PermissionMode::default(), PermissionMode::AutoMode);
}

#[test]
fn test_display_name() {
    assert_eq!(PermissionMode::AutoMode.display_name(), "Auto");
    assert_eq!(PermissionMode::Bypass.display_name(), "Bypass");
}

#[test]
fn test_from_u8_valid() {
    assert_eq!(PermissionMode::from(0u8), PermissionMode::AutoMode);
    assert_eq!(PermissionMode::from(1u8), PermissionMode::Bypass);
}

#[test]
fn test_from_u8_invalid_falls_back_to_auto() {
    // 关键安全属性：未知/陈旧的值一律回退 Auto（默认档），
    // **绝不能**因为一个异常值意外滑进 Bypass（全放行）。
    for v in [2u8, 3, 4, 5, 255] {
        assert_eq!(
            PermissionMode::from(v),
            PermissionMode::AutoMode,
            "u8={v} 应回退 Auto"
        );
    }
}

#[test]
fn test_shared_new_and_load() {
    let shared = SharedPermissionMode::new(PermissionMode::Bypass);
    assert_eq!(shared.load(), PermissionMode::Bypass);
}

#[test]
fn test_shared_store_and_load() {
    let shared = SharedPermissionMode::new(PermissionMode::AutoMode);
    shared.store(PermissionMode::Bypass);
    assert_eq!(shared.load(), PermissionMode::Bypass);
}

#[test]
fn test_shared_cycle_single_thread() {
    let shared = SharedPermissionMode::new(PermissionMode::AutoMode);
    assert_eq!(shared.cycle(), PermissionMode::Bypass);
    assert_eq!(shared.cycle(), PermissionMode::AutoMode);
    assert_eq!(shared.cycle(), PermissionMode::Bypass);
}

#[test]
fn test_shared_cycle_concurrent() {
    let shared = SharedPermissionMode::new(PermissionMode::AutoMode);
    let shared_clone = shared.clone();
    let barrier = Arc::new(std::sync::Barrier::new(4));

    let mut handles = vec![];
    for _ in 0..4 {
        let s = shared_clone.clone();
        let b = barrier.clone();
        handles.push(thread::spawn(move || {
            b.wait();
            for _ in 0..100 {
                s.cycle();
            }
        }));
    }

    for h in handles {
        h.join().unwrap();
    }

    // 最终状态必须是合法值（只剩两档）
    let final_mode = shared.load();
    assert!(matches!(
        final_mode,
        PermissionMode::AutoMode | PermissionMode::Bypass
    ));
}
