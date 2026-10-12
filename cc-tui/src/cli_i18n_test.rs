use super::*;
use crate::Cli;
use clap::CommandFactory;

#[test]
fn test_cli_help_uses_selected_language() {
    for (language, expected) in [
        ("en", "Print the response and exit"),
        ("zh-CN", "输出响应后退出"),
    ] {
        let mut command = localized_command(Cli::command(), &LcRegistry::new(Some(language)));
        let help = command.render_long_help().to_string();
        assert!(help.contains(expected), "CLI 帮助应使用所选语言");
        assert!(help.contains("--output-format"));
        assert!(!help.contains("cli-help-"), "帮助不应泄露翻译键");
        if language == "en" {
            assert!(
                !help
                    .chars()
                    .any(|character| ('一'..='鿿').contains(&character)),
                "英文长帮助不能残留中文文档"
            );
        }
    }
}

#[test]
fn test_cli_subcommand_help_uses_selected_language() {
    for (language, expected) in [("en", "Install a plugin"), ("zh-CN", "安装插件")] {
        let mut command = localized_command(Cli::command(), &LcRegistry::new(Some(language)));
        let plugin = command
            .find_subcommand_mut("plugin")
            .expect("plugin 子命令存在");
        let help = plugin.render_long_help().to_string();
        assert!(help.contains(expected), "子命令帮助应使用所选语言");
        assert!(!help.contains("cli-command-"));
    }
}

#[test]
fn test_cli_nested_help_preserves_uninstall_scope_hint() {
    for (language, expected) in [
        ("en", "omit to remove from all scopes"),
        ("zh-CN", "不指定则从所有范围移除"),
    ] {
        let mut command = localized_command(Cli::command(), &LcRegistry::new(Some(language)));
        let uninstall = command
            .find_subcommand_mut("plugin")
            .expect("plugin 存在")
            .find_subcommand_mut("uninstall")
            .expect("uninstall 存在");
        let help = uninstall.render_long_help().to_string();
        assert!(help.contains(expected), "嵌套帮助必须保留参数默认语义");
        assert!(!help.contains("cli-help-"));
    }
}
