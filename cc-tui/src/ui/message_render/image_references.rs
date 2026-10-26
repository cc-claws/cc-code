use super::{push_link_hits_for_wrapped, wrap_line_spans_rich, BranchPrefix};
use cc_widgets::markdown::LinkHit;
use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};
use std::path::PathBuf;

pub(super) fn append(
    lines: &mut Vec<Line<'static>>,
    links: &mut Vec<LinkHit>,
    content: &str,
    paths: &[(usize, PathBuf)],
    width: usize,
    hyperlinks_enabled: bool,
) {
    let mut ids = crate::clipboard::image_placeholder::parse_placeholders(content);
    ids.sort_unstable();
    ids.dedup();
    for id in ids {
        let label = crate::clipboard::image_placeholder::format_placeholder(id);
        let url = hyperlinks_enabled
            .then(|| paths.iter().find(|(image_id, _)| *image_id == id))
            .flatten()
            .and_then(|(_, path)| url::Url::from_file_path(path).ok());
        let style = Style::default().fg(crate::ui::theme::cur().inactive);
        let style = if url.is_some() {
            style.add_modifier(Modifier::UNDERLINED)
        } else {
            style
        };
        let wrapped = wrap_line_spans_rich(
            Line::from(Span::styled(label.clone(), style)),
            width.saturating_sub(BranchPrefix::WIDTH).max(1),
        );
        if let Some(url) = url {
            let hit = LinkHit {
                line: 0,
                g_start: 0,
                g_end: label.chars().count(),
                url: url.to_string(),
            };
            push_link_hits_for_wrapped(links, lines.len(), &[hit], &wrapped, BranchPrefix::WIDTH);
        }
        for (index, segment) in wrapped.into_iter().enumerate() {
            let prefix = if index == 0 {
                BranchPrefix::span(false)
            } else {
                Span::raw(BranchPrefix::HANG)
            };
            let mut spans = vec![prefix];
            spans.extend(segment.line.spans);
            // 附件在正文气泡之外，不能继承 user_message_background。
            lines.push(Line::from(spans));
        }
    }
}

#[cfg(test)]
#[path = "image_references_test.rs"]
mod tests;
