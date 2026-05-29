#[cfg(test)]
mod tests {
    use crate::sync::{
        protocol::{SettingsItem, SyncItems},
        scanner,
    };
    use std::{fs, path::Path};
    use tempfile::TempDir;

    fn make_home_dir() -> TempDir {
        TempDir::new().expect("创建临时目录失败")
    }

    fn prepare_settings(home: &Path, content: &str) {
        let peri_dir = home.join(".peri");
        fs::create_dir_all(&peri_dir).expect("创建 .peri 目录");
        fs::write(peri_dir.join("settings.json"), content).expect("写入 settings.json");
    }

    #[test]
    fn test_scan_settings_existing_file() {
        let home = make_home_dir();
        prepare_settings(home.path(), r#"{"key":"value"}"#);
        let result = scanner::scan_settings(home.path());
        assert!(result.is_some());
        let item = result.unwrap();
        assert_eq!(item.content, r#"{"key":"value"}"#);
    }

    #[test]
    fn test_scan_settings_missing_file() {
        let home = make_home_dir();
        let result = scanner::scan_settings(home.path());
        assert!(result.is_none());
    }

    #[test]
    fn test_scan_skills_with_files() {
        let home = make_home_dir();
        let skill_dir = home.path().join(".claude").join("skills").join("my-skill");
        fs::create_dir_all(&skill_dir).expect("创建 skills 目录");
        fs::write(skill_dir.join("SKILL.md"), b"skill content").expect("写入 SKILL.md");
        // 也创建一个子文件来测试递归扫描
        let sub_dir = skill_dir.join("sub");
        fs::create_dir_all(&sub_dir).expect("创建子目录");
        fs::write(sub_dir.join("EXTRA.md"), b"extra content").expect("写入 EXTRA.md");

        let result = scanner::scan_skills(home.path());
        assert_eq!(result.files.len(), 2);
        let paths: Vec<&str> = result.files.iter().map(|f| f.path.as_str()).collect();
        assert!(paths.contains(&"my-skill/SKILL.md"));
        assert!(paths.contains(&"my-skill/sub/EXTRA.md"));
    }

    #[test]
    fn test_scan_skills_empty_dir() {
        let home = make_home_dir();
        let skill_dir = home.path().join(".claude").join("skills");
        fs::create_dir_all(&skill_dir).expect("创建空 skills 目录");
        let result = scanner::scan_skills(home.path());
        assert!(result.files.is_empty());
    }

    #[test]
    fn test_scan_mcp_both_configs() {
        let home = make_home_dir();
        let cwd = make_home_dir();
        fs::write(home.path().join(".mcp.json"), r#"{"global":true}"#).expect("写入全局 .mcp.json");
        fs::write(cwd.path().join(".mcp.json"), r#"{"project":true}"#).expect("写入项目 .mcp.json");

        let result = scanner::scan_mcp(home.path(), cwd.path());
        assert!(result.global.is_some());
        assert!(result.project.is_some());
        assert_eq!(result.global.unwrap(), r#"{"global":true}"#);
        assert_eq!(result.project.unwrap(), r#"{"project":true}"#);
    }

    #[test]
    fn test_scan_all_respects_filter() {
        let home = make_home_dir();
        let cwd = make_home_dir();
        prepare_settings(home.path(), r#"{"k":"v"}"#);

        // 只同步 settings，不同步 skills
        let filter = SyncItems {
            settings: Some(SettingsItem {
                content: String::new(),
                claude_content: None,
            }),
            skills: None,
            mcp: None,
            plugins: None,
        };

        let result = scanner::scan_all(home.path(), cwd.path(), &filter);
        assert!(result.items.settings.is_some());
        assert!(result.items.skills.is_none());
        assert!(result.items.mcp.is_none());
        assert!(result.items.plugins.is_none());
    }

    #[test]
    fn test_scan_all_timestamp_is_recent() {
        use std::time::{SystemTime, UNIX_EPOCH};

        let home = make_home_dir();
        let cwd = make_home_dir();
        let filter = SyncItems::default();

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let result = scanner::scan_all(home.path(), cwd.path(), &filter);
        assert!(result.timestamp > 0);
        // 时间戳应在当前时间前后 5 秒范围内
        assert!(
            result.timestamp >= now.saturating_sub(5),
            "timestamp should be recent (>= {}), got {}",
            now.saturating_sub(5),
            result.timestamp
        );
        assert!(
            result.timestamp <= now + 5,
            "timestamp should not be in the future"
        );
    }
}
