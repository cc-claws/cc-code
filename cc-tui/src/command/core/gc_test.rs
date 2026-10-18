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

/// 回归（#593）：差值不得用 `saturating_sub` 钳位。
///
/// 真实读数里 jemalloc resident（93.6 MB）可以大于内核 RSS（84.9 MB），此前
/// 被钳成 `0 B`，渲染出 `OS RSS(84.9 MB) - jemalloc resident(93.6 MB) = 0 B`
/// 这种算术不成立的等式；负差与「两边相等」必须可区分。
#[test]
fn test_signed_gap_keeps_negative() {
    assert_eq!(signed_gap(84_900_000, 93_600_000), -8_700_000);
    assert_eq!(signed_gap(93_600_000, 93_600_000), 0);
    assert_eq!(signed_gap(100, 40), 60);
}
