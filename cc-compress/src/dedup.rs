//! 相邻重复行的折叠。

/// 相邻重复行的折叠下限：连续出现达到此数才折叠。
const DEDUP_MIN_RUN: usize = 3;

/// 折叠**相邻且完全相同**的行：连续出现 ≥ [`DEDUP_MIN_RUN`] 次时保留一行并追加 `[xN]`。
///
/// 判据：
/// - 只折叠**相邻**的重复，不跨距离去重——日志里两处相同的 `warning:` 属不同事件；
/// - **空行豁免**：连续空行是排版而非噪音，折叠会破坏可读性；
/// - 连续次数不足下限时原样保留（2 次重复不值得引入标记）。
pub fn dedup_adjacent_lines(input: &str) -> String {
    let lines: Vec<&str> = input.lines().collect();
    let mut out: Vec<String> = Vec::with_capacity(lines.len());
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];
        // 空行豁免：成组原样输出
        if line.trim().is_empty() {
            let start = i;
            while i < lines.len() && lines[i].trim().is_empty() {
                i += 1;
            }
            out.extend(lines[start..i].iter().map(|s| (*s).to_string()));
            continue;
        }

        let mut run = 1;
        while i + run < lines.len() && lines[i + run] == line {
            run += 1;
        }

        if run >= DEDUP_MIN_RUN {
            out.push(format!("{line} [x{run}]"));
        } else {
            out.extend(std::iter::repeat_n(line.to_string(), run));
        }
        i += run;
    }

    out.join("\n")
}
