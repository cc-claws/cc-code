//! token 估算与「过滤器不得让输出变大」守卫。

/// 以 `bytes / 4` 估算 token 数（不引入 tokenizer，只用于同文本间的相对比较）。
pub fn estimate_tokens(text: &str) -> usize {
    text.len().div_ceil(4)
}

/// 「过滤器不得让输出变大」守卫：过滤结果更费 token 时回退原文。
///
/// 小输出、短行场景下，过滤器追加的标记行（如 `[+N lines omitted]`）可能比
/// 省下的内容还多，导致**反向膨胀**。所有过滤器出口都应经过此守卫。
///
/// 估算值相等时保留过滤结果——此时无额外 token 成本，而过滤结果通常更规整。
pub fn never_worse(raw: &str, filtered: String) -> String {
    if estimate_tokens(&filtered) > estimate_tokens(raw) {
        raw.to_string()
    } else {
        filtered
    }
}
