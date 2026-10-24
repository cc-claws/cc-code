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

/// size class 表的占比列：分母为 0（allocated 读不到）时必须显示 `—`，
/// 不能把「没读到总量」渲染成 `0.0%`。
#[test]
fn test_fmt_share_handles_zero_denominator() {
    assert_eq!(fmt_share(0, 0), "—");
    assert_eq!(fmt_share(1024, 0), "—");
    assert_eq!(fmt_share(0, 1024), "0.0%");
    assert_eq!(fmt_share(1024, 1024), "100.0%");
    assert_eq!(fmt_share(1, 8), "12.5%");
    // 与真实 /gc 读数同量级：640 B × 5562 = 3,559,680 / 18,968,656
    assert_eq!(fmt_share(3_559_680, 18_968_656), "18.8%");
}

/// size class 表的尺寸列必须精确。
///
/// `fmt_bytes` 的精度不足以区分相邻 size class（1024 B 与 1280 B 只差一档取整），
/// 同一张表里出现两行看不出区别的尺寸就无法定位「哪一档吃了多少」。
#[test]
fn test_fmt_bytes_exact_keeps_size_class_distinct() {
    assert_eq!(
        fmt_bytes(1024),
        fmt_bytes(1280),
        "前提：fmt_bytes 会把这两个相邻 size class 渲染成同一个字符串"
    );
    assert_eq!(fmt_bytes_exact(1024), "1,024 B");
    assert_eq!(fmt_bytes_exact(1280), "1,280 B");
    assert_eq!(fmt_bytes_exact(786_432), "786,432 B");
}

#[test]
fn test_fmt_grouped_thousands() {
    assert_eq!(fmt_grouped(0), "0");
    assert_eq!(fmt_grouped(999), "999");
    assert_eq!(fmt_grouped(1_000), "1,000");
    assert_eq!(fmt_grouped(29_230), "29,230");
    assert_eq!(fmt_grouped(1_048_576), "1,048,576");
}
