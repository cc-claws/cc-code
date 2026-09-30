//! 应用主目录解析（#289）。
//!
//! v0.6.86 起应用主目录为 `~/.cc-code`；旧版 `~/.peri` 仅作向后兼容回退。
//!
//! 解析规则（与 `peri-tui` 主配置的「新优先、旧回退」约定一致）：
//! - 新路径存在 → 用新路径；
//! - 否则旧路径存在 → 回退旧路径（老用户数据不丢失）；
//! - 两者都不存在 → 返回新路径（新写入一律走 `~/.cc-code`，不再创建 `~/.peri`）。

use std::path::PathBuf;

fn home_dir() -> PathBuf {
    dirs_next::home_dir().unwrap_or_else(|| PathBuf::from("."))
}

/// 新版应用主目录 `~/.cc-code`。
pub fn app_home_dir() -> PathBuf {
    app_home_dir_in(&home_dir())
}

/// [`app_home_dir`] 的可注入 home 版本（便于测试与调用方传入的 home）。
pub fn app_home_dir_in(home: &std::path::Path) -> PathBuf {
    home.join(".cc-code")
}

/// 旧版应用主目录 `~/.peri`（仅向后兼容，不再写入新数据）。
pub fn legacy_app_home_dir() -> PathBuf {
    legacy_app_home_dir_in(&home_dir())
}

/// [`legacy_app_home_dir`] 的可注入 home 版本。
pub fn legacy_app_home_dir_in(home: &std::path::Path) -> PathBuf {
    home.join(".peri")
}

/// 解析应用数据**文件**路径（`settings.json`、`oauth_tokens.json` 等）。
pub fn app_data_path(file_name: &str) -> PathBuf {
    app_data_path_in(&home_dir(), file_name)
}

/// [`app_data_path`] 的可注入 home 版本。
pub fn app_data_path_in(home: &std::path::Path, file_name: &str) -> PathBuf {
    let new = app_home_dir_in(home).join(file_name);
    let legacy = legacy_app_home_dir_in(home).join(file_name);
    if new.exists() || !legacy.exists() {
        new
    } else {
        legacy
    }
}

/// 全局 `settings.json` 路径（MCP 全局配置等）。
///
/// 修复 #289 的核心：此前 MCP 全局配置硬编码读取 `~/.peri/settings.json`，
/// 导致用户在新版 `~/.cc-code/settings.json` 中的配置被静默忽略。
pub fn global_settings_path() -> PathBuf {
    global_settings_path_in(&home_dir())
}

/// [`global_settings_path`] 的可注入 home 版本。
pub fn global_settings_path_in(home: &std::path::Path) -> PathBuf {
    app_data_path_in(home, "settings.json")
}

/// 解析应用数据**目录**（如 `threads/`）。
pub fn app_data_dir(dir_name: &str) -> PathBuf {
    let home = home_dir();
    app_data_dir_in(&home, dir_name)
}

/// [`app_data_dir`] 的可注入 home 版本。
pub fn app_data_dir_in(home: &std::path::Path, dir_name: &str) -> PathBuf {
    let new = app_home_dir_in(home).join(dir_name);
    let legacy = legacy_app_home_dir_in(home).join(dir_name);
    if new.exists() || !legacy.exists() {
        new
    } else {
        legacy
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 路径解析永不 panic，且新旧目录互斥时语义明确。
    #[test]
    fn test_app_home_dirs_are_distinct() {
        assert_ne!(app_home_dir(), legacy_app_home_dir());
        assert!(app_home_dir().ends_with(".cc-code"));
        assert!(legacy_app_home_dir().ends_with(".peri"));
    }

    /// 两者都不存在时返回新路径（新写入走新目录）。
    #[test]
    fn test_app_data_path_prefers_new_dir() {
        // 用一个不可能存在的文件名，保证"两者都不存在"分支
        let p = app_data_path("cc-code-fix-probe-9f3a2b1c.json");
        assert!(
            p.starts_with(app_home_dir()),
            "新文件应落到 ~/.cc-code，实际：{}",
            p.display()
        );
    }

    /// #289：新旧回退语义 —— 新存在用新，仅旧存在用旧，都没有用新。
    #[test]
    fn test_global_settings_path_fallback_semantics() {
        let home = std::env::temp_dir().join("cc-code-app-home-test");
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();

        // 都没有 → 新路径
        assert_eq!(
            global_settings_path_in(&home),
            home.join(".cc-code").join("settings.json")
        );

        // 仅旧存在 → 回退旧路径（老用户数据不丢失）
        let legacy = home.join(".peri").join("settings.json");
        std::fs::create_dir_all(legacy.parent().unwrap()).unwrap();
        std::fs::write(&legacy, "{}").unwrap();
        assert_eq!(global_settings_path_in(&home), legacy);

        // 新旧都存在 → 新路径优先
        let new = home.join(".cc-code").join("settings.json");
        std::fs::create_dir_all(new.parent().unwrap()).unwrap();
        std::fs::write(&new, "{}").unwrap();
        assert_eq!(global_settings_path_in(&home), new);

        let _ = std::fs::remove_dir_all(&home);
    }
}
