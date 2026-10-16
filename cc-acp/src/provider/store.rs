use super::config::PeriConfig;
use anyhow::Result;
use std::path::{Path, PathBuf};

/// 配置文件路径：~/.cc-code/settings.json
pub fn config_path() -> PathBuf {
    dirs_next::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".cc-code")
        .join("settings.json")
}

/// 工作区配置文件路径：{cwd}/.cc-code/settings.json
/// 文件不存在时返回 None
pub fn workspace_config_path() -> Option<PathBuf> {
    let cwd = std::env::current_dir().ok()?;
    let path = cwd.join(".cc-code").join("settings.json");
    if path.exists() {
        Some(path)
    } else {
        None
    }
}

/// 加载配置（全局 + 工作区合并），文件不存在时返回默认空配置
///
/// 先加载 ~/.cc-code/settings.json 获取全局配置，
/// 再检测当前工作目录的 .cc-code/settings.json 是否存在，
/// 若存在则加载并以工作区字段覆盖全局对应字段。
pub fn load() -> Result<PeriConfig> {
    load_for_cwd(&std::env::current_dir()?)
}

/// 按会话目录合并工作区配置，不改变进程级 current_dir。
pub fn load_for_cwd(cwd: &Path) -> Result<PeriConfig> {
    let mut merged = load_from(&config_path())?;
    let ws_path = cwd.join(".cc-code").join("settings.json");
    if ws_path.exists() {
        let workspace = load_from(&ws_path)?;
        merged.config.merge_overrides(workspace.config);
    }
    Ok(merged)
}

/// 运行中的会话沿用内存中的模型选择；跨项目的其余设置重新从全局及目标目录加载。
pub fn load_for_session(cwd: &Path, fallback: &PeriConfig) -> Result<PeriConfig> {
    let mut merged = load_from(&config_path())?;
    if merged.config.providers.is_empty()
        || !merged
            .config
            .providers
            .iter()
            .any(|provider| provider.id == fallback.config.active_provider_id)
    {
        merged.config.providers = fallback.config.providers.clone();
    }
    merged.config.active_provider_id = fallback.config.active_provider_id.clone();
    merged.config.active_alias = fallback.config.active_alias.clone();
    merged.config.thinking = fallback.config.thinking.clone();
    let workspace = cwd.join(".cc-code").join("settings.json");
    if workspace.exists() {
        merged.config.merge_overrides(load_from(&workspace)?.config);
    }
    Ok(merged)
}

/// 从指定路径加载配置
pub fn load_from(path: &Path) -> Result<PeriConfig> {
    if !path.exists() {
        return Ok(PeriConfig::default());
    }
    let content = std::fs::read_to_string(path)?;
    let cfg: PeriConfig = serde_json::from_str(&content)?;
    Ok(cfg)
}

/// 原子写回配置文件（先写临时文件，再 rename，避免写入中断导致文件损坏）
pub fn save(cfg: &PeriConfig) -> Result<()> {
    save_to(cfg, &config_path())
}

/// 将配置写入指定路径
pub fn save_to(cfg: &PeriConfig, path: &Path) -> Result<()> {
    // 确保目录存在
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let content = serde_json::to_string_pretty(cfg)?;

    // atomic write（owner-only：配置含 providers[].apiKey，不能落盘成 0644）
    let tmp_path = path.with_extension("json.tmp");
    std::fs::write(&tmp_path, content)?;
    let _ = cc_agent::fs::restrict_to_owner(&tmp_path);
    std::fs::rename(&tmp_path, path)?;

    Ok(())
}

#[cfg(test)]
#[path = "store_test.rs"]
mod store_tests;
