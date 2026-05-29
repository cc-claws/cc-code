use crate::{
    agent::compact::{
        config::CompactConfig,
        invariant::{adjust_index_to_preserve_invariants, group_messages_by_round},
    },
    messages::{BaseMessage, ContentBlock, MessageContent},
};

fn find_tool_name_for_tool_result(messages: &[BaseMessage], tool_call_id: &str) -> Option<String> {
    for msg in messages.iter().rev() {
        if let BaseMessage::Ai { tool_calls, .. } = msg {
            for tc in tool_calls {
                if tc.id == tool_call_id {
                    return Some(tc.name.clone());
                }
            }
        }
    }
    None
}

fn compact_tool_result_content(content: &mut MessageContent, config: &CompactConfig) -> bool {
    let blocks = content.content_blocks();

    let has_image_or_doc = blocks.iter().any(|b| {
        matches!(
            b,
            ContentBlock::Image { .. } | ContentBlock::Document { .. }
        )
    });

    if !has_image_or_doc {
        return false;
    }

    // 纯图像/文档内容（无文本）也可以被压缩，不跳过

    let mut modified = false;
    let new_blocks: Vec<ContentBlock> = blocks
        .into_iter()
        .map(|b| match &b {
            ContentBlock::Image { source } => {
                let size_chars = match source {
                    // Base64 编码膨胀 4/3 倍，需用解码后大小估算
                    crate::messages::ImageSource::Base64 { data, .. } => data.len() * 3 / 4,
                    crate::messages::ImageSource::Url { url } => url.len(),
                };
                let token_est = size_chars / 4;
                modified = true;
                if token_est > config.re_inject_max_tokens_per_file as usize {
                    ContentBlock::text(format!("[compacted: image ~{} tokens]", token_est))
                } else {
                    ContentBlock::text("[image]")
                }
            }
            ContentBlock::Document { source, .. } => {
                let size_chars = match source {
                    crate::messages::DocumentSource::Base64 { data, .. } => data.len() * 3 / 4,
                    crate::messages::DocumentSource::Text { text } => text.len(),
                    crate::messages::DocumentSource::Url { url } => url.len(),
                };
                let token_est = size_chars / 4;
                modified = true;
                if token_est > config.re_inject_max_tokens_per_file as usize {
                    ContentBlock::text(format!("[compacted: document ~{} tokens]", token_est))
                } else {
                    ContentBlock::text("[document]")
                }
            }
            _ => b,
        })
        .collect();

    if modified {
        *content = MessageContent::blocks(new_blocks);
    }
    modified
}

pub fn micro_compact_enhanced(
    config: &CompactConfig,
    messages: &mut [BaseMessage],
    ancestor_len: usize,
) -> usize {
    if messages.is_empty() {
        return 0;
    }

    // 只对 own messages（ancestor_len..）进行分组和压缩
    if ancestor_len >= messages.len() {
        return 0;
    }
    let own_messages = &mut messages[ancestor_len..];

    let rounds = group_messages_by_round(own_messages);
    let total_rounds = rounds.len();
    let stale_threshold = config.micro_compact_stale_steps;
    let stale_round_limit = total_rounds.saturating_sub(stale_threshold);

    let mut round_index = vec![0usize; own_messages.len()];
    for (ri, round) in rounds.iter().enumerate() {
        #[allow(clippy::needless_range_loop)]
        for mi in round.start..round.end {
            if mi < own_messages.len() {
                round_index[mi] = ri;
            }
        }
    }

    let mut compactable_indices: Vec<usize> = Vec::new();
    for (i, msg) in own_messages.iter().enumerate() {
        if let BaseMessage::Tool {
            tool_call_id,
            is_error,
            ..
        } = msg
        {
            if *is_error {
                continue;
            }
            if round_index[i] >= stale_round_limit {
                continue;
            }
            let tool_name = find_tool_name_for_tool_result(own_messages, tool_call_id);
            match tool_name {
                Some(ref name) if config.micro_compactable_tools.contains(name) => {}
                _ => continue,
            }
            compactable_indices.push(i);
        }
    }

    if compactable_indices.is_empty() {
        let mut image_cleared = 0;
        for i in 0..own_messages.len() {
            if round_index[i] >= stale_round_limit {
                continue;
            }
            if let BaseMessage::Tool {
                content, is_error, ..
            } = &mut own_messages[i]
            {
                if *is_error {
                    continue;
                }
                if compact_tool_result_content(content, config) {
                    image_cleared += 1;
                }
            }
        }
        return image_cleared;
    }

    let compact_start = *compactable_indices.first().unwrap();
    let compact_end = *compactable_indices.last().unwrap() + 1;
    let (adj_start, adj_end) =
        adjust_index_to_preserve_invariants(own_messages, compact_start, compact_end);

    let mut cleared = 0;
    for i in adj_start..adj_end {
        if round_index[i] >= stale_round_limit {
            continue;
        }
        let (tc_id, is_err) = match &own_messages[i] {
            BaseMessage::Tool {
                tool_call_id,
                is_error,
                ..
            } => (tool_call_id.clone(), *is_error),
            _ => continue,
        };
        if is_err {
            continue;
        }
        let tool_name = find_tool_name_for_tool_result(own_messages, &tc_id);
        let in_whitelist = match tool_name {
            Some(ref name) => config.micro_compactable_tools.contains(name),
            None => false,
        };
        if !in_whitelist {
            continue;
        }

        if let BaseMessage::Tool { content, .. } = &mut own_messages[i] {
            let original_text = content.text_content();
            // 跳过已压缩的消息，避免重复处理（覆盖 chars/image/document 三种格式）
            if original_text.starts_with("[compacted:") {
                continue;
            }
            let was_modified = compact_tool_result_content(content, config);

            if !was_modified {
                // 纯文本为空且无图像/文档 → 不压缩
                if original_text.is_empty() {
                    continue;
                }
                *content = MessageContent::text(format!(
                    "[compacted: {} chars]",
                    original_text.chars().count()
                ));
            }
            cleared += 1;
        }
    }

    cleared
}

#[cfg(test)]
#[path = "micro_test.rs"]
mod tests;
