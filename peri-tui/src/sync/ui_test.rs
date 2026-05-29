#[cfg(test)]
mod tests {
    use crate::sync::{protocol::SyncItems, ui};

    #[test]
    fn test_build_default_items_all_keys_present() {
        let items = ui::build_default_items();
        assert_eq!(items.len(), 4, "应有 4 个预设项");
        let keys: Vec<&str> = items.iter().map(|i| i.key).collect();
        assert!(keys.contains(&"settings"));
        assert!(keys.contains(&"skills"));
        assert!(keys.contains(&"mcp"));
        assert!(keys.contains(&"plugins"));
    }

    #[test]
    fn test_build_default_items_default_selected() {
        let items = ui::build_default_items();
        // settings, skills, mcp 默认选中；plugins 默认不选
        assert!(items.iter().find(|i| i.key == "settings").unwrap().selected);
        assert!(items.iter().find(|i| i.key == "skills").unwrap().selected);
        assert!(items.iter().find(|i| i.key == "mcp").unwrap().selected);
        assert!(!items.iter().find(|i| i.key == "plugins").unwrap().selected);
    }

    #[test]
    fn test_confirm_sync_empty_items() {
        // 全部为 None 时应显示 0 项
        let items = SyncItems::default();
        // confirm_sync 需要交互输入，这里只测试构造
        // 验证 SyncItems::default() 所有字段为 None
        assert!(items.settings.is_none());
        assert!(items.skills.is_none());
        assert!(items.mcp.is_none());
        assert!(items.plugins.is_none());
    }

    #[test]
    fn test_progress_bar_new() {
        let pb = ui::ProgressBar::new(10, "测试");
        // 验证构造函数不 panic，update 和 finish 也不 panic
        pb.update(3);
        pb.finish();
    }

    #[test]
    fn test_progress_bar_update_zero() {
        let pb = ui::ProgressBar::new(100, "测试");
        // total > 0 时更新不应 panic
        pb.update(0);
        pb.update(50);
        pb.update(100);
    }

    #[test]
    fn test_progress_bar_zero_total() {
        let pb = ui::ProgressBar::new(0, "测试");
        // total 为 0 时更新不应 panic（除零保护）
        pb.update(0);
        pb.update(5);
        pb.finish();
    }
}
