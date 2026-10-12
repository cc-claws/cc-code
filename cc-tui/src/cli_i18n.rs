//! CLI 帮助在 clap 解析前使用与 TUI 相同的语言设置。

use cc_tui::i18n::LcRegistry;
use clap::Command;
use std::io::Write;

pub fn localized_command(mut command: Command, lc: &LcRegistry) -> Command {
    let command_name = command.get_name().to_string();
    let about = lc.tr(&format!("cli-command-{}", command.get_name()));
    command = command.about(about.clone()).long_about(about);
    for id in [
        "yolo",
        "approve",
        "print",
        "output_format",
        "max_turns",
        "bare",
        "permission_mode",
        "skip_permissions",
        "model",
        "effort",
        "cont",
        "resume",
        "session_id",
        "session_name",
        "no_session_persistence",
        "allowed_tools",
        "disallowed_tools",
        "settings",
        "cwd",
        "agent",
        "server",
        "json",
        "plugin",
        "scope",
    ] {
        if command.get_arguments().any(|arg| arg.get_id() == id) {
            command = command.mut_arg(id, |arg| {
                let key = if id == "scope" && command_name == "uninstall" {
                    "cli-help-uninstall-scope".to_string()
                } else {
                    format!("cli-help-{}", id.replace('_', "-"))
                };
                let help = lc.tr(&key);
                arg.help(help.clone()).long_help(help)
            });
        }
    }
    for name in [
        "acp",
        "update",
        "sync",
        "plugin",
        "sender",
        "receiver",
        "list",
        "install",
        "uninstall",
    ] {
        if command.get_subcommands().any(|sub| sub.get_name() == name) {
            command = command.mut_subcommand(name, |sub| localized_command(sub, lc));
        }
    }
    command
}

pub fn message(key: &str, args: &[(&str, &str)]) -> String {
    let args: Vec<_> = args
        .iter()
        .map(|(key, value)| ((*key).into(), (*value).into()))
        .collect();
    cc_tui::i18n::global().tr_args(key, &args)
}

pub fn write_stderr(message: &str) {
    let _ = writeln!(std::io::stderr().lock(), "{message}");
}

#[cfg(test)]
#[path = "cli_i18n_test.rs"]
mod tests;
