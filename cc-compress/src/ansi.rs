//! ANSI 转义剥离与终端覆盖输出规范化。

use regex::Regex;
use std::sync::LazyLock;

static ANSI_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\x1b(?:[@-Z\\-_]|\[[0-?]*[ -/]*[@-~])").expect("valid ANSI regex")
});

/// 剥离终端 ANSI 转义码（颜色、光标控制等），并规范化回车符。
///
/// 带回车符的行是终端「原地重绘」（进度条、spinner）——只有最后一段是终态，
/// 前面的都是被覆盖的中间帧，全部丢弃。
pub fn strip_ansi(input: &str) -> String {
    let stripped = ANSI_REGEX.replace_all(input, "");
    // 处理带 \r 的终端覆盖输出（如进度条）：保留最后一段
    let mut normalized_lines = Vec::new();
    for line in stripped.lines() {
        if line.contains('\r') {
            if let Some(last) = line.split('\r').rfind(|s| !s.is_empty()) {
                normalized_lines.push(last);
            }
        } else {
            normalized_lines.push(line);
        }
    }
    normalized_lines.join("\n")
}
