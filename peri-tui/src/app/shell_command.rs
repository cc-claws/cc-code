use std::sync::Arc;

use chrono::Utc;
use tokio::sync::mpsc;

use super::*;
use crate::shell_exec::CommandOutput;
use crate::shell_history::ShellCommandRecord;
use crate::thread::{ThreadId, ThreadMeta, ThreadStore};

#[derive(Default)]
pub struct ShellCommandRuntime {
    pub stdin_tx: Option<mpsc::Sender<String>>,
    pub running_record_id: Option<String>,
    pub stdin_lines: Vec<String>,
}

impl ShellCommandRuntime {
    fn is_running(&self) -> bool {
        self.running_record_id.is_some()
    }
}

impl App {
    pub fn run_shell_command(&mut self, command: String) {
        let command = command.trim().to_string();
        if command.is_empty() {
            self.push_system_note("请输入 shell 命令".to_string());
            self.render_rebuild();
            return;
        }
        if self.session_mgr.current().ui.loading {
            self.push_system_note("已有任务正在运行，暂不能启动 shell 命令".to_string());
            self.render_rebuild();
            return;
        }

        let thread_id = match self.ensure_shell_thread(&command) {
            Some(id) => id,
            None => return,
        };
        let record_id = uuid::Uuid::now_v7().to_string();
        let cwd = self.services.cwd.clone();
        let started_at = Utc::now();
        let anchor_message_id = self
            .session_mgr
            .current()
            .agent
            .origin_messages
            .last()
            .map(|m| m.id().as_uuid().to_string());
        let (stdin_tx, stdin_rx) = mpsc::channel(32);

        self.push_input_history(format!("!{}", command));
        self.session_mgr.current_mut().metadata.last_human_message = Some(format!("!{}", command));
        self.session_mgr.current_mut().messages.view_messages.push(
            MessageViewModel::shell_command_pending(
                record_id.clone(),
                command.clone(),
                cwd.clone(),
            ),
        );
        self.session_mgr.current_mut().shell_command = ShellCommandRuntime {
            stdin_tx: Some(stdin_tx),
            running_record_id: Some(record_id.clone()),
            stdin_lines: Vec::new(),
        };
        self.set_loading(true);
        self.session_mgr
            .current_mut()
            .spinner_state
            .set_mode(peri_widgets::SpinnerMode::ToolUse);
        self.session_mgr
            .current_mut()
            .spinner_state
            .set_verb(Some("Shell"));
        self.scroll_to_bottom();
        self.render_rebuild();

        let tx = self.services.bg_event_tx.clone();
        tokio::spawn(async move {
            let output = match crate::shell_exec::execute_shell_command_with_stdin(
                &command,
                &cwd,
                Some(stdin_rx),
            )
            .await
            {
                Ok(output) => output,
                Err(e) => CommandOutput {
                    stdout: String::new(),
                    stderr: format!("{e:#}"),
                    exit_code: -1,
                },
            };
            let completed_at = Utc::now();
            let record = ShellCommandRecord {
                id: record_id,
                thread_id,
                command,
                cwd,
                stdin: Vec::new(),
                stdout: output.stdout,
                stderr: output.stderr,
                exit_code: output.exit_code,
                started_at,
                completed_at,
                anchor_message_id,
            };
            let _ = tx.send(AgentEvent::ShellCommandCompleted(record)).await;
        });
    }

    pub(crate) fn is_shell_command_running(&self) -> bool {
        self.session_mgr.current().shell_command.is_running()
    }

    pub(crate) fn send_shell_stdin_line(&mut self, line: String) {
        let record_id = self
            .session_mgr
            .current()
            .shell_command
            .running_record_id
            .clone();
        let Some(record_id) = record_id else {
            return;
        };
        let tx = self.session_mgr.current().shell_command.stdin_tx.clone();
        let Some(tx) = tx else {
            self.push_system_note("shell stdin 已关闭".to_string());
            self.render_rebuild();
            return;
        };

        self.session_mgr
            .current_mut()
            .shell_command
            .stdin_lines
            .push(line.clone());
        self.append_shell_stdin_to_vm(&record_id, line.clone());
        self.session_mgr.current_mut().ui.textarea = build_textarea(true);
        self.scroll_to_bottom();
        self.render_rebuild();

        tokio::spawn(async move {
            let _ = tx.send(line).await;
        });
    }

    pub(crate) fn close_shell_stdin(&mut self) {
        if !self.is_shell_command_running() {
            return;
        }
        self.session_mgr.current_mut().shell_command.stdin_tx = None;
        self.session_mgr
            .current_mut()
            .spinner_state
            .set_verb(Some("Shell"));
        self.session_mgr.current_mut().ui.textarea = build_textarea(true);
    }

    pub(crate) fn handle_shell_command_completed(
        &mut self,
        mut record: ShellCommandRecord,
    ) -> (bool, bool, bool) {
        let matches_running = self
            .session_mgr
            .current()
            .shell_command
            .running_record_id
            .as_deref()
            == Some(record.id.as_str());
        if matches_running {
            record.stdin = self.session_mgr.current().shell_command.stdin_lines.clone();
            self.session_mgr.current_mut().shell_command = ShellCommandRuntime::default();
            self.set_loading(false);
        }

        self.persist_shell_record(record.clone());

        let current_thread_id = self.session_mgr.current().current_thread_id.clone();
        if current_thread_id.as_deref() == Some(record.thread_id.as_str()) {
            let mut replaced = false;
            let session = self.session_mgr.current_mut();
            for vm in &mut session.messages.view_messages {
                if let MessageViewModel::ShellCommand { id, .. } = vm {
                    if id == &record.id {
                        *vm = MessageViewModel::shell_command_completed(&record);
                        replaced = true;
                        break;
                    }
                }
            }
            if !replaced {
                session
                    .messages
                    .view_messages
                    .push(MessageViewModel::shell_command_completed(&record));
            }
            self.scroll_to_bottom();
            self.render_rebuild();
        }

        (true, false, false)
    }

    fn append_shell_stdin_to_vm(&mut self, record_id: &str, line: String) {
        for vm in &mut self.session_mgr.current_mut().messages.view_messages {
            if let MessageViewModel::ShellCommand {
                id,
                stdin,
                exit_code,
                ..
            } = vm
            {
                if id == record_id && exit_code.is_none() {
                    stdin.push(line);
                    vm.recompute_hash();
                    break;
                }
            }
        }
    }

    fn ensure_shell_thread(&mut self, command: &str) -> Option<ThreadId> {
        if let Some(id) = self.session_mgr.current().current_thread_id.clone() {
            return Some(id);
        }
        let store = self.services.thread_store.clone();
        let cwd = self.services.cwd.clone();
        let mut meta = ThreadMeta::new(cwd);
        meta.title = Some(shell_thread_title(command));
        let created = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(store.create_thread(meta))
        });
        match created {
            Ok(thread_id) => {
                self.session_mgr.current_mut().current_thread_id = Some(thread_id.clone());
                Some(thread_id)
            }
            Err(e) => {
                tracing::error!(error = %e, "创建 shell thread 失败");
                self.push_system_note(format!("创建 shell 会话失败: {e:#}"));
                self.render_rebuild();
                None
            }
        }
    }

    fn persist_shell_record(&self, record: ShellCommandRecord) {
        let shell_store = self.services.shell_command_store.clone();
        let thread_store = self.services.thread_store.clone();
        let title = shell_thread_title(&record.command);
        tokio::spawn(async move {
            if let Err(e) = shell_store.append(&record).await {
                tracing::warn!(error = %e, "保存 shell 命令历史失败");
            }
            touch_shell_thread(thread_store, &record.thread_id, &title, record.completed_at).await;
        });
    }

    pub(crate) fn merge_shell_records_into_view(
        &self,
        mut view_msgs: Vec<MessageViewModel>,
        base_msgs: &[BaseMessage],
        shell_records: Vec<ShellCommandRecord>,
    ) -> Vec<MessageViewModel> {
        let mut anchor_positions = std::collections::HashMap::new();
        let mut visible_pos = 0usize;
        for msg in base_msgs {
            if !matches!(msg, BaseMessage::System { .. }) {
                visible_pos += 1;
                anchor_positions.insert(
                    msg.id().as_uuid().to_string(),
                    visible_pos.min(view_msgs.len()),
                );
            }
        }

        let mut inserted_base_positions = Vec::new();
        for record in shell_records {
            let base_pos = record
                .anchor_message_id
                .as_ref()
                .and_then(|id| anchor_positions.get(id))
                .copied()
                .unwrap_or_else(|| {
                    if record.anchor_message_id.is_some() {
                        view_msgs.len()
                    } else {
                        0
                    }
                });
            let inserted_before = inserted_base_positions
                .iter()
                .filter(|&&pos| pos <= base_pos)
                .count();
            let insert_pos = (base_pos + inserted_before).min(view_msgs.len());
            view_msgs.insert(
                insert_pos,
                MessageViewModel::shell_command_completed(&record),
            );
            inserted_base_positions.push(base_pos);
        }
        view_msgs
    }
}

fn shell_thread_title(command: &str) -> String {
    let title = format!("!{}", command.trim());
    if title.chars().count() <= 50 {
        title
    } else {
        let prefix: String = title.chars().take(49).collect();
        format!("{}…", prefix)
    }
}

async fn touch_shell_thread(
    thread_store: Arc<dyn ThreadStore>,
    thread_id: &ThreadId,
    title: &str,
    completed_at: chrono::DateTime<Utc>,
) {
    let mut meta = match thread_store.load_meta(thread_id).await {
        Ok(meta) => meta,
        Err(e) => {
            tracing::warn!(error = %e, thread_id = %thread_id, "加载 shell thread meta 失败");
            return;
        }
    };
    if meta.title.is_none() {
        meta.title = Some(title.to_string());
    }
    meta.updated_at = completed_at;
    if let Err(e) = thread_store.update_meta(thread_id, meta).await {
        tracing::warn!(error = %e, thread_id = %thread_id, "更新 shell thread meta 失败");
    }
}

#[cfg(test)]
#[path = "shell_command_test.rs"]
mod tests;
