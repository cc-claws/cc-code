//! `/export` 命令 — 导出对话到文件或剪贴板。
//!
//! 用法：
//!   /export              → 导出为 Markdown 到 cwd
//!   /export report.md    → 直接导出为指定文件
//!   /export --clipboard  → 复制纯文本到剪贴板
//!   /export -c           → 同上

use crate::app::App;
use crate::command::Command;

use super::super::super::export::{
    filename::{generate_default_filename, infer_format_from_filename},
    renderer::{render_messages, ExportFormat},
};

pub struct ExportCommand;

impl Command for ExportCommand {
    fn name(&self) -> &str {
        "export"
    }

    fn description(&self, _lc: &crate::i18n::LcRegistry) -> String {
        "Export conversation to file or clipboard".to_string()
    }

    fn aliases(&self) -> Vec<&str> {
        vec!["save"]
    }

    fn execute(&self, app: &mut App, args: &str) {
        let args = args.trim();
        let session = app.active();
        let messages = &session.agent.origin_messages;

        let cwd = match std::env::current_dir() {
            Ok(p) => p,
            Err(e) => {
                push_note(app, format!("Cannot get cwd: {e}"));
                return;
            }
        };

        if args == "--clipboard" || args == "-c" {
            let content = render_messages(messages, ExportFormat::PlainText);
            match crate::clipboard::copy::copy_to_clipboard(&content) {
                Ok(_) => push_note(app, format!("Copied {} chars to clipboard", content.len())),
                Err(e) => push_note(app, format!("Clipboard error: {e}")),
            }
        } else if !args.is_empty() {
            let format = infer_format_from_filename(args);
            let path = cwd.join(args);
            let content = render_messages(messages, format);
            match std::fs::write(&path, &content) {
                Ok(_) => push_note(app, format!("Exported to: {}", path.display())),
                Err(e) => push_note(app, format!("Write error: {e}")),
            }
        } else {
            let format = ExportFormat::Markdown;
            let filename = generate_default_filename(messages, format);
            let path = cwd.join(&filename);
            let content = render_messages(messages, format);
            match std::fs::write(&path, &content) {
                Ok(_) => push_note(app, format!("Exported to: {}", path.display())),
                Err(e) => push_note(app, format!("Write error: {e}")),
            }
        }
    }
}

fn push_note(app: &mut App, msg: String) {
    app.active_mut().messages.push_system_note(msg);
    app.render_rebuild();
}
