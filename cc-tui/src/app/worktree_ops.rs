use std::path::Path;

use cc_acp::worktree::{Worktree, WorktreeList};

use super::{panel_manager::PanelState, worktree_panel::WorktreePanel, AgentEvent, App};

#[derive(Debug, PartialEq, Eq)]
pub enum WorktreeRequest {
    Browse,
    Create {
        branch: Option<String>,
        base_ref: Option<String>,
    },
    Switch(String),
}

pub enum WorktreeOutcome {
    Browse(WorktreeList),
    Activate {
        worktree: Worktree,
        thread_id: Option<String>,
        created: bool,
    },
}

impl App {
    /// Git 和会话查询在后台执行；完成事件先核对来源，再变更界面目录。
    pub(crate) fn request_worktree(&mut self, request: WorktreeRequest) {
        if !self.can_switch_thread_context() {
            self.push_system_note(self.services.lc.tr("thread-busy"));
            return;
        }
        let Some(acp) = self.acp_client.clone() else {
            self.push_system_note("ACP 未连接，无法管理 worktree".to_string());
            return;
        };
        let session_id = self.session_mgr.current().metadata.session_id;
        self.session_mgr
            .current_mut()
            .metadata
            .worktree_request_pending = true;
        let cwd = self.services.cwd.clone();
        let tx = self.services.bg_event_tx.clone();
        let store = self.services.thread_store.clone();
        tokio::spawn(async move {
            let result = async {
                let (worktree, created) = match request {
                    WorktreeRequest::Browse => {
                        return acp.list_worktrees(&cwd).await.map(WorktreeOutcome::Browse);
                    }
                    WorktreeRequest::Create { branch, base_ref } => (
                        acp.create_worktree(&cwd, branch.as_deref(), base_ref.as_deref())
                            .await?,
                        true,
                    ),
                    WorktreeRequest::Switch(selector) => {
                        let list = acp.list_worktrees(&cwd).await?;
                        (resolve_worktree(&list, &selector)?.clone(), false)
                    }
                };
                // 按目录恢复最近的主会话，子 agent 和隐藏线程不能成为切换目标。
                let thread_id = if created {
                    None
                } else {
                    match store.list_threads().await {
                        Ok(threads) => threads
                            .into_iter()
                            .filter(|thread| {
                                !thread.hidden
                                    && thread.parent_thread_id.is_none()
                                    && same_directory(&thread.cwd, &worktree.path)
                            })
                            .max_by_key(|thread| thread.updated_at)
                            .map(|thread| thread.id),
                        Err(error) => return Err(format!("读取 worktree 会话失败: {error}")),
                    }
                };
                Ok(WorktreeOutcome::Activate {
                    worktree,
                    thread_id,
                    created,
                })
            }
            .await;
            let _ = tx
                .send(AgentEvent::WorktreeCompleted {
                    session_id,
                    cwd,
                    result,
                })
                .await;
        });
    }

    pub(crate) fn handle_worktree_completed(
        &mut self,
        session_id: uuid::Uuid,
        cwd: String,
        result: Result<WorktreeOutcome, String>,
    ) {
        if self.session_mgr.current().metadata.session_id != session_id || self.services.cwd != cwd
        {
            if let Ok(WorktreeOutcome::Activate {
                worktree,
                created: true,
                ..
            }) = result
            {
                self.push_system_note(format!(
                    "Worktree 已创建，来源会话已变化；可稍后切换: {}",
                    worktree.path
                ));
            }
            return;
        }
        self.session_mgr
            .current_mut()
            .metadata
            .worktree_request_pending = false;
        match result {
            Err(error) => self.push_system_note(format!("Worktree 操作失败: {error}")),
            Ok(WorktreeOutcome::Browse(list)) => {
                self.open_panel(PanelState::Worktree(WorktreePanel::new(list)));
            }
            Ok(WorktreeOutcome::Activate {
                worktree,
                thread_id,
                created,
            }) => {
                if same_directory(&cwd, &worktree.path) {
                    self.push_system_note(format!("当前工作目录: {}", self.services.cwd));
                    return;
                }
                let activated = if let Some(thread_id) = thread_id {
                    self.open_thread(thread_id)
                } else {
                    self.new_thread_in_cwd(worktree.path.clone())
                };
                if activated {
                    self.push_system_note(format!(
                        "{} worktree: {}",
                        if created {
                            "已创建并切换到"
                        } else {
                            "已切换到"
                        },
                        self.services.cwd
                    ));
                } else {
                    self.push_system_note(format!(
                        "{}；未切换会话，可稍后使用 /worktree switch \"{}\" 重试",
                        if created {
                            "Worktree 已创建"
                        } else {
                            "Worktree 已保留"
                        },
                        worktree.path
                    ));
                }
            }
        }
    }
}

pub(crate) fn same_directory(left: &str, right: &str) -> bool {
    match (
        Path::new(left).canonicalize(),
        Path::new(right).canonicalize(),
    ) {
        (Ok(left), Ok(right)) => left == right,
        _ => Path::new(left) == Path::new(right),
    }
}

fn resolve_worktree<'a>(list: &'a WorktreeList, selector: &str) -> Result<&'a Worktree, String> {
    let matches: Vec<_> = list
        .worktrees
        .iter()
        .enumerate()
        .filter(|(index, tree)| {
            selector == (index + 1).to_string()
                || tree.branch.as_deref() == Some(selector)
                || same_directory(&tree.path, selector)
        })
        .map(|(_, tree)| tree)
        .collect();
    match matches.as_slice() {
        [tree] if tree.bare => Err("不能切换到 bare 仓库".to_string()),
        [tree] if tree.prunable || !Path::new(&tree.path).is_dir() => {
            Err(format!("Worktree 目录不可用: {}", tree.path))
        }
        [tree] => Ok(tree),
        [] => Err(format!(
            "未找到 worktree: {selector}。使用 /worktree 查看目录和编号"
        )),
        _ => Err(format!("Worktree 匹配不唯一: {selector}，请使用完整路径")),
    }
}

#[cfg(test)]
#[path = "worktree_ops_test.rs"]
mod tests;
