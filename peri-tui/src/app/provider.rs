// Re-export LlmProvider from peri-acp (single source of truth)
pub use peri_acp::provider::LlmProvider;

#[cfg(test)]
#[path = "provider_test.rs"]
mod tests;
