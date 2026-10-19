use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span, Text},
    widgets::Paragraph,
    Frame,
};

use cc_widgets::BorderedPanel;

use crate::{
    app::{
        login_panel::{LoginEditField, LoginPanel, LoginPanelMode},
        App,
    },
    ui::theme,
};

/// /login 面板渲染（底部展开区）
pub(crate) fn render_login_panel(f: &mut Frame, panel: &LoginPanel, app: &mut App, area: Rect) {
    let lc = &app.services.lc;
    let border_color = match panel.mode {
        LoginPanelMode::Browse => theme::cur().prompt_border,
        LoginPanelMode::Edit => theme::cur().warning,
        LoginPanelMode::New => theme::cur().success,
        LoginPanelMode::ConfirmDelete => theme::cur().error,
    };

    let title = match panel.mode {
        LoginPanelMode::Browse => lc.tr("login-panel-title-browse"),
        LoginPanelMode::Edit => lc.tr("login-panel-title-edit"),
        LoginPanelMode::New => lc.tr("login-panel-title-new"),
        LoginPanelMode::ConfirmDelete => lc.tr("login-panel-title-confirm-delete"),
    };

    let inner = BorderedPanel::new(Span::styled(
        title,
        Style::default()
            .fg(theme::cur().thinking)
            .add_modifier(Modifier::BOLD),
    ))
    .border_style(Style::default().fg(border_color))
    .render(f, area);

    app.session_mgr.current_mut().ui.panel_area = Some(inner);

    let active_provider_id = app
        .services
        .peri_config
        .as_ref()
        .map(|c| c.config.active_provider_id.as_str())
        .unwrap_or("");

    match panel.mode {
        LoginPanelMode::Browse => {
            let mut lines: Vec<Line> = Vec::new();
            for (i, p) in panel.providers.iter().enumerate() {
                if i > 0 {
                    lines.push(Line::from(""));
                }
                let is_cursor = i == panel.cursor();
                let is_active = p.id == active_provider_id;
                let bullet = if is_active { "●" } else { "○" };
                let cursor_char = if is_cursor { theme::SYMBOL_CURSOR } else { " " };
                let name = p.display_name().to_string();
                let type_tag = format!("({})", p.provider_type);
                let row_style = if is_active {
                    Style::default().fg(theme::cur().success)
                } else if is_cursor {
                    Style::default().fg(theme::cur().thinking)
                } else {
                    Style::default().fg(theme::cur().text)
                };
                let cursor_style = theme::cursor_style();
                lines.push(Line::from(vec![
                    Span::styled(format!("{} ", cursor_char), cursor_style),
                    Span::styled(format!("{} ", bullet), row_style),
                    Span::styled(format!("{} ", name), row_style.add_modifier(Modifier::BOLD)),
                    Span::styled(type_tag, Style::default().fg(theme::cur().inactive)),
                ]));
                // 模型名子行
                let m = &p.models;
                let fmt_model = |v: &str| -> String {
                    if v.is_empty() {
                        lc.tr("login-no-model")
                    } else {
                        v.to_string()
                    }
                };
                lines.push(Line::from(vec![
                    Span::styled("       ", Style::default().fg(theme::cur().inactive)),
                    Span::styled(
                        "Opus ",
                        Style::default()
                            .fg(theme::cur().inactive)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(fmt_model(&m.opus), Style::default().fg(theme::cur().inactive)),
                    Span::styled(
                        "  Sonnet ",
                        Style::default()
                            .fg(theme::cur().inactive)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(fmt_model(&m.sonnet), Style::default().fg(theme::cur().inactive)),
                    Span::styled(
                        "  Haiku ",
                        Style::default()
                            .fg(theme::cur().inactive)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(fmt_model(&m.haiku), Style::default().fg(theme::cur().inactive)),
                    Span::styled(
                        "  Fable ",
                        Style::default()
                            .fg(theme::cur().inactive)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(fmt_model(&m.fable), Style::default().fg(theme::cur().inactive)),
                ]));
            }
            if panel.providers.is_empty() {
                lines.push(Line::from(Span::styled(
                    lc.tr("login-empty-hint"),
                    Style::default().fg(theme::cur().inactive),
                )));
            }
            lines.truncate(inner.height as usize);
            f.render_widget(Paragraph::new(Text::from(lines)), inner);
        }

        LoginPanelMode::Edit | LoginPanelMode::New => {
            let mut lines: Vec<Line> = vec![Line::from("")];
            let fields: &[(LoginEditField, &str, &str, usize)] = &[
                (
                    LoginEditField::Name,
                    "Name        ",
                    &panel.buf_name,
                    panel.cur_name,
                ),
                (LoginEditField::Type, "Type        ", &panel.buf_type, 0),
                (
                    LoginEditField::BaseUrl,
                    "Base URL    ",
                    &panel.buf_base_url,
                    panel.cur_base_url,
                ),
                (
                    LoginEditField::ApiKey,
                    "API Key     ",
                    &panel.buf_api_key,
                    panel.cur_api_key,
                ),
                (
                    LoginEditField::OpusModel,
                    "Opus Model  ",
                    &panel.buf_opus_model,
                    panel.cur_opus_model,
                ),
                (
                    LoginEditField::SonnetModel,
                    "Sonnet Model",
                    &panel.buf_sonnet_model,
                    panel.cur_sonnet_model,
                ),
                (
                    LoginEditField::HaikuModel,
                    "Haiku Model ",
                    &panel.buf_haiku_model,
                    panel.cur_haiku_model,
                ),
                (
                    LoginEditField::FableModel,
                    "Fable Model ",
                    &panel.buf_fable_model,
                    panel.cur_fable_model,
                ),
            ];

            for (field, label, value, cursor) in fields {
                let is_active = *field == panel.edit_field;
                let value_display = if *field == LoginEditField::Type {
                    let types = ["openai", "anthropic"];
                    types
                        .iter()
                        .map(|t| {
                            if *t == *value {
                                format!("[{}]", t)
                            } else {
                                t.to_string()
                            }
                        })
                        .collect::<Vec<_>>()
                        .join("  ")
                } else if *field == LoginEditField::ApiKey && !is_active {
                    mask_api_key(value)
                } else if is_active {
                    let (before, after) = crate::app::edit_display_parts(value, *cursor);
                    format!("{}█{}", before, after)
                } else {
                    value.to_string()
                };

                let (label_style, value_style) = if is_active {
                    (
                        Style::default()
                            .fg(theme::cur().thinking)
                            .add_modifier(Modifier::BOLD),
                        Style::default().fg(theme::cur().thinking),
                    )
                } else {
                    (
                        Style::default().fg(theme::cur().inactive),
                        Style::default().fg(theme::cur().text),
                    )
                };

                lines.push(Line::from(vec![
                    Span::styled(format!("  {} ", label), label_style),
                    Span::styled(format!(" {}", value_display), value_style),
                ]));
            }

            lines.truncate(inner.height as usize);
            f.render_widget(Paragraph::new(Text::from(lines)), inner);
        }

        LoginPanelMode::ConfirmDelete => {
            let mut list_lines: Vec<Line> = Vec::new();
            for (i, p) in panel.providers.iter().enumerate() {
                let is_cursor = i == panel.cursor();
                let is_active = p.id == active_provider_id;
                let bullet = if is_active { "●" } else { "○" };
                let cursor_char = if is_cursor { theme::SYMBOL_CURSOR } else { " " };
                let row_style = if is_active {
                    Style::default().fg(theme::cur().success)
                } else if is_cursor {
                    Style::default().fg(theme::cur().thinking)
                } else {
                    Style::default().fg(theme::cur().text)
                };
                let cursor_style = theme::cursor_style();
                list_lines.push(Line::from(vec![
                    Span::styled(format!("{} ", cursor_char), cursor_style),
                    Span::styled(format!("{} ", bullet), row_style),
                    Span::styled(
                        p.display_name().to_string(),
                        row_style.add_modifier(Modifier::BOLD),
                    ),
                ]));
            }
            list_lines.truncate(inner.height.saturating_sub(3) as usize);
            f.render_widget(Paragraph::new(Text::from(list_lines)), inner);

            let confirm_y = inner.y + inner.height.saturating_sub(2);
            let confirm_area = Rect {
                y: confirm_y,
                height: 2,
                ..inner
            };
            if let Some(p) = panel.providers.get(panel.cursor()) {
                let confirm_lines = vec![
                    Line::from(""),
                    Line::from(vec![
                        Span::styled(
                            lc.tr("login-confirm-delete-label"),
                            Style::default().fg(theme::cur().text),
                        ),
                        Span::styled(
                            p.display_name().to_string(),
                            Style::default()
                                .fg(theme::cur().error)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            lc.tr("login-confirm-delete-question"),
                            Style::default().fg(theme::cur().text),
                        ),
                    ]),
                ];
                f.render_widget(Paragraph::new(Text::from(confirm_lines)), confirm_area);
            }
        }
    }
}

fn mask_api_key(key: &str) -> String {
    let chars: Vec<char> = key.chars().collect();
    let len = chars.len();
    if len <= 8 {
        return "*".repeat(len);
    }
    let prefix: String = chars[..4].iter().collect();
    let suffix: String = chars[len - 4..].iter().collect();
    format!("{}****{}", prefix, suffix)
}

#[cfg(test)]
mod tests {
    use crate::{
        app::{
            login_panel::{LoginEditField, LoginPanel, LoginPanelMode},
            App,
        },
        config::ProviderConfig,
    };
    include!("login_test.rs");
}
