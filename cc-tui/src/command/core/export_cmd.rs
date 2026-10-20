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

    fn description(&self, lc: &crate::i18n::LcRegistry) -> String {
        lc.tr("command-export-description")
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
                let note = app.services.lc.tr_args(
                    "export-cwd-error",
                    &[("error".into(), e.to_string().into())],
                );
                push_note(app, note);
                return;
            }
        };

        if args == "--clipboard" || args == "-c" {
            let content = render_messages(messages, ExportFormat::PlainText);
            match crate::clipboard::copy::copy_to_clipboard(&content) {
                Ok(_) => {
                    let note = app.services.lc.tr_args(
                        "statusbar-copied",
                        &[("count".into(), content.len().to_string().into())],
                    );
                    push_note(app, note);
                }
                Err(e) => {
                    let note = app.services.lc.tr_args(
                        "export-clipboard-error",
                        &[("error".into(), e.to_string().into())],
                    );
                    push_note(app, note);
                }
            }
        } else if !args.is_empty() {
            let format = infer_format_from_filename(args);
            let path = cwd.join(args);
            let content = render_messages(messages, format);
            match std::fs::write(&path, &content) {
                Ok(_) => {
                    let note = app.services.lc.tr_args(
                        "export-written",
                        &[("path".into(), path.display().to_string().into())],
                    );
                    push_note(app, note);
                }
                Err(e) => {
                    let note = app.services.lc.tr_args(
                        "export-write-error",
                        &[("error".into(), e.to_string().into())],
                    );
                    push_note(app, note);
                }
            }
        } else {
            let format = ExportFormat::Markdown;
            let filename = generate_default_filename(messages, format);
            let path = cwd.join(&filename);
            let content = render_messages(messages, format);
            match std::fs::write(&path, &content) {
                Ok(_) => {
                    let note = app.services.lc.tr_args(
                        "export-written",
                        &[("path".into(), path.display().to_string().into())],
                    );
                    push_note(app, note);
                }
                Err(e) => {
                    let note = app.services.lc.tr_args(
                        "export-write-error",
                        &[("error".into(), e.to_string().into())],
                    );
                    push_note(app, note);
                }
            }
        }
    }
}

fn push_note(app: &mut App, msg: String) {
    app.active_mut().messages.push_system_note(msg);
    app.render_rebuild();
}
