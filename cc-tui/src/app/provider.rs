// Re-export LlmProvider from cc-acp (single source of truth)
pub use cc_acp::provider::LlmProvider;

#[cfg(test)]
#[path = "provider_test.rs"]
mod tests;
