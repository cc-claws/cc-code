/// 从字符串生成短 hash（FNV-1a，6 位十六进制，确定性）。
///
/// 用于为每个 Agent 实例生成唯一的显示标识符。
pub fn instance_hash(s: &str) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in s.bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{:06x}", hash as u32)
}

/// 从后台任务结果字符串中解析 task_id 短格式（前 8 位）。
///
/// 输入格式: `"Background task bg-{uuid} started..."`
/// 输出: `Some("{前8位}")` 或 `None`（解析失败时优雅降级）
pub fn parse_bg_hash(result: &str) -> Option<String> {
    result
        .strip_prefix("Background task bg-")
        .and_then(|rest| rest.split(' ').next())
        .map(|uuid| uuid.chars().take(8).collect())
}

const SYSTEM_REMINDER_OPEN: &str = "<system-reminder>";
const SYSTEM_REMINDER_CLOSE: &str = "</system-reminder>";

/// 剥离文本中**所有** `<system-reminder>...</system-reminder>` 片段，返回清洗后的文本。
///
/// 与「首尾各 replacen 一次」的旧做法不同，这里按区间切除，因此标签出现在中间
/// （如 `agent_submit` 把提醒拼在用户输入之后）同样能剥干净。
///
/// 未闭合的开标签（流式渲染中途只收到前半段）会连同其后的内容一并剥离——此时
/// 后半段必然是提醒正文，保留它只会把内部标记漏到界面上。
///
/// 返回值第二项表示原文是否含有开标签，供调用方判断是否为系统提醒消息。
pub fn strip_system_reminder(raw: &str) -> (String, bool) {
    if !raw.contains(SYSTEM_REMINDER_OPEN) {
        return (raw.to_string(), false);
    }

    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(start) = rest.find(SYSTEM_REMINDER_OPEN) {
        out.push_str(&rest[..start]);
        let after_open = &rest[start + SYSTEM_REMINDER_OPEN.len()..];
        match after_open.find(SYSTEM_REMINDER_CLOSE) {
            Some(end) => rest = &after_open[end + SYSTEM_REMINDER_CLOSE.len()..],
            // 未闭合：剩余内容全属提醒正文，直接结束。
            None => {
                rest = "";
                break;
            }
        }
    }
    out.push_str(rest);
    (out.trim().to_string(), true)
}
