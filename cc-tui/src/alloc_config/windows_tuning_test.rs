use super::*;

/// mimalloc 的 show_errors/verbose 是进程级全局选项，`init()`/`set_option()` 还会
/// 读进程环境变量。默认值与 env 覆盖必须放在同一个测试里顺序断言：拆成两个测试时
/// 它们在同一个测试二进制内并行执行，会互相污染（Windows CI 曾因此偶发失败，#545）。
#[test]
fn test_windows_mimalloc_stderr_defaults_and_env_override() {
    init();
    let show_errors =
        unsafe { libmimalloc_sys::mi_option_get(libmimalloc_sys::mi_option_show_errors) };
    let verbose = unsafe { libmimalloc_sys::mi_option_get(libmimalloc_sys::mi_option_verbose) };
    assert_eq!(show_errors, 0, "默认应静音 mimalloc show_errors");
    assert_eq!(verbose, 0, "默认应静音 mimalloc verbose");

    std::env::set_var("MIMALLOC_SHOW_ERRORS", "1");
    set_option(
        libmimalloc_sys::mi_option_show_errors,
        &["MIMALLOC_SHOW_ERRORS"],
        0,
    );
    let overridden =
        unsafe { libmimalloc_sys::mi_option_get(libmimalloc_sys::mi_option_show_errors) };
    assert_eq!(overridden, 1, "环境变量显式设置时应允许覆盖");

    std::env::remove_var("MIMALLOC_SHOW_ERRORS");
    init();
    let restored =
        unsafe { libmimalloc_sys::mi_option_get(libmimalloc_sys::mi_option_show_errors) };
    assert_eq!(restored, 0, "复位后应重回静音态");
}
