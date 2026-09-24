use super::*;

#[test]
fn test_conpty_host_legacy_and_unknown_fail_closed() {
    assert!(!compatible_host("conhost.exe", 10, 0, 19041));
    assert!(!compatible_host("other.exe", 1, 24, 0));
    assert!(!compatible_host("OpenConsole.exe", 1, 0, 0));
    assert!(
        !compatible_host("conhost.exe", 10, 0, 22621),
        "未实测的系统宿主不能仅凭 OS 版本放行"
    );
}

#[test]
fn test_conpty_host_modern_openconsole() {
    assert!(compatible_host("OpenConsole.exe", 1, 24, 2607));
    assert!(compatible_host("OPENCONSOLE.EXE", 1, 24, 2607));
}
