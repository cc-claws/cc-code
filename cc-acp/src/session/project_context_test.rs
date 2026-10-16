use super::*;
use cc_middlewares::hooks::types::{HookEvent, HookType};

fn make_hook(plugin_id: &str, root: &Path) -> RegisteredHook {
    RegisteredHook {
        hook: serde_json::from_value::<HookType>(serde_json::json!({
            "type": "command", "command": "echo test"
        }))
        .unwrap(),
        event: HookEvent::PreToolUse,
        matcher: None,
        plugin_name: plugin_id.to_string(),
        plugin_id: plugin_id.to_string(),
        plugin_root: root.to_path_buf(),
        plugin_data_dir: root.to_path_buf(),
        plugin_options: HashMap::new(),
    }
}

#[test]
fn test_project_hooks_drop_previous_project_and_keep_plugin_global_order() {
    let target = tempfile::tempdir().unwrap();
    let previous = tempfile::tempdir().unwrap();
    let groups = vec![
        vec![make_hook("plugin:test", previous.path())],
        vec![make_hook("settings.global", previous.path())],
        vec![make_hook("settings.local", previous.path())],
    ];
    let groups = project_hook_groups(groups, target.path().to_str().unwrap());
    let ids: Vec<_> = groups
        .iter()
        .flatten()
        .map(|hook| hook.plugin_id.as_str())
        .collect();
    assert_eq!(
        ids,
        vec!["plugin:test", "settings.global"],
        "旧项目 hooks 必须移除，插件及全局顺序保持不变"
    );
}
