use super::*;

/// 回归：RSS 变化的方向符号不得颠倒。
///
/// 此前 gc 汇总用 `before - after` 计算 delta 且仅在 `>= 0` 时加 `+`，
/// 导致内存**下降**（197.9 → 196.6）被显示为 `+1.3 MB`。
#[test]
fn test_signed_delta_byte_direction() {
    const MB: isize = 1024 * 1024;
    // 下降 1.3 MB：after - before = -1.3MB → "-1.3 MB"
    assert_eq!(
        fmt_signed_delta(-13 * MB / 10, fmt_bytes),
        "-1.3 MB",
        "内存下降应显示负号"
    );
    // 上升 1.3 MB → "+1.3 MB"
    assert_eq!(fmt_signed_delta(13 * MB / 10, fmt_bytes), "+1.3 MB");
    // 无变化 → "±0 B"
    assert_eq!(fmt_signed_delta(0, fmt_bytes), "±0 B");
}

/// 回归：OS RSS（MB 整数）走同一符号约定。
#[test]
fn test_signed_delta_megabyte_direction() {
    assert_eq!(fmt_signed_delta(-1, fmt_mb_from_usize), "-1 MB");
    assert_eq!(fmt_signed_delta(3, fmt_mb_from_usize), "+3 MB");
    assert_eq!(fmt_signed_delta(0, fmt_mb_from_usize), "±0 MB");
}
