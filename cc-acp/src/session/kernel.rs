//! Persist a kernel switch before publishing it to the live session.

use std::path::Path;

use cc_agent::{kernel::Kernel, thread::ThreadStore};

use crate::provider::{save_to, PeriConfig};

pub fn describe(
    frozen: Option<&super::executor::FrozenSessionData>,
    config: &PeriConfig,
    provider: &crate::provider::LlmProvider,
    cwd: &str,
    plugin_agent_dirs: &[std::path::PathBuf],
) -> Result<serde_json::Value, cc_agent::kernel::UnknownKernel> {
    let kernel = super::executor::resolve_kernel(frozen, config)?;
    let prompt = match frozen {
        Some(frozen) => {
            super::frozen::system_prompt_for_kernel(frozen, kernel, cwd, plugin_agent_dirs)
        }
        None => crate::prompt::build_system_prompt_for_kernel(
            None,
            cwd,
            crate::prompt::PromptFeatures::detect(),
            plugin_agent_dirs,
            None,
            config.config.language.as_deref(),
            kernel,
        ),
    };
    Ok(serde_json::json!({
        "kernelId": kernel.id(),
        "scope": if frozen.is_some() { "session" } else { "default" },
        "systemPrompt": crate::agent::builder::finalize_system_prompt(prompt, kernel, provider.model_name()),
    }))
}

pub async fn persist_switch(
    store: &dyn ThreadStore,
    session_id: &str,
    config: &PeriConfig,
    config_path: &Path,
) -> anyhow::Result<()> {
    let old_kernel = if session_id.is_empty() {
        None
    } else {
        Some(store.load_meta(&session_id.to_string()).await?.kernel_id)
    };
    if old_kernel.is_some() {
        let kernel = Kernel::from_config(config.config.agent_kernel.as_deref());
        store
            .update_kernel_snapshot(&session_id.to_string(), Some(kernel.id().to_string()))
            .await?;
    }
    if let Err(error) = save_to(config, config_path) {
        if let Some(previous) = old_kernel {
            store
                .update_kernel_snapshot(&session_id.to_string(), previous)
                .await
                .map_err(|rollback| {
                    anyhow::anyhow!("保存内核配置失败: {error}; 恢复会话内核失败: {rollback}")
                })?;
        }
        return Err(error);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use cc_agent::thread::{FilesystemThreadStore, SqliteThreadStore, ThreadMeta};
    use std::sync::Arc;

    #[tokio::test]
    async fn persistence_failure_restores_both_store_implementations() {
        let directory = tempfile::tempdir().unwrap();
        let stores: Vec<Arc<dyn ThreadStore>> = vec![
            Arc::new(FilesystemThreadStore::new(directory.path().join("threads"))),
            Arc::new(
                SqliteThreadStore::new(directory.path().join("threads.db"))
                    .await
                    .unwrap(),
            ),
        ];
        for store in stores {
            let mut meta = ThreadMeta::new(directory.path().to_string_lossy());
            meta.kernel_id = Some("dsh".to_string());
            let id = store.create_thread(meta).await.unwrap();
            let mut next = PeriConfig::default();
            next.config.agent_kernel = Some("pi".to_string());
            // A directory cannot be replaced with a settings file.
            assert!(persist_switch(store.as_ref(), &id, &next, directory.path())
                .await
                .is_err());
            assert_eq!(
                store.load_meta(&id).await.unwrap().kernel_id.as_deref(),
                Some("dsh")
            );
            let path = directory.path().join("settings.json");
            persist_switch(store.as_ref(), &id, &next, &path)
                .await
                .unwrap();
            assert_eq!(
                store.load_meta(&id).await.unwrap().kernel_id.as_deref(),
                Some("pi")
            );
            assert_eq!(
                crate::provider::store::load_from(&path)
                    .unwrap()
                    .config
                    .agent_kernel
                    .as_deref(),
                Some("pi")
            );
        }
    }

    #[tokio::test]
    async fn nonexistent_session_does_not_change_settings() {
        let directory = tempfile::tempdir().unwrap();
        let store = FilesystemThreadStore::new(directory.path().join("threads"));
        let path = directory.path().join("settings.json");
        assert!(
            persist_switch(&store, "missing", &PeriConfig::default(), &path)
                .await
                .is_err()
        );
        assert!(!path.exists());
    }
}
