use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span, Text},
    widgets::Paragraph,
    Frame,
};

use perihelion_widgets::BorderedPanel;

use crate::app::setup_wizard::{FormField, FormMode, SetupSource, SetupStep, SetupWizardPanel};
use crate::ui::theme;

/// Setup 向导全屏渲染入口
pub(crate) fn render_setup_wizard(f: &mut Frame, app: &crate::app::App) {
    let area = f.area();
    let wizard = app.global_ui.setup_wizard.as_ref().unwrap();

    match wizard.step {
        SetupStep::Choose => render_step_choose(f, wizard, area),
        SetupStep::Form => render_step_form(f, wizard, area),
        SetupStep::Done => render_step_done(f, wizard, area),
    }
}

fn render_step_choose(f: &mut Frame, wizard: &SetupWizardPanel, area: Rect) {
    let inner = BorderedPanel::new(Span::styled(
        " ── Peri Setup ── Welcome ",
        Style::default()
            .fg(theme::ACCENT)
            .add_modifier(Modifier::BOLD),
    ))
    .border_style(Style::default().fg(theme::ACCENT))
    .render(f, area);

    let mut lines: Vec<Line> = vec![
        Line::from(""),
        Line::from(Span::styled(
            " Choose how to configure your provider:",
            Style::default().fg(theme::MUTED),
        )),
        Line::from(""),
    ];

    for (i, src) in SetupSource::ALL.iter().enumerate() {
        let is_cursor = i == wizard.choose_cursor;
        let cursor_char = if is_cursor { "❯" } else { " " };
        let cursor_style = Style::default().fg(theme::THINKING);
        let label_style = if is_cursor {
            Style::default()
                .fg(theme::THINKING)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
                .fg(theme::TEXT)
                .add_modifier(Modifier::BOLD)
        };
        let desc_style = if is_cursor {
            Style::default().fg(theme::THINKING)
        } else {
            Style::default().fg(theme::MUTED)
        };
        lines.push(Line::from(vec![
            Span::styled(format!("{} ", cursor_char), cursor_style),
            Span::styled(format!("{} ", src.label()), label_style),
        ]));
        lines.push(Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled(src.description(), desc_style),
        ]));
        lines.push(Line::from(""));
    }

    lines.push(make_hint_line(&[
        ("Enter", "确认"),
        ("↑/↓", "选择"),
        ("Esc", "退出"),
    ]));
    f.render_widget(Paragraph::new(Text::from(lines)), inner);
}

fn render_step_form(f: &mut Frame, wizard: &SetupWizardPanel, area: Rect) {
    match wizard.form_mode {
        FormMode::Browse => render_form_browse(f, wizard, area),
        FormMode::Edit => render_form_edit(f, wizard, area),
    }
}

/// Browse 模式：只读列表 + Submit
fn render_form_browse(f: &mut Frame, wizard: &SetupWizardPanel, area: Rect) {
    let inner = BorderedPanel::new(Span::styled(
        " ── Peri Setup ── Configure Providers ",
        Style::default()
            .fg(theme::ACCENT)
            .add_modifier(Modifier::BOLD),
    ))
    .border_style(Style::default().fg(theme::ACCENT))
    .render(f, area);

    let mut lines: Vec<Line> = vec![Line::from("")];

    let submit_pos = wizard.providers.len();

    for (idx, mp) in wizard.providers.iter().enumerate() {
        let is_cursor = idx == wizard.browse_cursor;
        let cursor = if is_cursor { "❯" } else { " " };
        let check_char = if mp.selected { "✓" } else { " " };
        let check_color = if mp.selected { theme::SAGE } else { theme::DIM };
        let name_style = if is_cursor {
            Style::default()
                .fg(theme::THINKING)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::TEXT)
        };
        let detail_style = if is_cursor {
            Style::default().fg(theme::THINKING)
        } else {
            Style::default().fg(theme::MUTED)
        };

        let key_summary = if mp.api_key.is_empty() {
            "(no key)".to_string()
        } else {
            mask_api_key(&mp.api_key)
        };

        lines.push(Line::from(vec![
            Span::styled(format!("{} ", cursor), Style::default().fg(theme::THINKING)),
            Span::styled(
                format!("[{}] ", check_char),
                Style::default().fg(check_color),
            ),
            Span::styled(format!("{} ", mp.provider_type.label()), name_style),
            Span::styled(
                format!("({}) ", mp.provider_id),
                Style::default().fg(theme::MUTED),
            ),
            Span::styled(key_summary, detail_style),
        ]));

        // 第二行：base_url 摘要
        if !mp.base_url.is_empty() {
            let url_style = if is_cursor {
                Style::default().fg(theme::DIM)
            } else {
                Style::default().fg(theme::DIM)
            };
            lines.push(Line::from(vec![
                Span::styled("     ", Style::default()),
                Span::styled(&mp.base_url, url_style),
            ]));
        }

        lines.push(Line::from(""));
    }

    // Submit 按钮
    let submit_active = wizard.browse_cursor == submit_pos;
    let submit_style = if submit_active {
        Style::default()
            .fg(theme::ACCENT)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::MUTED)
    };
    let submit_cursor = if submit_active { "❯ " } else { "  " };
    lines.push(Line::from(vec![
        Span::styled(submit_cursor, Style::default().fg(theme::THINKING)),
        Span::styled(" Submit", submit_style),
    ]));

    lines.push(Line::from(""));
    lines.push(make_hint_line(&[
        ("Enter", "编辑/提交"),
        ("Space", "勾选"),
        ("↑/↓", "选择"),
        ("Esc", "返回"),
    ]));

    f.render_widget(Paragraph::new(Text::from(lines)), inner);
}

/// Edit 模式：编辑单个 provider 的所有字段
fn render_form_edit(f: &mut Frame, wizard: &SetupWizardPanel, area: Rect) {
    let mp = &wizard.providers[wizard.active_provider];
    let header = format!(
        " ── Setup ── Edit: {} ({}) ",
        mp.provider_type.label(),
        mp.provider_id
    );

    let inner = BorderedPanel::new(Span::styled(
        header,
        Style::default()
            .fg(theme::ACCENT)
            .add_modifier(Modifier::BOLD),
    ))
    .border_style(Style::default().fg(theme::ACCENT))
    .render(f, area);

    let mut lines: Vec<Line> = vec![Line::from("")];

    lines.push(render_field_line(
        "Type     ",
        FormField::ProviderType,
        format!("[{}]", mp.provider_type.label()),
        wizard.form_focus,
    ));

    let pid_display = edit_display(
        &mp.provider_id,
        mp.cur_provider_id,
        wizard.form_focus == FormField::ProviderId,
    );
    lines.push(render_field_line(
        "ID       ",
        FormField::ProviderId,
        pid_display,
        wizard.form_focus,
    ));

    let url_display = edit_display(
        &mp.base_url,
        mp.cur_base_url,
        wizard.form_focus == FormField::BaseUrl,
    );
    lines.push(render_field_line(
        "Base URL ",
        FormField::BaseUrl,
        url_display,
        wizard.form_focus,
    ));

    let key_display = if wizard.form_focus == FormField::ApiKey {
        let (before, after) = crate::app::edit_display_parts(&mp.api_key, mp.cur_api_key);
        format!("{}▏{}", before, after)
    } else if mp.api_key.is_empty() {
        String::new()
    } else {
        "•".repeat(mp.api_key.len())
    };
    lines.push(render_field_line(
        "API Key  ",
        FormField::ApiKey,
        key_display,
        wizard.form_focus,
    ));

    lines.push(Line::from(Span::styled(
        "  ─────────────────────────────────",
        Style::default().fg(theme::DIM),
    )));

    let alias_labels = [
        ("Opus  ", FormField::OpusModel, 0),
        ("Sonnet", FormField::SonnetModel, 1),
        ("Haiku ", FormField::HaikuModel, 2),
    ];
    for (label, field, ai) in alias_labels {
        let model_display = edit_display(
            &mp.aliases[ai].model_id,
            mp.aliases[ai].cursor,
            wizard.form_focus == field,
        );
        lines.push(render_field_line(
            &format!("{} Model ", label),
            field,
            model_display,
            wizard.form_focus,
        ));
    }

    // Confirm 按钮
    let confirm_active = wizard.form_focus == FormField::Confirm;
    let confirm_style = if confirm_active {
        Style::default()
            .fg(theme::ACCENT)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::MUTED)
    };
    let confirm_cursor = if confirm_active { "❯ " } else { "  " };
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled(confirm_cursor, Style::default().fg(theme::THINKING)),
        Span::styled(" Confirm", confirm_style),
    ]));

    lines.push(Line::from(""));
    lines.push(make_hint_line(&[
        ("Enter", "确认"),
        ("←/→", "切换类型"),
        ("Esc", "返回列表"),
    ]));

    f.render_widget(Paragraph::new(Text::from(lines)), inner);
}

/// 渲染单个字段行（带光标指示器）
fn render_field_line(
    label: &str,
    field: FormField,
    value: String,
    focus: FormField,
) -> Line<'static> {
    let is_active = focus == field;
    let cursor = if is_active { "❯ " } else { "  " };
    let lbl = if is_active {
        Style::default()
            .fg(theme::THINKING)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::MUTED)
    };
    let val = if is_active {
        Style::default().fg(theme::THINKING)
    } else {
        Style::default().fg(theme::TEXT)
    };
    let label_owned = label.to_string();
    Line::from(vec![
        Span::styled(cursor, Style::default().fg(theme::THINKING)),
        Span::styled(label_owned, lbl),
        Span::styled(format!(" {}", value), val),
    ])
}

/// 编辑字段显示：活跃时显示光标 ▏，否则显示值
fn edit_display(value: &str, cursor: usize, active: bool) -> String {
    if active {
        let (before, after) = crate::app::edit_display_parts(value, cursor);
        format!("{}▏{}", before, after)
    } else {
        value.to_string()
    }
}

fn render_step_done(f: &mut Frame, wizard: &SetupWizardPanel, area: Rect) {
    let inner = BorderedPanel::new(Span::styled(
        " ── Setup Complete ✓ ",
        Style::default()
            .fg(theme::SAGE)
            .add_modifier(Modifier::BOLD),
    ))
    .border_style(Style::default().fg(theme::SAGE))
    .render(f, area);

    let mut lines = vec![Line::from("")];

    let selected: Vec<_> = wizard.providers.iter().filter(|p| p.selected).collect();
    for mp in &selected {
        lines.push(Line::from(vec![
            Span::styled(" ● ", Style::default().fg(theme::SAGE)),
            Span::styled(
                format!("{} ", mp.provider_type.label()),
                Style::default().fg(theme::TEXT),
            ),
            Span::styled(
                format!("({})", mp.provider_id),
                Style::default().fg(theme::MUTED),
            ),
        ]));
        lines.push(Line::from(vec![
            Span::styled("   Key: ", Style::default().fg(theme::MUTED)),
            Span::styled(mask_api_key(&mp.api_key), Style::default().fg(theme::TEXT)),
        ]));
        let alias_labels = ["Opus", "Sonnet", "Haiku"];
        for (i, label) in alias_labels.iter().enumerate() {
            lines.push(Line::from(vec![
                Span::styled(
                    format!("   {:>6} → ", label),
                    Style::default().fg(theme::MUTED),
                ),
                Span::styled(&mp.aliases[i].model_id, Style::default().fg(theme::ACCENT)),
            ]));
        }
        lines.push(Line::from(""));
    }

    lines.push(Line::from(vec![
        Span::styled(" 按 ", Style::default().fg(theme::TEXT)),
        Span::styled(
            "Enter",
            Style::default()
                .fg(theme::SAGE)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" 开始使用", Style::default().fg(theme::TEXT)),
    ]));

    f.render_widget(Paragraph::new(Text::from(lines)), inner);
}

/// 生成底部快捷键提示行
fn make_hint_line(items: &[(&'static str, &'static str)]) -> Line<'static> {
    let mut spans: Vec<Span> = Vec::new();
    for (i, (key, desc)) in items.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled("  ", Style::default()));
        }
        spans.push(Span::styled(
            *key,
            Style::default()
                .fg(theme::WARNING)
                .add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(
            format!(":{}", desc),
            Style::default().fg(theme::MUTED),
        ));
    }
    Line::from(spans)
}

/// API Key 脱敏
fn mask_api_key(key: &str) -> String {
    let chars: Vec<char> = key.chars().collect();
    let len = chars.len();
    if len <= 8 {
        "•".repeat(len)
    } else {
        let prefix: String = chars[..4].iter().collect();
        let suffix: String = chars[len - 4..].iter().collect();
        format!("{}••••{}", prefix, suffix)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::setup_wizard::SetupWizardPanel;
    use crate::app::App;

    #[test]
    fn test_mask_api_key() {
        assert_eq!(mask_api_key(""), "");
        assert_eq!(mask_api_key("short"), "•••••");
        assert_eq!(mask_api_key("sk-ant-test-key-12345"), "sk-a••••2345");
    }

    async fn render_headless(
        wizard: SetupWizardPanel,
    ) -> (App, crate::ui::headless::HeadlessHandle) {
        let (mut app, mut handle) = App::new_headless(120, 30).await;
        app.global_ui.setup_wizard = Some(wizard);
        handle
            .terminal
            .draw(|f| crate::ui::main_ui::render(f, &mut app))
            .unwrap();
        (app, handle)
    }

    #[tokio::test]
    async fn test_render_step_choose() {
        let wizard = SetupWizardPanel::new();
        let (_, handle) = render_headless(wizard).await;
        assert!(handle.contains("Custom API"));
        assert!(handle.contains("Claude Code"));
    }

    #[tokio::test]
    async fn test_render_step_form() {
        let mut wizard = SetupWizardPanel::new();
        wizard.step = SetupStep::Form;
        let (_, handle) = render_headless(wizard).await;
        assert!(handle.contains("Configure"));
        assert!(handle.contains("Submit"));
    }

    #[tokio::test]
    async fn test_render_done_page() {
        let mut wizard = SetupWizardPanel::new();
        wizard.step = SetupStep::Done;
        wizard.providers[0].api_key = "sk-ant-test1234xyz".to_string();
        let (_, handle) = render_headless(wizard).await;
        assert!(handle.contains("Complete"));
    }
}
