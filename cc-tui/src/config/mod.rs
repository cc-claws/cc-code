// Re-export config types from cc-acp (single source of truth)
pub use cc_acp::provider::{
    AppConfig, PeriConfig, ProviderConfig, ProviderModels, ThinkingConfig,
};

// Re-export store functions from cc-acp
pub use cc_acp::provider::{config_path, load, load_from, save, save_to, workspace_config_path};

#[cfg(test)]
#[path = "types_test.rs"]
mod tests;
