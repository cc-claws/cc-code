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

/// 剥离文本中 `<system-reminder>` 标记，返回清洗后的文本与「是否为系统提醒」。
///
/// 与「首尾各 replacen 一次」的旧做法不同，这里按位置判定，因此标签出现在中间
/// （如 `agent_submit` 把提醒拼在用户输入之后）同样能处理干净。
///
/// 两种形态的处理方式不同，取决于**开标签之前有没有用户正文**：
///
/// - 标签前有正文（拼接形态：`用户输入\n\n<system-reminder>后台通知</system-reminder>`）：
///   整段提醒连同正文一并切除——通知已由 `SystemNote` 单独呈现，留在用户气泡里
///   只会重复且嘈杂。
/// - 标签前无正文（整体包裹形态：`<system-reminder>compact 摘要</system-reminder>`）：
///   只剥标记、保留提醒正文——摘要本身就是用户需要看到的对话内容。
///
/// 未闭合的开标签（流式渲染中途只收到前半段）视为提醒正文：标签前有正文时整体
/// 切除，否则保留已到达的部分。
///
/// 返回值第二项表示原文是否含有开标签，供调用方判断是否为系统提醒消息。
pub fn strip_system_reminder(raw: &str) -> (String, bool) {
    let Some(first) = raw.find(SYSTEM_REMINDER_OPEN) else {
        return (raw.to_string(), false);
    };
    // 标签前是否有用户正文：决定提醒正文是「切除」还是「保留」。
    let has_leading_text = !raw[..first].trim().is_empty();

    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(start) = rest.find(SYSTEM_REMINDER_OPEN) {
        out.push_str(&rest[..start]);
        let after_open = &rest[start + SYSTEM_REMINDER_OPEN.len()..];
        match after_open.find(SYSTEM_REMINDER_CLOSE) {
            Some(end) => {
                // 整体包裹形态保留正文，拼接形态丢弃。
                if !has_leading_text {
                    out.push_str(after_open[..end].trim());
                }
                rest = &after_open[end + SYSTEM_REMINDER_CLOSE.len()..];
            }
            None => {
                // 未闭合：剩余内容全属提醒正文。
                if !has_leading_text {
                    out.push_str(after_open.trim());
                }
                rest = "";
                break;
            }
        }
    }
    out.push_str(rest);
    (out.trim().to_string(), true)
}
