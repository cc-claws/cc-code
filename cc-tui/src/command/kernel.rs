use cc_agent::kernel::Kernel;

use crate::{app::App, command::Command, ui::message_view::MessageViewModel};

/// `/kernel` —— 运行时在 cc / dsh / pi 三个内核之间切换。
pub struct KernelCommand;

const USAGE: &str = "用法：/kernel [cc|dsh|pi] | /kernel show";

impl KernelCommand {
    fn push_notice(app: &mut App, text: String) {
        app.session_mgr
            .current_mut()
            .messages
            .view_messages
            .push(MessageViewModel::system(text));
        app.render_rebuild();
    }

    fn current_kernel(app: &App) -> Kernel {
        let id = app
            .services
            .peri_config
            .as_ref()
            .and_then(|c| c.config.agent_kernel.as_deref())
            .unwrap_or("cc");
        Kernel::from_config(Some(id))
    }

    fn show(&self, app: &mut App) {
        let kernel = Self::current_kernel(app);
        let model = app
            .services
            .peri_config
            .as_ref()
            .map(|c| c.config.active_alias.clone())
            .unwrap_or_default();
        let prompt = kernel.prompt_for(&model);
        // 与 builder.rs 中实际拼进请求的 Git Attribution 保持同一来源，
        // 否则 /kernel show 的输出会与真实请求分叉。
        let attribution = cc_middlewares::GitAttributionMiddleware::attribution_text(&model);
        let kernel_prompt = if prompt.is_empty() {
            "(cc 内核使用内置系统提示词，无附加内核段落)"
        } else {
            prompt
        };
        Self::push_notice(
            app,
            format!(
                "当前 Agent 内核: {}\n可用内核: cc / dsh / pi\n\n## Kernel Prompt\n{}\n\n## Git Attribution\n{}",
                kernel.id(),
                kernel_prompt,
                attribution
            ),
        );
    }
}

impl Command for KernelCommand {
    fn name(&self) -> &str {
        "kernel"
    }

    fn description(&self, _lc: &crate::i18n::LcRegistry) -> String {
        "Switch the agent kernel (cc / dsh / pi)".to_string()
    }

    fn execute(&self, app: &mut App, args: &str) {
        let arg = args.trim().to_lowercase();
        match arg.as_str() {
            "cc" | "dsh" | "pi" => {
                let cfg = app
                    .services
                    .peri_config
                    .get_or_insert_with(Default::default);
                cfg.config.agent_kernel = Some(arg.clone());
                if let Err(e) = App::save_config(cfg, app.services.config_path_override.as_deref())
                {
                    Self::push_notice(app, format!("保存配置失败: {e}"));
                    return;
                }
                if let Some(ref acp_client) = app.acp_client {
                    let acp = acp_client.clone();
                    let val = arg.clone();
                    tokio::spawn(async move {
                        let _ = acp.set_config_option("agent_kernel", &val).await;
                    });
                }
                Self::push_notice(
                    app,
                    format!(
                        "已切换到 {} 内核。历史消息中的工具调用记录保留，但新内核的工具语义可能与历史不同。",
                        arg
                    ),
                );
            }
            "show" => self.show(app),
            "" => {
                let kernel = Self::current_kernel(app);
                Self::push_notice(
                    app,
                    format!(
                        "当前 Agent 内核: {}\n可用内核: cc / dsh / pi\n{}",
                        kernel.id(),
                        USAGE
                    ),
                );
            }
            _ => Self::push_notice(
                app,
                format!("未知内核: {}。可用内核: cc / dsh / pi\n{}", arg, USAGE),
            ),
        }
    }
}
