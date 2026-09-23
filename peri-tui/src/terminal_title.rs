//! 终端标题（Status Surface）模块
//!
//! 对齐 OpenAI Codex CLI (`codex-rs`) 规范，将终端标题设计为与底部状态栏平级的状态表面抽象。
//! 包含多段动态信息拼接、生命周期状态机、会话主题提炼与底层 I/O 安全去重机制。

use unicode_segmentation::UnicodeSegmentation;

/// 终端标题最大字符数上限（对齐 Codex: MAX_TERMINAL_TITLE_CHARS = 240）
pub const MAX_TERMINAL_TITLE_CHARS: usize = 240;

/// 终端标题 Spinner 动画帧集合（对齐 Codex: 10 帧盲文点阵）
pub const TERMINAL_TITLE_SPINNER_FRAMES: [&str; 10] =
    ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalTitleStatusKind {
    /// 会话就绪，无活动任务
    Idle,
    /// 工具执行中 / 派发中
    Working,
    /// LLM 思考/流式推理中
    Thinking,
    /// 等待用户交互（HITL 审批、AskUser 问答等）
    ActionRequired,
    /// 任务刚刚完成响应
    Done,
}

/// 终端标题状态表面的组成项
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalTitleItem<'a> {
    /// 旋转动画帧（Working 状态下为 spinner 字符，如 "⠋"）
    pub activity: Option<&'a str>,
    /// 当前工程/工作区目录名
    pub project_name: &'a str,
    /// 当前会话主题短标题
    pub thread_title: Option<&'a str>,
    /// 生命周期状态
    pub status: TerminalTitleStatusKind,
}

impl<'a> TerminalTitleItem<'a> {
    pub fn new(
        status: TerminalTitleStatusKind,
        project_name: &'a str,
        thread_title: Option<&'a str>,
        activity: Option<&'a str>,
    ) -> Self {
        Self {
            activity,
            project_name,
            thread_title,
            status,
        }
    }

    /// 格式化终端标题：有会话主题时展示「主题 | 项目」；未命名时展示项目名
    pub fn format_title(&self) -> String {
        let thread_truncated = self
            .thread_title
            .map(|t| t.trim())
            .filter(|t| !t.is_empty())
            .map(|t| truncate_terminal_title_part(t, 48));

        let project = self.project_name.trim();
        let project_str = if project.is_empty() {
            "cc-code"
        } else {
            project
        };
        let project_part = truncate_terminal_title_part(project_str, 24);

        let main_title = match thread_truncated {
            Some(title) => format!("{title} | {project_part}"),
            None => project_part,
        };

        match self.status {
            TerminalTitleStatusKind::Idle => main_title,
            TerminalTitleStatusKind::Done => format!("✴ {main_title}"),
            TerminalTitleStatusKind::Working | TerminalTitleStatusKind::Thinking => {
                let spinner = self.activity.unwrap_or("⠋");
                format!("{spinner} {main_title}")
            }
            TerminalTitleStatusKind::ActionRequired => {
                let prefix = self.activity.unwrap_or("[ ! ] Action Required");
                format!("{prefix}  {main_title}")
            }
        }
    }
}

/// 按照 Grapheme 字形簇截断字符串并在超出时闭合以 `...`（对齐 Codex: truncate_terminal_title_part）
pub fn truncate_terminal_title_part(value: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    let mut graphemes = value.graphemes(true);
    let head: String = graphemes.by_ref().take(max_chars).collect();
    if graphemes.next().is_none() || max_chars <= 3 {
        return head;
    }
    let mut truncated = head.graphemes(true).take(max_chars - 3).collect::<String>();
    truncated.push_str("...");
    truncated
}

/// 检查字符是否属于被禁用的控制字符或 Trojan Source / Bidi 隐形字符（对齐 Codex: is_disallowed_terminal_title_char）
fn is_disallowed_terminal_title_char(ch: char) -> bool {
    if ch.is_control() {
        return true;
    }
    matches!(
        ch,
        '\u{00AD}'
            | '\u{034F}'
            | '\u{061C}'
            | '\u{180E}'
            | '\u{200B}'..='\u{200F}'
            | '\u{202A}'..='\u{202E}'
            | '\u{2060}'..='\u{206F}'
            | '\u{FE00}'..='\u{FE0F}'
            | '\u{FEFF}'
            | '\u{FFF9}'..='\u{FFFB}'
            | '\u{1BCA0}'..='\u{1BCA3}'
            | '\u{E0100}'..='\u{E01EF}'
    )
}

/// 清洗终端标题字符串（对齐 Codex: sanitize_terminal_title）
///
/// 具备：空白符折叠（whitespace collapsing）、不可见/Bidi字符剔除、240字符硬上限截断
pub fn sanitize_title(title: &str) -> String {
    let mut sanitized = String::new();
    let mut chars_written = 0;
    let mut pending_space = false;
    for ch in title.chars() {
        if ch.is_whitespace() {
            // 遇到空白符时标记 pending_space（自动忽略首部空白）
            pending_space = !sanitized.is_empty();
            continue;
        }
        if is_disallowed_terminal_title_char(ch) {
            continue;
        }
        if pending_space {
            let remaining = MAX_TERMINAL_TITLE_CHARS.saturating_sub(chars_written);
            if remaining > 1 {
                sanitized.push(' ');
                chars_written += 1;
                pending_space = false;
            }
        }
        if chars_written >= MAX_TERMINAL_TITLE_CHARS {
            break;
        }
        sanitized.push(ch);
        chars_written += 1;
    }
    sanitized
}

/// 从首轮 Prompt 中轻量提炼会话主题（3~5个词或短语）
pub fn extract_thread_title(prompt: &str) -> Option<String> {
    let mut cleaned = prompt.trim();
    if cleaned.is_empty() {
        return None;
    }

    // 过滤 <system-reminder>...</system-reminder>
    let mut remaining = cleaned.to_string();
    while let Some(start) = remaining.find("<system-reminder>") {
        if let Some(end) = remaining[start..].find("</system-reminder>") {
            let full_end = start + end + "</system-reminder>".len();
            let before = remaining[..start].trim();
            let after = remaining[full_end..].trim();
            remaining = format!("{} {}", before, after);
        } else {
            break;
        }
    }
    cleaned = remaining.trim();

    if cleaned.is_empty() {
        return None;
    }

    // 过滤 Markdown 代码块 ```...``` 或 ~~~...~~~
    let mut no_code_blocks = String::new();
    let mut in_code_block = false;
    for line in cleaned.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_code_block = !in_code_block;
            continue;
        }
        if !in_code_block {
            no_code_blocks.push_str(line);
            no_code_blocks.push('\n');
        }
    }
    let effective_text = if !no_code_blocks.trim().is_empty() {
        no_code_blocks.trim()
    } else {
        cleaned
    };

    // 取首个有效非空行（跳过纯分割线）
    let mut first_line = "";
    for line in effective_text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("```")
            || trimmed.starts_with("~~~")
            || trimmed.starts_with("---")
            || trimmed.starts_with("===")
            || trimmed.starts_with("___")
        {
            continue;
        }
        if !trimmed.is_empty() {
            first_line = trimmed;
            break;
        }
    }
    if first_line.is_empty() {
        first_line = effective_text.lines().next().unwrap_or("").trim();
    }

    // 去除前导 markdown 符号（如 '#', '-', '*', '>', 数字标号）
    let mut s = first_line;
    while s.starts_with(['#', '*', '-', '>', ' ', '\t']) {
        s = s[1..].trim_start();
    }
    // 去除类似 "1. " 或 "1、" 的有序列表前缀
    if let Some((pos, ch)) = s
        .char_indices()
        .find(|(_, c)| *c == '.' || *c == '、' || *c == '．')
    {
        let prefix = &s[..pos];
        if !prefix.is_empty() && prefix.chars().all(|c| c.is_ascii_digit()) {
            s = s[pos + ch.len_utf8()..].trim_start();
        }
    }

    // 去除 URL（如 https://... 或 http://...）
    let mut processed_line = s.to_string();
    if let Some(url_start) = processed_line
        .find("http://")
        .or_else(|| processed_line.find("https://"))
    {
        let url_end = processed_line[url_start..]
            .char_indices()
            .find(|(_, c)| c.is_whitespace() || *c == '，' || *c == '。' || *c == ',')
            .map(|(idx, _)| url_start + idx)
            .unwrap_or(processed_line.len());
        let url_part = processed_line[url_start..url_end].to_string();
        let before = processed_line[..url_start].trim();
        let after = processed_line[url_end..].trim();
        let combined = if before.is_empty() {
            after.to_string()
        } else if after.is_empty() {
            before.to_string()
        } else {
            format!("{before} {after}")
        };

        if !combined.is_empty() {
            processed_line = combined;
        } else {
            // 若整行仅有 URL，提取 URL 末尾的有意义 path segment
            if let Some(last_slash) = url_part.rfind('/') {
                let segment = &url_part[last_slash + 1..];
                if !segment.is_empty() {
                    let sanitized = sanitize_title(segment);
                    return if sanitized.is_empty() {
                        None
                    } else {
                        Some(sanitized)
                    };
                }
            }
        }
    }

    s = processed_line.trim();
    if s.is_empty() {
        return None;
    }

    // 检查是否包含 CJK 字符
    let has_cjk = s.chars().any(|c| ('\u{4e00}'..='\u{9fa5}').contains(&c));

    if has_cjk {
        let mut text = s;
        for prefix in &[
            "请帮我",
            "请问",
            "帮我",
            "请",
            "想问下",
            "我想",
            "麻烦帮我",
            "麻烦",
        ] {
            if let Some(stripped) = text.strip_prefix(prefix) {
                text = stripped.trim_start();
            }
        }
        // 按常见句子截断标点（逗号、句号、分号、问号、感叹号）
        let end_idx = text
            .char_indices()
            .find(|(_, c)| {
                matches!(
                    c,
                    '，' | '。' | '！' | '？' | ',' | '.' | '!' | '?' | ';' | '；'
                )
            })
            .map(|(idx, _)| idx)
            .unwrap_or(text.len());
        let phrase = text[..end_idx].trim();
        let candidate = if phrase.is_empty() { text } else { phrase };

        // 字符级截取最多 20 个字符
        let truncated: String = candidate.chars().take(20).collect();
        let sanitized = sanitize_title(&truncated);
        if sanitized.is_empty() {
            None
        } else {
            Some(sanitized)
        }
    } else {
        // 纯英文处理：按词分词
        let mut words: Vec<&str> = s.split_whitespace().collect();
        // 过滤常见提示前缀如 "please", "can you", "help me"
        if let Some(first) = words.first() {
            let lower = first.to_lowercase();
            if matches!(lower.as_str(), "please" | "can" | "could" | "help" | "i") {
                words.remove(0);
                if let Some(second) = words.first() {
                    let lower2 = second.to_lowercase();
                    if matches!(lower2.as_str(), "you" | "me") {
                        words.remove(0);
                    }
                }
            }
        }
        if words.is_empty() {
            return None;
        }
        let take_count = words.len().min(5);
        let phrase = words[..take_count].join(" ");
        let cleaned_phrase = phrase.trim_end_matches(['.', '?', '!', ',', ';']);
        let truncated: String = cleaned_phrase.chars().take(35).collect();
        let sanitized = sanitize_title(&truncated);
        if sanitized.is_empty() {
            None
        } else {
            Some(sanitized)
        }
    }
}

/// 清空/还原终端标题（TUI 退出时调用）
pub fn clear_terminal_title() {
    let _ = ratatui::crossterm::execute!(
        std::io::stdout(),
        ratatui::crossterm::terminal::SetTitle("")
    );
}

/// 异步调用 LLM 从首轮 Prompt 中智能概括提炼 3~5 个词的精准会话主题（对齐 Codex: ThreadMetadataGenerationService）
pub async fn generate_thread_title_llm(
    provider: peri_acp::provider::LlmProvider,
    prompt: &str,
) -> Option<String> {
    let prompt_trimmed = prompt.trim();
    if prompt_trimmed.is_empty() {
        return None;
    }

    // 截取前 500 个字符送给大模型，避免大文本输入拖慢标题生成
    let truncated_input: String = prompt_trimmed.chars().take(500).collect();

    let model = provider.into_model();
    let system_instruction = "你是一个会话主题生成工具。请根据用户首轮输入，概括提炼出最精准、简明的话题短标题（3~6个中文汉字或3~5个英文单词）。\
严格要求：只输出标题本身，严禁任何标点符号、引号、冒号、换行或解释性文字。";

    let request =
        peri_agent::llm::types::LlmRequest::new(vec![peri_agent::messages::BaseMessage::human(
            format!("用户任务输入：\n{truncated_input}"),
        )])
        .with_system(system_instruction)
        .with_max_tokens(30);

    // 8 秒硬超时，超时或失败自动返回 None（回退本地保底标题）
    let result =
        tokio::time::timeout(std::time::Duration::from_secs(8), model.invoke(request)).await;

    match result {
        Ok(Ok(response)) => {
            let raw = response.message.content();
            let mut cleaned = raw.trim();
            // 剥离大模型可能输出的包裹符号
            cleaned =
                cleaned.trim_matches(['"', '\'', '`', '“', '”', '《', '》', '：', ':', '.', '。']);
            let sanitized = sanitize_title(cleaned);
            if sanitized.is_empty() {
                None
            } else {
                Some(truncate_terminal_title_part(&sanitized, 48))
            }
        }
        Ok(Err(e)) => {
            tracing::debug!(error = %e, "LLM thread title generation failed");
            None
        }
        Err(_) => {
            tracing::debug!("LLM thread title generation timed out, using fallback");
            None
        }
    }
}

#[cfg(test)]
#[path = "terminal_title_test.rs"]
mod terminal_title_test;
