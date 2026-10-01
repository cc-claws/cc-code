use crate::messages::BaseMessage;

#[derive(Debug, Clone)]
pub(crate) struct MessageRound {
    pub(crate) start: usize,
    pub(crate) end: usize,
}

pub(crate) fn group_messages_by_round(messages: &[BaseMessage]) -> Vec<MessageRound> {
    let mut rounds = Vec::new();
    let mut i = 0;
    while i < messages.len() {
        let round_start = i;
        if matches!(&messages[i], BaseMessage::Ai { tool_calls, .. } if !tool_calls.is_empty()) {
            let tool_call_ids: Vec<String> = messages[i]
                .tool_calls()
                .iter()
                .map(|tc| tc.id.clone())
                .collect();
            let tc_count = tool_call_ids.len();
            let mut end = i + 1;
            let mut matched = 0;
            while end < messages.len() && matched < tc_count {
                if let BaseMessage::Tool { tool_call_id, .. } = &messages[end] {
                    if tool_call_ids.contains(tool_call_id) {
                        matched += 1;
                    } else {
                        break;
                    }
                } else {
                    break;
                }
                end += 1;
            }
            rounds.push(MessageRound {
                start: round_start,
                end,
            });
            i = end;
        } else {
            rounds.push(MessageRound {
                start: round_start,
                end: i + 1,
            });
            i += 1;
        }
    }
    rounds
}

fn find_tool_pair_boundary(messages: &[BaseMessage], index: usize) -> (usize, usize) {
    let tool_call_id = match &messages[index] {
        BaseMessage::Tool { tool_call_id, .. } => tool_call_id.clone(),
        _ => return (index, index + 1),
    };

    let ai_index = messages[..index]
        .iter()
        .enumerate()
        .rev()
        .find_map(|(i, msg)| {
            if msg.has_tool_calls() {
                let ids: Vec<&str> = msg.tool_calls().iter().map(|tc| tc.id.as_str()).collect();
                if ids.contains(&tool_call_id.as_str()) {
                    return Some(i);
                }
            }
            None
        })
        .unwrap_or(index);

    let all_tc_ids: Vec<String> = messages[ai_index]
        .tool_calls()
        .iter()
        .map(|tc| tc.id.clone())
        .collect();

    let mut end = ai_index + 1;
    while end < messages.len() {
        if let BaseMessage::Tool {
            tool_call_id: tc_id,
            ..
        } = &messages[end]
        {
            if all_tc_ids.iter().any(|id| id == tc_id) {
                end += 1;
            } else {
                break;
            }
        } else {
            break;
        }
    }

    (ai_index, end)
}

pub(crate) fn adjust_index_to_preserve_invariants(
    messages: &[BaseMessage],
    start: usize,
    end: usize,
) -> (usize, usize) {
    if messages.is_empty() || start >= end || start >= messages.len() {
        return (start.min(messages.len()), end.min(messages.len()));
    }

    let mut adjusted_start = start;
    let mut adjusted_end = end.min(messages.len());

    let (pair_start, _pair_end) = find_tool_pair_boundary(messages, adjusted_start);
    if pair_start < adjusted_start {
        adjusted_start = pair_start;
    }

    if adjusted_end < messages.len() {
        let (pair_start, pair_end) = find_tool_pair_boundary(messages, adjusted_end);
        if pair_start < adjusted_end && pair_end > adjusted_end {
            adjusted_end = pair_end;
        }
    }

    // 使用 while 循环替代 for 循环，确保边界扩展后新加入的 Tool 消息也被检查
    let mut i = adjusted_start;
    while i < adjusted_end {
        if matches!(&messages[i], BaseMessage::Tool { .. }) {
            let (ps, pe) = find_tool_pair_boundary(messages, i);
            if ps < adjusted_start {
                adjusted_start = ps;
            }
            if pe > adjusted_end {
                adjusted_end = pe.min(messages.len());
            }
        }
        i += 1;
    }

    (adjusted_start, adjusted_end)
}

#[cfg(test)]
#[path = "invariant_test.rs"]
mod tests;
