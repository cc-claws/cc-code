//! ACP Compact execution — manual `/compact` triggered by user command.
//!
//! Reads session history, runs full_compact + re_inject, pushes events
//! through transport, and updates session state.

use std::sync::Arc;

use peri_acp::session::event_sink::EventSink;
use peri_acp::session::event_sink::TransportEventSink;
use peri_acp::transport::types::AcpError;
use peri_agent::agent::compact::{full_compact, re_inject};
use peri_agent::agent::events::{AgentEvent as ExecutorEvent, CompactFileInfo};
use peri_agent::messages::BaseMessage;
use serde_json::{json, Value};
use tracing::{info, warn};

use super::SharedSessions;

/// Execute manual full compact on a session.
///
/// Reads history from session state, runs `full_compact()` + `re_inject()`,
/// pushes `CompactStarted`/`CompactCompleted` events through transport,
/// and updates session history with the compressed messages.
pub(crate) async fn execute_compact(
    session_id: &str,
    sessions: &SharedSessions,
    provider: &Arc<parking_lot::RwLock<crate::app::agent::LlmProvider>>,
    peri_config: &Arc<parking_lot::RwLock<crate::config::PeriConfig>>,
    transport: &Arc<dyn peri_acp::transport::AcpTransport>,
) -> Result<Value, AcpError> {
    // 读取 session 数据
    let (cwd, history) = {
        let sessions = sessions.lock().await;
        let state = sessions
            .get(session_id)
            .ok_or_else(|| AcpError::new(-32602, format!("session not found: {session_id}")))?;
        (state.cwd.clone(), state.history.clone())
    };

    if history.is_empty() {
        return Err(AcpError::new(-32603, "no history to compact"));
    }

    // compact 配置
    let compact_config = peri_config
        .read()
        .config
        .compact
        .clone()
        .unwrap_or_default();
    let effective_config = {
        let mut c = compact_config.clone();
        c.apply_env_overrides();
        c
    };

    // 获取 compact model
    let compact_model: Arc<dyn peri_agent::llm::BaseModel> = {
        let p = provider.read().clone();
        p.clone().into_model().into()
    };

    let event_sink = Arc::new(TransportEventSink::new(Arc::clone(transport)));

    // 发送 CompactStarted 事件
    event_sink
        .push_event(session_id, &ExecutorEvent::CompactStarted, 0)
        .await;

    // 执行 full_compact
    let compact_result =
        match full_compact(&history, compact_model.as_ref(), &effective_config, "").await {
            Ok(r) => r,
            Err(e) => {
                warn!(error = %e, "Manual compact: full_compact failed");
                event_sink
                    .push_event(
                        session_id,
                        &ExecutorEvent::CompactError {
                            message: e.to_string(),
                        },
                        0,
                    )
                    .await;
                return Err(AcpError::new(-32603, format!("compact failed: {e}")));
            }
        };

    info!(
        summary_len = compact_result.summary.len(),
        "Manual compact: full_compact completed"
    );

    // 执行 re_inject
    let re_inject_result = re_inject(&history, &effective_config, &cwd).await;

    info!(
        files_injected = re_inject_result.files_injected,
        skills_injected = re_inject_result.skills_injected,
        "Manual compact: re_inject completed"
    );

    // 提取文件和 skill 信息
    let files = extract_file_info(&re_inject_result.messages);
    let skills = extract_skill_names(&re_inject_result.messages);

    // 摘要作为 Human 消息（与 auto-compact 路径和 Claude Code 实现对齐）
    let summary_content = format!(
        "{}\n\n[上下文已压缩，请根据摘要继续工作]",
        compact_result.summary
    );
    let mut new_messages = vec![BaseMessage::human(summary_content)];
    new_messages.extend(re_inject_result.messages.clone());

    // 发送 CompactCompleted 事件
    event_sink
        .push_event(
            session_id,
            &ExecutorEvent::CompactCompleted {
                summary: compact_result.summary,
                files: files.clone(),
                skills: skills.clone(),
                micro_cleared: 0,
                messages: new_messages.clone(),
            },
            0,
        )
        .await;

    // 更新 session history
    {
        let mut sessions = sessions.lock().await;
        if let Some(state) = sessions.get_mut(session_id) {
            state.history = new_messages;
        }
    }

    info!("Manual compact: completed and session updated");
    Ok(json!({ "success": true }))
}

fn extract_file_info(messages: &[BaseMessage]) -> Vec<CompactFileInfo> {
    let mut files = Vec::new();
    for msg in messages {
        let content = msg.content();
        if let Some(rest) = content.strip_prefix("[最近读取的文件: ") {
            let path = rest.lines().next().unwrap_or("");
            let line_count = rest.lines().count().saturating_sub(1);
            if !path.is_empty() {
                files.push(CompactFileInfo {
                    path: path.to_string(),
                    lines: line_count,
                });
            }
        }
    }
    files
}

fn extract_skill_names(messages: &[BaseMessage]) -> Vec<String> {
    let mut skills = Vec::new();
    for msg in messages {
        let content = msg.content();
        if let Some(rest) = content.strip_prefix("[激活的 Skill 指令: ") {
            let name = rest.lines().next().unwrap_or("");
            if !name.is_empty() {
                skills.push(name.to_string());
            }
        }
    }
    skills
}
