//! Event mapping from ExecutorEvent to ACP SessionUpdate and peri/agent_event routing.

pub mod mapper;
pub use mapper::{map_event, MappedEvent};
