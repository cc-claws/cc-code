use std::time::Instant;

use crate::ui::message_view::{MessageViewModel, ToolPhase};

use super::MessagePipeline;

pub(super) struct ShellRuntime {
    started_at: Instant,
    timeout_ms: u64,
    backgrounded: bool,
}

impl MessagePipeline {
    pub(super) fn shell_runtime_is_backgrounded(&self, source: Option<&str>, id: &str) -> bool {
        self.shell_runtimes
            .get(&(source.map(str::to_owned), id.to_owned()))
            .is_some_and(|runtime| runtime.backgrounded)
    }

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
        timeout_ms: u64,
    ) {
        self.sync_shell_runtime(source_agent_id, tool_call_id, started_at, timeout_ms, false);
    }

    /// 从仍存活的任务槽恢复 UI 元数据；线程切换会清空 pipeline，但任务所有权不变。
    pub(crate) fn sync_shell_runtime(
        &mut self,
        source_agent_id: Option<&str>,
        tool_call_id: &str,
        started_at: Instant,
        timeout_ms: u64,
        backgrounded: bool,
    ) -> bool {
        let key = (source_agent_id.map(str::to_owned), tool_call_id.to_owned());
        let changed = if let Some(runtime) = self.shell_runtimes.get_mut(&key) {
            let changed = runtime.started_at != started_at
                || runtime.timeout_ms != timeout_ms
                || runtime.backgrounded != backgrounded;
            if changed {
                runtime.started_at = started_at;
                runtime.timeout_ms = timeout_ms;
                runtime.backgrounded = backgrounded;
            }
            changed
        } else {
            self.shell_runtimes.insert(
                key,
                ShellRuntime {
                    started_at,
                    timeout_ms,
                    backgrounded,
                },
            );
            true
        };
        let cleared = backgrounded && self.clear_tool_output(source_agent_id, tool_call_id);
        changed || cleared
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
                    phase,
                    started_at,
                    shell_backgrounded,
                    live_output,
                    ..
                } if tool_name == "Bash"
                    && !*is_error
                    && (*phase != ToolPhase::Finished
                        || crate::ui::message_view::BackgroundTaskStarted::parse(content)
                            .is_some()) =>
                {
                    if let Some(runtime) = self
                        .shell_runtimes
                        .get(&(source_agent_id.map(str::to_owned), tool_call_id.clone()))
                    {
                        // ToolStart 已有起点；spawn 注册仅给旧路径和后台恢复补元数据。
                        let changed = started_at.is_none()
                            || *shell_backgrounded != runtime.backgrounded
                            || (runtime.backgrounded && live_output.is_some());
                        if started_at.is_none() {
                            *started_at = Some(runtime.started_at);
                        }
                        // timeout 只来自模型显式入参，不能把执行器默认期限显示出来。
                        *shell_backgrounded = runtime.backgrounded;
                        if runtime.backgrounded {
                            *live_output = None;
                        }
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
