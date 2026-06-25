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
        match s {
            "text" => Ok(OutputFormat::Text),
            "json" => Ok(OutputFormat::Json),
            "stream-json" => Ok(OutputFormat::StreamJson),
            _ => Err(format!(
                "未知的输出格式: '{}'（可选值: text, json, stream-json）",
                s
            )),
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
        match s {
            "user" => Ok(PluginScope::User),
            "project" => Ok(PluginScope::Project),
            "local" => Ok(PluginScope::Local),
            _ => Err(format!(
                "未知的插件范围: '{}'（可选值: user, project, local）",
                s
            )),
        }
    }
}

impl From<PluginScope> for peri_middlewares::plugin::InstallScope {
    fn from(scope: PluginScope) -> Self {
        match scope {
            PluginScope::User => peri_middlewares::plugin::InstallScope::User,
            PluginScope::Project => peri_middlewares::plugin::InstallScope::Project,
            PluginScope::Local => peri_middlewares::plugin::InstallScope::Local,
        }
    }
}

// ─── 测试 ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_output_format_parse() {
        // 合法值
        assert_eq!("text".parse::<OutputFormat>().unwrap(), OutputFormat::Text);
        assert_eq!("json".parse::<OutputFormat>().unwrap(), OutputFormat::Json);
        assert_eq!(
            "stream-json".parse::<OutputFormat>().unwrap(),
            OutputFormat::StreamJson
        );
        // 非法值返回中文错误
        let err = "xml".parse::<OutputFormat>().unwrap_err();
        assert!(err.contains("未知的输出格式"), "错误消息应包含中文提示");
        assert!(err.contains("xml"));
    }

    #[test]
    fn test_plugin_scope_parse() {
        assert_eq!("user".parse::<PluginScope>().unwrap(), PluginScope::User);
        assert_eq!(
            "project".parse::<PluginScope>().unwrap(),
            PluginScope::Project
        );
        assert_eq!("local".parse::<PluginScope>().unwrap(), PluginScope::Local);
        // 非法值
        let err = "global".parse::<PluginScope>().unwrap_err();
        assert!(err.contains("未知的插件范围"));

        // From<PluginScope> for InstallScope 转换
        assert_eq!(
            peri_middlewares::plugin::InstallScope::from(PluginScope::User),
            peri_middlewares::plugin::InstallScope::User
        );
        assert_eq!(
            peri_middlewares::plugin::InstallScope::from(PluginScope::Project),
            peri_middlewares::plugin::InstallScope::Project
        );
        assert_eq!(
            peri_middlewares::plugin::InstallScope::from(PluginScope::Local),
            peri_middlewares::plugin::InstallScope::Local
        );
    }
}
