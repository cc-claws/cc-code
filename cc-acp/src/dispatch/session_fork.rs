//! Fork a session: create a new thread and copy messages from source.

use cc_agent::{
    messages::BaseMessage,
    thread::{ThreadId, ThreadMeta, ThreadStore},
};

/// Fork a session by creating a new thread and copying source messages.
///
/// Returns `Ok((new_thread_id, copied_messages))` on success.
/// The caller is responsible for inserting the new session into its session map.
pub async fn fork_session(
    thread_store: &dyn ThreadStore,
    source_thread_id: &str,
    source_messages: &[BaseMessage],
    cwd: &str,
) -> Result<(String, Vec<BaseMessage>), String> {
    fork_session_with_kernel(thread_store, source_thread_id, source_messages, cwd, None).await
}

pub async fn fork_session_with_kernel(
    thread_store: &dyn ThreadStore,
    source_thread_id: &str,
    source_messages: &[BaseMessage],
    cwd: &str,
    kernel_id: Option<String>,
) -> Result<(String, Vec<BaseMessage>), String> {
    let source_meta = thread_store
        .load_meta(&source_thread_id.to_string())
        .await
        .map_err(|error| format!("Source thread not found: {error}"))?;
    let mut meta = ThreadMeta::new(cwd);
    meta.kernel_id = kernel_id.or(source_meta.kernel_id);
    if let Some(id) = meta.kernel_id.as_deref() {
        cc_agent::kernel::Kernel::try_from_id(id).map_err(|error| error.to_string())?;
    }
    let new_thread_id = thread_store
        .create_thread(meta)
        .await
        .map_err(|e| format!("Thread creation failed: {e}"))?;

    if !source_messages.is_empty() {
        if let Err(e) = thread_store
            .append_messages(&ThreadId::from(new_thread_id.clone()), source_messages)
            .await
        {
            if let Err(cleanup) = thread_store.delete_thread(&new_thread_id).await {
                tracing::warn!(error = %cleanup, "session/fork: failed to clean up incomplete thread");
            }
            return Err(format!("Failed to copy session history: {e}"));
        }
    }

    tracing::info!(
        source = %source_thread_id,
        new = %new_thread_id,
        msg_count = source_messages.len(),
        "Session forked"
    );

    Ok((new_thread_id, source_messages.to_vec()))
}
