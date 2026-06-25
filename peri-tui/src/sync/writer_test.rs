#[cfg(test)]
mod tests {
    use crate::sync::{
        protocol::{FileEntry, FilesItem, McpItem, SettingsItem, SyncItems},
        writer,
    };
    use std::{fs, path::Path};
    use tempfile::TempDir;

    #[test]
    fn test_validate_normal_relative_path() {
        let base = Path::new("/tmp/base");
        let result = writer::validate_and_resolve(base, "my-skill/SKILL.md");
        assert!(result.is_ok(), "正常相对路径应通过");
        let resolved = result.unwrap();
        assert!(resolved.starts_with(base));
        assert!(resolved.ends_with("my-skill/SKILL.md"));
    }

    #[test]
    fn test_validate_rejects_absolute_path() {
        let base = Path::new("/tmp/base");
        let result = writer::validate_and_resolve(base, "/etc/passwd");
        assert!(result.is_err(), "绝对路径应被拒绝");
        match result.unwrap_err() {
            writer::WriteError::PathTraversal(_) => {}
            _ => panic!("应返回 PathTraversal 错误"),
        }
    }

    #[test]
    fn test_validate_rejects_parent_dir_traversal() {
        let base = Path::new("/tmp/base");
        let result = writer::validate_and_resolve(base, "../.ssh/authorized_keys");
        assert!(result.is_err(), "../ 穿越应被拒绝");
    }

    #[test]
    fn test_validate_rejects_hidden_traversal() {
        let base = Path::new("/tmp/base");
        // foo/../../bar → depth: 1 → 0 → -1
        let result = writer::validate_and_resolve(base, "foo/../../bar");
        assert!(result.is_err(), "foo/../../bar 穿越应被拒绝");
    }

    #[test]
    fn test_write_file_entry_creates_parent_dirs() {
        let tmp = TempDir::new().expect("创建临时目录");
        let base = tmp.path();
        let entry = FileEntry {
            path: "a/b/c.txt".into(),
            content: b"hi".to_vec(),
        };
        writer::write_file_entry(base, &entry).expect("写入应成功");
        let written = base.join("a/b/c.txt");
        assert!(written.exists(), "文件应被创建");
        assert_eq!(fs::read_to_string(&written).unwrap(), "hi");
    }

    #[test]
    fn test_write_file_entry_rejects_traversal() {
        let tmp = TempDir::new().expect("创建临时目录");
        let base = tmp.path();
        let entry = FileEntry {
            path: "../bad.txt".into(),
            content: b"x".to_vec(),
        };
        let result = writer::write_file_entry(base, &entry);
        assert!(result.is_err(), "路径穿越应被拒绝");
    }

    #[test]
    fn test_write_sync_items_settings_with_backup() {
        let home = TempDir::new().expect("创建临时 home");
        let cwd = TempDir::new().expect("创建临时 cwd");
        let home_p = home.path();
        let cwd_p = cwd.path();

        // 创建预先存在的 settings.json
        let peri_dir = home_p.join(".peri");
        fs::create_dir_all(&peri_dir).unwrap();
        fs::write(peri_dir.join("settings.json"), "old").unwrap();

        let items = SyncItems {
            settings: Some(SettingsItem {
                content: "new".into(),
                claude_content: None,
            }),
            skills: None,
            mcp: None,
            plugins: None,
        };

        writer::write_sync_items(home_p, cwd_p, &items).expect("写入应成功");

        // 新文件内容为 "new"
        assert_eq!(
            fs::read_to_string(home_p.join(".peri/settings.json")).unwrap(),
            "new"
        );
        // 备份文件内容为 "old"
        assert_eq!(
            fs::read_to_string(home_p.join(".peri/settings.json.bak")).unwrap(),
            "old"
        );
    }

    #[test]
    fn test_write_sync_items_all_categories() {
        let home = TempDir::new().expect("创建临时 home");
        let cwd = TempDir::new().expect("创建临时 cwd");
        let home_p = home.path();
        let cwd_p = cwd.path();

        let items = SyncItems {
            settings: Some(SettingsItem {
                content: r#"{"model":"sonnet"}"#.into(),
                claude_content: None,
            }),
            skills: Some(FilesItem {
                files: vec![FileEntry {
                    path: "test-skill/SKILL.md".into(),
                    content: b"# Test Skill".to_vec(),
                }],
            }),
            mcp: Some(McpItem {
                global: Some(r#"{"global":true}"#.into()),
                project: Some(r#"{"project":true}"#.into()),
            }),
            plugins: Some(FilesItem {
                files: vec![FileEntry {
                    path: "my-plugin/manifest.json".into(),
                    content: b"{}".to_vec(),
                }],
            }),
        };

        writer::write_sync_items(home_p, cwd_p, &items).expect("全部写入应成功");

        // settings
        assert_eq!(
            fs::read_to_string(home_p.join(".peri/settings.json")).unwrap(),
            r#"{"model":"sonnet"}"#
        );
        // skills
        assert_eq!(
            fs::read_to_string(home_p.join(".claude/skills/test-skill/SKILL.md")).unwrap(),
            "# Test Skill"
        );
        // MCP global
        assert_eq!(
            fs::read_to_string(home_p.join(".mcp.json")).unwrap(),
            r#"{"global":true}"#
        );
        // MCP project
        assert_eq!(
            fs::read_to_string(cwd_p.join(".mcp.json")).unwrap(),
            r#"{"project":true}"#
        );
        // plugins
        assert_eq!(
            fs::read_to_string(home_p.join(".claude/plugins/cache/my-plugin/manifest.json"))
                .unwrap(),
            "{}"
        );
    }
}
