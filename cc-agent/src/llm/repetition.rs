/// 流式重复检测器
///
/// 在 LLM 流式输出过程中检测退化重复（degenerate output）。
/// LLM 有时会在 thinking/reasoning 中重复同一句话数十次，
/// 检测到后可提前终止流，避免浪费 token 和污染历史。
///
/// # 检测算法
///
/// 按句号/换行/感叹号/问号分割文本为句子片段，如果连续 N 个片段**完全相同**，
/// 则判定为退化重复。正常 LLM 输出不可能出现连续 3 个一模一样的句子，
/// 因此不会误伤。
///
/// 流式重复检测器
pub struct RepetitionDetector {
    /// 上次检测时的文本长度
    last_check_len: usize,
    /// 检测间隔（字符数），每增长此数量执行一次检测
    check_interval: usize,
    /// 连续重复次数阈值
    repeat_threshold: usize,
    /// 最小检测长度（低于此长度不检测）
    min_length: usize,
}

impl RepetitionDetector {
    /// 创建默认检测器
    ///
    /// - 检测间隔：500 字符
    /// - 连续重复阈值：10 次
    /// - 最小检测长度：200 字符
    pub fn new() -> Self {
        Self {
            last_check_len: 0,
            check_interval: 500,
            repeat_threshold: 10,
            min_length: 200,
        }
    }

    /// 检查累积文本是否出现退化重复
    ///
    /// 首次达到 `min_length` 时检测一次，之后每增长 `check_interval` 字符检测一次。
    /// 返回 `true` 表示检测到退化重复，建议终止流。
    pub fn check(&mut self, accumulated: &str) -> bool {
        let len = accumulated.len();
        if len < self.min_length {
            return false;
        }
        // 首次检测：达到 min_length 即触发
        // 后续检测：每增长 check_interval 触发一次
        if self.last_check_len > 0 && len < self.last_check_len + self.check_interval {
            return false;
        }
        self.last_check_len = len;

        Self::is_degenerate(accumulated, self.repeat_threshold)
    }

    /// 按句子边界分割，检测连续完全相同的片段
    fn is_degenerate(text: &str, threshold: usize) -> bool {
        let parts: Vec<&str> = text
            .split_inclusive(['.', '\n', '!', '?'])
            .map(|s| s.trim())
            .filter(|s| s.len() >= 20)
            .collect();

        if parts.len() < threshold {
            return false;
        }

        let mut consecutive = 1;
        for i in 1..parts.len() {
            if parts[i] == parts[i - 1] {
                consecutive += 1;
                if consecutive >= threshold {
                    return true;
                }
            } else {
                consecutive = 1;
            }
        }
        false
    }
}

impl Default for RepetitionDetector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "repetition_test.rs"]
mod tests;
