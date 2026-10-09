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
    pub sections: Vec<Vec<Line<'static>>>,
    pub summary: Vec<Line<'static>>,
    pub choices: Vec<Line<'static>>,
    pub hints: Vec<Line<'static>>,
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
            let choice = prompt
                .choices
                .get(index)
                .copied()
                .unwrap_or(ApprovalChoice::Once);
            let mut rows = vec![tool_header(
                item,
                index,
                index == cursor,
                (prompt.items.len() > 1).then_some(choice),
                &app.services.cwd,
                width,
                lc,
            )];
            rows.extend(input_rows(item, width, lc, index == cursor));
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
                Line::styled(
                    truncate_to_display_width(&row, width),
                    Style::default().fg(theme::MUTED),
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
            choice_rows(
                item,
                prompt
                    .choices
                    .get(cursor)
                    .copied()
                    .unwrap_or(ApprovalChoice::Once),
                width,
                lc,
            )
        })
        .unwrap_or_default();
    let hints = wrap_words(&lc.tr("hitl-key-hint"), width)
        .into_iter()
        .map(|row| Line::styled(row, Style::default().fg(theme::MUTED)))
        .collect();
    Some(ApprovalContent {
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

fn description_key(item: &BatchItem, choice: ApprovalChoice) -> &'static str {
    match choice {
        ApprovalChoice::Once => "hitl-description-once",
        ApprovalChoice::Reject => "hitl-description-reject",
        ApprovalChoice::Session if item.tool_name == "Bash" => "hitl-description-session-command",
        ApprovalChoice::Session
            if matches!(item.tool_name.as_str(), "Read" | "Write" | "Edit")
                && (item
                    .input
                    .get("file_path")
                    .and_then(serde_json::Value::as_str)
                    .is_some()
                    || item
                        .input
                        .get("path")
                        .and_then(serde_json::Value::as_str)
                        .is_some()) =>
        {
            "hitl-description-session-file"
        }
        ApprovalChoice::Session => "hitl-description-session-tool",
    }
}

fn choice_rows(
    item: &BatchItem,
    selected: ApprovalChoice,
    width: usize,
    lc: &LcRegistry,
) -> Vec<Line<'static>> {
    let labels = ApprovalChoice::ALL.map(|choice| lc.tr(choice_key(choice)));
    let descriptions = ApprovalChoice::ALL.map(|choice| lc.tr(description_key(item, choice)));
    let label_width = labels.iter().map(|label| label.width()).max().unwrap_or(0);
    let description_width = descriptions
        .iter()
        .map(|text| text.width())
        .max()
        .unwrap_or(0);
    let wide = label_width + description_width + 8 <= width;
    let mut rows = Vec::new();
    for (index, choice) in ApprovalChoice::ALL.into_iter().enumerate() {
        let is_selected = choice == selected;
        let marker = if is_selected { "❯ ● " } else { "  ○ " };
        let style = if is_selected {
            Style::default()
                .fg(if choice == ApprovalChoice::Reject {
                    theme::ERROR
                } else {
                    theme::THINKING
                })
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::MUTED)
        };
        let label = format!("{marker}{}", labels[index]);
        let mut spans = vec![Span::styled(
            truncate_to_display_width(&label, width),
            style,
        )];
        if wide {
            spans.push(Span::raw(
                " ".repeat(label_width.saturating_sub(labels[index].width()) + 4),
            ));
            spans.push(Span::styled(
                descriptions[index].clone(),
                Style::default().fg(theme::MUTED),
            ));
        }
        rows.push(Line::from(spans));
    }
    if !wide {
        rows.extend(
            wrap_words(&descriptions[selected.index()], width)
                .into_iter()
                .map(|text| Line::styled(text, Style::default().fg(theme::MUTED))),
        );
    }
    rows
}

fn tool_header(
    item: &BatchItem,
    index: usize,
    active: bool,
    choice: Option<ApprovalChoice>,
    cwd: &str,
    width: usize,
    lc: &LcRegistry,
) -> Line<'static> {
    let action = lc.tr(match item.tool_name.as_str() {
        "Bash" => "hitl-run-command",
        "Edit" => "hitl-edit-file",
        "Read" => "hitl-read-file",
        "Write" => "hitl-write-file",
        "Glob" | "Grep" | "WebSearch" => "hitl-search",
        _ => "hitl-call-tool",
    });
    let prefix = if choice.is_some() {
        format!("{} {} ", if active { "❯" } else { " " }, index + 1)
    } else {
        String::new()
    };
    let heading = format!(
        "{prefix}{} {action} {} {}",
        sanitize_display_text(&item.tool_name),
        lc.tr("hitl-working-directory"),
        sanitize_display_text(cwd)
    );
    let selection = choice.map(|value| {
        lc.tr_args(
            "hitl-selected-choice",
            &[("choice".into(), lc.tr(choice_key(value)).into())],
        )
    });
    let reserved = selection.as_ref().map(|text| text.width() + 2).unwrap_or(0);
    let mut spans = vec![Span::styled(
        truncate_to_display_width(&heading, width.saturating_sub(reserved)),
        Style::default()
            .fg(if active {
                theme::THINKING
            } else {
                theme::TEXT_SOFT
            })
            .add_modifier(if active {
                Modifier::BOLD
            } else {
                Modifier::empty()
            }),
    )];
    if let (Some(choice), Some(selection)) = (choice, selection) {
        spans.push(Span::styled(
            truncate_to_display_width(&format!("  {selection}"), width),
            Style::default().fg(match choice {
                ApprovalChoice::Once => theme::SAGE,
                ApprovalChoice::Session => theme::THINKING,
                ApprovalChoice::Reject => theme::ERROR,
            }),
        ));
    }
    Line::from(spans)
}

fn input_rows(item: &BatchItem, width: usize, lc: &LcRegistry, active: bool) -> Vec<Line<'static>> {
    let mut rows = Vec::new();
    let Some(map) = item.input.as_object() else {
        push_text(&mut rows, &item.input.to_string(), width, theme::TEXT_SOFT);
        return rows;
    };
    let command = (item.tool_name == "Bash")
        .then(|| map.get("command").and_then(serde_json::Value::as_str))
        .flatten();
    if let Some(command) = command {
        push_text(&mut rows, command, width, theme::TEXT_SOFT);
    }
    let path = ["file_path", "path"].into_iter().find_map(|key| {
        map.get(key)
            .and_then(serde_json::Value::as_str)
            .map(|path| (key, path))
    });
    if let Some((_, path)) = path {
        push_text(&mut rows, path, width, theme::TEXT_SOFT);
    }
    let edit = if item.tool_name == "Edit" {
        map.get("old_string")
            .and_then(serde_json::Value::as_str)
            .zip(map.get("new_string").and_then(serde_json::Value::as_str))
    } else {
        None
    };
    if active {
        if let Some((old, new)) = edit {
            push_text(&mut rows, &lc.tr("hitl-diff-title"), width, theme::MUTED);
            push_diff(&mut rows, old, new, width);
        }
    }
    for (key, value) in map {
        if (key == "command" && command.is_some())
            || path.is_some_and(|(path_key, _)| key == path_key)
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
            theme::TEXT_SOFT,
        );
    }
    if rows.is_empty() {
        push_text(&mut rows, "{}", width, theme::TEXT_SOFT);
    }
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

fn push_diff(rows: &mut Vec<Line<'static>>, old: &str, new: &str, width: usize) {
    let old = sanitize_display_text_preserving_newlines(old);
    let new = sanitize_display_text_preserving_newlines(new);
    let old: Vec<_> = old.split('\n').collect();
    let new: Vec<_> = new.split('\n').collect();
    let prefix = old
        .iter()
        .zip(&new)
        .take_while(|(left, right)| left == right)
        .count();
    let suffix = old[prefix..]
        .iter()
        .rev()
        .zip(new[prefix..].iter().rev())
        .take_while(|(left, right)| left == right)
        .count();
    let context_before = old[..prefix].iter().map(|text| ("  ", *text, theme::MUTED));
    let removed = old[prefix..old.len() - suffix]
        .iter()
        .map(|text| ("− ", *text, theme::ERROR));
    let added = new[prefix..new.len() - suffix]
        .iter()
        .map(|text| ("+ ", *text, theme::SAGE));
    let context_after = old[old.len() - suffix..]
        .iter()
        .map(|text| ("  ", *text, theme::MUTED));
    for (marker, text, color) in context_before
        .chain(removed)
        .chain(added)
        .chain(context_after)
    {
        for (index, part) in wrap_text(text, width.saturating_sub(2).max(1))
            .into_iter()
            .enumerate()
        {
            rows.push(Line::styled(
                format!("{}{part}", if index == 0 { marker } else { "  " }),
                Style::default().fg(color),
            ));
        }
    }
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
        .map(|text| Line::styled(text, Style::default().fg(theme::WARNING)))
        .collect()
}
