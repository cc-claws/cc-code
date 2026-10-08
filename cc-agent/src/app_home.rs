//! 应用主目录解析。
//!
//! 应用主目录统一为 `~/.cc-code`；改名前的旧目录 `~/.peri` **不再兼容**
//! （不再读取、不再回退——项目已是 cc-code）。

use std::path::PathBuf;

fn home_dir() -> PathBuf {
    dirs_next::home_dir().unwrap_or_else(|| PathBuf::from("."))
}

/// 应用主目录 `~/.cc-code`。
pub fn app_home_dir() -> PathBuf {
    app_home_dir_in(&home_dir())
}

/// [`app_home_dir`] 的可注入 home 版本（便于测试与调用方传入的 home）。
pub fn app_home_dir_in(home: &std::path::Path) -> PathBuf {
    home.join(".cc-code")
}

/// 解析应用数据**文件**路径（`settings.json`、`oauth_tokens.json` 等）。
pub fn app_data_path(file_name: &str) -> PathBuf {
    app_data_path_in(&home_dir(), file_name)
}

/// [`app_data_path`] 的可注入 home 版本。
pub fn app_data_path_in(home: &std::path::Path, file_name: &str) -> PathBuf {
    app_home_dir_in(home).join(file_name)
}

/// 全局 `settings.json` 路径（MCP 全局配置等）。
pub fn global_settings_path() -> PathBuf {
    global_settings_path_in(&home_dir())
}

/// [`global_settings_path`] 的可注入 home 版本。
pub fn global_settings_path_in(home: &std::path::Path) -> PathBuf {
    app_data_path_in(home, "settings.json")
}

/// 解析应用数据**目录**（如 `threads/`）。
pub fn app_data_dir(dir_name: &str) -> PathBuf {
    app_data_dir_in(&home_dir(), dir_name)
}

/// [`app_data_dir`] 的可注入 home 版本。
pub fn app_data_dir_in(home: &std::path::Path, dir_name: &str) -> PathBuf {
    app_home_dir_in(home).join(dir_name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_home_dir_is_cc_code() {
        assert!(app_home_dir().ends_with(".cc-code"));
    }

    /// 所有数据路径都落在 `~/.cc-code` 下，绝不引用旧目录 `.peri`。
    #[test]
    fn test_data_paths_never_use_legacy_peri() {
        let home = std::path::Path::new("/tmp/cc-code-home-probe");
        for p in [
            app_data_path_in(home, "settings.json"),
            app_data_path_in(home, "oauth_tokens.json"),
            app_data_dir_in(home, "threads"),
            global_settings_path_in(home),
        ] {
            let s = p.to_string_lossy();
            assert!(s.contains(".cc-code"), "应落在 ~/.cc-code：{s}");
            assert!(!s.contains(".peri"), "不应引用旧目录 .peri：{s}");
        }
    }

    #[test]
    fn test_global_settings_path_is_under_cc_code() {
        let home = std::env::temp_dir().join("cc-code-app-home-test");
        assert_eq!(
            global_settings_path_in(&home),
            home.join(".cc-code").join("settings.json")
        );
    }
}
