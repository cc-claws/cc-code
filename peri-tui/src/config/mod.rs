pub mod store;

// Re-export config types from peri-acp (single source of truth)
pub use peri_acp::provider::{
    AppConfig, PeriConfig, ProviderConfig, ProviderModels, ThinkingConfig,
};

pub use store::{load, save};

#[cfg(test)]
#[path = "types_test.rs"]
mod tests;
