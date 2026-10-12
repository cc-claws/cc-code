use crate::{
    app::{App, MessageViewModel},
    command::Command,
};

pub struct HelpCommand;

impl Command for HelpCommand {
    fn name(&self) -> &str {
        "help"
    }

    fn description(&self, _lc: &crate::i18n::LcRegistry) -> String {
        _lc.tr("command-help-description")
    }

    fn execute(&self, app: &mut App, _args: &str) {
        // 使用 dispatch 时刷新的列表（command_registry 已被 std::mem::take 取出）。
        let lc = &app.services.lc;
        let mut lines = vec![lc.tr("help-available-commands")];
        for (name, desc, aliases) in &app.session_mgr.current_mut().commands.command_help_list {
            let alias_str = if aliases.is_empty() {
                String::new()
            } else {
                format!(
                    " {}",
                    lc.tr_args(
                        "help-alias-prefix",
                        &[("aliases".into(), aliases.join(", /").into())]
                    )
                )
            };
            lines.push(format!("  /{:<10} {}{}", name, desc, alias_str));
        }

        // Skills 说明
        let skills_count = app.session_mgr.current_mut().commands.skills.len();
        if skills_count > 0 {
            lines.push("".to_string());
            lines.push(lc.tr_args(
                "help-skills-count",
                &[("count".into(), skills_count.into())],
            ));
        } else {
            lines.push("".to_string());
            lines.push(lc.tr("help-skills-empty"));
        }

        // 全局快捷键提示
        lines.push("".to_string());
        lines.push(lc.tr_args(
            "help-shortcuts",
            &[(
                "palette".into(),
                crate::event::keyboard::command_palette_label().into(),
            )],
        ));

        let vm = MessageViewModel::system(lines.join("\n"));
        app.session_mgr
            .current_mut()
            .messages
            .view_messages
            .push(vm);
        app.render_rebuild();
    }
}
