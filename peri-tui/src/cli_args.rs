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

// ─── EffortLevel ──────────────────────────────────────────────────────────

/// 推理强度等级
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EffortLevel {
    Low,
    #[default]
    Medium,
    High,
    Max,
}

impl FromStr for EffortLevel {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "low" => Ok(EffortLevel::Low),
            "medium" => Ok(EffortLevel::Medium),
            "high" => Ok(EffortLevel::High),
            "max" => Ok(EffortLevel::Max),
            _ => Err(format!(
                "未知的推理强度: '{}'（可选值: low, medium, high, max）",
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

// ─── RunOptions ───────────────────────────────────────────────────────────

/// 运行时选项（非 print 模式 / print 模式共用）
pub struct RunOptions {
    pub permission_mode: Option<String>,
    pub skip_permissions: bool,
    pub model: Option<String>,
    pub effort: Option<EffortLevel>,
    pub resume_session: Option<String>,
    pub continue_session: bool,
    pub session_id: Option<String>,
    pub session_name: Option<String>,
    pub no_session_persistence: bool,
    pub allowed_tools: Vec<String>,
    pub disallowed_tools: Vec<String>,
    pub max_turns: Option<u32>,
    pub bare: bool,
    pub settings: Option<String>,
    pub print_mode: Option<String>,
    pub output_format: OutputFormat,
}

// ─── validate_args ────────────────────────────────────────────────────────

/// 校验 RunOptions，返回警告消息列表
pub fn validate_args(opts: &RunOptions, is_print_mode: bool) -> Vec<String> {
    let mut warnings = Vec::new();

    if !is_print_mode {
        // 非 print 模式下检查仅限 print 模式的参数
        if opts.output_format != OutputFormat::Text {
            warnings.push("output_format 仅在 print 模式下有效".to_string());
        }
        if opts.max_turns.is_some() {
            warnings.push("max_turns 仅在 print 模式下有效".to_string());
        }
        if opts.bare {
            warnings.push("bare 仅在 print 模式下有效".to_string());
        }
        if opts.no_session_persistence {
            warnings.push("no_session_persistence 仅在 print 模式下有效".to_string());
        }
    }

    // 互斥检查（所有模式通用）
    if opts.skip_permissions && opts.permission_mode.is_some() {
        warnings.push("skip_permissions 与 permission_mode 不应同时指定".to_string());
    }
    if opts.continue_session && opts.resume_session.is_some() {
        warnings.push("continue_session 与 resume_session 不应同时指定".to_string());
    }
    if !opts.allowed_tools.is_empty() && !opts.disallowed_tools.is_empty() {
        warnings.push("allowed_tools 与 disallowed_tools 不应同时指定".to_string());
    }

    warnings
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
    fn test_effort_level_parse() {
        assert_eq!("low".parse::<EffortLevel>().unwrap(), EffortLevel::Low);
        assert_eq!(
            "medium".parse::<EffortLevel>().unwrap(),
            EffortLevel::Medium
        );
        assert_eq!("high".parse::<EffortLevel>().unwrap(), EffortLevel::High);
        assert_eq!("max".parse::<EffortLevel>().unwrap(), EffortLevel::Max);
        // 非法值
        let err = "ultra".parse::<EffortLevel>().unwrap_err();
        assert!(err.contains("未知的推理强度"));
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

    #[test]
    fn test_validate_tui_mode_warns_print_only_args() {
        // 非 print 模式，但传了 print-only 参数 → 应产生 4 个警告
        let opts = RunOptions {
            permission_mode: None,
            skip_permissions: false,
            model: None,
            effort: None,
            resume_session: None,
            continue_session: false,
            session_id: None,
            session_name: None,
            no_session_persistence: true,
            allowed_tools: vec![],
            disallowed_tools: vec![],
            max_turns: Some(10),
            bare: true,
            settings: None,
            print_mode: None,
            output_format: OutputFormat::Json,
        };
        let warnings = validate_args(&opts, false);
        assert_eq!(warnings.len(), 4, "应有 4 个 print-only 警告");
        assert!(warnings.iter().any(|w| w.contains("output_format")));
        assert!(warnings.iter().any(|w| w.contains("max_turns")));
        assert!(warnings.iter().any(|w| w.contains("bare")));
        assert!(warnings
            .iter()
            .any(|w| w.contains("no_session_persistence")));
    }

    #[test]
    fn test_validate_print_mode_no_warnings() {
        // print 模式下，print-only 参数不会产生警告
        let opts = RunOptions {
            permission_mode: None,
            skip_permissions: false,
            model: None,
            effort: None,
            resume_session: None,
            continue_session: false,
            session_id: None,
            session_name: None,
            no_session_persistence: true,
            allowed_tools: vec![],
            disallowed_tools: vec![],
            max_turns: Some(10),
            bare: true,
            settings: None,
            print_mode: Some("prompt".to_string()),
            output_format: OutputFormat::Json,
        };
        let warnings = validate_args(&opts, true);
        assert_eq!(warnings.len(), 0, "print 模式下不应有警告");
    }

    #[test]
    fn test_validate_conflicting_permissions() {
        // skip_permissions + permission_mode 同时指定 → 警告
        let opts = RunOptions {
            permission_mode: Some("default".to_string()),
            skip_permissions: true,
            model: None,
            effort: None,
            resume_session: None,
            continue_session: false,
            session_id: None,
            session_name: None,
            no_session_persistence: false,
            allowed_tools: vec![],
            disallowed_tools: vec![],
            max_turns: None,
            bare: false,
            settings: None,
            print_mode: None,
            output_format: OutputFormat::Text,
        };
        let warnings = validate_args(&opts, false);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("skip_permissions"));
        assert!(warnings[0].contains("permission_mode"));
    }
}
