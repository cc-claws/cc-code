//! CLI interaction: checkbox list, progress bar, confirm prompt
//! Uses crossterm re-exported via ratatui (no extra dependency)

use std::io::{self, Write};

use anyhow::Result;
use ratatui::crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    style::{Color, Print, ResetColor, SetForegroundColor},
    terminal::{self, Clear, ClearType},
};

use crate::sync::protocol::SyncItems;

pub struct SelectableItem {
    pub key: &'static str,
    pub label: &'static str,
    pub detail: String,
    pub selected: bool,
}

/// Build default item list (receiver-side presets)
pub fn build_default_items() -> Vec<SelectableItem> {
    vec![
        SelectableItem {
            key: "settings",
            label: "Settings",
            detail: ".peri/settings.json + .claude/settings.json".into(),
            selected: true,
        },
        SelectableItem {
            key: "skills",
            label: "Skills",
            detail: "~/.claude/skills/".into(),
            selected: true,
        },
        SelectableItem {
            key: "mcp",
            label: "MCP Config",
            detail: "~/.mcp.json + project .mcp.json".into(),
            selected: true,
        },
        SelectableItem {
            key: "plugins",
            label: "Plugins",
            detail: "~/.claude/plugins/cache/".into(),
            selected: false,
        },
    ]
}

/// Interactive checkbox list. ↑↓ navigate, Space toggle, Enter confirm.
pub fn select_sync_items(items: &mut [SelectableItem]) -> Result<SyncItems> {
    let mut stdout = io::stdout();
    terminal::enable_raw_mode()?;
    execute!(stdout, cursor::Hide)?;

    let mut cursor_pos: usize = 0;
    let result: Result<SyncItems>;

    loop {
        execute!(stdout, Clear(ClearType::All), cursor::MoveTo(0, 0))?;
        for (i, item) in items.iter().enumerate() {
            if i == cursor_pos {
                execute!(stdout, SetForegroundColor(Color::Cyan))?;
            }
            let check = if item.selected { "[x]" } else { "[ ]" };
            execute!(
                stdout,
                Print(format!("{} {} - {}\r\n", check, item.label, item.detail))
            )?;
            if i == cursor_pos {
                execute!(stdout, ResetColor)?;
            }
        }
        execute!(
            stdout,
            Print("\r\nNavigate: \u{2191}\u{2193}  Toggle: Space  Confirm: Enter\r\n")
        )?;
        stdout.flush()?;

        let event = event::read()?;
        match event {
            Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
                KeyCode::Up => {
                    cursor_pos = cursor_pos.saturating_sub(1);
                }
                KeyCode::Down => {
                    cursor_pos = (cursor_pos + 1).min(items.len().saturating_sub(1));
                }
                KeyCode::Char(' ') => {
                    items[cursor_pos].selected = !items[cursor_pos].selected;
                }
                KeyCode::Enter => {
                    let sync_items = SyncItems {
                        settings: items
                            .iter()
                            .find(|i| i.key == "settings")
                            .filter(|i| i.selected)
                            .map(|_| Default::default()),
                        skills: items
                            .iter()
                            .find(|i| i.key == "skills")
                            .filter(|i| i.selected)
                            .map(|_| Default::default()),
                        mcp: items
                            .iter()
                            .find(|i| i.key == "mcp")
                            .filter(|i| i.selected)
                            .map(|_| Default::default()),
                        plugins: items
                            .iter()
                            .find(|i| i.key == "plugins")
                            .filter(|i| i.selected)
                            .map(|_| Default::default()),
                    };
                    result = Ok(sync_items);
                    break;
                }
                KeyCode::Esc => {
                    result = Err(anyhow::anyhow!("Cancelled by user"));
                    break;
                }
                _ => {}
            },
            _ => {}
        }
    }

    terminal::disable_raw_mode()?;
    execute!(stdout, cursor::Show)?;
    result
}

/// Confirm prompt: show summary, wait for y/N
pub fn confirm_sync(items: &SyncItems) -> Result<bool> {
    let mut count = 0;
    let mut details = Vec::new();

    if items.settings.is_some() {
        count += 1;
        details.push("  Settings (.peri/settings.json + .claude/settings.json)");
    }
    if items.skills.is_some() {
        count += 1;
        details.push("  Skills (~/.claude/skills/)");
    }
    if items.mcp.is_some() {
        count += 1;
        details.push("  MCP Config (~/.mcp.json)");
    }
    if items.plugins.is_some() {
        count += 1;
        details.push("  Plugins (~/.claude/plugins/)");
    }

    println!("{} item(s) to sync:", count);
    for d in &details {
        println!("{d}");
    }
    print!("\nConfirm? [y/N]: ");
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let trimmed = input.trim().to_lowercase();

    Ok(trimmed == "y" || trimmed == "yes")
}

/// Simple progress bar
pub struct ProgressBar {
    total: u64,
    label: &'static str,
}

impl ProgressBar {
    pub fn new(total: u64, label: &'static str) -> Self {
        Self { total, label }
    }

    pub fn update(&self, current: u64) {
        let pct = current
            .checked_mul(100)
            .and_then(|x| x.checked_div(self.total))
            .unwrap_or(100) as usize;
        let pct = pct.min(100);
        let filled = pct * 20 / 100;
        let empty = 20 - filled;
        print!(
            "\r{}: [{}{}] {}%",
            self.label,
            "█".repeat(filled),
            "░".repeat(empty),
            pct
        );
        let _ = io::stdout().flush();
    }

    pub fn finish(&self) {
        println!();
    }
}

/// Clear current line and print message
pub fn println_overwrite(s: &str) {
    print!("\r{}\r\n", s);
    let _ = io::stdout().flush();
}
