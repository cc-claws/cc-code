use crate::plugin::loader::LoadedPlugin;
use rust_create_agent::agent::state::State;
use rust_create_agent::middleware::r#trait::Middleware;
use std::sync::Arc;

pub struct PluginMiddleware {
    plugins: Arc<Vec<LoadedPlugin>>,
}

impl PluginMiddleware {
    pub fn new(plugins: Vec<LoadedPlugin>) -> Self {
        Self {
            plugins: Arc::new(plugins),
        }
    }

    pub fn plugins(&self) -> &[LoadedPlugin] {
        &self.plugins
    }
}

#[async_trait::async_trait]
impl<S: State> Middleware<S> for PluginMiddleware {
    fn name(&self) -> &str {
        "PluginMiddleware"
    }

    async fn before_agent(&self, _state: &mut S) -> rust_create_agent::error::AgentResult<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin::loader::tests::make_manifest_with_commands;
    use rust_create_agent::agent::state::AgentState;
    use rust_create_agent::middleware::r#trait::Middleware;
    use std::collections::HashMap;
    use std::path::PathBuf;

    fn make_loaded_plugin(name: &str) -> LoadedPlugin {
        LoadedPlugin {
            name: name.into(),
            version: "1.0.0".into(),
            install_path: PathBuf::new(),
            manifest: make_manifest_with_commands(vec![]),
            commands: vec![],
            skills_dirs: vec![],
            agents_dirs: vec![],
            mcp_servers: HashMap::new(),
            data_path: PathBuf::new(),
            hooks_config: None,
            marketplace: String::new(),
        }
    }

    #[test]
    fn test_middleware_name() {
        let mw = PluginMiddleware::new(vec![]);
        assert_eq!(Middleware::<AgentState>::name(&mw), "PluginMiddleware");
    }

    #[tokio::test]
    async fn test_middleware_before_agent_noop() {
        let mw = PluginMiddleware::new(vec![]);
        let mut state = AgentState::new("/tmp");
        let result = mw.before_agent(&mut state).await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_middleware_plugins_accessor() {
        let mw = PluginMiddleware::new(vec![make_loaded_plugin("test")]);
        assert_eq!(mw.plugins().len(), 1);
    }
}
