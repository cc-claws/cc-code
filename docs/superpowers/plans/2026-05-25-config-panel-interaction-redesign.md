# Config Panel Interaction Redesign

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Redesign `/config` panel from a Browse/Edit two-step mode to a single direct-edit mode with grouped fields, consistent Space-toggle behavior, and inline descriptions.

**Architecture:** Eliminate `ConfigPanelMode` enum and `PanelList` from `ConfigPanel`. Fields are navigated via Up/Down with row-index constants (same pattern as `ModelPanel`). Boolean/select fields toggle via Space/Left/Right; text fields accept keyboard input directly. Enter saves all fields and closes; Esc discards and closes. Fields are visually grouped into "General" (Autocompact, CompactThreshold, Language, Proactiveness) and "Prompt Overrides" (Persona, Tone) with group headers.

**Tech Stack:** Rust, ratatui, tui-textarea, unicode-width, Fluent i18n

---

### Task 1: Add i18n strings for groups, descriptions, and status bar hints

**Files:**
- Modify: `peri-tui/locales/en/main.ftl`
- Modify: `peri-tui/locales/zh-CN/main.ftl`

- [ ] **Step 1: Add i18n entries to English locale**

In `peri-tui/locales/en/main.ftl`, after the existing `config-field-proactiveness` entry (line 220), insert:

```ftl
# Config panel groups
config-group-general = General
config-group-prompt-overrides = Prompt Overrides

# Config field descriptions
config-desc-autocompact = (ON/OFF — auto-compact context when full)
config-desc-threshold = 50-99% — trigger threshold for auto-compact
config-desc-language = en, zh-CN, or leave empty for auto
config-desc-persona = Override system prompt persona (empty = default)
config-desc-tone = Override system prompt tone (empty = default)
config-desc-proactiveness = low / medium / high — agent initiative level

# Config panel status bar hints
hint-config-field = :Field
hint-config-toggle = :Toggle
hint-config-save = :Save & close
```

- [ ] **Step 2: Add i18n entries to Chinese locale**

In `peri-tui/locales/zh-CN/main.ftl`, after the existing `config-field-proactiveness` entry (line 219), insert:

```ftl
# Config panel groups
config-group-general = 通用
config-group-prompt-overrides = 提示词覆盖

# Config field descriptions
config-desc-autocompact = （开/关 — 上下文满时自动压缩）
config-desc-threshold = 50-99% — 自动压缩触发阈值
config-desc-language = en, zh-CN，或留空为自动
config-desc-persona = 覆盖系统提示词 persona（留空=默认）
config-desc-tone = 覆盖系统提示词 tone（留空=默认）
config-desc-proactiveness = low / medium / high — agent 主动性级别

# Config panel status bar hints
hint-config-field = :字段
hint-config-toggle = :切换
hint-config-save = :保存关闭
```

- [ ] **Step 3: Verify i18n strings exist by counting entries**

```bash
grep -c "config-group-general\|config-group-prompt-overrides" peri-tui/locales/en/main.ftl peri-tui/locales/zh-CN/main.ftl
```
Expected: 2 lines per file (2 entries in each locale).

---

### Task 2: Simplify ConfigPanel struct — remove browse/edit mode

**Files:**
- Modify: `peri-tui/src/app/config_panel.rs`

- [ ] **Step 1: Remove `ConfigPanelMode` enum, add row index constants**

Replace the enum block (lines 14-20) with row index constants matching `ModelPanel` pattern:

```rust
// ─── 行索引常量 ─────────────────────────────────────────────────────────────────

pub const ROW_GENERAL_HEADER: usize = 0;
pub const ROW_AUTOCOMPACT: usize = 1;
pub const ROW_THRESHOLD: usize = 2;
pub const ROW_LANGUAGE: usize = 3;
pub const ROW_PROACTIVENESS: usize = 4;
pub const ROW_SEPARATOR: usize = 5;
pub const ROW_OVERRIDES_HEADER: usize = 6;
pub const ROW_PERSONA: usize = 7;
pub const ROW_TONE: usize = 8;
pub const ROW_COUNT: usize = 9;
```

Delete `ConfigPanelMode` enum (lines 16-20). Remove `config_panel_mode` imports in the render file later.

- [ ] **Step 2: Simplify `ConfigPanel` struct**

Replace the struct definition (lines 71-88):

```rust
#[derive(Clone)]
pub struct ConfigPanel {
    /// 光标所在行（0..ROW_COUNT-1），跳过 header/separator 行
    pub cursor: usize,
    // 编辑缓冲区
    pub buf_autocompact: bool,
    pub buf_threshold: String,
    pub cur_threshold: usize,
    pub buf_language: String,
    pub cur_language: usize,
    pub buf_persona: String,
    pub cur_persona: usize,
    pub buf_tone: String,
    pub cur_tone: usize,
    pub buf_proactiveness: String,
}
```

Remove `mode: ConfigPanelMode`, `browse_list: PanelList<ConfigEditField>`, `edit_field: ConfigEditField`.

- [ ] **Step 3: Simplify `from_config`**

Replace the constructor (lines 91-130):

```rust
impl ConfigPanel {
    pub fn from_config(cfg: &PeriConfig) -> Self {
        let compact_config = cfg.config.compact.as_ref();
        let autocompact = compact_config
            .map(|c| c.auto_compact_enabled)
            .unwrap_or(true);
        let threshold = compact_config
            .map(|c| format!("{}", (c.auto_compact_threshold * 100.0) as u8))
            .unwrap_or_else(|| "85".to_string());
        let proactiveness = cfg
            .config
            .proactiveness
            .clone()
            .unwrap_or_else(|| "medium".to_string());

        Self {
            cursor: ROW_AUTOCOMPACT,
            buf_autocompact: autocompact,
            buf_threshold: threshold,
            cur_threshold: 0,
            buf_language: cfg.config.language.clone().unwrap_or_default(),
            cur_language: 0,
            buf_persona: cfg.config.persona.clone().unwrap_or_default(),
            cur_persona: 0,
            buf_tone: cfg.config.tone.clone().unwrap_or_default(),
            cur_tone: 0,
            buf_proactiveness: proactiveness,
        }
    }
```

Remove `browse_list` initialization.

- [ ] **Step 4: Replace `enter_edit`, `field_next`, `field_prev` with cursor navigation helpers**

Replace lines 132-166 with:

```rust
    /// 光标下移，跳过 header/separator 行
    pub fn cursor_down(&mut self) {
        self.cursor = next_editable_row(self.cursor, false);
    }

    /// 光标上移，跳过 header/separator 行
    pub fn cursor_up(&mut self) {
        self.cursor = next_editable_row(self.cursor, true);
    }
}

/// 返回下一个可编辑行索引（跳过 header 和 separator 行）
fn next_editable_row(current: usize, reverse: bool) -> usize {
    let editable: &[usize] = &[
        ROW_AUTOCOMPACT,
        ROW_THRESHOLD,
        ROW_LANGUAGE,
        ROW_PROACTIVENESS,
        ROW_PERSONA,
        ROW_TONE,
    ];
    if reverse {
        // Find the largest editable row < current
        editable.iter().rev().find(|&&r| r < current).copied().unwrap_or(editable[editable.len() - 1])
    } else {
        // Find the smallest editable row > current
        editable.iter().find(|&&r| r > current).copied().unwrap_or(editable[0])
    }
}
```

Remove `enter_edit()`, `field_next()`, `field_prev()`, `active_field()`, `field_display_value()`.

- [ ] **Step 5: Keep unchanged methods**

Retain unchanged: `cycle_autocompact()`, `cycle_proactiveness()`, `paste_text()`, `apply_edit()`, `field_label()`.

Remove `field_count()` (unused after redesign).

- [ ] **Step 6: Delete `ConfigEditField` enum entirely**

Replace the `ConfigEditField` enum (lines 22-65) with the row index constants from Step 1. The `label()` method is replaced by `field_label()` which already uses index-based matching.

Keep `field_label(index: usize) -> &'static str` but update its match arms to use new row constants. Remove `field_display_value()` (dead code — render code now uses raw buffers directly).

```rust
    pub fn field_label(index: usize) -> &'static str {
        match index {
            ROW_AUTOCOMPACT => "Autocompact",
            ROW_THRESHOLD => "Compact 阈值",
            ROW_LANGUAGE => "语言",
            ROW_PERSONA => "Persona",
            ROW_TONE => "Tone",
            ROW_PROACTIVENESS => "Proactiveness",
            _ => "???",
        }
    }
```

- [ ] **Step 7: Remove unused imports**

Remove `use super::panel_list::PanelList;` (line 10). Remove unused `ConfigPanelMode` import in render file later.

---

### Task 3: Rewrite render_config_panel for single direct-edit mode

**Files:**
- Modify: `peri-tui/src/ui/main_ui/panels/config.rs`

- [ ] **Step 1: Rewrite `render_config_panel`**

Replace the entire file content:

```rust
use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span, Text},
    widgets::Paragraph,
    Frame,
};

use peri_widgets::BorderedPanel;

use crate::app::config_panel::{
    ConfigPanel, ROW_AUTOCOMPACT, ROW_COUNT, ROW_GENERAL_HEADER, ROW_LANGUAGE, 
    ROW_OVERRIDES_HEADER, ROW_PERSONA, ROW_PROACTIVENESS, ROW_SEPARATOR, 
    ROW_THRESHOLD, ROW_TONE,
};
use crate::app::App;
use crate::ui::theme;

/// /config 面板渲染 — 直编辑模式
pub(crate) fn render_config_panel(f: &mut Frame, panel: &ConfigPanel, app: &mut App, area: Rect) {
    let lc = &app.services.lc;

    let inner = BorderedPanel::new(Span::styled(
        lc.tr("config-panel-title-browse"),
        Style::default()
            .fg(theme::THINKING)
            .add_modifier(Modifier::BOLD),
    ))
    .border_style(Style::default().fg(theme::BORDER))
    .render(f, area);

    app.session_mgr.sessions[app.session_mgr.active]
        .ui
        .panel_area = Some(inner);

    let mut lines: Vec<Line> = Vec::new();

    for row in 0..ROW_COUNT {
        match row {
            ROW_GENERAL_HEADER => {
                lines.push(Line::from(Span::styled(
                    lc.tr("config-group-general"),
                    Style::default().fg(theme::SUCCESS).add_modifier(Modifier::BOLD),
                )));
            }
            ROW_SEPARATOR => {
                lines.push(Line::from(""));
            }
            ROW_OVERRIDES_HEADER => {
                lines.push(Line::from(Span::styled(
                    lc.tr("config-group-prompt-overrides"),
                    Style::default().fg(theme::SUCCESS).add_modifier(Modifier::BOLD),
                )));
            }
            ROW_AUTOCOMPACT => render_bool_row(&mut lines, panel, row, lc),
            ROW_THRESHOLD => render_text_row(
                &mut lines, panel, row, &panel.buf_threshold, panel.cur_threshold, lc,
            ),
            ROW_LANGUAGE => render_text_row(
                &mut lines, panel, row, &panel.buf_language, panel.cur_language, lc,
            ),
            ROW_PROACTIVENESS => render_select_row(&mut lines, panel, row, lc),
            ROW_PERSONA => render_text_row(
                &mut lines, panel, row, &panel.buf_persona, panel.cur_persona, lc,
            ),
            ROW_TONE => render_text_row(
                &mut lines, panel, row, &panel.buf_tone, panel.cur_tone, lc,
            ),
            _ => {}
        }
    }

    lines.truncate(inner.height as usize);
    f.render_widget(Paragraph::new(Text::from(lines)), inner);
}

fn is_active(panel: &ConfigPanel, row: usize) -> bool {
    panel.cursor == row
}

fn active_style() -> Style {
    Style::default()
        .fg(theme::THINKING)
        .add_modifier(Modifier::BOLD)
}

fn inactive_style() -> Style {
    Style::default().fg(theme::MUTED)
}

fn text_style() -> Style {
    Style::default().fg(theme::TEXT)
}

/// 渲染布尔行：Autocompact
fn render_bool_row(
    lines: &mut Vec<Line<'static>>,
    panel: &ConfigPanel,
    row: usize,
    lc: &crate::i18n::LcRegistry,
) {
    let active = is_active(panel, row);
    let label_style = if active { active_style() } else { inactive_style() };

    let on_off = if panel.buf_autocompact {
        vec![
            Span::styled(
                format!("[{}]", lc.tr("config-value-on")),
                active_style(),
            ),
            Span::styled(format!("  {}", lc.tr("config-value-off")), inactive_style()),
            Span::styled(
                format!("  \u{2190} {}", lc.tr("config-desc-autocompact")),
                inactive_style(),
            ),
        ]
    } else {
        vec![
            Span::styled(format!("{}  ", lc.tr("config-value-on")), inactive_style()),
            Span::styled(
                format!("[{}]", lc.tr("config-value-off")),
                active_style(),
            ),
            Span::styled(
                format!("  \u{2190} {}", lc.tr("config-desc-autocompact")),
                inactive_style(),
            ),
        ]
    };

    let mut spans = vec![
        Span::styled(
            format!("{:<16}", ConfigPanel::field_label(row)),
            label_style,
        ),
    ];
    spans.extend(on_off);
    lines.push(Line::from(spans));
}

/// 渲染三选一行：Proactiveness
fn render_select_row(
    lines: &mut Vec<Line<'static>>,
    panel: &ConfigPanel,
    row: usize,
    lc: &crate::i18n::LcRegistry,
) {
    let active = is_active(panel, row);
    let label_style = if active { active_style() } else { inactive_style() };
    let vals = ["low", "medium", "high"];
    let spans: Vec<Span> = vals
        .iter()
        .flat_map(|v| {
            let cur = panel.buf_proactiveness.as_str();
            if *v == cur {
                vec![
                    Span::styled(format!("[{}]", v), active_style()),
                    Span::styled("  ", Style::default()),
                ]
            } else {
                vec![
                    Span::styled(v.to_string(), inactive_style()),
                    Span::styled("  ", Style::default()),
                ]
            }
        })
        .collect();

    let mut line_spans = vec![
        Span::styled(
            format!("{:<16}", ConfigPanel::field_label(row)),
            label_style,
        ),
    ];
    line_spans.extend(spans);
    line_spans.push(Span::styled(
        format!("\u{2190} {}", lc.tr("config-desc-proactiveness")),
        inactive_style(),
    ));
    lines.push(Line::from(line_spans));
}

/// 渲染文本输入行（CompactThreshold / Language / Persona / Tone）
fn render_text_row(
    lines: &mut Vec<Line<'static>>,
    panel: &ConfigPanel,
    row: usize,
    buf: &str,
    cursor: usize,
    lc: &crate::i18n::LcRegistry,
) {
    let active = is_active(panel, row);
    let label_style = if active { active_style() } else { inactive_style() };
    let value_style = if active { active_style() } else { text_style() };

    let value_display = if active {
        let (before, after) = crate::app::edit_display_parts(buf, cursor);
        format!("{}█{}", before, after)
    } else if buf.is_empty() {
        "-".to_string()
    } else {
        buf.to_string()
    };

    let desc_key = match row {
        ROW_THRESHOLD => "config-desc-threshold",
        ROW_LANGUAGE => "config-desc-language",
        ROW_PERSONA => "config-desc-persona",
        ROW_TONE => "config-desc-tone",
        _ => "",
    };

    lines.push(Line::from(vec![
        Span::styled(
            format!("{:<16}", ConfigPanel::field_label(row)),
            label_style,
        ),
        Span::styled(value_display, value_style),
        Span::styled(
            format!("  \u{2190} {}", lc.tr(desc_key)),
            inactive_style(),
        ),
    ]));
}
```

- [ ] **Step 2: Remove old `render_text_field` helper and `ConfigEditField` imports**

The old `render_text_field` function and `ConfigEditField` import are replaced by the new `render_text_row` / `render_bool_row` / `render_select_row` functions.

---

### Task 4: Rewrite key handling for direct-edit mode

**Files:**
- Modify: `peri-tui/src/app/config_panel.rs`

- [ ] **Step 1: Replace `handle_key` in `impl PanelComponent for ConfigPanel`**

Replace lines 318-446 (the entire `handle_key` method):

```rust
    fn handle_key(&mut self, input: Input, ctx: &mut PanelContext<'_>) -> EventResult {
        use tui_textarea::Key;
        match input {
            Input { key: Key::Esc, .. } => EventResult::ClosePanel,
            Input { key: Key::Up, .. } => {
                self.cursor_up();
                EventResult::Consumed
            }
            Input { key: Key::Down, .. } => {
                self.cursor_down();
                EventResult::Consumed
            }
            Input {
                key: Key::Enter, ..
            } => {
                let Some(cfg) = ctx.services.peri_config.as_mut() else {
                    return EventResult::Consumed;
                };
                match self.apply_edit(cfg, &ctx.services.lc) {
                    Ok(()) => {
                        if let Some(ref lang) = cfg.config.language {
                            let _ = ctx.services.lc.switch(lang);
                        }
                        if let Err(e) = App::save_config(
                            cfg,
                            ctx.services.config_path_override.as_deref(),
                        ) {
                            ctx.session_mgr.sessions[ctx.session_mgr.active]
                                .messages
                                .push_system_note(ctx.services.lc.tr_args(
                                    "app-config-save-failed",
                                    &[("error".into(), e.to_string().into())],
                                ));
                        } else {
                            ctx.session_mgr.sessions[ctx.session_mgr.active]
                                .messages
                                .push_system_note(ctx.services.lc.tr("app-config-saved"));
                        }
                        EventResult::ClosePanel
                    }
                    Err(err_msg) => {
                        ctx.session_mgr.sessions[ctx.session_mgr.active]
                            .messages
                            .push_system_note(err_msg);
                        EventResult::Consumed
                    }
                }
            }
            // Space: toggle for boolean/select fields, input for text fields
            Input {
                key: Key::Char(' '),
                ctrl: false,
                ..
            } => {
                match self.cursor {
                    ROW_AUTOCOMPACT => self.cycle_autocompact(),
                    ROW_PROACTIVENESS => self.cycle_proactiveness(),
                    _ => self.input_char(' '),
                }
                EventResult::Consumed
            }
            // Left/Right: toggle for boolean/select, cursor move for text
            Input {
                key: Key::Left,
                ctrl: false,
                ..
            }
            | Input {
                key: Key::Right,
                ctrl: false,
                ..
            } => {
                match self.cursor {
                    ROW_AUTOCOMPACT => self.cycle_autocompact(),
                    ROW_PROACTIVENESS => self.cycle_proactiveness(),
                    _ => {
                        self.handle_text_key(input);
                    }
                }
                EventResult::Consumed
            }
            // All other keys go to text input if cursor is on a text field
            _ => {
                self.handle_text_key(input);
                EventResult::Consumed
            }
        }
    }
```

- [ ] **Step 2: Add helper methods to ConfigPanel**

Add these methods to `impl ConfigPanel`:

```rust
    /// Input a character into the active text field
    fn input_char(&mut self, c: char) {
        match self.cursor {
            ROW_THRESHOLD => {
                super::handle_edit_key(
                    &mut self.buf_threshold,
                    &mut self.cur_threshold,
                    Input { key: tui_textarea::Key::Char(c), ctrl: false, alt: false, shift: false },
                );
            }
            ROW_LANGUAGE => {
                super::handle_edit_key(
                    &mut self.buf_language,
                    &mut self.cur_language,
                    Input { key: tui_textarea::Key::Char(c), ctrl: false, alt: false, shift: false },
                );
            }
            ROW_PERSONA => {
                super::handle_edit_key(
                    &mut self.buf_persona,
                    &mut self.cur_persona,
                    Input { key: tui_textarea::Key::Char(c), ctrl: false, alt: false, shift: false },
                );
            }
            ROW_TONE => {
                super::handle_edit_key(
                    &mut self.buf_tone,
                    &mut self.cur_tone,
                    Input { key: tui_textarea::Key::Char(c), ctrl: false, alt: false, shift: false },
                );
            }
            _ => {}
        }
    }

    /// Route a key to the active text field
    fn handle_text_key(&mut self, input: Input) {
        match self.cursor {
            ROW_THRESHOLD => {
                super::handle_edit_key(&mut self.buf_threshold, &mut self.cur_threshold, input);
            }
            ROW_LANGUAGE => {
                super::handle_edit_key(&mut self.buf_language, &mut self.cur_language, input);
            }
            ROW_PERSONA => {
                super::handle_edit_key(&mut self.buf_persona, &mut self.cur_persona, input);
            }
            ROW_TONE => {
                super::handle_edit_key(&mut self.buf_tone, &mut self.cur_tone, input);
            }
            _ => {}
        }
    }
```

- [ ] **Step 3: Update `handle_paste`**

Replace `handle_paste` (lines 448-451):

```rust
    fn handle_paste(&mut self, text: &str, _ctx: &mut PanelContext<'_>) -> EventResult {
        let text: String = text.chars().filter(|&c| c != '\n' && c != '\r').collect();
        match self.cursor {
            ROW_THRESHOLD => {
                let buf = &mut self.buf_threshold;
                let cursor = &mut self.cur_threshold;
                let char_count = buf.chars().count();
                if *cursor > char_count { *cursor = char_count; }
                let byte_pos = buf.char_indices().nth(*cursor).map(|(i, _)| i).unwrap_or(buf.len());
                buf.insert_str(byte_pos, &text);
                *cursor += text.chars().count();
            }
            ROW_LANGUAGE => {
                let buf = &mut self.buf_language;
                let cursor = &mut self.cur_language;
                let char_count = buf.chars().count();
                if *cursor > char_count { *cursor = char_count; }
                let byte_pos = buf.char_indices().nth(*cursor).map(|(i, _)| i).unwrap_or(buf.len());
                buf.insert_str(byte_pos, &text);
                *cursor += text.chars().count();
            }
            ROW_PERSONA => {
                let buf = &mut self.buf_persona;
                let cursor = &mut self.cur_persona;
                let char_count = buf.chars().count();
                if *cursor > char_count { *cursor = char_count; }
                let byte_pos = buf.char_indices().nth(*cursor).map(|(i, _)| i).unwrap_or(buf.len());
                buf.insert_str(byte_pos, &text);
                *cursor += text.chars().count();
            }
            ROW_TONE => {
                let buf = &mut self.buf_tone;
                let cursor = &mut self.cur_tone;
                let char_count = buf.chars().count();
                if *cursor > char_count { *cursor = char_count; }
                let byte_pos = buf.char_indices().nth(*cursor).map(|(i, _)| i).unwrap_or(buf.len());
                buf.insert_str(byte_pos, &text);
                *cursor += text.chars().count();
            }
            _ => {}
        }
        EventResult::Consumed
    }
```

- [ ] **Step 4: Remove `handle_scroll` and `set_scroll_offset` methods**

Replace lines 453-466 with no-op implementations since there's no browse list to scroll:

```rust
    fn handle_scroll(&mut self, _lines: i16, _ctx: &mut PanelContext<'_>) -> EventResult {
        EventResult::NotConsumed
    }

    fn set_scroll_offset(&mut self, _offset: u16) {}
```

- [ ] **Step 5: Update `handle_mouse` for direct click-to-select**

Replace lines 468-490:

```rust
    fn handle_mouse(
        &mut self,
        mouse: ratatui::crossterm::event::MouseEvent,
        area: Rect,
        ctx: &mut PanelContext<'_>,
    ) -> EventResult {
        use ratatui::crossterm::event::{MouseButton, MouseEventKind};
        if mouse.kind == MouseEventKind::Down(MouseButton::Left) {
            let relative_y = mouse.row.saturating_sub(area.y);
            if relative_y >= 1 {
                let clicked = (relative_y - 1) as usize;
                if clicked < ROW_COUNT {
                    // Only set cursor on editable rows
                    if matches!(clicked, ROW_AUTOCOMPACT | ROW_THRESHOLD | ROW_LANGUAGE | ROW_PROACTIVENESS | ROW_PERSONA | ROW_TONE) {
                        self.cursor = clicked;
                        return EventResult::Consumed;
                    }
                }
            }
        }
        EventResult::NotConsumed
    }
```

- [ ] **Step 6: Update `desired_height`**

Replace lines 492-497:

```rust
    fn desired_height(&self, _screen_height: u16, _screen_width: u16) -> u16 {
        14
    }
```

- [ ] **Step 7: Update `status_bar_hints`**

Replace lines 511-531:

```rust
    fn status_bar_hints(&self, lc: &crate::i18n::LcRegistry) -> Vec<(String, String)> {
        vec![
            ("↑↓".to_string(), lc.tr("hint-config-field")),
            ("Space".to_string(), lc.tr("hint-config-toggle")),
            ("Enter".to_string(), lc.tr("hint-config-save")),
            ("Esc".to_string(), lc.tr("key-close")),
        ]
    }
```

- [ ] **Step 8: Remove unused imports from `config_panel.rs`**

Remove `use crate::config::PeriConfig;` from the render file (now imported differently). Keep `use crate::config::PeriConfig;` in `config_panel.rs` (still used by `from_config`).

---

### Task 5: Update tests

**Files:**
- Modify: `peri-tui/src/app/config_panel_test.rs`

- [ ] **Step 1: Rewrite test file for direct-edit mode**

Replace the entire file:

```rust
use super::*;

fn make_lc() -> crate::i18n::LcRegistry {
    crate::i18n::LcRegistry::default()
}

#[test]
fn test_config_panel_from_config_defaults() {
    let cfg = PeriConfig::default();
    let panel = ConfigPanel::from_config(&cfg);
    assert!(panel.buf_autocompact);
    assert_eq!(panel.buf_threshold, "85");
    assert!(panel.buf_language.is_empty());
    assert_eq!(panel.buf_proactiveness, "medium");
    // 光标应在第一个可编辑行
    assert_eq!(panel.cursor, ROW_AUTOCOMPACT);
}

#[test]
fn test_config_panel_cursor_navigation() {
    let mut panel = ConfigPanel::from_config(&PeriConfig::default());
    // 从 Autocompact 向下
    panel.cursor_down();
    assert_eq!(panel.cursor, ROW_THRESHOLD);
    panel.cursor_down();
    assert_eq!(panel.cursor, ROW_LANGUAGE);
    panel.cursor_down();
    assert_eq!(panel.cursor, ROW_PROACTIVENESS);
    panel.cursor_down();
    assert_eq!(panel.cursor, ROW_PERSONA);
    panel.cursor_down();
    assert_eq!(panel.cursor, ROW_TONE);
    // 到底后循环回顶
    panel.cursor_down();
    assert_eq!(panel.cursor, ROW_AUTOCOMPACT);
    // 反向
    panel.cursor_up();
    assert_eq!(panel.cursor, ROW_TONE);
}

#[test]
fn test_config_panel_cursor_skips_headers() {
    let mut panel = ConfigPanel::from_config(&PeriConfig::default());
    // 手动设到 separator 行
    panel.cursor = ROW_SEPARATOR;
    panel.cursor_down();
    assert_eq!(panel.cursor, ROW_PERSONA); // 跳到下一个可编辑行
    panel.cursor_up();
    assert_eq!(panel.cursor, ROW_PROACTIVENESS); // 跳到上一个可编辑行
}

#[test]
fn test_config_panel_cycle_autocompact() {
    let mut panel = ConfigPanel::from_config(&PeriConfig::default());
    assert!(panel.buf_autocompact);
    panel.cycle_autocompact();
    assert!(!panel.buf_autocompact);
    panel.cycle_autocompact();
    assert!(panel.buf_autocompact);
}

#[test]
fn test_config_panel_cycle_proactiveness() {
    let mut panel = ConfigPanel::from_config(&PeriConfig::default());
    panel.buf_proactiveness = "low".to_string();
    panel.cycle_proactiveness();
    assert_eq!(panel.buf_proactiveness, "medium");
    panel.cycle_proactiveness();
    assert_eq!(panel.buf_proactiveness, "high");
    panel.cycle_proactiveness();
    assert_eq!(panel.buf_proactiveness, "low");
}

#[test]
fn test_config_panel_apply_edit_saves_to_config() {
    let lc = make_lc();
    let mut cfg = PeriConfig::default();
    let mut panel = ConfigPanel::from_config(&cfg);
    panel.buf_language = "zh-CN".to_string();
    panel.buf_persona = "Rust expert".to_string();
    panel.buf_tone = "concise".to_string();
    panel.buf_proactiveness = "high".to_string();
    panel.apply_edit(&mut cfg, &lc).unwrap();
    assert_eq!(cfg.config.language.as_deref(), Some("zh-CN"));
    assert_eq!(cfg.config.persona.as_deref(), Some("Rust expert"));
    assert_eq!(cfg.config.tone.as_deref(), Some("concise"));
    assert_eq!(cfg.config.proactiveness.as_deref(), Some("high"));
}

#[test]
fn test_config_panel_apply_edit_compact_threshold() {
    let lc = make_lc();
    let mut cfg = PeriConfig::default();
    let mut panel = ConfigPanel::from_config(&cfg);
    panel.buf_threshold = "90".to_string();
    panel.apply_edit(&mut cfg, &lc).unwrap();
    let compact = cfg.config.compact.unwrap();
    assert!((compact.auto_compact_threshold - 0.90).abs() < 0.001);
}

#[test]
fn test_config_panel_apply_edit_invalid_threshold_clamps() {
    let lc = make_lc();
    let mut cfg = PeriConfig::default();
    let mut panel = ConfigPanel::from_config(&cfg);
    panel.buf_threshold = "30".to_string();
    panel.apply_edit(&mut cfg, &lc).unwrap();
    let compact = cfg.config.compact.unwrap();
    assert!((compact.auto_compact_threshold - 0.50).abs() < 0.001);
}

#[test]
fn test_config_panel_apply_edit_language_validation_valid() {
    let lc = make_lc();
    let mut cfg = PeriConfig::default();
    let mut panel = ConfigPanel::from_config(&cfg);
    panel.buf_language = "en".to_string();
    assert!(panel.apply_edit(&mut cfg, &lc).is_ok());
    assert_eq!(cfg.config.language.as_deref(), Some("en"));

    let mut panel = ConfigPanel::from_config(&PeriConfig::default());
    panel.buf_language = "zh-CN".to_string();
    assert!(panel.apply_edit(&mut cfg, &lc).is_ok());
    assert_eq!(cfg.config.language.as_deref(), Some("zh-CN"));
}

#[test]
fn test_config_panel_apply_edit_language_validation_empty() {
    let lc = make_lc();
    let mut cfg = PeriConfig::default();
    let mut panel = ConfigPanel::from_config(&cfg);
    panel.buf_language = String::new();
    assert!(panel.apply_edit(&mut cfg, &lc).is_ok());
    assert_eq!(cfg.config.language, None);

    let mut panel = ConfigPanel::from_config(&PeriConfig::default());
    panel.buf_language = "auto".to_string();
    assert!(panel.apply_edit(&mut cfg, &lc).is_ok());
    assert_eq!(cfg.config.language, None);
}

#[test]
fn test_config_panel_apply_edit_language_validation_invalid() {
    let lc = make_lc();
    let mut cfg = PeriConfig::default();
    let mut panel = ConfigPanel::from_config(&cfg);
    panel.buf_language = "fr".to_string();
    let result = panel.apply_edit(&mut cfg, &lc);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.contains("Unsupported language"), "错误消息应包含 'Unsupported language': {}", err);
    assert!(err.contains("fr"), "错误消息应包含无效语言: {}", err);
    assert_eq!(cfg.config.language, None);
}
```

- [ ] **Step 2: Remove old tests that depend on Browse mode**

Removed tests:
- `test_config_panel_field_navigation` — relied on `ConfigEditField` enum and `field_count()`
- `test_config_panel_active_field_text_editable` — relied on `active_field()` and `ConfigEditField`
- `test_config_panel_field_display_language` — `field_display_value()` is dead code

---

### Task 6: Update panel_config.rs (if needed)

**Files:**
- Verify: `peri-tui/src/app/panel_config.rs`

- [ ] **Step 1: Verify no changes needed to `open_config_panel`**

Read `peri-tui/src/app/panel_config.rs` and confirm `open_config_panel()` still works — it creates `ConfigPanel::from_config(cfg)` and wraps it. No changes needed since `from_config` signature is unchanged.

- [ ] **Step 2: Verify `config_panel_apply` is no longer called from key handler**

The old `handle_key` called `apply_edit` directly (not via `config_panel_apply`). The new `handle_key` also calls `apply_edit` directly. The `config_panel_apply` method on `App` is a separate public API that may be called elsewhere — leave it unchanged.

---

### Task 7: Build, test, and verify

**Files:**
- None (verification only)

- [ ] **Step 1: Build the crate**

```bash
cargo build -p peri-tui 2>&1 | tail -30
```
Expected: Compilation succeeds with zero errors.

- [ ] **Step 2: Run config panel tests**

```bash
cargo test -p peri-tui --lib config_panel_test 2>&1
```
Expected: All 9 tests pass.

- [ ] **Step 3: Run linter**

```bash
cargo clippy -p peri-tui -- -D warnings 2>&1 | tail -20
```
Expected: Zero warnings.

- [ ] **Step 4: Run full test suite**

```bash
cargo test -p peri-tui --lib 2>&1 | tail -20
```
Expected: All tests pass.

- [ ] **Step 5: Commit**

```bash
git add peri-tui/src/app/config_panel.rs \
        peri-tui/src/app/config_panel_test.rs \
        peri-tui/src/ui/main_ui/panels/config.rs \
        peri-tui/locales/en/main.ftl \
        peri-tui/locales/zh-CN/main.ftl
git commit -m "refactor(tui): redesign /config panel for direct-edit single mode

Remove Browse/Edit two-step mode. All fields directly editable on open.
Fields grouped into 'General' and 'Prompt Overrides' with descriptions.
Space toggles boolean/select fields, text fields accept keyboard input.
Enter saves all and closes; Esc discards and closes. Up/Down navigates.

This follows the same interaction pattern as /model panel.

Co-Authored-By: deepseek-v4-pro <deepseek-ai@claude-code-best.win>"
```
