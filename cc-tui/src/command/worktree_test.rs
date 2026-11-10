use super::*;

#[test]
fn worktree_command_parses_creation_and_paths_with_spaces() {
    assert_eq!(parse_request("").unwrap(), WorktreeRequest::Browse);
    assert_eq!(parse_request("list").unwrap(), WorktreeRequest::Browse);
    assert_eq!(
        parse_request("create").unwrap(),
        WorktreeRequest::Create {
            branch: None,
            base_ref: None
        }
    );
    assert_eq!(
        parse_request("create feat/demo main").unwrap(),
        WorktreeRequest::Create {
            branch: Some("feat/demo".into()),
            base_ref: Some("main".into())
        }
    );
    assert_eq!(
        parse_request("switch \"D:/my projects/功能\"").unwrap(),
        WorktreeRequest::Switch("D:/my projects/功能".into())
    );
    assert!(parse_request("switch").is_err());
    assert!(parse_request("create a b c").is_err());
    assert!(parse_request("list unexpected").is_err());
}

#[test]
fn worktree_command_is_registered_and_blocked_while_busy() {
    let registry = super::super::default_registry();
    assert!(registry.find("worktree").unwrap().blocked_while_busy());
    assert_eq!(registry.find("worktrees").unwrap().name(), "worktree");
}
