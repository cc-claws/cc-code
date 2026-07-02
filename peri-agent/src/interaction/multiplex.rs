use std::sync::Arc;

use async_trait::async_trait;

use crate::interaction::{
    ApprovalDecision, InteractionContext, InteractionResponse, UserInteractionBroker,
};

/// 多路 broker：将多个子 broker 的请求竞速，先到先得
pub struct MultiplexBroker {
    brokers: Vec<(String, Arc<dyn UserInteractionBroker>)>,
}

impl MultiplexBroker {
    pub fn new(brokers: Vec<(String, Arc<dyn UserInteractionBroker>)>) -> Self {
        Self { brokers }
    }
}

#[async_trait]
impl UserInteractionBroker for MultiplexBroker {
    async fn request(&self, ctx: InteractionContext) -> InteractionResponse {
        if self.brokers.is_empty() {
            return InteractionResponse::Decisions(vec![]);
        }
        if self.brokers.len() == 1 {
            return self.brokers[0].1.request(ctx).await;
        }

        // Spawn all brokers in parallel, race via mpsc channel
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        for (name, broker) in &self.brokers {
            let ctx = ctx.clone();
            let broker = broker.clone();
            let name = name.clone();
            let tx = tx.clone();
            tokio::spawn(async move {
                let response = broker.request(ctx).await;
                let _ = tx.send((name, response));
            });
        }
        // Drop the original sender so rx.recv() returns None when all spawned tasks are done
        drop(tx);

        // Race: 收集第一个响应。如果是全 Reject（如 ChannelBroker 无授权），
        // 继续等下一个响应——TUI broker 可能还在等待用户点击。
        let mut first_reject: Option<(String, InteractionResponse)> = None;
        loop {
            match rx.recv().await {
                Some((name, response)) => {
                    let all_reject = matches!(
                        &response,
                        InteractionResponse::Decisions(decisions)
                            if decisions.iter().all(|d| matches!(d, ApprovalDecision::Reject { .. }))
                    );
                    if all_reject && first_reject.is_none() {
                        // 暂存全 Reject 响应，继续等其他 broker
                        first_reject = Some((name, response));
                        continue;
                    }
                    // 非全 Reject（有 Approve 或 Questions），立即采用
                    return tag_source(response, &name);
                }
                None => {
                    // 所有 broker 都返回了，用暂存的全 Reject 或空兜底
                    let (source_name, response) = first_reject.unwrap_or_else(|| {
                        ("error".to_string(), InteractionResponse::Decisions(vec![]))
                    });
                    return tag_source(response, &source_name);
                }
            }
        }
    }
}

/// Tag all ApprovalDecision variants with the broker's name
fn tag_source(response: InteractionResponse, source: &str) -> InteractionResponse {
    match response {
        InteractionResponse::Decisions(decisions) => {
            let tagged: Vec<_> = decisions
                .into_iter()
                .map(|d| match d {
                    ApprovalDecision::Approve { .. } => ApprovalDecision::Approve {
                        source: Some(source.to_string()),
                    },
                    ApprovalDecision::Reject { reason, .. } => ApprovalDecision::Reject {
                        reason,
                        source: Some(source.to_string()),
                    },
                    other => other,
                })
                .collect();
            InteractionResponse::Decisions(tagged)
        }
        InteractionResponse::Answers(answers) => InteractionResponse::Answers(answers),
    }
}
