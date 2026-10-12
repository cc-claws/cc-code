use std::str::FromStr;

// ─── OutputFormat ─────────────────────────────────────────────────────────

/// 输出格式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OutputFormat {
    #[default]
    Text,
    Json,
    StreamJson,
}

impl FromStr for OutputFormat {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse_with_language(s, &cc_tui::i18n::global())
    }
}

impl OutputFormat {
    fn parse_with_language(s: &str, lc: &cc_tui::i18n::LcRegistry) -> Result<Self, String> {
        match s {
            "text" => Ok(OutputFormat::Text),
            "json" => Ok(OutputFormat::Json),
            "stream-json" => Ok(OutputFormat::StreamJson),
            _ => Err(lc.tr_args("cli-output-format-error", &[("value".into(), s.into())])),
        }
    }
}

// ─── PluginScope ──────────────────────────────────────────────────────────

/// 插件安装范围
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PluginScope {
    #[default]
    User,
    Project,
    Local,
}

impl FromStr for PluginScope {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse_with_language(s, &cc_tui::i18n::global())
    }
}

impl PluginScope {
    fn parse_with_language(s: &str, lc: &cc_tui::i18n::LcRegistry) -> Result<Self, String> {
        match s {
            "user" => Ok(PluginScope::User),
            "project" => Ok(PluginScope::Project),
            "local" => Ok(PluginScope::Local),
            _ => Err(lc.tr_args("cli-plugin-scope-error", &[("value".into(), s.into())])),
        }
    }
}

impl From<PluginScope> for cc_middlewares::plugin::InstallScope {
    fn from(scope: PluginScope) -> Self {
        match scope {
            PluginScope::User => cc_middlewares::plugin::InstallScope::User,
            PluginScope::Project => cc_middlewares::plugin::InstallScope::Project,
            PluginScope::Local => cc_middlewares::plugin::InstallScope::Local,
        }
    }
}

// ─── 测试 ─────────────────────────────────────────────────────────────────

#[cfg(test)]
#[path = "cli_args_test.rs"]
mod tests;
