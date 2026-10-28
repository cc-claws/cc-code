use cc_middlewares::hitl::BatchItem;
use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::{
    app::{
        tool_display::{sanitize_display_text, sanitize_display_text_preserving_newlines},
        App, ApprovalChoice, InteractionPrompt,
    },
    i18n::LcRegistry,
    ui::{message_render::truncate_to_display_width, theme},
};

pub(super) struct ApprovalContent {
    /// 顶部标题：工具名（对齐参考实现的 `Bash command` / `Edit file`）。
    pub title: String,
    /// 副标题（对齐参考实现：Edit/Write 显示文件路径，Skill 显示警告语，Bash 无）。
    pub subtitle: Option<String>,
    /// 提问句（对齐参考实现的 `Do you want to proceed?`）。
    pub question: String,
    /// 审批原因说明（对齐参考实现的 decisionReason 区）。空表示不渲染。
    pub notice: Vec<String>,
    pub sections: Vec<Vec<Line<'static>>>,
    pub summary: Vec<Line<'static>>,
    pub choices: Vec<Line<'static>>,
    pub hints: Vec<Line<'static>>,
}

/// 工具名 → 参考实现的标题文案。
fn tool_title(item: &BatchItem, lc: &LcRegistry) -> String {
    match item.tool_name.as_str() {
        "Bash" => lc.tr("hitl-title-bash"),
        "Edit" => lc.tr("hitl-title-edit"),
        "Write" => lc.tr("hitl-title-write"),
        "Read" => lc.tr("hitl-title-read"),
        "Grep" | "Glob" => lc.tr("hitl-title-search"),
        "Skill" => {
            // 对齐参考实现：`Use skill "<name>"?`
            let name = item
                .input
                .get("skill")
                .or_else(|| item.input.get("name"))
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default();
            if name.is_empty() {
                lc.tr("hitl-title-skill")
            } else {
                lc.tr_args("hitl-title-skill-named", &[("name".into(), name.into())])
            }
        }
        // MCP 工具名形如 `mcp__server__tool`，标题用 `Call <tool>`
        other if other.starts_with("mcp__") => {
            let short = other.rsplit("__").next().unwrap_or(other);
            lc.tr_args("hitl-title-mcp", &[("tool".into(), short.into())])
        }
        other => other.to_string(),
    }
}

/// 工具名 → 副标题（文件路径）。Bash/Skill 等无副标题。
fn tool_subtitle(item: &BatchItem) -> Option<String> {
    item.input
        .get("file_path")
        .or_else(|| item.input.get("notebook_path"))
        .and_then(serde_json::Value::as_str)
        .map(sanitize_display_text)
}

/// 工具名 → 提问句。
fn tool_question(item: &BatchItem, lc: &LcRegistry) -> String {
    let file = item
        .input
        .get("file_path")
        .or_else(|| item.input.get("path"))
        .and_then(serde_json::Value::as_str)
        .and_then(|path| std::path::Path::new(path).file_name())
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    match item.tool_name.as_str() {
        "Edit" if !file.is_empty() => lc.tr_args(
            "hitl-question-edit",
            &[("file".into(), file.to_string().into())],
        ),
        "Write" if !file.is_empty() => lc.tr_args(
            "hitl-question-write",
            &[("file".into(), file.to_string().into())],
        ),
        _ => lc.tr("hitl-question-default"),
    }
}

pub(super) fn build_content(app: &App, width: usize) -> Option<ApprovalContent> {
    let Some(InteractionPrompt::Approval(prompt)) =
        &app.session_mgr.current().agent.interaction_prompt
    else {
        return None;
    };
    let lc = &app.services.lc;
    let cursor = prompt.cursor.min(prompt.items.len().saturating_sub(1));
    let sections = prompt
        .items
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let mut rows = Vec::new();
            // 批量场景：非当前项只输出一行摘要（`<序号> <工具> <路径>`），
            // 让用户知道后面还有哪些待审；当前项展开完整内容。
            if prompt.items.len() > 1 && index != cursor {
                let target = item
                    .input
                    .get("file_path")
                    .or_else(|| item.input.get("path"))
                    .or_else(|| item.input.get("command"))
                    .and_then(serde_json::Value::as_str)
                    .map(sanitize_display_text)
                    .unwrap_or_default();
                let mut head = format!(
                    "{} {}",
                    sanitize_display_text(&item.tool_name),
                    truncate_to_display_width(&target, width.saturating_sub(12))
                );
                // 附带非路径的简单参数（如 replace_all: true），便于批量时辨认
                if let Some(map) = item.input.as_object() {
                    for (key, value) in map {
                        if matches!(
                            key.as_str(),
                            "file_path" | "path" | "command" | "old_string" | "new_string"
                        ) {
                            continue;
                        }
                        let rendered = value
                            .as_str()
                            .map(str::to_owned)
                            .unwrap_or_else(|| value.to_string());
                        head.push_str(&format!(" · {key}: {rendered}"));
                    }
                }
                rows.push(Line::styled(
                    head,
                    Style::default().fg(theme::cur().inactive),
                ));
                return rows;
            }
            // 当前项：标题行已由 popup 层统一绘制，这里只输出工具参数内容。
            rows.extend(input_rows(item, width, index == cursor));
            rows
        })
        .collect();
    let summary = if prompt.items.len() > 1 {
        let tool = prompt
            .items
            .get(cursor)
            .map(|item| sanitize_display_text(&item.tool_name))
            .unwrap_or_default();
        let target = lc.tr_args(
            "hitl-selection-target",
            &[
                ("current".into(), ((cursor + 1) as i64).into()),
                ("count".into(), (prompt.items.len() as i64).into()),
                ("tool".into(), tool.into()),
            ],
        );
        let count = |choice| {
            prompt
                .choices
                .iter()
                .filter(|value| **value == choice)
                .count() as i64
        };
        let counts = lc.tr_args(
            "hitl-summary-three",
            &[
                ("once".into(), count(ApprovalChoice::Once).into()),
                ("session".into(), count(ApprovalChoice::Session).into()),
                ("rejected".into(), count(ApprovalChoice::Reject).into()),
            ],
        );
        let combined = format!("{target} · {counts}");
        let rows = if combined.width() <= width {
            vec![combined]
        } else {
            let mut rows = wrap_words(&target, width);
            rows.extend(wrap_words(&counts, width));
            rows
        };
        rows.into_iter()
            .map(|row| {
                // 缩进 1 格，与标题/提问句/选项对齐
                Line::styled(
                    format!(
                        " {}",
                        truncate_to_display_width(&row, width.saturating_sub(1))
                    ),
                    Style::default().fg(theme::cur().inactive),
                )
            })
            .collect()
    } else {
        Vec::new()
    };
    let choices = prompt
        .items
        .get(cursor)
        .map(|item| {
            // 高危操作（offers_standing=false）不提供「本次会话不再询问」档，
            // 对齐参考实现 只留 Yes / No 两项。
            let rows = choice_rows(
                item,
                prompt
                    .choices
                    .get(cursor)
                    .copied()
                    .unwrap_or(ApprovalChoice::Once),
                width,
                lc,
            );
            if prompt.offers_standing {
                rows
            } else {
                // 命中高危：只留 同意 / 拒绝，并重新编号为 1. 2.
                let session_label = lc.tr("hitl-choice-session");
                rows.into_iter()
                    .filter(|line| {
                        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
                        !text.contains(&session_label)
                    })
                    .enumerate()
                    .map(|(index, line)| renumber_choice(line, index + 1))
                    .collect()
            }
        })
        .unwrap_or_default();
    let hints = wrap_words(&lc.tr("hitl-key-hint"), width)
        .into_iter()
        .map(|row| Line::styled(row, Style::default().fg(theme::cur().inactive)))
        .collect();
    let title = prompt
        .items
        .get(cursor)
        .map(|item| tool_title(item, lc))
        .unwrap_or_else(|| lc.tr("hitl-single-title"));
    let question = prompt
        .items
        .get(cursor)
        .map(|item| tool_question(item, lc))
        .unwrap_or_else(|| lc.tr("hitl-question-default"));
    // Skill 场景：副标题用「可能使用此技能中的说明/代码/文件」警告语（对齐 CC）。
    let subtitle = prompt.items.get(cursor).and_then(|item| {
        if item.tool_name == "Skill" {
            Some(lc.tr("hitl-skill-caveat"))
        } else {
            tool_subtitle(item)
        }
    });
    // 审批原因区（对齐 CC 的 decisionReason）：每行一条，末尾附 /permissions 提示。
    let notice = prompt.notice.clone();
    Some(ApprovalContent {
        title,
        subtitle,
        question,
        notice,
        sections,
        summary,
        choices,
        hints,
    })
}

fn choice_key(choice: ApprovalChoice) -> &'static str {
    match choice {
        ApprovalChoice::Once => "hitl-choice-once",
        ApprovalChoice::Session => "hitl-choice-session",
        ApprovalChoice::Reject => "hitl-choice-reject",
    }
}

/// 把选项行的序号改写为 `index`（用于过滤掉某项后重新编号）。
fn renumber_choice(line: Line<'static>, index: usize) -> Line<'static> {
    let want = format!("{index}. ");
    let spans = line
        .spans
        .into_iter()
        .map(|mut span| {
            let text = span.content.to_string();
            // 序号形如 `1. ` / `2. `，只替换形如 `<digit>. ` 的片段
            if let Some(pos) = text.find(". ") {
                let head = &text[..pos];
                if !head.is_empty() && head.chars().all(|c| c.is_ascii_digit()) {
                    span.content = format!("{want}{}", &text[pos + 2..]).into();
                }
            }
            span
        })
        .collect::<Vec<_>>();
    Line::from(spans)
}

/// 选项列表：对齐参考实现的 `❯ 1. Yes` / `  2. No` 形式。
///
/// - 焦点项前缀 `❯ ` + 序号，用 thinking 紫
/// - 非焦点项前缀 2 空格 + 序号，用 body 灰
/// - 序号单独用更暗的灰色（与参考实现一致）
fn choice_rows(
    _item: &BatchItem,
    selected: ApprovalChoice,
    width: usize,
    lc: &LcRegistry,
) -> Vec<Line<'static>> {
    let labels = ApprovalChoice::ALL.map(|choice| lc.tr(choice_key(choice)));
    let mut rows = Vec::new();
    for (index, choice) in ApprovalChoice::ALL.into_iter().enumerate() {
        let is_selected = choice == selected;
        let number = format!("{}. ", index + 1);
        let label_text = labels[index].to_string();

        let mut spans: Vec<Span<'static>> = Vec::with_capacity(4);
        // 外层统一缩进 1 格，与标题/命令/提问句对齐；
        // 焦点项在缩进后放 `❯ `，非焦点项用 2 空格占住同样的宽度。
        spans.push(Span::raw(" "));
        if is_selected {
            spans.push(Span::styled(
                "❯ ",
                Style::default().fg(theme::cur().thinking),
            ));
            spans.push(Span::styled(
                number,
                Style::default().fg(theme::cur().inactive),
            ));
            spans.push(Span::styled(
                label_text,
                Style::default()
                    .fg(theme::cur().thinking)
                    .add_modifier(Modifier::BOLD),
            ));
        } else {
            spans.push(Span::raw("  "));
            spans.push(Span::styled(
                number,
                Style::default().fg(theme::cur().inactive),
            ));
            spans.push(Span::styled(
                label_text,
                Style::default().fg(theme::cur().text_soft),
            ));
        }
        let _ = width;
        rows.push(Line::from(spans));
    }
    rows
}

fn input_rows(item: &BatchItem, width: usize, active: bool) -> Vec<Line<'static>> {
    let mut rows = Vec::new();
    let Some(map) = item.input.as_object() else {
        push_text(
            &mut rows,
            &item.input.to_string(),
            width,
            theme::cur().text_soft,
        );
        return rows;
    };
    // Skill 场景：只渲染 description 正文（不带 `description:` 前缀，也不显示技能名），
    // 并对每行加 `│ ` gutter，对齐参考实现的引用块样式。
    if item.tool_name == "Skill" {
        if let Some(desc) = map.get("description").and_then(serde_json::Value::as_str) {
            let desc = sanitize_display_text_preserving_newlines(desc);
            for row in wrap_text(&desc, width.saturating_sub(2).max(20)) {
                rows.push(Line::from(vec![
                    Span::styled("│ ", Style::default().fg(theme::cur().inactive)),
                    Span::styled(row, Style::default().fg(theme::cur().text_soft)),
                ]));
            }
        }
        if rows.is_empty() {
            push_text(&mut rows, "{}", width, theme::cur().text_soft);
        }
        return rows;
    }
    let command = (item.tool_name == "Bash")
        .then(|| map.get("command").and_then(serde_json::Value::as_str))
        .flatten();
    if let Some(command) = command {
        push_text(&mut rows, command, width, theme::cur().text_soft);
    }
    // 命令/脚本超过一行时，为每行加 `│` gutter（对齐参考实现的 needsGutter）
    if rows.len() > 1 {
        rows = rows
            .into_iter()
            .map(|row| {
                let mut spans = vec![Span::styled(
                    "│ ",
                    Style::default().fg(theme::cur().inactive),
                )];
                spans.extend(row.spans);
                Line::from(spans)
            })
            .collect();
    }
    // 只有「操作单个文件」的工具，其路径才提升为弹窗副标题、内容区不再重复：
    // Edit / Write / Read / NotebookEdit。Grep 的 `path` 是搜索范围，属普通参数。
    let path_is_subtitle = matches!(
        item.tool_name.as_str(),
        "Edit" | "Write" | "Read" | "NotebookEdit"
    );
    let path = path_is_subtitle
        .then(|| {
            ["file_path", "notebook_path", "path"]
                .into_iter()
                .find_map(|key| {
                    map.get(key)
                        .and_then(serde_json::Value::as_str)
                        .map(|path| (key, path))
                })
        })
        .flatten();
    let edit = if item.tool_name == "Edit" {
        map.get("old_string")
            .and_then(serde_json::Value::as_str)
            .zip(map.get("new_string").and_then(serde_json::Value::as_str))
    } else {
        None
    };
    // Edit 场景：路径已由审批弹窗副标题显示，diff 自己也带文件头，这里不再重复打印。
    // 文件路径由审批弹窗副标题统一显示，内容区不再重复。
    let _ = path;
    if active {
        if let Some((old, new)) = edit {
            // 对齐参考实现：不打印「修改对比」说明行，直接渲染带行号 +
            // 上下文 + 语法高亮的 diff（复用 cc-widgets::diff，与消息区同一实现）。
            push_rich_diff(
                &mut rows,
                path.map(|(_, p)| p).unwrap_or_default(),
                old,
                new,
                width,
            );
        }
    }
    // Write 的 content：不带 `content:` 前缀，多行加 `│ ` gutter（对齐参考实现的引用块）。
    if item.tool_name == "Write" {
        if let Some(content) = map.get("content").and_then(serde_json::Value::as_str) {
            let content = sanitize_display_text_preserving_newlines(content);
            for row in wrap_text(&content, width.saturating_sub(2).max(20)) {
                rows.push(Line::from(vec![
                    Span::styled("│ ", Style::default().fg(theme::cur().inactive)),
                    Span::styled(row, Style::default().fg(theme::cur().text_soft)),
                ]));
            }
        }
    }
    // 其余参数：按语义优先级排序（pattern/query 最前，owner/repo 次之），
    // 而不是 JSON 字典序，避免 `glob` 排在 `pattern` 前面这种反直觉顺序。
    const PRIORITY: &[&str] = &[
        "pattern",
        "query",
        "url",
        "command",
        "path",
        "owner",
        "repo",
        "title",
        "old_string",
        "new_string",
        "offset",
        "limit",
    ];
    let mut rest: Vec<(&String, &serde_json::Value)> = map.iter().collect();
    rest.sort_by_key(|(key, _)| {
        PRIORITY
            .iter()
            .position(|p| p == key)
            .unwrap_or(PRIORITY.len())
    });
    for (key, value) in rest {
        if (key == "command" && command.is_some())
            || path.is_some_and(|(path_key, _)| key == path_key)
            || (key == "content" && item.tool_name == "Write")
            || ((key == "old_string" || key == "new_string") && edit.is_some())
        {
            continue;
        }
        let value = value
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| value.to_string());
        push_text(
            &mut rows,
            &format!("{key}: {value}"),
            width,
            theme::cur().text_soft,
        );
    }
    // 内容为空时不再输出 `{}` 占位：审批弹窗的内容区已有上下虚线包裹，
    // 留空比显示无意义的 `{}` 更接近参考实现。
    rows
}

fn push_text(
    rows: &mut Vec<Line<'static>>,
    text: &str,
    width: usize,
    color: ratatui::style::Color,
) {
    rows.extend(
        wrap_text(&sanitize_display_text_preserving_newlines(text), width)
            .into_iter()
            .map(|text| Line::styled(text, Style::default().fg(color))),
    );
}

/// 富 diff：复用 `cc_widgets::diff`，得到带行号 gutter、上下文行与语法高亮的行。
///
/// 与消息区的 diff 渲染共用同一套实现（`cc_widgets::diff::render_diff`），
/// 对齐参考实现的 `color-diff-napi` 输出形态。
///
/// `render_diff` 会在开头输出「文件路径行 + 增删摘要行」，但审批弹窗的标题区
/// 已经显示了路径、参考实现也不打印摘要，因此这里跳过前两行。
fn push_rich_diff(
    rows: &mut Vec<Line<'static>>,
    file_path: &str,
    old: &str,
    new: &str,
    width: usize,
) {
    let input = cc_widgets::diff::DiffInput {
        file_path: file_path.to_string(),
        old_content: sanitize_display_text_preserving_newlines(old),
        new_content: sanitize_display_text_preserving_newlines(new),
        is_new_file: old.is_empty(),
        is_deleted_file: new.is_empty(),
        is_binary: false,
    };
    let theme = theme::widgets_theme();
    let rendered = cc_widgets::diff::render_diff(&input, width.saturating_sub(2).max(20), &theme);
    // 跳过「路径标题行」和「Added N lines, removed M lines」摘要行：
    // 前者与审批弹窗副标题重复，后者 参考实现不显示。
    let skip = rendered
        .iter()
        .take(2)
        .take_while(|line| {
            let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
            text.contains(file_path)
                || text.trim_start().starts_with("Added ")
                || text.trim_start().starts_with("Removed ")
        })
        .count();
    rows.extend(rendered.into_iter().skip(skip).map(replace_remove_marker));
}

/// 把 diff 行里的 ASCII 删除标记 `- ` 换成 U+2212 `−`（对齐参考实现的减号）。
fn replace_remove_marker(line: Line<'static>) -> Line<'static> {
    let spans = line
        .spans
        .into_iter()
        .map(|mut span| {
            if span.content.contains("- ") {
                span.content = span.content.replace("- ", "− ").into();
            }
            span
        })
        .collect::<Vec<_>>();
    Line::from(spans)
}

/// 在字素边界按显示列宽折行，保留原换行、缩进与连续长参数。
fn wrap_text(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut rows = Vec::new();
    for line in text.split('\n') {
        let mut current = String::new();
        let mut columns = 0;
        for grapheme in line.graphemes(true) {
            let grapheme_width = grapheme.width();
            if columns + grapheme_width > width && !current.is_empty() {
                rows.push(std::mem::take(&mut current));
                columns = 0;
            }
            current.push_str(grapheme);
            columns += grapheme_width;
        }
        rows.push(current);
    }
    rows
}

fn wrap_words(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut rows = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        if !line.is_empty() && line.width() + word.width() + 1 > width {
            rows.push(std::mem::take(&mut line));
        }
        if word.width() > width {
            let mut parts = wrap_text(word, width);
            line = parts.pop().unwrap_or_default();
            rows.extend(parts);
        } else {
            if !line.is_empty() {
                line.push(' ');
            }
            line.push_str(word);
        }
    }
    if !line.is_empty() {
        rows.push(line);
    }
    rows
}

pub(super) fn hidden_notice(app: &App, count: usize, width: usize) -> Vec<Line<'static>> {
    let text = app.services.lc.tr_args(
        "hitl-hidden-lines",
        &[("count".into(), (count as i64).into())],
    );
    wrap_words(&text, width)
        .into_iter()
        .map(|text| Line::styled(text, Style::default().fg(theme::cur().warning)))
        .collect()
}
