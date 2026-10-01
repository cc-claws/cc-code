use std::time::Instant;

use crate::ui::message_view::MessageViewModel;

use super::MessagePipeline;

pub(super) struct ShellRuntime {
    started_at: Instant,
    execution_timeout_ms: u64,
    backgrounded: bool,
}

impl MessagePipeline {
    pub(crate) fn remove_shell_runtime(
        &mut self,
        source_agent_id: Option<&str>,
        tool_call_id: &str,
    ) {
        self.shell_runtimes
            .remove(&(source_agent_id.map(str::to_owned), tool_call_id.to_owned()));
    }

    #[cfg(test)]
    pub(crate) fn register_shell_runtime(
        &mut self,
        source_agent_id: Option<&str>,
        tool_call_id: &str,
        started_at: Instant,
        execution_timeout_ms: u64,
    ) {
        self.sync_shell_runtime(
            source_agent_id,
            tool_call_id,
            started_at,
            execution_timeout_ms,
            false,
        );
    }

    /// 从仍存活的任务槽恢复 UI 元数据；线程切换会清空 pipeline，但任务所有权不变。
    pub(crate) fn sync_shell_runtime(
        &mut self,
        source_agent_id: Option<&str>,
        tool_call_id: &str,
        started_at: Instant,
        execution_timeout_ms: u64,
        backgrounded: bool,
    ) -> bool {
        let key = (source_agent_id.map(str::to_owned), tool_call_id.to_owned());
        if let Some(runtime) = self.shell_runtimes.get_mut(&key) {
            let changed = runtime.started_at != started_at
                || runtime.execution_timeout_ms != execution_timeout_ms
                || runtime.backgrounded != backgrounded;
            if changed {
                runtime.started_at = started_at;
                runtime.execution_timeout_ms = execution_timeout_ms;
                runtime.backgrounded = backgrounded;
            }
            return changed;
        }
        self.shell_runtimes.insert(
            key,
            ShellRuntime {
                started_at,
                execution_timeout_ms,
                backgrounded,
            },
        );
        true
    }

    #[cfg(test)]
    pub(crate) fn set_shell_runtime_backgrounded(
        &mut self,
        source_agent_id: Option<&str>,
        tool_call_id: &str,
        backgrounded: bool,
    ) -> bool {
        let Some(runtime) = self
            .shell_runtimes
            .get_mut(&(source_agent_id.map(str::to_owned), tool_call_id.to_owned()))
        else {
            return false;
        };
        if runtime.backgrounded == backgrounded {
            return false;
        }
        runtime.backgrounded = backgrounded;
        true
    }

    pub(crate) fn apply_shell_runtime(&self, messages: &mut [MessageViewModel]) -> bool {
        self.apply_shell_runtime_for_agent(messages, None)
    }

    fn apply_shell_runtime_for_agent(
        &self,
        messages: &mut [MessageViewModel],
        source_agent_id: Option<&str>,
    ) -> bool {
        let mut any_changed = false;
        for vm in messages {
            let changed = match vm {
                MessageViewModel::ToolBlock {
                    tool_name,
                    tool_call_id,
                    content,
                    is_error,
                    started_at,
                    execution_timeout_ms,
                    shell_backgrounded,
                    ..
                } if tool_name == "Bash"
                    && !*is_error
                    && (content.is_empty()
                        || crate::ui::message_view::BackgroundTaskStarted::parse(content)
                            .is_some()) =>
                {
                    if let Some(runtime) = self
                        .shell_runtimes
                        .get(&(source_agent_id.map(str::to_owned), tool_call_id.clone()))
                    {
                        let changed = *started_at != Some(runtime.started_at)
                            || *execution_timeout_ms != Some(runtime.execution_timeout_ms)
                            || *shell_backgrounded != runtime.backgrounded;
                        *started_at = Some(runtime.started_at);
                        *execution_timeout_ms = Some(runtime.execution_timeout_ms);
                        *shell_backgrounded = runtime.backgrounded;
                        changed
                    } else {
                        false
                    }
                }
                MessageViewModel::SubAgentGroup {
                    instance_id,
                    recent_messages,
                    ..
                } => self.apply_shell_runtime_for_agent(recent_messages, instance_id.as_deref()),
                _ => false,
            };
            if changed {
                vm.recompute_hash();
                any_changed = true;
            }
        }
        any_changed
    }
}

#[cfg(test)]
#[path = "shell_runtime_test.rs"]
mod tests;
