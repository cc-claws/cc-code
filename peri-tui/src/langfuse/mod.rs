//! Temporary bridge: re-exports Langfuse types from peri-acp.
//! Will be removed in Step 6-j when old dependencies are cleaned up.

pub use peri_acp::langfuse::{
    config::LangfuseConfig, session::LangfuseSession, tracer::LangfuseTracer,
};
