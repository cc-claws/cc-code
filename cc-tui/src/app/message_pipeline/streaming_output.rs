use super::{MessagePipeline, MessageViewModel, ToolOutputPreview, ToolPhase};

impl MessagePipeline {
    pub(super) fn clear_tool_output(&mut self, source: Option<&str>, id: &str) -> bool {
        if let Some(source) = source {
            if let Some(sub) = self.find_running_subagent_mut(source) {
                if let Some(vm) = sub.recent_messages.iter_mut().find(|vm| {
                    matches!(vm, MessageViewModel::ToolBlock { tool_call_id, .. } if tool_call_id == id)
                }) {
                    if let MessageViewModel::ToolBlock { live_output, .. } = vm {
                        let cleared = live_output.take().is_some();
                        if cleared { vm.recompute_hash(); }
                        return cleared;
                    }
                }
            }
            false
        } else {
            self.pending_tools
                .get_mut(id)
                .is_some_and(|pending| pending.live_output.take().is_some())
        }
    }

    pub(super) fn update_tool_output(
        &mut self,
        id: &str,
        output: ToolOutputPreview,
        source: Option<&str>,
    ) {
        if self.shell_runtime_is_backgrounded(source, id) {
            return;
        }
        if let Some(source) = source {
            if let Some(sub) = self.find_running_subagent_mut(source) {
                if let Some(vm) = sub.recent_messages.iter_mut().find(|vm| {
                    matches!(vm, MessageViewModel::ToolBlock { tool_call_id, .. } if tool_call_id == id)
                }) {
                    if let MessageViewModel::ToolBlock {
                        tool_name,
                        phase: ToolPhase::Running,
                        live_output,
                        is_error: false,
                        shell_backgrounded: false,
                        ..
                    } = vm
                    {
                        if tool_name == "Bash" && accepts_output(live_output.as_ref(), &output) {
                            *live_output = Some(output);
                            vm.recompute_hash();
                        }
                    }
                }
            }
        } else if let Some(pending) = self.pending_tools.get_mut(id) {
            if pending.name == "Bash"
                && pending.phase == ToolPhase::Running
                && accepts_output(pending.live_output.as_ref(), &output)
            {
                pending.live_output = Some(output);
            }
        }
    }
}

fn accepts_output(previous: Option<&ToolOutputPreview>, next: &ToolOutputPreview) -> bool {
    previous.is_none_or(|previous| {
        (next.total_bytes, next.elapsed_ms) >= (previous.total_bytes, previous.elapsed_ms)
    })
}

#[cfg(test)]
#[path = "streaming_output_test.rs"]
mod tests;
