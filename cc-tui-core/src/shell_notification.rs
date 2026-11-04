/// 后台任务通知的展示形态：一句人读文案 + 状态强调色。
///
/// 文案与配色对齐参考实现的 `UserAgentNotificationMessage`：
/// 只有一句 summary 上屏，状态只体现在颜色与句中措辞上。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellNotificationDisplay {
    pub text: String,
    pub tone: crate::message_view::NoteTone,
}

/// 将后台 shell 控制通知转换为 TUI 可读提示。
pub fn shell_notification_display_text(raw: &str) -> Option<ShellNotificationDisplay> {
    let lc = crate::i18n::global();
    shell_notification_display_text_with(raw, &lc)
}

/// 与 [`shell_notification_display_text`] 相同，但显式传入语言注册表（便于测试与显式上下文）。
pub fn shell_notification_display_text_with(
    raw: &str,
    lc: &crate::i18n::LcRegistry,
) -> Option<ShellNotificationDisplay> {
    let notification = unwrap_system_reminder(raw.trim());
    // 合并注入后单条消息可能包含多个通知块（甚至混合类型），按文档顺序逐块解析。
    const TAGS: [&str; 2] = [
        "background-task-completed",
        "background-task-waiting-for-input",
    ];
    let mut items: Vec<ShellNotificationDisplay> = Vec::new();
    let mut cursor = notification;
    while let Some((tag, body_start, block_end)) = next_notification_block(cursor, &TAGS) {
        if let Some(item) = render_notification_block(tag, &cursor[body_start..block_end], lc) {
            items.push(item);
        }
        cursor = &cursor[block_end + format!("</{tag}>").len()..];
    }

    if items.is_empty() {
        return None;
    }
    // 多块合并时取最严重的语义色（错误 > 警告 > 成功），文案按行拼接。
    let tone = items
        .iter()
        .map(|item| item.tone)
        .min_by_key(|tone| tone.severity())
        .unwrap_or(crate::message_view::NoteTone::Success);
    Some(ShellNotificationDisplay {
        text: items
            .into_iter()
            .map(|item| item.text)
            .collect::<Vec<_>>()
            .join("\n"),
        tone,
    })
}

/// 在 `cursor` 中定位最早出现的通知块，返回 `(tag, body_start, body_end)`。
///
/// `body_start`/`body_end` 为开标签之后、闭标签之前的正文区间（相对 `cursor`）。
pub fn next_notification_block<'a>(cursor: &str, tags: &[&'a str]) -> Option<(&'a str, usize, usize)> {
    tags.iter()
        .filter_map(|tag| {
            let open = format!("<{tag}>");
            let start = cursor.find(&open)?;
            let body_start = start + open.len();
            let close = format!("</{tag}>");
            let body_end = cursor[body_start..].find(&close)? + body_start;
            Some((*tag, body_start, body_end))
        })
        .min_by_key(|(_, body_start, _)| *body_start)
}

/// 把单个完成/等待输入通知块渲染为一行人类可读文本。
pub fn render_notification_block(
    tag: &str,
    block: &str,
    lc: &crate::i18n::LcRegistry,
) -> Option<ShellNotificationDisplay> {
    let subject = notification_subject(block);
    match tag {
        "background-task-completed" => {
            let status = extract_xml_tag(block, "status")
                .map(xml_unescape)
                .map(|s| super::tool_display::sanitize_display_text(&s))
                .unwrap_or_else(|| "completed".to_string());
            Some(notification_for_status(&status, &subject, lc))
        }
        "background-task-waiting-for-input" => Some(ShellNotificationDisplay {
            text: lc.tr_args(
                "shell-notify-waiting-input",
                &[("subject".to_string(), subject.into())],
            ),
            // 参考实现的 stall 通知没有 `<status>`，渲染时落到默认前景色。
            tone: crate::message_view::NoteTone::Neutral,
        }),
        _ => None,
    }
}

/// 把 `completed (exit 0)` 这类状态映射成与参考实现一致的整句文案 + 强调色。
///
/// 文案形态对齐 `UserAgentNotificationMessage` 的 summary：
/// `Background command "<subject>" completed (exit code 0)`。
pub fn notification_for_status(
    status: &str,
    subject: &str,
    lc: &crate::i18n::LcRegistry,
) -> ShellNotificationDisplay {
    use crate::message_view::NoteTone;
    let detail = status_detail(status);
    let exit_code = detail.and_then(parse_exit_code);
    let subject_arg = || ("subject".to_string(), subject.to_string().into());
    // 负退出码（channel 异常兜底）与 128+n（被信号终止）在参考实现里属于
    // killed → `was stopped`，不应渲染成红字失败。
    if matches!(exit_code, Some(code) if code < 0 || (128..=165).contains(&code)) {
        return ShellNotificationDisplay {
            text: lc.tr_args("shell-notify-stopped", &[subject_arg()]),
            tone: NoteTone::Warning,
        };
    }
    if status.starts_with("failed") {
        let text = match exit_code {
            Some(code) => lc.tr_args(
                "shell-notify-failed",
                &[subject_arg(), ("code".to_string(), code.to_string().into())],
            ),
            None => lc.tr_args(
                "shell-notify-failed-reason",
                &[
                    subject_arg(),
                    (
                        "reason".to_string(),
                        detail.unwrap_or("failed").to_string().into(),
                    ),
                ],
            ),
        };
        ShellNotificationDisplay {
            text,
            tone: NoteTone::Error,
        }
    } else if status.starts_with("timed out") {
        ShellNotificationDisplay {
            text: lc.tr_args("shell-notify-timed-out", &[subject_arg()]),
            tone: NoteTone::Warning,
        }
    } else if status.starts_with("cancelled") || status == "terminated" {
        ShellNotificationDisplay {
            text: lc.tr_args("shell-notify-stopped", &[subject_arg()]),
            tone: NoteTone::Warning,
        }
    } else {
        let text = match exit_code {
            Some(code) => lc.tr_args(
                "shell-notify-completed",
                &[subject_arg(), ("code".to_string(), code.to_string().into())],
            ),
            None => lc.tr_args("shell-notify-completed-plain", &[subject_arg()]),
        };
        ShellNotificationDisplay {
            text,
            tone: NoteTone::Success,
        }
    }
}

/// 从 `exit 0` 这类细节里解析退出码；其它细节（超时原因、错误消息）返回 `None`。
pub fn parse_exit_code(detail: &str) -> Option<i32> {
    detail.strip_prefix("exit ")?.trim().parse().ok()
}

/// 通知行里「任务标识」的最大显示列宽。
///
/// 按显示列宽而非字符数截断：CJK 每个字占 2 列，按字符数截会超宽；
/// [`crate::text_width::truncate_to_display_width`] 自带 `…` 收口。
pub const NOTIFICATION_SUBJECT_MAX_WIDTH: usize = 60;

/// 通知的人读标识：模型写的 `<description>` 优先，缺失时回退 `<command>`。
///
/// 命令文本对读者没有语义（`cd … && export …` 之类），只作为最后手段；
/// 两者都缺失时用兜底文案，保证通知行不会空着。
pub fn notification_subject(block: &str) -> String {
    let read = |tag: &str| {
        extract_xml_tag(block, tag)
            .map(xml_unescape)
            .map(|s| super::tool_display::sanitize_display_text(&s))
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    };
    let text = read("description")
        .or_else(|| read("command"))
        .unwrap_or_else(|| "shell command".to_string());
    crate::text_width::truncate_to_display_width(&text, NOTIFICATION_SUBJECT_MAX_WIDTH)
}

/// 从 `completed (exit 0)` 这类状态里取括号内的细节。
///
/// 动词前缀（`Background shell completed`）已经表达状态，这里只把括号里的
/// exit code / 错误原因拿出来，避免双层括号。
pub fn status_detail(status: &str) -> Option<&str> {
    let open = status.find('(')?;
    let close = status.rfind(')')?;
    (close > open)
        .then(|| status[open + 1..close].trim())
        .filter(|detail| !detail.is_empty())
}

pub fn unwrap_system_reminder(raw: &str) -> &str {
    raw.strip_prefix("<system-reminder>")
        .and_then(|s| s.strip_suffix("</system-reminder>"))
        .map(str::trim)
        .unwrap_or(raw)
}

pub fn extract_xml_tag<'a>(raw: &'a str, tag: &str) -> Option<&'a str> {
    let start_tag = format!("<{}>", tag);
    let end_tag = format!("</{}>", tag);
    let start = raw.find(&start_tag)? + start_tag.len();
    let end = raw[start..].find(&end_tag)? + start;
    Some(&raw[start..end])
}

/// XML 转义（command/last_output 可能含 `<` `>` `&`，如 shell 重定向、错误信息）。
///
/// `pub(crate)` 供 `shell_command.rs` 构造 `!` 命令上下文片段时复用。
pub fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

pub fn xml_unescape(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}
