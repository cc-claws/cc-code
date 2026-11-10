use crate::app::{worktree_ops::WorktreeRequest, App};

use super::Command;

pub struct WorktreeCommand;

pub(crate) const USAGE: &str =
    "/worktree [list | create [branch] [base-ref] | switch <branch|path|number>]";

impl Command for WorktreeCommand {
    fn name(&self) -> &str {
        "worktree"
    }

    fn aliases(&self) -> Vec<&str> {
        vec!["worktrees"]
    }

    fn description(&self, _lc: &crate::i18n::LcRegistry) -> String {
        "Create or switch Git worktrees and their conversations".to_string()
    }

    fn execute(&self, app: &mut App, args: &str) {
        match parse_request(args) {
            Ok(request) => app.request_worktree(request),
            Err(error) => app.push_system_note(error),
        }
    }
}

fn parse_request(args: &str) -> Result<WorktreeRequest, String> {
    let args = args.trim();
    let (action, rest) = args.split_once(char::is_whitespace).unwrap_or((args, ""));
    let rest = rest.trim();
    match action {
        "" | "list" if rest.is_empty() => Ok(WorktreeRequest::Browse),
        "create" => {
            let parts: Vec<_> = rest.split_whitespace().collect();
            if parts.len() > 2 {
                return Err(USAGE.to_string());
            }
            Ok(WorktreeRequest::Create {
                branch: parts.first().map(|s| s.to_string()),
                base_ref: parts.get(1).map(|s| s.to_string()),
            })
        }
        "switch" if !rest.is_empty() => {
            // 路径参数保留空格；成对引号仅用于兼容用户的 shell 输入习惯。
            let selector = if rest.starts_with('"') && rest.ends_with('"') && rest.len() >= 2 {
                &rest[1..rest.len() - 1]
            } else {
                rest
            };
            if selector.is_empty() {
                return Err(USAGE.to_string());
            }
            Ok(WorktreeRequest::Switch(selector.to_string()))
        }
        _ => Err(USAGE.to_string()),
    }
}

#[cfg(test)]
#[path = "worktree_test.rs"]
mod tests;
