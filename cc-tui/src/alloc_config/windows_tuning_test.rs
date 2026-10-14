use super::*;

#[test]
fn test_windows_mimalloc_suppresses_stderr_by_default() {
    init();
    let show_errors =
        unsafe { libmimalloc_sys::mi_option_get(libmimalloc_sys::mi_option_show_errors) };
    let verbose = unsafe { libmimalloc_sys::mi_option_get(libmimalloc_sys::mi_option_verbose) };
    assert_eq!(show_errors, 0, "默认应静音 mimalloc show_errors");
    assert_eq!(verbose, 0, "默认应静音 mimalloc verbose");
}

#[test]
fn test_windows_mimalloc_respects_env_override() {
    std::env::set_var("MIMALLOC_SHOW_ERRORS", "1");
    set_option(
        libmimalloc_sys::mi_option_show_errors,
        &["MIMALLOC_SHOW_ERRORS"],
        0,
    );
    let show_errors =
        unsafe { libmimalloc_sys::mi_option_get(libmimalloc_sys::mi_option_show_errors) };
    assert_eq!(show_errors, 1, "环境变量显式设置时应允许覆盖");

    // 清理并复位
    std::env::remove_var("MIMALLOC_SHOW_ERRORS");
    init();
    let show_errors_reset =
        unsafe { libmimalloc_sys::mi_option_get(libmimalloc_sys::mi_option_show_errors) };
    assert_eq!(show_errors_reset, 0, "复位后应重回静音态");
}
